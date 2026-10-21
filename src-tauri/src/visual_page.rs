//! One pending, single-use visual document; never a file server or an engine-data origin.
use serde::Serialize;
use std::sync::Mutex;
use tauri::http::{Request, Response};

pub const OFFLINE: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";
pub const LIVE: &str = "default-src 'none'; connect-src https:; script-src 'unsafe-inline' https:; style-src 'unsafe-inline' https:; img-src data: blob: https:; font-src data: https:; media-src data: blob: https:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";
// Independent from the screen's unchanged loader authorization. Both header policies must pass.
const LOADER: &str = "script-src 'sha256-THVAIZwBzgCVsIldon5G2RmhMqd/u89imJA/c4WJido=' 'strict-dynamic'; sandbox allow-scripts; frame-ancestors tauri://localhost http://tauri.localhost";
const DEV_ANCESTOR: &str = if cfg!(debug_assertions) { " http://localhost:5173" } else { "" };
const LIMIT: usize = 8 * 1024 * 1024;

struct Page {
    path: String,
    bytes: Vec<u8>,
    live: bool,
}
struct Session {
    id: String,
    revision: u64,
    pending: Option<Page>,
}
#[derive(Default)]
pub struct Pages(Mutex<Option<Session>>);

#[derive(Serialize)]
pub struct Prepared {
    pub url: String,
    pub live: bool,
}

fn origin() -> &'static str {
    if cfg!(windows) { "http://faber-visual.localhost" } else { "faber-visual://localhost" }
}

