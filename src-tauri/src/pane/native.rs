//! The one WebView2 call the pane needs: a DevTools Protocol method sent through WebView2's own
//! `CallDevToolsProtocolMethod`, so the pane is driven without a DevTools port that any program on the PC could reach
//! (the spike's rule 5, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md).
#![allow(unsafe_code)]

use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
use windows_core::HSTRING;

type Slot = Arc<Mutex<Option<oneshot::Sender<Result<String, String>>>>>;

fn answer(slot: &Slot, result: Result<String, String>) {
    if let Some(tx) = crate::lock(slot).take() {
        let _ = tx.send(result);
    }
}

/// Sends `method` with its JSON `params` to the pane's page and answers with the result's JSON, or why it failed.
pub fn devtools(webview: &tauri::Webview, method: &str, params: String) -> oneshot::Receiver<Result<String, String>> {
    let (tx, rx) = oneshot::channel();
    // The answer comes once: from the completion handler, or from a call that failed before it was made.
    let tx: Slot = Arc::new(Mutex::new(Some(tx)));
    let (method, failed) = (HSTRING::from(method), tx.clone());
    let queued = webview.with_webview(move |platform| {
        let done = tx.clone();
        let handler =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result: windows_core::Result<()>, json: String| {
                answer(&done, result.map(|()| json).map_err(|e| e.message()));
                Ok(())
            }));
        // SAFETY: the controller and its core webview are live COM objects that Tauri hands over on the thread that owns
        // them; the strings outlive the call, and WebView2 keeps its own reference to the handler until it completes.
        let called = unsafe { platform.controller().CoreWebView2() }
            .and_then(|core| unsafe { core.CallDevToolsProtocolMethod(&method, &HSTRING::from(params), &handler) });
        if let Err(e) = called {
            answer(&tx, Err(e.message()));
        }
    });
    if let Err(e) = queued {
        answer(&failed, Err(e.to_string()));
    }
    rx
}
