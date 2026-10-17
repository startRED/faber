//! Faber's own browser pane (R161 step 7): one child webview of the one window, on the same WebView2 environment, so it
//! shares the browser and GPU processes (the spike's rules, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md).
//! The agent opens a page in it through the engine's browser tool; the person sees it when they open it from the reply,
//! placed over the running column. Away from the screen it is moved out of the window, never hidden, so its pictures
//! stay fresh, and it closes once nobody has used it for a while.
//!
//! Only http and https pages load in it, never Faber's own `*.localhost` origins; it opens no new windows, downloads
//! nothing and keeps no cookies or logins once it closes (a private profile), so one project's agent never reads a
//! session another conversation or the person left there. A page in it cannot call Faber: the command handler answers
//! only the main screen (`main.rs`), and the screen's capability names only that webview. One page opens at a time, and
//! never while the person is looking at the pane: the agent then hears that the person has it.

mod native;

use base64::Engine as _;
use faber_engine::browser::{Browser, MAX_TEXT, MAX_TITLE, MAX_URL, Page};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewBuilder, WebviewUrl};

const LABEL: &str = "site";
/// Where the pane waits while nobody looks: outside the window, where it keeps drawing (the spike's rule 2).
const PARKED: LogicalPosition<f64> = LogicalPosition { x: -20_000.0, y: -20_000.0 };
/// The size the agent's pages load at while parked, a common laptop window.
const SIZE: LogicalSize<f64> = LogicalSize { width: 1280.0, height: 800.0 };
/// How long a page has to finish loading before its text is read anyway.
const LOAD: Duration = Duration::from_secs(30);
/// The longest one DevTools call may take; a page that holds it longer is closed with the pane.
const CALL: Duration = Duration::from_secs(10);
/// Unused this long and not on screen, the pane closes and gives its memory back (the spike's rule 4).
const IDLE: Duration = Duration::from_secs(5 * 60);
/// The picture in the reply is this share of the page's size.
const PICTURE_SCALE: f64 = 0.375;

/// Whether the pane may load `url`: a web page, never one of Faber's own origins (nor, in a development build, the screen's
/// own dev server).
fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let dev_screen = cfg!(debug_assertions) && host == "localhost" && url.port() == Some(5173);
    matches!(url.scheme(), "http" | "https") && !host.is_empty() && !host.ends_with(".localhost") && !dev_screen
}

pub struct Pane {
    app: AppHandle,
    state: Mutex<State>,
    /// The pages that started and the last of them that finished, so an open waits for its own page and never for a late
    /// end of the one before.
    loads: tokio::sync::watch::Sender<(u64, u64)>,
    /// Held from a navigation to the last read of its page, so two calls never read each other's page.
    opening: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct State {
    webview: Option<Webview>,
    shown: bool,
    used: Option<Instant>,
    /// An idle close is already waiting; one at a time, however often the pane is used.
    idle_watch: bool,
}

impl Pane {
    pub fn new(app: AppHandle) -> Arc<Pane> {
        Arc::new(Pane { app, state: Mutex::default(), loads: tokio::sync::watch::channel((0, 0)).0, opening: tokio::sync::Mutex::new(()) })
    }

    fn webview(&self) -> Option<Webview> {
        crate::lock(&self.state).webview.clone()
    }

    /// Loads `url` in the pane, making it first if it is closed. Answers the count of started pages to wait past.
    fn load(self: &Arc<Self>, url: &str) -> Result<u64, String> {
        let parsed: Url = url.parse().map_err(|_| "the address is not valid".to_string())?;
        if !allowed(&parsed) {
            return Err("only web pages open in Faber's browser".into());
        }
        let before = self.loads.borrow().0;
        self.touch();
        if let Some(webview) = self.webview() {
            if webview.navigate(parsed.clone()).is_ok() {
                return Ok(before);
            }
            // A pane left from a window that closed and came back: it is made again.
            self.close();
        }
        let window = self.app.get_window("main").ok_or("Faber's window is closed")?;
        let (loads, app) = (self.loads.clone(), self.app.clone());
        let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(parsed))
            .incognito(true)
            .on_navigation(allowed)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_page_load(move |_, payload| match payload.event() {
                PageLoadEvent::Started => loads.send_modify(|(started, _)| *started += 1),
                PageLoadEvent::Finished => {
                    loads.send_modify(|(started, finished)| *finished = *started);
                    // The screen names the page it shows by where it is now, never by the card it was opened from.
                    let _ = app.emit_to("main", "faber:site", payload.url().as_str());
                }
            });
        let webview = window.add_child(builder, PARKED, SIZE).map_err(|e| e.to_string())?;
        crate::lock(&self.state).webview = Some(webview);
        Ok(before)
    }

