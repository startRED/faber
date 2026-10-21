//! URL-level network protection for the main screen's sandboxed visual frames.
use std::sync::atomic::{AtomicU64, Ordering};

static WINDOW: AtomicU64 = AtomicU64::new(0);
static READY: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "linux")]
static OFFLINE_NAVIGATION: AtomicU64 = AtomicU64::new(0);
#[cfg(any(target_os = "macos", target_os = "linux", test))]
static INSTALLATION: tokio::sync::Notify = tokio::sync::Notify::const_new();

pub fn ready() -> bool {
    let generation = WINDOW.load(Ordering::Acquire);
    generation != 0 && READY.load(Ordering::Acquire) == generation
}

pub fn navigation_ready() -> bool {
    #[cfg(target_os = "linux")]
    {
        let generation = WINDOW.load(Ordering::Acquire);
        generation != 0 && OFFLINE_NAVIGATION.load(Ordering::Acquire) == generation
    }
    #[cfg(not(target_os = "linux"))]
    ready()
}

#[cfg(any(windows, target_os = "macos"))]
pub(crate) fn installed(generation: u64) {
    // A late WebKit compilation for a retired window must never authorize its replacement.
    if WINDOW.load(Ordering::Acquire) == generation {
        READY.store(generation, Ordering::Release);
        #[cfg(any(target_os = "macos", target_os = "linux", test))]
        INSTALLATION.notify_waiters();
    }
}

/// Wait without polling before issuing a URL whose child navigation requires a native top-document guard.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
pub(crate) async fn wait_navigation_ready(timeout: std::time::Duration) -> bool {
    let installed = INSTALLATION.notified();
    tokio::pin!(installed);
    installed.as_mut().enable();
    if !navigation_ready() {
        let _ = tokio::time::timeout(timeout, installed).await;
    }
    navigation_ready()
}

pub fn install(window: &tauri::WebviewWindow) {
    let generation = WINDOW.fetch_add(1, Ordering::AcqRel) + 1;
    #[cfg(windows)]
    crate::pane::native::visual_network(window, generation);
    #[cfg(target_os = "macos")]
    crate::visual_native::visual_network(window, generation);
    #[cfg(target_os = "linux")]
    install_offline_navigation(window, generation);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let _ = (window, generation);
}

#[cfg(target_os = "linux")]
fn install_offline_navigation(window: &tauri::WebviewWindow, generation: u64) {
    use webkit2gtk::glib::prelude::Cast;
    use webkit2gtk::{PolicyDecisionExt, PolicyDecisionType, ResponsePolicyDecision, ResponsePolicyDecisionExt, URIRequestExt, WebViewExt};

    let _ = window.with_webview(move |view| {
        view.inner().connect_decide_policy(|_, decision, kind| {
            if kind != PolicyDecisionType::Response {
                return false;
            }
            let Some(response) = decision.dynamic_cast_ref::<ResponsePolicyDecision>() else { return false };
            let source = response.request().and_then(|request| request.uri());
            if source.is_some_and(|source| block_visual_top_document(&source, response.is_main_frame_main_resource())) {
                decision.ignore();
                return true;
            }
            false
        });
        if WINDOW.load(Ordering::Acquire) == generation {
            // This only authorizes offline child navigation; no Linux network guard has been installed.
            OFFLINE_NAVIGATION.store(generation, Ordering::Release);
            INSTALLATION.notify_waiters();
        }
    });
}

#[cfg(any(target_os = "linux", test))]
fn block_visual_top_document(source: &str, is_main_frame: bool) -> bool {
    is_main_frame
        && tauri::Url::parse(source).is_ok_and(|url| url.scheme() == "faber-visual" || url.host_str() == Some("faber-visual.localhost"))
}

#[cfg(test)]
#[test]
fn visual_network_gtk_policy_keeps_offline_visual_frames_and_refuses_top_documents() {
    for source in [
        "faber-visual://localhost/0123456789abcdef0123456789abcdef.html",
        "http://faber-visual.localhost/0123456789abcdef0123456789abcdef.html",
    ] {
        assert!(crate::visual_page::navigation_allowed(&tauri::Url::parse(source).unwrap()));
        assert!(!block_visual_top_document(source, false), "offline child was refused: {source}");
        assert!(block_visual_top_document(source, true), "visual replaced the screen: {source}");
    }
    for source in ["tauri://localhost/index.html", "http://tauri.localhost/index.html"] {
        assert!(!block_visual_top_document(source, true));
    }
    // A response policy does not expose frame ancestry; never mistake screen assets for nested documents.
    for source in [
        "tauri://localhost/assets/screen.js",
        "tauri://localhost/assets/screen.css",
        "tauri://localhost/assets/mark.svg",
        "http://tauri.localhost/assets/screen.js",
    ] {
        assert!(!block_visual_top_document(source, false), "the native frame policy blocked a screen asset: {source}");
    }
}

