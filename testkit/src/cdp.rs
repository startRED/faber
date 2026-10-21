//! A small DevTools client: the page's target list over HTTP, then one WebSocket. It can evaluate a script, wait for a
//! condition, click, type, press keys and take a screenshot. WebView2 (Windows) is the only webview that has the port.

use base64::Engine as _;
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

/// How long a wait lasts unless a scenario says otherwise.
pub const DEFAULT_WAIT: Duration = Duration::from_secs(15);

/// The longest one protocol call may take, events and all.
pub const CALL_LIMIT: Duration = Duration::from_secs(30);

/// A browser-visible HTTPS origin answered from a loopback fixture by the trusted runner, never by the internet.
pub const HTTPS_FIXTURE_ORIGIN: &str = "https://visual-fixture.example";

pub struct Cdp {
    socket: WebSocket<Limited>,
    next: u64,
    /// An outer deadline (a wait that polls) that every call under it must meet as well.
    deadline: Option<Instant>,
    frame: Value,
    loaded: Option<String>,
    https_fixture: Option<SocketAddr>,
}

/// The time left until `deadline`. Zero is an error too, because a socket timeout of zero means "no timeout".
fn left(deadline: Instant, what: &str) -> Result<Duration, String> {
    deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or_else(|| format!("timed out waiting for {what}"))
}

fn protocol_timed_out(error: &tungstenite::Error, deadline: Instant) -> bool {
    Instant::now() >= deadline
        || matches!(error, tungstenite::Error::Io(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock))
}

/// A connection that cannot outlast its deadline. The socket's own timeout restarts with every read, so a peer that
/// sends a header or a frame a byte at a time would keep it going for ever; here the timeout is set to what is left of
/// the one deadline before each read and each write, whoever asks (the HTTP parser or the WebSocket framing).
struct Limited {
    stream: TcpStream,
    deadline: Instant,
}

impl Limited {
    fn time_left(&self) -> io::Result<Duration> {
        left(self.deadline, "the DevTools port").map_err(|e| io::Error::new(io::ErrorKind::TimedOut, e))
    }
}

impl Read for Limited {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.set_read_timeout(Some(self.time_left()?))?;
        self.stream.read(buf)
    }
}

impl Write for Limited {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.set_write_timeout(Some(self.time_left()?))?;
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.set_write_timeout(Some(self.time_left()?))?;
        self.stream.flush()
    }
}

impl Cdp {
    /// Waits for the app's DevTools port to list a page, then connects to it. The whole of it, every connect, write and
    /// read, shares one deadline.
    pub fn connect(port: u16, timeout: Duration) -> Result<Cdp, String> {
        Cdp::connect_where(port, timeout, |url| app_url(&Value::from(url)))
    }

