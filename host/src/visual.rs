//! One pending, single-use visual document; never a file server or an engine-data origin.

use base64::Engine as _;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{Mutex, OnceLock};

pub const OFFLINE: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";
pub const LIVE: &str = "default-src 'none'; connect-src https:; script-src 'unsafe-inline' https:; style-src 'unsafe-inline' https:; img-src data: blob: https:; font-src data: https:; media-src data: blob: https:; base-uri 'none'; form-action 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'";
pub const NETWORK_READY: bool = !cfg!(target_os = "linux");
pub const ELECTRON_ORIGIN: &str = "faber-visual://app";
pub const LIMIT: usize = 8 * 1024 * 1024;

// Independent from the screen's unchanged loader authorization. Both header policies must pass.
const LOADER_HASH: &str = "'sha256-0bC38SaDD0B+6jvfczkP7RXTEuwNXQv1ydsZ0yMX1lg='";
const ANCESTORS: &str = "frame-ancestors tauri://localhost http://tauri.localhost faber://app";
const DEV_ANCESTOR: &str = if cfg!(debug_assertions) { " http://localhost:5173" } else { "" };
const GUARD_JS: &str = include_str!("../../src-tauri/src/visual.js");

struct Guard {
    script: String,
    hash: String,
}

fn guard() -> &'static Guard {
    static GUARD: OnceLock<Guard> = OnceLock::new();
    GUARD.get_or_init(|| {
        // The tag removes itself before the parser observer or any script in the page runs.
        let script = format!("document.currentScript?.remove();\n{}", GUARD_JS.replace("\r\n", "\n"));
        let digest = Sha256::digest(script.as_bytes());
        let hash = format!("'sha256-{}'", base64::engine::general_purpose::STANDARD.encode(digest));
        Guard { script, hash }
    })
}

/// The guard goes first in the document, before any markup of the author's, so nothing the author wrote can come before it:
/// no `<head>` hidden in a comment can move it, and an author `<meta>` policy only applies to what is parsed after it. The
/// header policy (`document`) already holds the guard's hash, so no policy of our own is needed ahead of it. Nothing is
/// searched for: the only thing skipped is a plain `<!doctype html>` at the start, because a script ahead of it would
/// put the page in quirks mode.
fn inject_guard(document: &str) -> Vec<u8> {
    let tag = format!("<script>{}</script>", guard().script);
    let at = simple_doctype_end(document);
    let mut out = String::with_capacity(document.len() + tag.len());
    out.push_str(&document[..at]);
    out.push_str(&tag);
    out.push_str(&document[at..]);
    out.into_bytes()
}

/// Where a leading `<!doctype html>` (any case, optional space) ends, else 0. Any other doctype (public identifiers can hold
/// a `>` in quotes) or anything before it gets the guard in front of it.
fn simple_doctype_end(document: &str) -> usize {
    let bytes = document.as_bytes();
    let start = if document.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let space = |at: usize| bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'));
    let mut at = start;
    while space(at) {
        at += 1;
    }
    let word = |at: usize, word: &str| bytes.get(at..at + word.len()).is_some_and(|got| got.eq_ignore_ascii_case(word.as_bytes()));
    if !word(at, "<!doctype") {
        return 0;
    }
    at += "<!doctype".len();
    if !space(at) {
        return 0;
    }
    while space(at) {
        at += 1;
    }
    if !word(at, "html") {
        return 0;
    }
    at += "html".len();
    while space(at) {
        at += 1;
    }
    if bytes.get(at) == Some(&b'>') { at + 1 } else { 0 }
}

fn page_path(uri: &str) -> Option<String> {
    let rest = uri
        .strip_prefix("faber-visual://app/")
        .or_else(|| uri.strip_prefix("faber-visual://localhost/"))
        .or_else(|| uri.strip_prefix("http://faber-visual.localhost/"))?;
    let token = rest.strip_suffix(".html")?;
    (token.len() == 32 && token.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))).then(|| format!("/{rest}"))
}

pub fn navigation_allowed(uri: &str) -> bool {
    uri == "about:blank" || page_path(uri).is_some()
}

struct Page {
    path: String,
    document: String,
    live: bool,
}

struct Session {
    id: String,
    revision: u64,
    pending: Option<Page>,
}