#[cfg(test)]
#[tokio::test]
#[cfg(any(windows, target_os = "macos"))]
async fn visual_network_a_retired_windows_late_install_cannot_authorize_its_replacement() {
    let retired = WINDOW.fetch_add(1, Ordering::AcqRel) + 1;
    let current = WINDOW.fetch_add(1, Ordering::AcqRel) + 1;
    installed(retired);
    assert!(!ready());
    assert!(
        !wait_navigation_ready(std::time::Duration::ZERO).await,
        "a failed installation must return a page error, not issue a blank URL"
    );
    let (was_ready, ()) = tokio::join!(wait_navigation_ready(std::time::Duration::from_secs(1)), async {
        tokio::task::yield_now().await;
        installed(current);
    });
    assert!(was_ready, "preparation must wait for asynchronous WebKit rule installation");
    assert!(ready());
    installed(retired);
    assert!(ready());
}

#[cfg(all(test, target_os = "linux"))]
#[tokio::test]
async fn visual_network_linux_opens_offline_children_only_after_the_top_document_guard_is_installed() {
    let generation = WINDOW.fetch_add(1, Ordering::AcqRel) + 1;
    let url = tauri::Url::parse("faber-visual://localhost/0123456789abcdef0123456789abcdef.html").unwrap();
    assert!(!crate::screen_navigation_allowed(&url));
    assert!(!wait_navigation_ready(std::time::Duration::ZERO).await);
    OFFLINE_NAVIGATION.store(generation, Ordering::Release);
    assert!(wait_navigation_ready(std::time::Duration::ZERO).await);
    assert!(crate::screen_navigation_allowed(&url), "GTK also reports child navigation to the screen's callback");
    assert!(!ready(), "installing a navigation policy must not enable Linux live networking");
    assert!(block_visual_top_document(url.as_str(), true));
}

/// WebView2 invokes every handler for Wry's filters too: only HTTPS belongs to this guard.
#[cfg(any(windows, test))]
pub(crate) fn block_https_request(source: &str) -> bool {
    tauri::Url::parse(source).is_ok_and(|url| url.scheme() == "https" && !faber_engine::visual::public_https(&url))
}

#[cfg(test)]
#[test]
fn visual_network_shared_wry_filters_leave_custom_protocol_responses_untouched() {
    for source in [
        "http://faber-visual.localhost/0123456789abcdef0123456789abcdef.html",
        "http://tauri.localhost/assets/screen.js",
        "http://ipc.localhost/invoke",
        "faber-visual://localhost/0123456789abcdef0123456789abcdef.html",
        "data:text/plain,hello",
        "blob:http://tauri.localhost/id",
        "https://example.com/",
    ] {
        assert!(!block_https_request(source), "another handler's response was replaced: {source}");
    }
    for source in ["https://127.0.0.1/", "https://192.168.1.1/", "https://ipc.localhost/", "https://tauri.localhost/"] {
        assert!(block_https_request(source), "private HTTPS request survived: {source}");
    }
}