    /// The same, for the first page whose address `wanted` accepts: a second webview on the same port, for example.
    pub fn connect_where(port: u16, timeout: Duration, wanted: fn(&str) -> bool) -> Result<Cdp, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let error = match page_url(port, deadline, wanted).and_then(|url| open(&url, deadline)) {
                Ok(mut cdp) => {
                    // An offscreen window is never focused; without this it ignores keys until someone clicks it.
                    cdp.set_deadline(Some(deadline));
                    cdp.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true }))?;
                    cdp.set_deadline(None);
                    return Ok(cdp);
                }
                Err(e) => e,
            };
            if Instant::now() >= deadline {
                return Err(format!("the app's DevTools port {port} did not offer a page ({error})"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Makes every call until `set_deadline(None)` meet `deadline` as well as its own limit.
    pub fn set_deadline(&mut self, deadline: Option<Instant>) -> Option<Instant> {
        std::mem::replace(&mut self.deadline, deadline)
    }

    /// Intercepts only the fixture's HTTPS origin after browser policy checks. The browser keeps its real CSP and native
    /// request filter; only the runner reads the local server. Opaque frames can have their own DevTools session.
    pub fn serve_https_fixture(&mut self, origin: &str) -> Result<(), String> {
        let address: SocketAddr = origin
            .strip_prefix("http://")
            .ok_or("the fixture must be loopback HTTP")?
            .parse()
            .map_err(|e| format!("the fixture address: {e}"))?;
        if !address.ip().is_loopback() {
            return Err("the fixture must be loopback HTTP".into());
        }
        self.https_fixture = Some(address);
        self.call(
            "Fetch.enable",
            json!({ "patterns": [{ "urlPattern": format!("{HTTPS_FIXTURE_ORIGIN}/*"), "requestStage": "Request" }] }),
        )?;
        self.call(
            "Target.setAutoAttach",
            json!({ "autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true,
            "filter": [{ "type": "iframe", "exclude": false }, { "exclude": true }] }),
        )?;
        Ok(())
    }

    // These commands must not wait inside read(): a nested call would consume the original call's answer.
    fn fixture_command(&mut self, method: &str, params: Value, session: &Value) -> Result<(), String> {
        self.next += 1;
        let mut command = json!({ "id": self.next, "method": method, "params": params });
        if let Some(session) = session.as_str() {
            command["sessionId"] = json!(session);
        }
        self.socket.send(Message::text(command.to_string())).map_err(|e| format!("DevTools fixture command: {e}"))
    }

    fn fixture_event(&mut self, event: &Value, deadline: Instant) -> Result<(), String> {
        let Some(address) = self.https_fixture else { return Ok(()) };
        let params = &event["params"];
        if event["method"] == "Target.attachedToTarget" {
            let session = &params["sessionId"];
            self.fixture_command(
                "Fetch.enable",
                json!({ "patterns": [{ "urlPattern": format!("{HTTPS_FIXTURE_ORIGIN}/*"), "requestStage": "Request" }] }),
                session,
            )?;
            return self.fixture_command("Runtime.runIfWaitingForDebugger", json!({}), session);
        }
        if event["method"] != "Fetch.requestPaused" {
            return Ok(());
        }
        let path = params["request"]["url"]
            .as_str()
            .and_then(|url| url.strip_prefix(HTTPS_FIXTURE_ORIGIN))
            .filter(|path| path.starts_with('/') && !path.contains(['\r', '\n']))
            .ok_or("an unexpected request reached the HTTPS fixture interceptor")?;
        let resource = match path {
            "/missing.js" | "/missing.svg" | "/missing.css" => Some((404, "text/plain", "")),
            "/fast.svg" => Some((200, "image/svg+xml", "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"/>")),
            "/fast.css" => Some((200, "text/css", "body { color: rgb(1, 2, 3); }")),
            "/import.css" => Some((200, "text/css", "#cssProbe { --visual-import: Ready; }")),
            "/integrity.js" => Some((200, "text/javascript", "window.visualIntegrityRan=true;")),
            "/metadata.js" => Some((
                200,
                "text/javascript",
                "{const s=document.currentScript;window.visualMetadata=s.id==='metadata-script'&&s.dataset.chart==='budget'&&s.crossOrigin==='anonymous'&&!s.hasAttribute('nonce')&&Array.from(s.attributes).every(a=>!a.name.startsWith('data-faber-'));}",
            )),
            _ => None,
        };
        if let Some((status, kind, body)) = resource {
            return self.fixture_command(
                "Fetch.fulfillRequest",
                json!({ "requestId": params["requestId"], "responseCode": status,
                "responseHeaders": [{ "name": "Content-Type", "value": kind }, { "name": "Access-Control-Allow-Origin", "value": "*" },
                    { "name": "Cache-Control", "value": "no-store" }], "body": base64::engine::general_purpose::STANDARD.encode(body) }),
                &event["sessionId"],
            );
        }
        if path == "/classic-no-cors.js" {
            return self.fixture_command(
                "Fetch.fulfillRequest",
                json!({ "requestId": params["requestId"], "responseCode": 200,
                    "responseHeaders": [{ "name": "Content-Type", "value": "text/javascript" },
                        { "name": "Cache-Control", "value": "no-store" }],
                    "body": base64::engine::general_purpose::STANDARD.encode("window.visualNoCorsClassic=true;") }),
                &event["sessionId"],
            );
        }
        let mut stream =
            TcpStream::connect_timeout(&address, left(deadline, "the HTTPS fixture")?).map_err(|e| format!("fixture connect: {e}"))?;
        stream.set_read_timeout(Some(left(deadline, "the HTTPS fixture")?)).map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(left(deadline, "the HTTPS fixture")?)).map_err(|e| e.to_string())?;
        write!(stream, "GET {path} HTTP/1.1\r\nHost: fixture\r\nConnection: close\r\n\r\n").map_err(|e| format!("fixture request: {e}"))?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|e| format!("fixture response: {e}"))?;
        let split = raw.windows(4).position(|bytes| bytes == b"\r\n\r\n").ok_or("the fixture response has no headers")?;
        if !raw.starts_with(b"HTTP/1.1 200 ") {
            return Err(format!("the HTTPS fixture refused {path}"));
        }
        // The public fixture's root redirects normally; the private target is never intercepted or answered here.
        if path == "/" {
            return self.fixture_command(
                "Fetch.fulfillRequest",
                json!({ "requestId": params["requestId"], "responseCode": 302,
                "responseHeaders": [{ "name": "Location", "value": format!("https://{address}/releases/latest") },
                    { "name": "Access-Control-Allow-Origin", "value": "*" }, { "name": "Cache-Control", "value": "no-store" }],
                "body": "" }),
                &event["sessionId"],
            );
        }
        let kind = if path == "/codex.zip" { "text/javascript" } else { "application/json" };
        if path == "/codex.zip" {
            // Give an incorrectly eager inline module time to run while the later classic script is still loading.
            std::thread::sleep(Duration::from_millis(100));
            left(deadline, "the delayed HTTPS script fixture")?;
        }
        self.fixture_command(
            "Fetch.fulfillRequest",
            json!({ "requestId": params["requestId"], "responseCode": 200,
            "responseHeaders": [{ "name": "Content-Type", "value": kind }, { "name": "Access-Control-Allow-Origin", "value": "*" },
                { "name": "Cache-Control", "value": "no-store" }],
            "body": base64::engine::general_purpose::STANDARD.encode(&raw[split + 4..]) }),
            &event["sessionId"],
        )
    }

    /// One protocol call. Load events are retained even when they arrive before the reply. The whole call shares one
    /// deadline, so a page that never stops sending events cannot keep it from ending.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let own = Instant::now() + CALL_LIMIT;
        let deadline = self.deadline.map_or(own, |outer| outer.min(own));
        let what = format!("the answer to {method}");
        self.next += 1;
        let id = self.next;
        let request = json!({ "id": id, "method": method, "params": params }).to_string();
        self.socket.get_mut().deadline = deadline;
        self.socket.send(Message::text(request)).map_err(|e| {
            if protocol_timed_out(&e, deadline) { format!("timed out waiting for {what}") } else { format!("DevTools send: {e}") }
        })?;
        loop {
            let reply = self.read(deadline, &what)?;
            if reply["id"] != json!(id) {
                continue;
            }
            return match reply.get("error") {
                Some(error) => Err(format!("{method}: {}", error["message"].as_str().unwrap_or("failed"))),
                None => Ok(reply["result"].clone()),
            };
        }
    }

    fn read(&mut self, deadline: Instant, what: &str) -> Result<Value, String> {
        self.socket.get_mut().deadline = deadline;
        loop {
            left(deadline, what)?;
            let message = self.socket.read().map_err(|e| {
                if protocol_timed_out(&e, deadline) {
                    format!("timed out waiting for {what}")
                } else {
                    format!("DevTools read ({what}): {e}")
                }
            })?;
            let Message::Text(text) = message else { continue };
            let reply: Value = serde_json::from_str(text.as_str()).map_err(|e| format!("DevTools sent bad JSON: {e}"))?;
            self.fixture_event(&reply, deadline)?;
            let params = &reply["params"];
            match reply["method"].as_str() {
                Some("Page.frameNavigated") if params["frame"]["parentId"].is_null() && !params["frame"]["id"].is_null() => {
                    self.frame = params["frame"].clone();
                }
                Some("Page.lifecycleEvent") if params["name"] == "load" && params["frameId"] == self.frame["id"] => {
                    self.loaded = params["loaderId"].as_str().map(str::to_string);
                }
                _ => {}
            }
            return Ok(reply);
        }
    }

    /// Subscribes before inspecting the document. Chromium replays lifecycle events for an already loaded page.
    pub fn wait_loaded(&mut self, deadline: Instant) -> Result<(), String> {
        let deadline = self.deadline.map_or(deadline, |o| o.min(deadline));
        let outer = self.set_deadline(Some(deadline));
        let result = (|| {
            self.call("Page.enable", json!({}))?;
            self.frame = self.call("Page.getFrameTree", json!({}))?["frameTree"]["frame"].clone();
            self.call("Page.setLifecycleEventsEnabled", json!({ "enabled": true }))?;
            self.loaded_after(deadline, None)
        })();
        self.set_deadline(outer);
        result
    }

    fn loaded_after(&mut self, deadline: Instant, previous: Option<&str>) -> Result<(), String> {
        loop {
            let loader = self.frame["loaderId"].as_str();
            if loader.is_some() && loader == self.loaded.as_deref() && loader != previous && app_url(&self.frame["url"]) {
                return Ok(());
            }
            self.read(deadline, "the app document to load")?;
        }
    }

    /// A reload must replace the main document; neither its old load event nor an iframe can satisfy this wait.
    pub fn reload(&mut self, deadline: Instant) -> Result<(), String> {
        let deadline = self.deadline.map_or(deadline, |o| o.min(deadline));
        let outer = self.set_deadline(Some(deadline));
        let result = (|| {
            let previous = self.frame["loaderId"].as_str().ok_or("the page has no loaded document")?.to_string();
            self.call("Page.reload", json!({}))?;
            self.loaded_after(deadline, Some(&previous))
        })();
        self.set_deadline(outer);
        result
    }

    /// Runs a script in the page and returns its value (a promise is awaited).
    pub fn eval(&mut self, expression: &str) -> Result<Value, String> {
        let params = json!({ "expression": expression, "returnByValue": true, "awaitPromise": true });
        let result = self.call("Runtime.evaluate", params)?;
        if let Some(thrown) = result.get("exceptionDetails") {
            let text = thrown["exception"]["description"].as_str().or(thrown["text"].as_str()).unwrap_or("the script threw");
            return Err(format!("page script failed: {text}"));
        }
        Ok(result["result"]["value"].clone())
    }

    /// Waits until a script returns something truthy. On the deadline it fails with `what`, the condition's own message.
    pub fn wait_for(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String> {
        let started = Instant::now();
        let deadline = Instant::now() + timeout;
        let outer = self.set_deadline(Some(self.deadline.map_or(deadline, |o| o.min(deadline))));
        let timed_out = || format!("timed out after {} s waiting for {what}", timeout.as_secs());
        let result = match self.settle(expression, Accept::Truthy, deadline) {
            Ok(Settled::Accepted(_)) => Ok(()),
            Ok(Settled::TimedOut(_)) => Err(timed_out()),
            // A call cut short by this deadline is the wait running out, said in the wait's own words.
            Err(_) if Instant::now() >= deadline => Err(timed_out()),
            Err(e) => Err(e),
        };
        self.deadline = outer;
        crate::timing::step("page-wait", started.elapsed());
        result
    }

    /// Reads `expression` in the page until `accept` holds for its value or `deadline` passes. The page itself reads it
    /// again at every change of the document, and at least every 50 ms, inside one protocol call of up to two seconds;
    /// asking from here every 100 ms cost a round trip each time and saw a change up to 100 ms late. A script the page
    /// cannot wrap (statements rather than one expression) is asked from here every 100 ms, as before. A script that throws
    /// fails at once, with what it threw.
    pub fn settle(&mut self, expression: &str, accept: Accept, deadline: Instant) -> Result<Settled, String> {
        // In parentheses a script that starts with `{` is an object, which is always true; on its own it is a block.
        let mut wrapped = !expression.trim_start().starts_with('{');
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if !wrapped {
                let value = self.eval(expression)?;
                if accept.holds(&value) {
                    return Ok(Settled::Accepted(value));
                }
                if Instant::now() + Duration::from_millis(100) >= deadline {
                    return Ok(Settled::TimedOut(value));
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            // The page answers a little before the deadline, so the answer is never cut off by it.
            let budget = left.saturating_sub(Duration::from_millis(150)).min(Duration::from_secs(2));
            let params = json!({ "expression": waiter(expression, accept, budget), "returnByValue": true, "awaitPromise": true });
            let result = self.call("Runtime.evaluate", params)?;
            if let Some(thrown) = result.get("exceptionDetails") {
                let text = thrown["exception"]["description"].as_str().or(thrown["text"].as_str()).unwrap_or("the script threw");
                if text.starts_with("SyntaxError") {
                    wrapped = false;
                    continue;
                }
                return Err(format!("page script failed: {text}"));
            }
            let answer = &result["result"]["value"];
            if let Some(error) = answer["error"].as_str() {
                return Err(format!("page script failed: {error}"));
            }
            let value = answer["value"].clone();
            if answer["done"] == true {
                return Ok(Settled::Accepted(value));
            }
            if Instant::now() + Duration::from_millis(150) >= deadline {
                return Ok(Settled::TimedOut(value));
            }
        }
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.call("Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y })).map(drop)
    }

    pub fn click_at(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.move_to(x, y)?;
        for (kind, buttons) in [("mousePressed", 1), ("mouseReleased", 0)] {
            let params = json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 });
            self.call("Input.dispatchMouseEvent", params)?;
        }
        Ok(())
    }

    /// A press of the right mouse button, which is what opens a context menu.
    pub fn right_click_at(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.move_to(x, y)?;
        for (kind, buttons) in [("mousePressed", 2), ("mouseReleased", 0)] {
            let params = json!({ "type": kind, "x": x, "y": y, "button": "right", "buttons": buttons, "clickCount": 1 });
            self.call("Input.dispatchMouseEvent", params)?;
        }
        Ok(())
    }

    /// Types text into the focused element, as the keyboard would.
    pub fn insert_text(&mut self, text: &str) -> Result<(), String> {
        self.call("Input.insertText", json!({ "text": text })).map(drop)
    }

    /// Presses a key or a chord such as `Enter`, `Escape` or `Control+,`.
    pub fn press(&mut self, chord: &str) -> Result<(), String> {
        let mut parts: Vec<&str> = chord.split('+').collect();
        // "Control++" is the plus key itself.
        if chord.ends_with("++") {
            parts.pop();
            parts.pop();
            parts.push("+");
        }
        let key = parts.pop().ok_or("empty key")?;
        let mut modifiers = 0;
        for modifier in parts {
            modifiers |= match modifier {
                "Alt" => 1,
                "Control" | "Ctrl" => 2,
                "Meta" => 4,
                "Shift" => 8,
                other => return Err(format!("unknown modifier {other} in {chord}")),
            };
        }
        let (name, code, virtual_key, text) = key_info(key)?;
        // Text is only typed when no command key is held.
        let typed = (modifiers & 7 == 0).then_some(text).flatten();
        for kind in ["keyDown", "keyUp"] {
            let mut params =
                json!({ "type": kind, "key": name, "code": code, "windowsVirtualKeyCode": virtual_key, "modifiers": modifiers });
            if let (Some(text), "keyDown") = (typed, kind) {
                params["text"] = json!(text);
            }
            self.call("Input.dispatchKeyEvent", params)?;
        }
        Ok(())
    }

    pub fn screenshot(&mut self, path: &Path) -> Result<(), String> {
        let shot = self.call("Page.captureScreenshot", json!({ "format": "png" }))?;
        let data = shot["data"].as_str().ok_or("the screenshot had no data")?;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        // Scenarios on different workers may name the same checkpoint: write beside it and rename, so a picture is never half
        // of one and never two writers on one file.
        static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{}-{}.tmp", std::process::id(), COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
        let temp = path.with_file_name(name);
        std::fs::write(&temp, bytes).map_err(|e| format!("{}: {e}", temp.display()))?;
        let mut moved = std::fs::rename(&temp, path);
        for _ in 0..5 {
            if moved.is_ok() {
                break;
            }
            // Windows refuses to replace a file another writer is replacing at that moment.
            std::thread::sleep(Duration::from_millis(20));
            moved = std::fs::rename(&temp, path);
        }
        moved.map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("{}: {e}", path.display())
        })
    }
}