pub struct Pages {
    origin: &'static str,
    session: Mutex<Option<Session>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Prepared {
    pub url: String,
    pub live: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document {
    pub body: String,
    pub csp: String,
}

impl Default for Pages {
    fn default() -> Self {
        Self::electron()
    }
}

impl Pages {
    pub const fn new(origin: &'static str) -> Self {
        Self { origin, session: Mutex::new(None) }
    }

    pub const fn electron() -> Self {
        Self::new(ELECTRON_ORIGIN)
    }

    pub fn open(&self) -> String {
        let id = uuid::Uuid::new_v4().simple().to_string();
        *crate::lock(&self.session) = Some(Session { id: id.clone(), revision: 0, pending: None });
        id
    }

    pub fn close(&self, id: &str) {
        let mut current = crate::lock(&self.session);
        if current.as_ref().is_some_and(|session| session.id == id) {
            *current = None;
        }
    }

    pub fn prepare(&self, id: &str, revision: u64, document: String, live: bool) -> Result<Prepared, &'static str> {
        if document.len() > LIMIT {
            return Err("error.messageGone");
        }
        let mut current = crate::lock(&self.session);
        let session = current.as_mut().filter(|session| session.id == id).ok_or("error.messageGone")?;
        if revision <= session.revision {
            return Err("error.messageGone");
        }
        session.revision = revision;
        let path = format!("/{}.html", uuid::Uuid::new_v4().simple());
        let url = format!("{}{path}", self.origin);
        session.pending = Some(Page { path, document, live });
        Ok(Prepared { url, live })
    }

    /// Consumes the single-use page at `uri`, with the frame guard injected and its hash added to the CSP header.
    pub fn document(&self, uri: &str) -> Result<Document, &'static str> {
        let path = page_path(uri).ok_or("error.messageGone")?;
        let page = {
            let mut current = crate::lock(&self.session);
            current
                .as_mut()
                .and_then(
                    |session| if session.pending.as_ref().is_some_and(|page| page.path == path) { session.pending.take() } else { None },
                )
                .ok_or("error.messageGone")?
        };
        let bytes = inject_guard(&page.document);
        let body = base64::engine::general_purpose::STANDARD.encode(bytes);
        let policy = if page.live { LIVE } else { OFFLINE };
        let csp = format!(
            "{policy}, script-src {LOADER_HASH} 'strict-dynamic' {}; sandbox allow-scripts; {ANCESTORS}{DEV_ANCESTOR}",
            guard().hash
        );
        Ok(Document { body, csp })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_script_never_closes_its_own_script_tag() {
        assert!(!GUARD_JS.to_ascii_lowercase().contains("</script"));
    }

    fn guarded(document: &str) -> String {
        String::from_utf8(inject_guard(document)).unwrap()
    }

    #[test]
    fn the_guard_is_the_first_thing_of_the_document_whatever_the_markup_holds() {
        let tag = format!("<script>{}</script>", guard().script);
        // A `<head>` inside a comment (or a script string) cannot draw the guard away from the front.
        assert_eq!(guarded("<!-- <head> --><p>x</p>"), format!("{tag}<!-- <head> --><p>x</p>"));
        assert_eq!(guarded("<script>var s = \"<head>\"</script><head></head>"), format!("{tag}<script>var s = \"<head>\"</script><head></head>"));
        assert_eq!(guarded(""), tag);
        // The page's own policy comes after the guard, so it only applies to what follows: it cannot forbid the guard.
        let own = "<meta http-equiv=\"Content-Security-Policy\" content=\"script-src 'none'\"><p>x</p>";
        let out = guarded(own);
        assert_eq!(out, format!("{tag}{own}"));
        assert!(out.find("<script>").unwrap() < out.find("<meta").unwrap());
        // The same with the meta tag written in other cases and inside a head.
        let head = "<HEAD><META HTTP-EQUIV=content-security-policy content=\"default-src 'none'\"></HEAD>";
        assert_eq!(guarded(head), format!("{tag}{head}"));
    }

    #[test]
    fn a_plain_doctype_stays_first_so_the_page_is_not_put_in_quirks_mode() {
        let tag = format!("<script>{}</script>", guard().script);
        assert_eq!(guarded("<!DOCTYPE html><p>x"), format!("<!DOCTYPE html>{tag}<p>x"));
        assert_eq!(guarded("\u{feff}  <!doctype  HTML ><p>x"), format!("\u{feff}  <!doctype  HTML >{tag}<p>x"));
        // Anything else before or inside the doctype puts the guard in front of everything.
        assert_eq!(guarded("<!DOCTYPE html PUBLIC \"a>b\"><p>x"), format!("{tag}<!DOCTYPE html PUBLIC \"a>b\"><p>x"));
        assert_eq!(guarded("<!-- c --><!DOCTYPE html><p>x"), format!("{tag}<!-- c --><!DOCTYPE html><p>x"));
        assert_eq!(guarded("<!DOCTYPEhtml><p>x"), format!("{tag}<!DOCTYPEhtml><p>x"));
        assert_eq!(guarded("<!doctype html"), format!("{tag}<!doctype html"));
    }

    #[test]
    fn visual_document_serves_only_the_requested_page_once_with_guard_and_both_header_policies() {
        let pages = Pages::electron();
        let id = pages.open();
        for live in [false, true] {
            let html = "<head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'\"><script>loader()</script></head><body><form name=documentElement>private page</form></body>";
            let page = pages.prepare(&id, if live { 2 } else { 1 }, html.into(), live).unwrap();
            assert!(page.url.starts_with("faber-visual://app/"));
            assert!(navigation_allowed(&page.url));
            let doc = pages.document(&page.url).unwrap();
            let decoded = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(&doc.body).unwrap()).unwrap();
            assert!(decoded.contains("private page"));
            assert!(decoded.contains("document.currentScript?.remove();"));
            assert!(decoded.contains("faber-visual-escape"));
            let meta_start = decoded.find("<meta http-equiv").unwrap();
            let guard_start = decoded.find("<script>document.currentScript?.remove();").unwrap();
            let loader_start = decoded.find("<script>loader()</script>").unwrap();
            assert!(guard_start < meta_start && meta_start < loader_start, "the guard is first, before the page's own policy and loader");
            let expected = format!(
                "{}, script-src {LOADER_HASH} 'strict-dynamic' {}; sandbox allow-scripts; {ANCESTORS}{DEV_ANCESTOR}",
                if live { LIVE } else { OFFLINE },
                guard().hash
            );
            assert_eq!(doc.csp, expected);
            assert_eq!(pages.document(&page.url), Err("error.messageGone"), "consumed URLs cannot be read again");
        }
    }