/// WebKit compiles child-request/document protection and top-document visual-origin denials into its network process.
/// URL rules cannot inspect DNS answers or prevent initial empty realms, which the CSP and all-frame guard protect.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn webkit_rules() -> String {
    use serde_json::json;
    let mut rules = vec![json!({"trigger":{"url-filter":".*","load-context":["child-frame"]},"action":{"type":"block"}})];
    for scheme in ["https", "data", "blob"] {
        rules.push(json!({"trigger":{"url-filter":format!("^{scheme}:"),"load-context":["child-frame"]},"action":{"type":"ignore-previous-rules"}}));
    }
    rules.push(json!({"trigger":{"url-filter":r"^faber-visual://localhost/[0-9a-f]+\.html$","load-context":["child-frame"]},"action":{"type":"ignore-previous-rules"}}));
    // WebKit's URL parser rejects alternation and counted repetitions; keep each alternative a separate rule.
    let mut blocked = vec![r"^https://[^/]*@".to_string(), r"^https://[^/.:]+\.?(:[0-9]+)?/".to_string()];
    for host in ["localhost", "local", "internal", "lan", "home"] {
        blocked.push(format!(r"^https://([^/:]*\.)?{host}\.?(:[0-9]+)?/"));
    }
    for prefix in ["0", "10", "127", "22[4-9]", "23[0-9]", "24[0-9]", "25[0-5]"] {
        blocked.push(format!(r"^https://{prefix}\.[0-9]+\.[0-9]+\.[0-9]+(:[0-9]+)?/"));
    }
    for prefix in [
        r"100\.6[4-9]",
        r"100\.[7-9][0-9]",
        r"100\.1[01][0-9]",
        r"100\.12[0-7]",
        r"169\.254",
        r"172\.1[6-9]",
        r"172\.2[0-9]",
        r"172\.3[01]",
        r"192\.168",
    ] {
        blocked.push(format!(r"^https://{prefix}\.[0-9]+\.[0-9]+(:[0-9]+)?/"));
    }
    blocked.push(r"^https://255\.255\.255\.255(:[0-9]+)?/".to_string());
    for address in [r"::[0-9a-f]*(:[0-9a-f]*)?", r"64:ff9b::[0-9a-f]*(:[0-9a-f]*)?", r"f[cd][^\]]*", r"fe[89abcdef][^\]]*", r"ff[^\]]*"] {
        blocked.push(format!(r"^https://\[{address}\]"));
    }
    for prefix in [
        r"[0-9a-f][0-9a-f]?:",
        r"a[0-9a-f][0-9a-f]:",
        r"7f[0-9a-f][0-9a-f]:",
        r"a9fe:",
        r"ac1[0-9a-f]:",
        r"c0a8:",
        r"64[4-7][0-9a-f]:",
        r"e[0-9a-f][0-9a-f][0-9a-f]:",
        r"f[0-9a-f][0-9a-f][0-9a-f]:",
        r"ffff:ffff",
    ] {
        blocked.push(format!(r"^https://\[::ffff:{prefix}[^\]]*\]"));
    }
    for pattern in blocked {
        rules.push(json!({"trigger":{"url-filter":pattern,"url-filter-is-case-sensitive":false,"load-context":["child-frame"]},"action":{"type":"block"}}));
    }
    // Only the visual page itself is a child document; Live exceptions authorize resources, never another frame.
    rules
        .push(json!({"trigger":{"url-filter":".*","resource-type":["document"],"load-context":["child-frame"]},"action":{"type":"block"}}));
    rules.push(json!({"trigger":{"url-filter":r"^faber-visual://localhost/[0-9a-f]+\.html$","resource-type":["document"],"load-context":["child-frame"]},"action":{"type":"ignore-previous-rules"}}));
    // Tauri's navigation callback discards WKNavigationAction.targetFrame, so the rule supplies the frame distinction.
    for pattern in [r"^faber-visual:", r"^http://faber-visual\.localhost/"] {
        rules.push(
            json!({"trigger":{"url-filter":pattern,"resource-type":["document"],"load-context":["top-frame"]},"action":{"type":"block"}}),
        );
    }
    serde_json::to_string(&rules).expect("fixed content rules serialize")
}