/// JavaScript's idea of true, for the value a waiting script returns.
/// When a value read in the page is the one a wait waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// Anything JavaScript calls true (and [`truthy`] agrees with).
    Truthy,
    /// A value with no `why`: the locator found its one element.
    NoWhy,
}

impl Accept {
    fn holds(self, value: &Value) -> bool {
        match self {
            Accept::Truthy => truthy(value),
            Accept::NoWhy => value["why"].is_null(),
        }
    }

    fn script(self) -> &'static str {
        match self {
            Accept::Truthy => "(value) => !!value",
            Accept::NoWhy => "(value) => value == null || value.why == null",
        }
    }
}

/// How a wait ended: with the value it waited for, or with the last value it read when time ran out.
#[derive(Debug, Clone, PartialEq)]
pub enum Settled {
    Accepted(Value),
    TimedOut(Value),
}

/// The page's side of [`Cdp::settle`]: it reads the expression, and until `accept` holds and `budget` lasts, reads it again
/// after each change of the document (at most once per 16 ms) or after 50 ms without one. It answers `{ done, value }`, or
/// `{ error }` when the expression threw. The expression goes in as code, as `Runtime.evaluate` would run it; the
/// markers let the test double find it.
fn waiter(expression: &str, accept: Accept, budget: Duration) -> String {
    format!(
        "/*faber-wait*/(async () => {{
  const read = async () => ({expression}) /*end*/;
  const accept = {accept};
  const until = performance.now() + {budget};
  // A read that never settles ends the wait at its budget, so the observer below is always let go.
  const late = Symbol('late');
  const budgetEnds = new Promise((r) => setTimeout(() => r(late), Math.max(0, until - performance.now())));
  let changed = false;
  let wake = null;
  const observer = new MutationObserver(() => {{ changed = true; if (wake) wake(); }});
  observer.observe(document, {{ subtree: true, childList: true, attributes: true, characterData: true }});
  try {{
    for (;;) {{
      changed = false;
      let value;
      try {{ value = await Promise.race([read(), budgetEnds]); }} catch (e) {{ return {{ error: String((e && e.stack) || e) }}; }}
      if (value === late) return {{ done: false, value: null }};
      if (accept(value)) return {{ done: true, value }};
      if (until - performance.now() <= 0) return {{ done: false, value }};
      await new Promise((r) => setTimeout(r, Math.min(16, Math.max(0, until - performance.now()))));
      if (!changed) await new Promise((r) => {{ wake = r; setTimeout(r, Math.min(50, Math.max(0, until - performance.now()))); }});
      wake = null;
    }}
  }} finally {{
    observer.disconnect();
  }}
}})()",
        accept = accept.script(),
        budget = budget.as_millis()
    )
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0 && !n.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// (key, code, Windows virtual key, text typed) for the keys scenarios use.
fn key_info(key: &str) -> Result<(String, String, u32, Option<&'static str>), String> {
    let fixed = |name: &str, code: &str, vk: u32, text: Option<&'static str>| Ok((name.to_string(), code.to_string(), vk, text));
    match key {
        "Enter" => fixed("Enter", "Enter", 13, Some("\r")),
        "Escape" => fixed("Escape", "Escape", 27, None),
        "Tab" => fixed("Tab", "Tab", 9, None),
        "Space" => fixed(" ", "Space", 32, Some(" ")),
        "ArrowLeft" => fixed("ArrowLeft", "ArrowLeft", 37, None),
        "ArrowRight" => fixed("ArrowRight", "ArrowRight", 39, None),
        "." => fixed(".", "Period", 190, Some(".")),
        "," => fixed(",", "Comma", 188, Some(",")),
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphabetic() => {
            let upper = k.to_ascii_uppercase();
            Ok((k.to_string(), format!("Key{upper}"), u32::from(upper.as_bytes()[0]), None))
        }
        other => Err(format!("the test client does not know the key {other}")),
    }
}

