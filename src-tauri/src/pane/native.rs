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
type Slot<T = String> = Arc<Mutex<Option<oneshot::Sender<Result<T, String>>>>>;

#[cfg(windows)]
fn answer<T>(slot: &Slot<T>, result: Result<T, String>) {
    if let Some(tx) = crate::lock(slot).take() {
        let _ = tx.send(result);
    }
}

/// Test builds only: read the actual profile's private-mode flag on its owning thread.
#[cfg(all(windows, feature = "test-hooks"))]
pub fn test_private(webview: &tauri::Webview) -> oneshot::Receiver<Result<bool, String>> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_13;
    use windows_core::Interface;
    let (tx, rx) = oneshot::channel();
    let tx: Slot<bool> = Arc::new(Mutex::new(Some(tx)));
    let failed = tx.clone();
    let queued = webview.with_webview(move |platform| {
        // SAFETY: read-only getters on live COM objects, on the UI thread Tauri gives us; no object leaves it.
        let private = (|| unsafe {
            let core = platform.controller().CoreWebView2()?.cast::<ICoreWebView2_13>()?;
            let profile = core.Profile()?;
            let mut private = windows_core::BOOL(0);
            profile.IsInPrivateModeEnabled(&mut private)?;
            Ok::<_, windows_core::Error>(private.as_bool())
        })();
        answer(&tx, private.map_err(|e| e.message()));
    });
    if let Err(error) = queued {
        answer(&failed, Err(error.to_string()));
    }
    rx
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

/// Clears everything the pane's private profile holds (cookies, every site's storage, cache, history), the data any other
/// private pane made meanwhile would share, and answers once WebView2 says it is gone.
#[cfg(windows)]
pub fn clear_all(webview: &tauri::Webview) -> oneshot::Receiver<Result<(), String>> {
    use webview2_com::ClearBrowsingDataCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_13, ICoreWebView2Profile2};
    use windows_core::Interface;
    let (tx, rx) = oneshot::channel();
    let tx: Slot<()> = Arc::new(Mutex::new(Some(tx)));
    let failed = tx.clone();
    let queued = webview.with_webview(move |platform| {
        let done = tx.clone();
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result: windows_core::Result<()>| {
            answer(&done, result.map_err(|e| e.message()));
            Ok(())
        }));
        // SAFETY: as in `devtools`: live COM objects on the thread that owns them, and WebView2 keeps its own reference to
        // the handler until it completes.
        let called = unsafe { platform.controller().CoreWebView2() }
            .and_then(|core| core.cast::<ICoreWebView2_13>())
            .and_then(|core| unsafe { core.Profile() })
            .and_then(|profile| profile.cast::<ICoreWebView2Profile2>())
            .and_then(|profile| unsafe { profile.ClearBrowsingDataAll(&handler) });
        if let Err(e) = called {
            answer(&tx, Err(e.message()));
        }
    });
    if let Err(e) = queued {
        answer(&failed, Err(e.to_string()));
    }
    rx
}

/// The browser's keys Faber answers while the pane's page has the focus (`ui/keys.ts`, `SITE_KEYS`), by the name the screen
/// knows them by, from the key pressed (a Windows virtual key) and whether Ctrl, Shift and Alt are down. The page never
/// sees them; every other key is the page's.
#[cfg(windows)]
pub fn site_key(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<&'static str> {
    const DIGITS: [&str; 9] = ["tab1", "tab2", "tab3", "tab4", "tab5", "tab6", "tab7", "tab8", "tab9"];
    match (key, ctrl, shift, alt) {
        (0x54, true, false, false) => Some("new"),
        (0x54, true, true, false) => Some("reopen"),
        (0x57, true, false, false) => Some("close"),
        (0x09, true, false, false) => Some("next"),
        (0x09, true, true, false) => Some("previous"),
        (0x31..=0x39, true, false, false) => DIGITS.get((key - 0x31) as usize).copied(),
        (0x4C, true, false, false) => Some("address"),
        (0x52, true, false, false) => Some("reload"),
        (0x25, false, false, true) => Some("back"),
        (0x27, false, false, true) => Some("forward"),
        _ => None,
    }
}

/// Calls `on` with the name of each of the browser's keys (`site_key`) pressed while the pane's page has the focus, and keeps
/// it from the page.
#[cfg(windows)]
pub fn keys(webview: &tauri::Webview, on: impl Fn(&'static str) + Send + 'static) {
    use webview2_com::AcceleratorKeyPressedEventHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
        COREWEBVIEW2_PHYSICAL_KEY_STATUS,
    };
    use winsafe::co::VK;
    let _ = webview.with_webview(move |platform| {
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else { return Ok(()) };
            let (mut kind, mut key, mut status) =
                (COREWEBVIEW2_KEY_EVENT_KIND::default(), 0u32, COREWEBVIEW2_PHYSICAL_KEY_STATUS::default());
            // SAFETY: the arguments are a live COM object for the length of this call, and the outputs are plain locals.
            unsafe {
                args.KeyEventKind(&mut kind)?;
                args.VirtualKey(&mut key)?;
                args.PhysicalKeyStatus(&mut status)?;
            }
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN {
                return Ok(());
            }
            let down = |vk: VK| winsafe::GetKeyState(vk).0;
            if let Some(name) = site_key(key, down(VK::CONTROL), down(VK::SHIFT), down(VK::MENU)) {
                // SAFETY: as above.
                unsafe { args.SetHandled(true)? };
                // A held key repeats: Ctrl+W held down closes one tab, not one after another.
                if !status.WasKeyDown.as_bool() {
                    on(name);
                }
            }
            Ok(())
        }));
        let mut token = 0i64;
        // SAFETY: the controller is a live COM object on the thread that owns it, and WebView2 keeps its own reference to the
        // handler while the pane lives.
        let _ = unsafe { platform.controller().add_AcceleratorKeyPressed(&handler, &mut token) };
    });
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
    use super::{site_key, visual_frame_navigation};

    #[test]
    fn the_browser_s_keys_on_the_page_are_the_screen_s_and_every_other_key_stays_the_page_s() {
        assert_eq!(site_key(0x54, true, false, false), Some("new"));
        assert_eq!(site_key(0x54, true, true, false), Some("reopen"));
        assert_eq!(site_key(0x09, true, true, false), Some("previous"));
        assert_eq!(site_key(0x39, true, false, false), Some("tab9"));
        assert_eq!(site_key(0x25, false, false, true), Some("back"));
        assert_eq!(site_key(0x25, false, false, false), None, "an arrow alone moves the page");
        assert_eq!(site_key(0x54, false, false, false), None, "a letter typed in the page");
        assert_eq!(site_key(0x43, true, false, false), None, "Ctrl+C copies in the page");
        assert_eq!(site_key(0x54, true, false, true), None, "AltGr is Ctrl+Alt: a character on many keyboards");
    }

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
