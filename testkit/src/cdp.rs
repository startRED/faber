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

pub struct Cdp {
    socket: WebSocket<Limited>,
    next: u64,
    /// An outer deadline (a wait that polls) that every call under it must meet as well.
    deadline: Option<Instant>,
}

/// The time left until `deadline`. Zero is an error too, because a socket timeout of zero means "no timeout".
fn left(deadline: Instant, what: &str) -> Result<Duration, String> {
    deadline.checked_duration_since(Instant::now()).filter(|d| !d.is_zero()).ok_or_else(|| format!("timed out waiting for {what}"))
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
        let deadline = Instant::now() + timeout;
        loop {
            let error = match page_url(port, deadline).and_then(|url| open(&url, deadline)) {
                Ok(mut cdp) => {
                    // An offscreen window is never focused; without this it ignores keys until someone clicks it.
                    let _ = cdp.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true }));
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

    /// One protocol call. Events that arrive meanwhile are ignored. The whole call, the send and every read, shares one
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
            if Instant::now() >= deadline { format!("timed out waiting for {what}") } else { format!("DevTools send: {e}") }
        })?;
        loop {
            let message = self.socket.read().map_err(|e| {
                // A read the socket cut short at the deadline is the deadline passing, not a broken socket.
                if Instant::now() >= deadline { format!("timed out waiting for {what}") } else { format!("DevTools read ({method}): {e}") }
            })?;
            let Message::Text(text) = message else { continue };
            let reply: Value = serde_json::from_str(text.as_str()).map_err(|e| format!("DevTools sent bad JSON: {e}"))?;
            if reply["id"] != json!(id) {
                continue;
            }
            return match reply.get("error") {
                Some(error) => Err(format!("{method}: {}", error["message"].as_str().unwrap_or("failed"))),
                None => Ok(reply["result"].clone()),
            };
        }
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

    /// Polls a script every 100 ms until it returns something truthy. On the deadline it fails with `what`, the
    /// condition's own message.
    pub fn wait_for(&mut self, what: &str, expression: &str, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        let outer = self.set_deadline(Some(self.deadline.map_or(deadline, |o| o.min(deadline))));
        let timed_out = || format!("timed out after {} s waiting for {what}", timeout.as_secs());
        let result = loop {
            match self.eval(expression) {
                Ok(value) if truthy(&value) => break Ok(()),
                Ok(_) => {}
                // A call cut short by this deadline is the wait running out, said in the wait's own words.
                Err(_) if Instant::now() >= deadline => break Err(timed_out()),
                Err(e) => break Err(e),
            }
            if Instant::now() >= deadline {
                break Err(timed_out());
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        self.deadline = outer;
        result
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
        std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// JavaScript's idea of true, for the value a waiting script returns.
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
fn page_url(port: u16, deadline: Instant) -> Result<String, String> {
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
        .filter(|t| t["type"] == "page" && !t["url"].as_str().unwrap_or_default().starts_with("devtools://"))
        .find_map(|t| t["webSocketDebuggerUrl"].as_str())
        .map(str::to_string)
        .ok_or_else(|| "no page is listed yet".to_string())
}

fn open(url: &str, deadline: Instant) -> Result<Cdp, String> {
    let host = url.trim_start_matches("ws://").split('/').next().unwrap_or_default();
    let addr: SocketAddr = host.parse().map_err(|e| format!("the DevTools address {host}: {e}"))?;
    let stream = TcpStream::connect_timeout(&addr, left(deadline, "the DevTools socket")?).map_err(|e| e.to_string())?;
    let (socket, _) = tungstenite::client(url, Limited { stream, deadline }).map_err(|e| format!("DevTools handshake: {e}"))?;
    Ok(Cdp { socket, next: 0, deadline: None })
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
                    let mut socket = tungstenite::accept(stream).unwrap();
                    while let Ok(Message::Text(text)) = socket.read() {
                        let request: Value = serde_json::from_str(text.as_str()).unwrap();
                        log.lock().unwrap().push(request.clone());
                        // An event before every answer: the client must skip it.
                        socket.send(Message::text(json!({ "method": "Page.frameNavigated", "params": {} }).to_string())).unwrap();
                        let reply = match request["method"].as_str().unwrap() {
                            "Runtime.evaluate" => match request["params"]["expression"].as_str().unwrap() {
                                "boom" => {
                                    json!({ "result": { "type": "object" }, "exceptionDetails": { "text": "Uncaught", "exception": { "description": "Error: boom" } } })
                                }
                                "1 + 1" => json!({ "result": { "type": "number", "value": 2 } }),
                                "late" => json!({ "result": { "type": "boolean", "value": request["id"].as_u64().unwrap() > 6 } }),
                                _ => json!({ "result": { "type": "undefined" } }),
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
                        json!([{ "type": "page", "url": "x", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/p") }]).to_string();
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
                        json!([{ "type": "page", "url": "x", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/p") }]).to_string();
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

    #[test]
    fn wait_for_polls_until_the_condition_holds_and_fails_with_its_own_message() {
        let (port, _) = mock();
        let mut cdp = Cdp::connect(port, Duration::from_secs(5)).unwrap();
        cdp.wait_for("the late condition", "late", Duration::from_secs(5)).unwrap();
        let error = cdp.wait_for("the heading to appear", "nothing", Duration::from_millis(250)).unwrap_err();
        assert!(error.contains("waiting for the heading to appear"), "{error}");
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