/// The page's own address on the DevTools socket, from `GET /json/list`.
fn page_url(port: u16, deadline: Instant, wanted: fn(&str) -> bool) -> Result<String, String> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    // One request gets 3 s at most, and never more than what is left of the caller's deadline; all of it, the headers
    // and the body included, is inside that.
    let request_deadline = deadline.min(Instant::now() + Duration::from_secs(3));
    let stream = TcpStream::connect_timeout(&addr, left(request_deadline, "the DevTools port")?).map_err(|e| e.to_string())?;
    let mut stream = Limited { stream, deadline: request_deadline };
    write!(stream, "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    // WebView2's server keeps the connection open after it answers, whatever `Connection` says, so waiting for the end of
    // the stream would never finish: the body is read by the length the headers give.
    let mut reader = BufReader::new(stream);
    let mut length = None;
    let mut first = true;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err(if first { "no HTTP answer".to_string() } else { "the HTTP answer ended inside its headers".to_string() });
        }
        first = false;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<u64>().ok();
        }
    }
    let mut body = Vec::new();
    match length {
        Some(length) => reader.take(length).read_to_end(&mut body),
        None => reader.read_to_end(&mut body),
    }
    .map_err(|e| e.to_string())?;
    let targets: Vec<Value> = serde_json::from_slice(&body).map_err(|e| format!("the target list is not JSON: {e}"))?;
    targets
        .iter()
        .filter(|t| t["type"] == "page" && t["url"].as_str().is_some_and(wanted))
        .find_map(|t| t["webSocketDebuggerUrl"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "no page is listed yet".to_string())
}

