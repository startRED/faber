//! Native WebView calls: the browser pane's private DevTools channel and the main screen's visual-frame request guard.
//! The pane uses `CallDevToolsProtocolMethod`, never a DevTools port another program on the PC could reach
//! (the spike's rule 5, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md).
#![allow(unsafe_code)]

#[cfg(windows)]
use std::sync::{Arc, Mutex};
#[cfg(windows)]
use tokio::sync::oneshot;
#[cfg(windows)]
use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
#[cfg(windows)]
use windows_core::HSTRING;

#[cfg(windows)]
type Slot = Arc<Mutex<Option<oneshot::Sender<Result<String, String>>>>>;

#[cfg(windows)]
fn answer(slot: &Slot, result: Result<String, String>) {
    if let Some(tx) = crate::lock(slot).take() {
        let _ = tx.send(result);
    }
}

/// Sends `method` with its JSON `params` to the pane's page and answers with the result's JSON, or why it failed.
#[cfg(windows)]
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

#[cfg(windows)]
fn visual_frame_navigation(nested: bool, uri: &str) -> bool {
    !nested && tauri::Url::parse(uri).is_ok_and(|url| crate::visual_page::navigation_allowed(&url))
}

#[cfg(windows)]
fn guard_visual_frame(frame: webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Frame, nested: bool) -> windows_core::Result<()> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Frame7;
    use webview2_com::{FrameChildFrameCreatedEventHandler, FrameNavigationStartingEventHandler, take_pwstr};
    use windows_core::{Interface, PWSTR};

    // SAFETY: FrameCreated runs on the owning WebView2 thread; each frame retains its own handlers, which never retain it.
    unsafe {
        let frame: ICoreWebView2Frame7 = frame.cast()?;
        let created = FrameChildFrameCreatedEventHandler::create(Box::new(move |_, args| {
            if let Some(args) = args {
                guard_visual_frame(args.Frame()?, true)?;
            }
            Ok(())
        }));
        let mut token = 0;
        frame.add_FrameCreated(&created, &mut token)?;
        // Frame-level handlers run after the core handler on the same cancellation args, including redirects.
        let navigation = FrameNavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            args.SetCancel(true)?;
            let mut raw = PWSTR::null();
            args.Uri(&mut raw)?;
            if visual_frame_navigation(nested, &take_pwstr(raw)) {
                args.SetCancel(false)?;
            }
            Ok(())
        }));
        frame.add_NavigationStarting(&navigation, &mut token)?;
    }
    Ok(())
}

/// Inspect only HTTPS, for every source including opaque-origin frames. Other schemes are denied by the frame's CSP.
#[cfg(windows)]
pub fn visual_network(window: &tauri::WebviewWindow, generation: u64) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL, COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL, ICoreWebView2_22,
    };
    use webview2_com::{FrameCreatedEventHandler, NavigationStartingEventHandler, WebResourceRequestedEventHandler, take_pwstr};
    use windows_core::{Interface, PWSTR};

    let queued = window.with_webview(move |platform| {
        // SAFETY: Tauri runs this callback on the live controller's owning thread. WebView2 retains the handler, its
        // captured environment and response until the webview closes. COM output strings are released by take_pwstr.
        let installed = unsafe {
            (|| -> windows_core::Result<()> {
                let core = platform.controller().CoreWebView2()?;
                let modern: ICoreWebView2_22 = core.cast()?;
                // Default-deny: a first-level frame can navigate only after its descendant guard was installed.
                let navigation = NavigationStartingEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        args.SetCancel(true)?;
                    }
                    Ok(())
                }));
                let mut token = 0;
                core.add_FrameNavigationStarting(&navigation, &mut token)?;
                let created = FrameCreatedEventHandler::create(Box::new(move |_, args| {
                    if let Some(args) = args {
                        guard_visual_frame(args.Frame()?, false)?;
                    }
                    Ok(())
                }));
                modern.add_FrameCreated(&created, &mut token)?;
                let env = platform.environment();
                let denied = env.CreateWebResourceResponse(
                    None,
                    403,
                    &HSTRING::from("Blocked"),
                    &HSTRING::from("Content-Length: 0\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\n"),
                )?;
                let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let request = args.Request()?;
                    let mut raw = PWSTR::null();
                    request.Uri(&mut raw)?;
                    // Filters and events are shared with Wry: never overwrite its custom-protocol responses.
                    if crate::visual_network::block_https_request(&take_pwstr(raw)) {
                        args.SetResponse(&denied)?;
                    }
                    Ok(())
                }));
                core.add_WebResourceRequested(&handler, &mut token)?;
                modern.AddWebResourceRequestedFilterWithRequestSourceKinds(
                    &HSTRING::from("https://*"),
                    COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
                    COREWEBVIEW2_WEB_RESOURCE_REQUEST_SOURCE_KINDS_ALL,
                )?;
                Ok(())
            })()
        };
        match installed {
            Ok(()) => crate::visual_network::installed(generation),
            Err(error) => eprintln!("visual network guard: {error}"),
        }
    });
    if let Err(error) = queued {
        eprintln!("visual network guard: {error}");
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::visual_frame_navigation;

    #[test]
    fn visual_native_frame_navigation_refuses_every_nested_destination() {
        let visual = "http://faber-visual.localhost/0123456789abcdef0123456789abcdef.html";
        for uri in [visual, "about:blank", "about:srcdoc", "https://example.com/", "http://ipc.localhost/", "javascript:void(0)"] {
            assert!(!visual_frame_navigation(true, uri), "a nested frame must never navigate to {uri}");
        }
        assert!(visual_frame_navigation(false, visual));
        assert!(visual_frame_navigation(false, "about:blank"));
        assert!(!visual_frame_navigation(false, "about:srcdoc"));
        assert!(!visual_frame_navigation(false, "https://example.com/"));
    }
}

#[cfg(target_os = "macos")]
pub fn visual_network(window: &tauri::WebviewWindow, generation: u64) {
    use block2::RcBlock;
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_foundation::{NSError, NSString};
    use objc2_web_kit::{WKContentRuleList, WKContentRuleListStore, WKUserContentController};

    let queued = window.with_webview(move |platform| {
        let Some(mtm) = MainThreadMarker::new() else { return };
        // SAFETY: Tauri supplies the live WKUserContentController on the main thread. Retain it through the asynchronous
        // compilation; WebKit copies the block, and its controller retains the compiled list after installation.
        unsafe {
            let Some(controller) = Retained::retain(platform.controller().cast::<WKUserContentController>()) else {
                return;
            };
            let Some(store) = WKContentRuleListStore::defaultStore(mtm) else { return };
            let completion = RcBlock::new(move |list: *mut WKContentRuleList, error: *mut NSError| {
                if let Some(list) = list.as_ref().filter(|_| error.is_null()) {
                    controller.addContentRuleList(list);
                    crate::visual_network::installed(generation);
                } else {
                    eprintln!("visual network guard: content rule compilation failed");
                }
            });
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&NSString::from_str("faber-visual-network-v1")),
                Some(&NSString::from_str(&crate::visual_network::webkit_rules())),
                Some(&completion),
            );
        }
    });
    if let Err(error) = queued {
        eprintln!("visual network guard: {error}");
    }
}