pub fn navigation_allowed(url: &tauri::Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    let transport = (url.scheme() == "faber-visual" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("faber-visual.localhost"));
    let path = url.path().strip_prefix('/').and_then(|path| path.strip_suffix(".html"));
    transport
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && path.is_some_and(|token| token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
}

impl Pages {
    pub fn open(&self) -> String {
        let id = uuid::Uuid::new_v4().simple().to_string();
        *crate::lock(&self.0) = Some(Session { id: id.clone(), revision: 0, pending: None });
        id
    }

    pub fn close(&self, id: &str) {
        let mut current = crate::lock(&self.0);
        if current.as_ref().is_some_and(|session| session.id == id) {
            *current = None;
        }
    }

    pub fn prepare(&self, id: &str, revision: u64, document: String, live: bool) -> Result<Prepared, &'static str> {
        if document.len() > LIMIT {
            return Err("error.messageGone");
        }
        let mut current = crate::lock(&self.0);
        let session = current.as_mut().filter(|session| session.id == id).ok_or("error.messageGone")?;
        if revision <= session.revision {
            return Err("error.messageGone");
        }
        session.revision = revision;
        let path = format!("/{}.html", uuid::Uuid::new_v4().simple());
        let url = format!("{}{path}", origin());
        session.pending = Some(Page { path, bytes: document.into_bytes(), live });
        Ok(Prepared { url, live })
    }

    pub fn respond(&self, label: &str, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        let url = tauri::Url::parse(&request.uri().to_string());
        let destination = request.headers().get("sec-fetch-dest").and_then(|value| value.to_str().ok());
        // WebView2 may omit headers added later by its network stack. Supplied destinations still constrain reads.
        let frame = destination == Some("iframe") || destination.is_none();
        let valid = label == "main"
            && request.method() == "GET"
            && frame
            && url.as_ref().is_ok_and(|url| url.as_str() != "about:blank" && navigation_allowed(url));
        let page = if valid {
            let mut current = crate::lock(&self.0);
            current.as_mut().and_then(|session| {
                if session.pending.as_ref().is_some_and(|page| request.uri().path() == page.path) { session.pending.take() } else { None }
            })
        } else {
            None
        };
        let (status, bytes, policy) = match page {
            Some(page) => (200, page.bytes, if page.live { LIVE } else { OFFLINE }),
            None => (403, Vec::new(), "default-src 'none'; sandbox"),
        };
        Response::builder()
            .status(status)
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Content-Security-Policy", format!("{policy}, {LOADER}{DEV_ANCESTOR}"))
            .header("Cache-Control", "no-store")
            .header("Referrer-Policy", "no-referrer")
            .header("X-Content-Type-Options", "nosniff")
            .body(bytes)
            .expect("fixed visual response headers are valid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(url: &str) -> Request<Vec<u8>> {
        Request::builder().uri(url).header("Sec-Fetch-Dest", "iframe").body(Vec::new()).unwrap()
    }

    #[test]
    fn visual_protocol_serves_only_the_requested_bytes_once_with_both_header_policies() {
        let pages = Pages::default();
        let id = pages.open();
        for live in [false, true] {
            let page = pages.prepare(&id, if live { 2 } else { 1 }, "<form name=documentElement>private page</form>".into(), live).unwrap();
            let response = pages.respond("main", request(&page.url));
            assert_eq!(response.status(), 200);
            assert_eq!(response.body(), b"<form name=documentElement>private page</form>");
            let policies: Vec<_> = response.headers().get_all("content-security-policy").iter().map(|p| p.to_str().unwrap()).collect();
            assert_eq!(policies, [format!("{}, {LOADER}{DEV_ANCESTOR}", if live { LIVE } else { OFFLINE }).as_str()]);
            assert!(!response.headers().contains_key("access-control-allow-origin"));
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(pages.respond("main", request(&page.url)).status(), 403, "consumed URLs cannot be read again");
        }
    }

    #[test]
    fn visual_protocol_missing_network_metadata_still_delivers_the_single_use_opaque_page() {
        let pages = Pages::default();
        let id = pages.open();
        let page = pages.prepare(&id, 1, "<p>Ready</p>".into(), false).unwrap();
        let missing = || Request::builder().uri(&page.url).body(Vec::new()).unwrap();
        assert_eq!(pages.respond("other-window", missing()).status(), 403);
        let response = pages.respond("main", missing());
        assert_eq!(response.status(), 200, "the native request need not yet contain Sec-Fetch-Dest");
        assert_eq!(response.body(), b"<p>Ready</p>");
        assert!(!response.headers().contains_key("access-control-allow-origin"));
        assert!(response.headers()["content-security-policy"].to_str().unwrap().contains("sandbox allow-scripts"));
        assert_eq!(pages.respond("main", missing()).status(), 403, "missing metadata cannot recover consumed bytes");
    }

    #[test]
    fn visual_protocol_refuses_other_views_methods_paths_and_reads_without_consuming_the_page() {
        let pages = Pages::default();
        let id = pages.open();
        let page = pages.prepare(&id, 1, "only this page".into(), false).unwrap();
        assert_eq!(pages.respond("browser", request(&page.url)).status(), 403);
        for path in ["/", "/index.html", "/../settings.json", "/%2e%2e/settings.json", "/other.html"] {
            assert_eq!(pages.respond("main", request(&format!("{}{path}", origin()))).status(), 403);
        }
        for destination in ["empty", "image", "document"] {
            let mut req = request(&page.url);
            req.headers_mut().insert("Sec-Fetch-Dest", destination.parse().unwrap());
            assert_eq!(pages.respond("main", req).status(), 403);
        }
        let mut post = request(&page.url);
        *post.method_mut() = tauri::http::Method::POST;
        assert_eq!(pages.respond("main", post).status(), 403);
        assert_eq!(pages.respond("main", request(&format!("{}?q=x", page.url))).status(), 403);
        assert_eq!(pages.respond("main", request(&page.url)).status(), 200);
    }

    #[test]
    fn visual_protocol_close_and_version_switch_release_bytes_and_refuse_late_preparations() {
        let pages = Pages::default();
        let first = pages.open();
        let old = pages.prepare(&first, 1, "old".into(), false).unwrap();
        let new = pages.prepare(&first, 2, "new".into(), true).unwrap();
        assert_eq!(pages.respond("main", request(&old.url)).status(), 403);
        assert_eq!(pages.respond("main", request(&new.url)).body(), b"new");
        let next = pages.open();
        pages.close(&first);
        assert!(pages.prepare(&first, 3, "late".into(), true).is_err());
        assert!(pages.prepare(&next, 1, "current".into(), false).is_ok());
        pages.close(&next);
        assert!(pages.prepare(&next, 2, "after close".into(), true).is_err());
        assert!(crate::lock(&pages.0).is_none());
    }

    #[test]
    fn visual_protocol_an_older_preparation_cannot_replace_the_selected_version() {
        let pages = Pages::default();
        let id = pages.open();
        let newest = pages.prepare(&id, 2, "selected".into(), false).unwrap();
        assert!(pages.prepare(&id, 1, "late old version".into(), true).is_err());
        assert!(pages.prepare(&id, 2, "duplicate".into(), true).is_err());
        assert!(pages.prepare(&id, 3, "x".repeat(LIMIT + 1), true).is_err());
        assert_eq!(pages.respond("main", request(&newest.url)).body(), b"selected");
        assert!(crate::lock(&pages.0).as_ref().unwrap().pending.is_none());
    }
}