fn app_url(url: &Value) -> bool {
    url.as_str().is_some_and(|url| url.starts_with("http://tauri.localhost/") || url.starts_with("tauri://localhost/"))
}

fn open(url: &str, deadline: Instant) -> Result<Cdp, String> {
    let host = url.trim_start_matches("ws://").split('/').next().unwrap_or_default();
    let addr: SocketAddr = host.parse().map_err(|e| format!("the DevTools address {host}: {e}"))?;
    let stream = TcpStream::connect_timeout(&addr, left(deadline, "the DevTools socket")?).map_err(|e| e.to_string())?;
    let (socket, _) = tungstenite::client(url, Limited { stream, deadline }).map_err(|e| format!("DevTools handshake: {e}"))?;
    Ok(Cdp { socket, next: 0, deadline: None, frame: Value::Null, loaded: None, https_fixture: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[test]
    fn space_is_a_key_the_client_knows() {
        let (name, code, virtual_key, text) = key_info("Space").unwrap();
        assert_eq!((name.as_str(), code.as_str(), virtual_key, text), (" ", "Space", 32, Some(" ")));
    }

    /// A stand-in for the app's DevTools port: the target list over HTTP and a scripted WebSocket.
    fn mock() -> (u16, Arc<Mutex<Vec<Value>>>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen: Arc<Mutex<Vec<Value>>> = Arc::default();
        let log = seen.clone();
        std::thread::spawn(move || {
            // WebView2 does not close the connection after it answers, so neither does this stand-in.
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = stream.peek(&mut head).unwrap();
                if head[..n].starts_with(b"GET /json/list") {
                    let mut stream = stream;
                    let mut sink = [0u8; 1024];
                    let _ = stream.read(&mut sink);
                    let targets = json!([
                        { "type": "other", "url": "x", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/NOPE") },
                        { "type": "page", "url": "about:blank", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/BLANK") },
                        { "type": "page", "url": "http://tauri.localhost/", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/page/P1") },
                    ])
                    .to_string();
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{targets}",
                        targets.len()
                    )
                    .unwrap();
                    held.push(stream);
                    continue;
                }
                let log = log.clone();
                std::thread::spawn(move || {
                    #[allow(clippy::result_large_err)] // Tungstenite fixes the callback's error type.
                    let mut socket = tungstenite::accept_hdr(stream, |request: &tungstenite::handshake::server::Request, response| {
                        assert_eq!(request.uri().path(), "/devtools/page/P1", "never attach to the provisional blank target");
                        Ok(response)
                    })
                    .unwrap();
                    while let Ok(Message::Text(text)) = socket.read() {
                        let request: Value = serde_json::from_str(text.as_str()).unwrap();
                        log.lock().unwrap().push(request.clone());
                        // An event before every answer: the client must skip it.
                        socket.send(Message::text(json!({ "method": "Page.frameNavigated", "params": {} }).to_string())).unwrap();
                        let reply = match request["method"].as_str().unwrap() {
                            "Page.getFrameTree" => {
                                json!({ "frameTree": { "frame": { "id": "main", "loaderId": "first", "url": "http://tauri.localhost/" } } })
                            }
                            "Page.setLifecycleEventsEnabled" => {
                                socket.send(Message::text(json!({ "method": "Page.lifecycleEvent", "params": { "frameId": "main", "loaderId": "first", "name": "load" } }).to_string())).unwrap();
                                json!({})
                            }
                            "Target.setAutoAttach" => {
                                socket
                                    .send(Message::text(
                                        json!({ "method": "Target.attachedToTarget", "params": {
                                    "sessionId": "visual-frame-session", "targetInfo": { "type": "iframe" }
                                } })
                                        .to_string(),
                                    ))
                                    .unwrap();
                                json!({})
                            }
                            "Runtime.evaluate" => match inner(request["params"]["expression"].as_str().unwrap()) {
                                Wrapped("late") => {
                                    let done = request["id"].as_u64().unwrap() > 6;
                                    json!({ "result": { "type": "object", "value": { "done": done, "value": done } } })
                                }
                                Wrapped("boom") => json!({ "result": { "type": "object", "value": { "error": "Error: boom" } } }),
                                Wrapped("a; b") => {
                                    json!({ "result": { "type": "object" }, "exceptionDetails": { "text": "Uncaught", "exception": { "description": "SyntaxError: Unexpected token ';'" } } })
                                }
                                // In parentheses it is an object, which a page calls true.
                                Wrapped("{ ready: false }") => {
                                    json!({ "result": { "type": "object", "value": { "done": true, "value": { "ready": false } } } })
                                }
                                Wrapped(_) => json!({ "result": { "type": "object", "value": { "done": false } } }),
                                Plain("a; b") => json!({ "result": { "type": "boolean", "value": request["id"].as_u64().unwrap() > 4 } }),
                                Plain("boom") => {
                                    json!({ "result": { "type": "object" }, "exceptionDetails": { "text": "Uncaught", "exception": { "description": "Error: boom" } } })
                                }
                                Plain("1 + 1") => json!({ "result": { "type": "number", "value": 2 } }),
                                Plain(
                                    fixture @ ("fixture"
                                    | "fixture-redirect"
                                    | "fixture-missing"
                                    | "fixture-image"
                                    | "fixture-stylesheet"
                                    | "fixture-missing-image"
                                    | "fixture-missing-stylesheet"
                                    | "fixture-no-cors"),
                                ) => {
                                    let path = match fixture {
                                        "fixture" => "/releases/latest",
                                        "fixture-redirect" => "/",
                                        "fixture-image" => "/fast.svg",
                                        "fixture-stylesheet" => "/fast.css",
                                        "fixture-missing-image" => "/missing.svg",
                                        "fixture-missing-stylesheet" => "/missing.css",
                                        "fixture-no-cors" => "/classic-no-cors.js",
                                        _ => "/missing.js",
                                    };
                                    socket
                                        .send(Message::text(
                                            json!({ "method": "Fetch.requestPaused", "sessionId": "visual-frame-session", "params": {
                                        "requestId": "fixture-request", "request": { "url": format!("{HTTPS_FIXTURE_ORIGIN}{path}") }
                                    } })
                                            .to_string(),
                                        ))
                                        .unwrap();
                                    json!({ "result": { "type": "boolean", "value": true } })
                                }
                                Plain(_) => json!({ "result": { "type": "undefined" } }),
                            },
                            "Page.captureScreenshot" => json!({ "data": "iVBORw0KGgo=" }),
                            _ => json!({}),
                        };
                        let answer = if request["method"] == "Nope.fail" {
                            json!({ "id": request["id"], "error": { "message": "no such method" } })
                        } else {
                            json!({ "id": request["id"], "result": reply })
                        };
                        socket.send(Message::text(answer.to_string())).unwrap();
                        if request["method"] == "Page.reload" {
                            for event in [
                                json!({ "method": "Page.lifecycleEvent", "params": { "frameId": "main", "loaderId": "first", "name": "load" } }),
                                json!({ "method": "Page.frameNavigated", "params": { "frame": { "id": "main", "loaderId": "second", "url": "http://tauri.localhost/" } } }),
                                json!({ "method": "Page.frameNavigated", "params": { "frame": { "id": "child", "parentId": "main", "loaderId": "second", "url": "http://tauri.localhost/" } } }),
                                json!({ "method": "Page.lifecycleEvent", "params": { "frameId": "child", "loaderId": "second", "name": "load" } }),
                            ] {
                                socket.send(Message::text(event.to_string())).unwrap();
                            }
                            socket.get_mut().set_read_timeout(Some(Duration::from_millis(100))).unwrap();
                            let error = socket.get_mut().peek(&mut [0u8; 1]).expect_err("no evaluation before the new main document loads");
                            assert!(matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut));
                            socket.get_mut().set_read_timeout(None).unwrap();
                            socket.send(Message::text(json!({ "method": "Page.lifecycleEvent", "params": { "frameId": "main", "loaderId": "second", "name": "load" } }).to_string())).unwrap();
                        }
                    }
                });
            }
        });
        (port, seen)
    }

    #[test]
    fn it_finds_the_page_target_and_matches_each_answer_to_its_call_skipping_events() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        assert_eq!(cdp.eval("1 + 1").unwrap(), json!(2));
        assert_eq!(cdp.eval("nothing").unwrap(), Value::Null);
        assert!(cdp.eval("boom").unwrap_err().contains("Error: boom"));
        assert!(cdp.call("Nope.fail", json!({})).unwrap_err().contains("no such method"));
        let methods: Vec<String> = seen.lock().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect();
        assert_eq!(methods[0], "Emulation.setFocusEmulationEnabled", "an offscreen window must be told it has focus");
        assert!(seen.lock().unwrap()[1]["params"]["awaitPromise"] == json!(true));
    }

    #[test]
    fn https_fixture_replies_in_the_child_session_without_consuming_the_outer_call() {
        let server = crate::server::Server::start(b"unused".to_vec()).unwrap();
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        assert!(cdp.serve_https_fixture("http://8.8.8.8:80").is_err());
        assert!(cdp.serve_https_fixture("https://127.0.0.1:80").is_err());
        cdp.serve_https_fixture(&server.origin).unwrap();
        assert_eq!(cdp.eval("fixture").unwrap(), json!(true));
        // Drain the separate fulfillment acknowledgment before inspecting what the stand-in received.
        assert_eq!(cdp.eval("1 + 1").unwrap(), json!(2));
        let seen = seen.lock().unwrap();
        let response = seen.iter().find(|request| request["method"] == "Fetch.fulfillRequest").unwrap();
        assert_eq!(response["sessionId"], "visual-frame-session");
        assert_eq!(response["params"]["requestId"], "fixture-request");
        let body = base64::engine::general_purpose::STANDARD.decode(response["params"]["body"].as_str().unwrap()).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["tag_name"], "setup-replay");
        assert_eq!(server.requests(), ["/releases/latest"]);
        assert!(
            seen.iter()
                .any(|request| request["method"] == "Runtime.runIfWaitingForDebugger" && request["sessionId"] == "visual-frame-session")
        );
    }

    #[test]
    fn https_fixture_redirect_leaves_the_private_destination_to_the_browser_and_native_filter() {
        let server = crate::server::Server::start(b"unused".to_vec()).unwrap();
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.serve_https_fixture(&server.origin).unwrap();
        assert_eq!(cdp.eval("fixture-redirect").unwrap(), json!(true));
        cdp.eval("1 + 1").unwrap();
        let seen = seen.lock().unwrap();
        let response = seen.iter().find(|request| request["method"] == "Fetch.fulfillRequest").unwrap();
        assert_eq!(response["params"]["responseCode"], 302);
        let location =
            response["params"]["responseHeaders"].as_array().unwrap().iter().find(|header| header["name"] == "Location").unwrap();
        assert_eq!(location["value"], format!("{}/releases/latest", server.origin.replacen("http:", "https:", 1)));
        assert_eq!(server.requests(), ["/"], "the runner never follows the redirect itself");
    }

    #[test]
    fn https_fixture_can_fail_a_child_script_without_contacting_any_server() {
        let server = crate::server::Server::start(b"unused".to_vec()).unwrap();
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.serve_https_fixture(&server.origin).unwrap();
        assert_eq!(cdp.eval("fixture-missing").unwrap(), json!(true));
        cdp.eval("1 + 1").unwrap();
        let seen = seen.lock().unwrap();
        let response = seen.iter().find(|request| request["method"] == "Fetch.fulfillRequest").unwrap();
        assert_eq!(response["sessionId"], "visual-frame-session");
        assert_eq!(response["params"]["responseCode"], 404);
        assert_eq!(response["params"]["body"], "");
        assert!(server.requests().is_empty());
    }

    #[test]
    fn https_fixture_answers_fast_image_and_stylesheet_loads_and_errors_without_a_server_request() {
        let server = crate::server::Server::start(b"unused".to_vec()).unwrap();
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.serve_https_fixture(&server.origin).unwrap();
        for (expression, status, kind, content) in [
            ("fixture-image", 200, "image/svg+xml", "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"/>"),
            ("fixture-stylesheet", 200, "text/css", "body { color: rgb(1, 2, 3); }"),
            ("fixture-missing-image", 404, "text/plain", ""),
            ("fixture-missing-stylesheet", 404, "text/plain", ""),
        ] {
            assert_eq!(cdp.eval(expression).unwrap(), json!(true));
            cdp.eval("1 + 1").unwrap();
            let seen = seen.lock().unwrap();
            let response = seen.iter().rfind(|request| request["method"] == "Fetch.fulfillRequest").unwrap();
            assert_eq!(response["sessionId"], "visual-frame-session");
            assert_eq!(response["params"]["responseCode"], status);
            let headers = response["params"]["responseHeaders"].as_array().unwrap();
            assert_eq!(headers.iter().find(|header| header["name"] == "Content-Type").unwrap()["value"], kind);
            let body = base64::engine::general_purpose::STANDARD.decode(response["params"]["body"].as_str().unwrap()).unwrap();
            assert_eq!(body, content.as_bytes());
        }
        assert!(server.requests().is_empty());
    }

    #[test]
    fn https_fixture_classic_script_can_omit_cors_authorization() {
        let server = crate::server::Server::start(b"unused".to_vec()).unwrap();
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.serve_https_fixture(&server.origin).unwrap();
        assert_eq!(cdp.eval("fixture-no-cors").unwrap(), json!(true));
        cdp.eval("1 + 1").unwrap();
        let seen = seen.lock().unwrap();
        let response = seen.iter().find(|request| request["method"] == "Fetch.fulfillRequest").unwrap();
        assert_eq!(response["params"]["responseCode"], 200);
        let headers = response["params"]["responseHeaders"].as_array().unwrap();
        assert!(!headers.iter().any(|header| header["name"] == "Access-Control-Allow-Origin"));
        let bytes = base64::engine::general_purpose::STANDARD.decode(response["params"]["body"].as_str().unwrap()).unwrap();
        assert_eq!(bytes, b"window.visualNoCorsClassic=true;");
        assert!(server.requests().is_empty());
    }

    #[test]
    fn loading_retains_events_before_the_reply_and_reload_requires_the_new_main_document() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.wait_loaded(Instant::now() + Duration::from_secs(2)).unwrap();
        cdp.reload(Instant::now() + Duration::from_secs(2)).unwrap();
        assert_eq!(cdp.eval("1 + 1").unwrap(), json!(2));
        assert_eq!(cdp.frame["loaderId"], "second");
        let methods: Vec<String> = seen.lock().unwrap().iter().map(|r| r["method"].as_str().unwrap().to_string()).collect();
        assert_eq!(
            methods,
            [
                "Emulation.setFocusEmulationEnabled",
                "Page.enable",
                "Page.getFrameTree",
                "Page.setLifecycleEventsEnabled",
                "Page.reload",
                "Runtime.evaluate"
            ]
        );
    }

    #[test]
    fn native_transport_timeout_keeps_deadline_semantics_before_the_clock_tick() {
        let future = Instant::now() + CALL_LIMIT;
        for kind in [io::ErrorKind::TimedOut, io::ErrorKind::WouldBlock] {
            assert!(protocol_timed_out(&tungstenite::Error::Io(io::Error::new(kind, "native socket timeout")), future));
        }
        let reset = tungstenite::Error::Io(io::Error::new(io::ErrorKind::ConnectionReset, "peer reset"));
        assert!(!protocol_timed_out(&reset, future));
        assert!(protocol_timed_out(&reset, Instant::now()));
    }

    // A page that never stops sending events and never answers must not keep a call from ending on time.
    #[test]
    fn a_call_ends_at_its_deadline_even_while_events_keep_arriving() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = stream.peek(&mut head).unwrap();
                if head[..n].starts_with(b"GET /json/list") {
                    let mut stream = stream;
                    let mut sink = [0u8; 1024];
                    let _ = stream.read(&mut sink);
                    let body =
                        json!([{ "type": "page", "url": "http://tauri.localhost/", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/p") }]).to_string();
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                    held.push(stream);
                    continue;
                }
                std::thread::spawn(move || {
                    let mut socket = tungstenite::accept(stream).unwrap();
                    // The focus call at connect gets its answer; every call after it gets only events, every 5 ms.
                    let Ok(Message::Text(_)) = socket.read() else { return };
                    socket.send(Message::text(json!({ "id": 1, "result": {} }).to_string())).unwrap();
                    let Ok(Message::Text(_)) = socket.read() else { return };
                    while socket.send(Message::text(json!({ "method": "Noise", "params": {} }).to_string())).is_ok() {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                });
            }
        });
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.set_deadline(Some(Instant::now() + Duration::from_millis(400)));
        let started = Instant::now();
        let error = cdp.call("Runtime.evaluate", json!({})).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(3), "the call took {:?}: {error}", started.elapsed());
        assert!(error.contains("timed out"), "{error}");
    }

    // A frame that arrives a byte at a time, each byte well inside the old per-read timeout, still ends at the deadline.
    #[test]
    fn a_frame_that_drips_in_slowly_ends_at_the_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming().flatten() {
                let mut head = [0u8; 16];
                let n = stream.peek(&mut head).unwrap();
                if head[..n].starts_with(b"GET /json/list") {
                    let mut stream = stream;
                    let mut sink = [0u8; 1024];
                    let _ = stream.read(&mut sink);
                    let body =
                        json!([{ "type": "page", "url": "http://tauri.localhost/", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/p") }]).to_string();
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                    held.push(stream);
                    continue;
                }
                std::thread::spawn(move || {
                    let mut socket = tungstenite::accept(stream).unwrap();
                    let Ok(Message::Text(_)) = socket.read() else { return };
                    socket.send(Message::text(json!({ "id": 1, "result": {} }).to_string())).unwrap();
                    let Ok(Message::Text(_)) = socket.read() else { return };
                    // A text frame of 100 bytes, sent one byte per 50 ms: 5 s in all, and no gap longer than 50 ms.
                    let frame: Vec<u8> = [vec![0x81, 100], vec![b' '; 100]].concat();
                    for byte in frame {
                        if socket.get_mut().write_all(&[byte]).is_err() {
                            return;
                        }
                        let _ = socket.get_mut().flush();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                });
            }
        });
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.set_deadline(Some(Instant::now() + Duration::from_millis(600)));
        let started = Instant::now();
        let error = cdp.call("Runtime.evaluate", json!({})).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2), "the call took {:?}: {error}", started.elapsed());
        assert!(error.contains("timed out"), "{error}");
    }

    #[test]
    fn it_gives_up_with_a_message_when_the_port_never_lists_a_page() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let error = Cdp::connect(port, Duration::from_millis(300)).err().expect("nothing listens there");
        assert!(error.contains("did not offer a page"), "{error}");
    }

    /// What the test double was asked to evaluate: a wait's expression, or a plain one.
    enum Asked<'a> {
        Wrapped(&'a str),
        Plain(&'a str),
    }
    use Asked::{Plain, Wrapped};

    fn inner(script: &str) -> Asked<'_> {
        match script.strip_prefix("/*faber-wait*/") {
            Some(rest) => {
                let start = rest.find("const read = async () => (").unwrap() + "const read = async () => (".len();
                Wrapped(&rest[start..rest.find(") /*end*/").unwrap()])
            }
            None => Plain(script),
        }
    }

    #[test]
    fn wait_for_waits_until_the_condition_holds_and_fails_with_its_own_message() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.wait_for("the late condition", "late", Duration::from_secs(5)).unwrap();
        let error = cdp.wait_for("the heading to appear", "nothing", Duration::from_millis(250)).unwrap_err();
        assert!(error.contains("waiting for the heading to appear"), "{error}");
        let error = cdp.wait_for("a broken condition", "boom", Duration::from_secs(5)).unwrap_err();
        assert!(error.contains("page script failed: Error: boom"), "a script that throws fails at once: {error}");
        // Statements cannot be wrapped as one expression: they are asked from here, as before.
        cdp.wait_for("statements", "a; b", Duration::from_secs(5)).unwrap();
        // A block is not wrapped into an object, which would pass at once.
        let error = cdp.wait_for("a block", "{ ready: false }", Duration::from_millis(400)).unwrap_err();
        assert!(error.contains("waiting for a block"), "{error}");
        let asked: Vec<String> =
            seen.lock().unwrap().iter().filter_map(|r| r["params"]["expression"].as_str().map(str::to_string)).collect();
        assert!(asked.iter().any(|e| e == "a; b") && asked.iter().any(|e| e.starts_with("/*faber-wait*/")), "{asked:?}");
    }

    #[test]
    fn the_page_side_of_a_wait_reads_the_expression_as_code_with_its_budget() {
        let script = waiter("document.title === 'x'", Accept::NoWhy, Duration::from_millis(1850));
        assert!(script.contains("const read = async () => (document.title === 'x') /*end*/;"));
        assert!(script.contains("const accept = (value) => value == null || value.why == null;"));
        assert!(script.contains("performance.now() + 1850;"));
        assert!(script.contains("observer.disconnect()"), "the observer never outlives the wait");
        assert!(script.contains("await Promise.race([read(), budgetEnds])"), "a read that never settles ends at the budget");
        assert!(Accept::NoWhy.holds(&json!({ "x": 1 })) && !Accept::NoWhy.holds(&json!({ "why": "two match" })));
        assert!(Accept::Truthy.holds(&json!([])) && !Accept::Truthy.holds(&json!("")) && !Accept::Truthy.holds(&Value::Null));
    }

    #[test]
    fn click_type_and_key_calls_become_the_protocol_events_a_real_user_would_cause() {
        let (port, seen) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.click_at(10.0, 20.0).unwrap();
        cdp.insert_text("hello").unwrap();
        cdp.press("Enter").unwrap();
        cdp.press("Control+,").unwrap();
        let log = seen.lock().unwrap();
        let events: Vec<(&str, &Value)> = log.iter().skip(1).map(|r| (r["method"].as_str().unwrap(), &r["params"])).collect();
        let kinds: Vec<&str> = events
            .iter()
            .map(|(m, p)| if *m == "Input.dispatchMouseEvent" || *m == "Input.dispatchKeyEvent" { p["type"].as_str().unwrap() } else { *m })
            .collect();
        assert_eq!(kinds, ["mouseMoved", "mousePressed", "mouseReleased", "Input.insertText", "keyDown", "keyUp", "keyDown", "keyUp"]);
        assert_eq!(events[1].1["x"], json!(10.0));
        assert_eq!(events[3].1["text"], json!("hello"));
        assert_eq!(events[4].1["text"], json!("\r"), "Enter types a line break, which is what submits a form");
        assert_eq!(
            (events[6].1["key"].clone(), events[6].1["code"].clone(), events[6].1["windowsVirtualKeyCode"].clone()),
            (json!(","), json!("Comma"), json!(188))
        );
        assert_eq!(events[6].1["modifiers"], json!(2));
        assert!(events[6].1.get("text").is_none(), "a chord with Control types nothing");
        drop(log);
        assert!(cdp.press("Hyper+x").is_err());
        assert!(cdp.press("F13").is_err());
    }

    #[test]
    fn a_screenshot_is_decoded_and_saved_where_asked() {
        let (port, _) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shots/home.png");
        cdp.screenshot(&path).unwrap();
        assert_eq!(&std::fs::read(path).unwrap()[1..4], b"PNG");
    }
}