#[cfg(test)]
#[test]
fn visual_network_webkit_rules_protect_children_and_refuse_visual_origin_top_documents() {
    let rules: serde_json::Value = serde_json::from_str(&webkit_rules()).unwrap();
    let rules = rules.as_array().unwrap();
    assert_eq!(rules[0]["action"]["type"], "block");
    assert_eq!(rules[1]["action"]["type"], "ignore-previous-rules");
    assert!(rules.len() > 6);
    let (children, top_documents) = rules.split_at(rules.len() - 2);
    for rule in children {
        assert_eq!(rule["trigger"]["load-context"], serde_json::json!(["child-frame"]));
    }
    for rule in top_documents {
        assert_eq!(rule["trigger"]["load-context"], serde_json::json!(["top-frame"]));
        assert_eq!(rule["trigger"]["resource-type"], serde_json::json!(["document"]));
        assert_eq!(rule["action"]["type"], "block");
    }
    let (requests, documents) = children.split_at(children.len() - 2);
    assert!(rules[1..5].iter().all(|rule| rule["action"]["type"] == "ignore-previous-rules"));
    assert!(requests[5..].iter().all(|rule| rule["action"]["type"] == "block"));
    assert_eq!(documents[0]["action"]["type"], "block");
    assert_eq!(documents[1]["action"]["type"], "ignore-previous-rules");
    assert!(documents.iter().all(|rule| rule["trigger"]["resource-type"] == serde_json::json!(["document"])));
    for rule in rules {
        let pattern = rule["trigger"]["url-filter"].as_str().unwrap();
        assert!(pattern.is_ascii() && !pattern.contains(['|', '{', '}']), "WebKit rejects this pattern syntax: {pattern}");
    }
    let blocked = |source: &str, document: bool| {
        let url = tauri::Url::parse(source).unwrap();
        let mut denied = false;
        for rule in children {
            if !document && rule["trigger"]["resource-type"].is_array() {
                continue;
            }
            let pattern = rule["trigger"]["url-filter"].as_str().unwrap();
            let regex = regex::RegexBuilder::new(pattern).case_insensitive(true).build().unwrap();
            if regex.is_match(url.as_str()) {
                denied = rule["action"]["type"] == "block";
            }
        }
        denied
    };
    for source in [
        "faber-visual://localhost/0123456789abcdef0123456789abcdef.html",
        "http://faber-visual.localhost/0123456789abcdef0123456789abcdef.html",
    ] {
        assert!(
            top_documents
                .iter()
                .any(|rule| { regex::Regex::new(rule["trigger"]["url-filter"].as_str().unwrap()).unwrap().is_match(source) }),
            "visual page can replace the main document: {source}"
        );
    }
    for source in [
        "http://example.com/",
        "file:///C:/private.txt",
        "tauri://localhost/",
        "https://localhost/",
        "https://a.localhost./",
        "https://router/",
        "https://router./",
        "https://router.:443/",
        "https://printer.home/",
        "https://127.9.0.1/",
        "https://2130706433/",
        "https://0x7f000001/",
        "https://10.12.0.1/",
        "https://172.16.0.1/",
        "https://172.31.255.255/",
        "https://192.168.0.1/",
        "https://169.254.0.1/",
        "https://100.64.0.1/",
        "https://100.127.255.255/",
        "https://240.0.0.1/",
        "https://255.255.255.254/",
        "https://[::1]/",
        "https://[::]/",
        "https://[fd00::1]/",
        "https://[fe80::1]/",
        "https://[fec0::1]/",
        "https://[feff:ffff::1]/",
        "https://[64:ff9b::7f00:1]/",
        "https://[64:ff9b::808:808]/",
        "https://[::7f00:1]/",
        "https://[::808:808]/",
        "https://[::ffff:127.0.0.1]/",
        "https://[::ffff:10.0.0.1]/",
        "https://[::ffff:172.16.0.1]/",
        "https://[::ffff:192.168.0.1]/",
        "https://[::ffff:169.254.0.1]/",
        "https://[::ffff:100.64.0.1]/",
        "https://[::ffff:240.0.0.1]/",
        "https://user:pass@example.com/",
        "https://ipc.localhost/",
        "https://tauri.localhost/",
    ] {
        assert!(blocked(source, false), "private child request survived: {source}");
    }
    for source in [
        "https://example.com/",
        "faber-visual://localhost/0123456789abcdef0123456789abcdef.html",
        "https://localhost.example.com/",
        "https://127.example.com/",
        "https://127.0.0.1.example.com/",
        "https://visual-fixture.example/data",
        "https://8.8.8.8/",
        "https://172.32.0.1/",
        "https://100.63.255.255/",
        "https://100.128.0.1/",
        "https://[2606:4700:4700::1111]/",
        "https://[::ffff:8.8.8.8]/",
        "data:text/plain,hello",
        "blob:https://example.com/id",
    ] {
        assert!(!blocked(source, false), "public child request blocked: {source}");
    }
    assert!(!blocked("faber-visual://localhost/0123456789abcdef0123456789abcdef.html", true));
    for source in [
        "https://example.com/frame.html",
        "https://127.0.0.1/frame.html",
        "data:text/html,<script>alert(1)</script>",
        "blob:faber-visual://localhost/id",
        "about:blank",
        "about:srcdoc",
        "file:///private.html",
        "faber-visual://localhost/other.html",
    ] {
        assert!(blocked(source, true), "Live resource exception authorized a child document: {source}");
    }
}