    #[test]
    fn visual_document_refuses_other_paths_queries_and_schemes_without_consuming_the_page() {
        let pages = Pages::electron();
        let id = pages.open();
        let page = pages.prepare(&id, 1, "only this page".into(), false).unwrap();
        assert!(navigation_allowed("about:blank"));
        assert_eq!(pages.document("about:blank"), Err("error.messageGone"));
        for bad in
            ["/", "/index.html", "/../settings.json", "/%2e%2e/settings.json", "/other.html", "/0123456789ABCDEF0123456789ABCDEF.html"]
        {
            let url = format!("{ELECTRON_ORIGIN}{bad}");
            assert!(!navigation_allowed(&url), "{url}");
            assert_eq!(pages.document(&url), Err("error.messageGone"));
        }
        for bad_url in [
            format!("{}?q=x", page.url),
            format!("{}#frag", page.url),
            page.url.replace("faber-visual://app/", "faber-visual://app:80/"),
            page.url.replace("faber-visual://app/", "faber-visual://user@app/"),
            page.url.replace("faber-visual://app/", "https://app/"),
        ] {
            assert!(!navigation_allowed(&bad_url), "{bad_url}");
            assert_eq!(pages.document(&bad_url), Err("error.messageGone"));
        }
        assert!(pages.document(&page.url).is_ok());
    }

    #[test]
    fn visual_document_close_and_version_switch_release_bytes_and_refuse_late_preparations() {
        let pages = Pages::electron();
        let first = pages.open();
        let old = pages.prepare(&first, 1, "old".into(), false).unwrap();
        let new = pages.prepare(&first, 2, "new".into(), true).unwrap();
        assert_eq!(pages.document(&old.url), Err("error.messageGone"));
        let decoded =
            String::from_utf8(base64::engine::general_purpose::STANDARD.decode(pages.document(&new.url).unwrap().body).unwrap()).unwrap();
        assert!(decoded.ends_with("new"));
        let next = pages.open();
        pages.close(&first);
        assert!(pages.prepare(&first, 3, "late".into(), true).is_err());
        assert!(pages.prepare(&next, 1, "current".into(), false).is_ok());
        pages.close(&next);
        assert!(pages.prepare(&next, 2, "after close".into(), true).is_err());
        assert!(crate::lock(&pages.session).is_none());
    }

    #[test]
    fn visual_document_an_older_preparation_cannot_replace_the_selected_version() {
        let pages = Pages::electron();
        let id = pages.open();
        let newest = pages.prepare(&id, 2, "selected".into(), false).unwrap();
        assert!(pages.prepare(&id, 1, "late old version".into(), true).is_err());
        assert!(pages.prepare(&id, 2, "duplicate".into(), true).is_err());
        assert!(pages.prepare(&id, 3, "x".repeat(LIMIT + 1), true).is_err());
        let decoded =
            String::from_utf8(base64::engine::general_purpose::STANDARD.decode(pages.document(&newest.url).unwrap().body).unwrap())
                .unwrap();
        assert!(decoded.ends_with("selected"));
        assert!(crate::lock(&pages.session).as_ref().unwrap().pending.is_none());
    }
}