    fn touch(self: &Arc<Self>) {
        let mut s = crate::lock(&self.state);
        s.used = Some(Instant::now());
        if std::mem::replace(&mut s.idle_watch, true) {
            return;
        }
        let pane = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                // Decided and taken under one lock, so a page that opens meanwhile is never closed under it.
                let wait = {
                    let mut s = crate::lock(&pane.state);
                    let left = s.used.map_or(Duration::ZERO, |u| IDLE.saturating_sub(u.elapsed()));
                    if left.is_zero() && !s.shown {
                        s.idle_watch = false;
                        Err(s.webview.take())
                    } else {
                        Ok(left.max(Duration::from_secs(1)))
                    }
                };
                match wait {
                    Ok(wait) => tokio::time::sleep(wait).await,
                    Err(webview) => {
                        if let Some(webview) = webview {
                            let _ = webview.close();
                        }
                        break;
                    }
                }
            }
        });
    }

    pub fn close(&self) {
        let webview = {
            let mut s = crate::lock(&self.state);
            s.shown = false;
            s.webview.take()
        };
        if let Some(webview) = webview {
            let _ = webview.close();
        }
    }

    /// Shows the pane over the screen's own rectangle for it, in the page's logical pixels.
    pub fn place(self: &Arc<Self>, x: f64, y: f64, width: f64, height: f64) -> bool {
        let Some(webview) = self.webview() else { return false };
        if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) || width < 1.0 || height < 1.0 {
            return false;
        }
        crate::lock(&self.state).shown = true;
        self.touch();
        webview.set_size(LogicalSize { width, height }).is_ok() && webview.set_position(LogicalPosition { x, y }).is_ok()
    }

    /// Takes the pane off the screen, back at the size the agent's pages load at.
    pub fn park(self: &Arc<Self>) {
        let Some(webview) = self.webview() else { return };
        crate::lock(&self.state).shown = false;
        self.touch();
        let _ = webview.set_position(PARKED);
        let _ = webview.set_size(SIZE);
    }

    /// Loads a page the person asked for from a reply, once no call of the agent's is reading one.
    pub async fn show_page(self: &Arc<Self>, url: &str) -> bool {
        let _turn = self.opening.lock().await;
        self.load(url).is_ok()
    }

    /// One DevTools call, with a deadline: a page that holds it longer closes with the pane, which opens fresh next time.
    async fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let webview = self.webview().ok_or("the browser closed")?;
        let answer = tokio::time::timeout(CALL, native::devtools(&webview, method, params.to_string())).await;
        let Ok(answer) = answer else {
            self.close();
            return Err("the page stopped answering".into());
        };
        let json = answer.map_err(|_| "the browser closed".to_string())??;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    async fn open_page(self: Arc<Self>, url: String) -> Result<Page, String> {
        let _turn = self.opening.lock().await;
        if crate::lock(&self.state).shown {
            return Err("the person is looking at Faber's browser now; try again once they go back".into());
        }
        let before = self.load(&url)?;
        let mut loads = self.loads.subscribe();
        // A page that never says it finished is read as it is once the time is up.
        let _ = tokio::time::timeout(LOAD, loads.wait_for(|(_, finished)| *finished > before)).await;
        // Pages that draw themselves after the load get a moment.
        tokio::time::sleep(Duration::from_millis(600)).await;
        // Cut inside the page, so a huge page never crosses into Faber whole; one more character tells the engine it was cut.
        let expression = format!(
            "(() => {{ const t = document.body ? document.body.innerText : ''; return {{ url: String(location.href).slice(0, {MAX_URL}), \
             title: String(document.title).slice(0, {MAX_TITLE}), text: String(t).slice(0, {}) }}; }})()",
            MAX_TEXT + 1
        );
        let read = self.call("Runtime.evaluate", json!({ "expression": expression, "returnByValue": true })).await?;
        let value = &read["result"]["value"];
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let picture = self
            .call(
                "Page.captureScreenshot",
                json!({
                    "format": "jpeg",
                    "quality": 60,
                    "clip": { "x": 0, "y": 0, "width": SIZE.width, "height": SIZE.height, "scale": PICTURE_SCALE }
                }),
            )
            .await
            .ok()
            .and_then(|shot| shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()));
        self.touch();
        Ok(Page { url: text("url"), title: text("title"), text: text("text"), picture })
    }
}

/// The engine's view of the pane.
pub struct PaneBrowser(pub Arc<Pane>);

/// A task that ends when the engine stops waiting for it, so a page the engine gave up on stops being read.
struct Abort(tauri::async_runtime::JoinHandle<Result<Page, String>>);

impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Browser for PaneBrowser {
    fn open(&self, url: String) -> Pin<Box<dyn Future<Output = Result<Page, String>> + Send>> {
        let pane = self.0.clone();
        // Making the webview waits for the window's thread, so it runs off the engine's.
        Box::pin(async move {
            let mut task = Abort(tauri::async_runtime::spawn(pane.open_page(url)));
            (&mut task.0).await.map_err(|e| e.to_string())?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_pages_load_and_never_faber_s_own_origins() {
        let ok = |u: &str| allowed(&u.parse().unwrap());
        assert!(ok("https://docs.rs/tokio") && ok("http://localhost:3000/") && ok("http://127.0.0.1:8080/"));
        for bad in [
            "http://tauri.localhost/",
            "https://ipc.localhost/",
            "http://x.localhost./",
            "file:///C:/",
            "tauri://localhost/",
            "data:text/html,x",
        ] {
            assert!(!ok(bad), "{bad} must not load");
        }
        assert_eq!(ok("http://localhost:5173/"), !cfg!(debug_assertions), "the dev screen is Faber's own in a dev build");
    }
}
