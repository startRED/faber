//! The pane's rules that need no view: what loads, how long things wait, how a screen size is drawn, what the agent hears.

use super::firefox;
use faber_engine::screens::Viewport;
use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

/// Unused this long, the agent's pane behind closes: a second page lives only while the agent works out of view.
pub(super) const BEHIND_IDLE: Duration = Duration::from_secs(60);
/// How long a page has to finish loading before its text is read anyway.
pub(super) const LOAD: Duration = Duration::from_secs(30);
/// The longest one DevTools call may take; a page that holds it longer is closed with the pane.
pub(super) const CALL: Duration = Duration::from_secs(10);
/// Unused this long and not on screen, the pane closes and gives its memory back (the spike's rule 4).
pub(super) const IDLE: Duration = Duration::from_secs(5 * 60);
/// How long the agent's call waits for the person to hand the browser back.
pub(super) const WAIT_PERSON: Duration = Duration::from_secs(30);
/// After a click, typing or going back, how long a new page has to start loading before the same page is read again.
pub(super) const NAVIGATES: Duration = Duration::from_millis(1_500);
/// The agent's actions, run inside the page: the function `act.js` exports, which the screen's checks import as it is.
const ACT: &str = include_str!("act.js");
/// The page's rounded corners and thin scrollbars, drawn in a world of Faber's own on every document.
pub(super) const ROUND: &str = include_str!("round.js");
/// How long a call for the page's corners may take before the corners wait for the next placing.
pub(super) const LOOK_CALL: Duration = Duration::from_secs(3);
/// The roundest corner a page takes, in logical pixels: a look's card is never rounder.
pub(super) const MAX_RADIUS: f64 = 32.0;
/// The picture in the reply is this share of the page's size.
pub(super) const PICTURE_SCALE: f64 = 0.375;
/// The longest side of a picture at a screen size of the agent's, in the picture's pixels, whatever the screen's density.
pub(super) const LOOK_SIDE: f64 = 640.0;
/// How long a page has to lay itself out again at a new screen size before it is looked at.
pub(super) const RESIZED: Duration = Duration::from_millis(400);
/// The least time between two writes of where the pane's page is: a page that changes its title in a loop writes the
/// conversation's file at most this often.
pub(super) const KEEP_EVERY: Duration = Duration::from_secs(2);
/// How long the person has to pick sites from a list before it must be read again.
pub(super) const LISTING: Duration = Duration::from_secs(5 * 60);
/// The longest the pane spends setting brought-over sign-ins, so a page the person or the agent waits for is never held up.
pub(super) const SETTING: Duration = Duration::from_secs(8);
/// The longest a pane waits for its private data to be cleared before it gives up.
pub(super) const CLEARING: Duration = Duration::from_secs(10);
/// At most this many conversations hold sign-ins at once; the oldest import goes first.
pub(super) const MAX_HELD: usize = 8;
/// The most sites the agent hears the person went to: the latest of them.
pub(super) const MAX_VISITED: usize = 5;
/// The agent's call found the pane it was sent to holding something else once its turn came: it is sent again.
pub(super) const MOVED: &str = "the person moved Faber's browser to another tab meanwhile; try again";
/// How many times the agent's call is sent before it gives up.
pub(super) const SENDS: usize = 3;

pub(super) fn act_function() -> &'static str {
    ACT.split_once("export default ").map_or(ACT, |(_, f)| f.trim_end().trim_end_matches(';'))
}

/// The share of a page at `size` its picture takes: the capture counts each of the screen's device pixels, so a dense phone
/// screen is scaled down further than a PC's, and every picture's longest side is `LOOK_SIDE`.
pub(super) fn look_scale(size: &Viewport) -> f64 {
    (LOOK_SIDE / (f64::from(size.width.max(size.height)) * size.scale)).min(1.0)
}

/// The DevTools parameters that draw the page at `size`, as that screen would: its width, height and density, and a phone's
/// or tablet's layout rules and touch.
pub(super) fn metrics(size: &Viewport) -> (Value, Value) {
    let mobile = size.mobile();
    (
        json!({ "width": size.width, "height": size.height, "deviceScaleFactor": size.scale, "mobile": mobile }),
        json!({ "enabled": mobile, "maxTouchPoints": if mobile { 5 } else { 1 } }),
    )
}

/// How much a page drawn at `size` shrinks so all of it fits a box of `width` by `height` logical pixels; never past its real size.
pub(super) fn fit_scale(size: &Viewport, width: f64, height: f64) -> f64 {
    (width / f64::from(size.width)).min(height / f64::from(size.height)).clamp(0.05, 1.0)
}

/// Whether the pane may load `url`: a web page, never one of Faber's own origins (nor, in a development build, the screen's
/// own dev server).
pub fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let dev_screen = cfg!(debug_assertions) && host == "localhost" && url.port() == Some(5173);
    matches!(url.scheme(), "http" | "https") && !host.is_empty() && !host.ends_with(".localhost") && !dev_screen
}

/// [`allowed`] for an address as text: what a shell asks before any navigation of a view.
pub fn allowed_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| allowed(&url))
}

/// The address as the pane loads it, or why it never does.
pub(super) fn checked(url: &str) -> Result<Url, String> {
    let parsed: Url = url.parse().map_err(|_| "the address is not valid".to_string())?;
    if !allowed(&parsed) {
        return Err("only web pages open in Faber's browser".into());
    }
    Ok(parsed)
}

/// The full address the owner last read, or the sentence that sends the agent to read first.
pub(super) fn bound(reported: Option<&str>) -> Result<&str, String> {
    reported.ok_or_else(|| "read the page first; click and type only run on a page you have read".to_string())
}

/// A cookie as the DevTools call sets it: one stored for a domain keeps it (its subdomains get it too), one stored for its
/// host alone is set by address, so it stays that host's. None for a host or path that is not one: the address is built
/// from both, and a path like `@other.example/` would move the cookie to another site.
pub(super) fn cookie_param(cookie: &firefox::Cookie) -> Option<Value> {
    let host = cookie.host.strip_prefix('.').unwrap_or(&cookie.host);
    let plain_host = !host.is_empty() && host.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
    let plain_path = cookie.path.starts_with('/') && cookie.path.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'?' | b'#' | b'\\'));
    if !plain_host || !plain_path {
        return None;
    }
    let mut param = json!({
        "name": cookie.name, "value": cookie.value, "path": cookie.path,
        "secure": cookie.secure, "httpOnly": cookie.http_only, "expires": cookie.expires,
    });
    if cookie.host.starts_with('.') {
        param["domain"] = json!(cookie.host);
    } else {
        let url = Url::parse(&format!("{}://{host}{}", if cookie.secure { "https" } else { "http" }, cookie.path)).ok()?;
        if url.host_str() != Some(host) {
            return None;
        }
        param["url"] = json!(url.as_str());
    }
    if let Some(same_site) = cookie.same_site {
        param["sameSite"] = json!(same_site);
    }
    Some(param)
}

pub(super) fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The screen's boxes the pane is placed over, named by numbers that only grow while the screen lives. A box that let the
/// pane go places it no more, and a box older than the one that has it cannot move or take it away. Each load of the screen
/// has an id of its own, chosen at random: a request of any other load (a screen that went away, its late calls) is ignored.
#[derive(Default)]
pub(super) struct Boxes {
    load: Option<u32>,
    /// Loads that were the current one before: a late start of one of them never takes the pane back.
    retired: Vec<u32>,
    parked: u64,
    placed: u64,
}

impl Boxes {
    /// Whether box `id` of screen load `load` may place the pane.
    pub(super) fn place(&mut self, load: u32, id: u64) -> bool {
        let fresh = self.load == Some(load) && id > self.parked && id >= self.placed;
        if fresh {
            self.placed = id;
        }
        fresh
    }

    /// Whether box `id` letting go takes the pane off the screen. Box 0 is a screen just loaded: it makes `load` the one that
    /// counts and forgets every box.
    pub(super) fn park(&mut self, load: u32, id: u64) -> bool {
        if id == 0 {
            if self.retired.contains(&load) {
                return false;
            }
            let mut retired = std::mem::take(&mut self.retired);
            if let Some(old) = self.load.filter(|old| *old != load) {
                retired.push(old);
                if retired.len() > 16 {
                    retired.remove(0);
                }
            }
            *self = Boxes { load: Some(load), retired, ..Boxes::default() };
            return true;
        }
        if self.load != Some(load) {
            return false;
        }
        self.parked = self.parked.max(id);
        id >= self.placed
    }
}

/// Runs `then` once no action of the agent is changing the page, and keeps further ones out while it runs.
pub(super) async fn when_idle<T>(acting: &tokio::sync::Mutex<()>, then: impl FnOnce() -> T) -> T {
    let _idle = acting.lock().await;
    then()
}

/// Notes a site the person went to, for the agent and so for its vendor: only the site's own name (`docs.github.com` is
/// `github.com`, since a subdomain can be a token or a tenant), only for web pages with a plain name, never a local address
/// or an intranet name, once for pages in a row on the same site, the latest few kept. A page that finishes after the person
/// left the pane is not counted: the list can fall short, never say more.
pub(super) fn went_to(visited: &mut Vec<String>, url: &Url) {
    let Some(host) = url.domain() else { return };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let plain = host.len() <= 253 && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.');
    let labels: Vec<&str> = host.split('.').collect();
    if !matches!(url.scheme(), "http" | "https") || !plain || labels.len() < 2 || labels.iter().any(|l| l.is_empty() || l.len() > 63) {
        return;
    }
    let tld = labels[labels.len() - 1];
    // Names that only mean something inside a network (RFC 6762, 6761, and the usual intranet ones) are never told.
    if matches!(
        tld,
        "local" | "localhost" | "internal" | "intranet" | "corp" | "home" | "lan" | "localdomain" | "test" | "invalid" | "arpa" | "private"
    ) {
        return;
    }
    // The last two labels, always: under a country's second level that is only `co.uk`, which says little, but any rule that
    // keeps a third label lets a subdomain through somewhere (`token.org.de`).
    let site = labels[labels.len() - 2..].join(".");
    if visited.last() != Some(&site) {
        visited.push(site);
    }
    if visited.len() > MAX_VISITED {
        visited.remove(0);
    }
}

/// The conversation pane `made`'s pages are kept for, while that pane is still the one open: a late title or load of a pane
/// that closed, or of the pane before a new one, keeps nothing.
pub(super) fn keeps_now(keeps_for: &Option<(u64, String)>, made: u64, open: bool) -> Option<String> {
    keeps_for.as_ref().filter(|(pane, _)| open && *pane == made && made != 0).map(|(_, conversation)| conversation.clone())
}

/// Whether the agent's call on `tab` goes to the pane behind: the first pane holds another tab of its conversation, the
/// person's, and the agent's tab is not the one in front (a blank tab in front that became the agent's shows its page).
pub(super) fn goes_behind(open: bool, owner: Option<&str>, held: Option<&str>, conversation: &str, tab: &str, front: Option<&str>) -> bool {
    holds_another(open, owner, held, conversation, tab) && front != Some(tab)
}

/// Whether a pane with a page of `owner`'s tab `held` holds another tab of `conversation` than `tab`: the person's, which the
/// agent's call never loads over. Another conversation's page is not: the pane passes to the agent's conversation as before.
pub(super) fn holds_another(open: bool, owner: Option<&str>, held: Option<&str>, conversation: &str, tab: &str) -> bool {
    open && owner == Some(conversation) && held.is_some_and(|held| held != tab)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_s_call_goes_behind_only_when_the_pane_holds_another_tab_of_its_conversation() {
        assert!(holds_another(true, Some("a"), Some("t1"), "a", "t2"), "the person's tab stays as it is");
        assert!(!holds_another(true, Some("a"), Some("t2"), "a", "t2"), "its own tab");
        assert!(!holds_another(true, Some("a"), None, "a", "t2"), "a page from before tabs");
        assert!(!holds_another(true, Some("b"), Some("t1"), "a", "t2"), "another conversation's page");
        assert!(!holds_another(false, Some("a"), Some("t1"), "a", "t2"), "no page open");
        assert!(goes_behind(true, Some("a"), Some("t1"), "a", "t2", Some("t1")), "the person looks at their own tab");
        assert!(!goes_behind(true, Some("a"), Some("t1"), "a", "t2", Some("t2")), "a blank tab in front became the agent's");
    }

    #[test]
    fn a_late_page_of_a_pane_that_closed_or_was_made_again_is_never_kept_in_another_conversation() {
        let a = Some((3, "a".to_string()));
        assert_eq!(keeps_now(&a, 3, true).as_deref(), Some("a"));
        assert_eq!(keeps_now(&a, 3, false), None, "the pane closed");
        assert_eq!(keeps_now(&None, 3, true), None, "closing forgot whose it was");
        assert_eq!(keeps_now(&Some((4, "b".to_string())), 3, true), None, "pane 3's late title, once pane 4 loads for b");
        assert_eq!(keeps_now(&a, 0, true), None, "a pane not made yet");
    }

    #[test]
    fn a_screen_size_the_person_picked_shrinks_to_fit_its_box_and_never_grows() {
        let pc = faber_engine::screens::named("1920x1080").unwrap();
        assert!((fit_scale(&pc, 960.0, 1000.0) - 0.5).abs() < 1e-9, "the width holds it back");
        assert!((fit_scale(&pc, 1920.0, 540.0) - 0.5).abs() < 1e-9, "the height holds it back");
        let phone = faber_engine::screens::named("390x844").unwrap();
        assert_eq!(fit_scale(&phone, 1600.0, 1000.0), 1.0, "a small screen in a big box keeps its real size");
        assert_eq!(fit_scale(&pc, 1.0, 1.0), 0.05, "a tiny box still draws something");
    }

    #[test]
    fn the_agent_hears_the_sites_the_person_went_to_by_name_once_each_in_a_row_and_only_the_latest() {
        let mut visited = Vec::new();
        for url in [
            "https://www.GitHub.com/a?code=secret",
            "https://docs.github.com./b",
            "https://docs.rs/",
            "about:blank",
            "http://localhost:3000/",
            "http://192.168.0.1/",
            "http://[::1]/",
            "http://intranet/",
            "https://a1b2c3d4e5.ngrok.io/",
            "https://shop.example.co.uk/",
            "https://ignore.previous!instructions.com/",
            "https://secret-token.box.com/",
            "https://alice.corp/",
            "https://printer.local/",
            "https://secret-token.org.de/",
        ] {
            went_to(&mut visited, &url.parse().unwrap());
        }
        assert_eq!(
            visited,
            ["docs.rs", "ngrok.io", "co.uk", "box.com", "org.de"],
            "site names only, never an address, a token or a local name; the latest five"
        );
        for n in 0..9 {
            went_to(&mut visited, &format!("https://s{n}.org/").parse().unwrap());
        }
        assert_eq!(visited.len(), MAX_VISITED);
        assert_eq!(visited.last().map(String::as_str), Some("s8.org"));
    }

    #[test]
    fn boxes_that_let_go_place_no_more_and_an_older_one_cannot_take_the_pane_from_a_newer() {
        let mut b = Boxes::default();
        assert!(b.park(7, 0), "a screen just loaded makes its load the one that counts");
        assert!(b.place(7, 1) && b.place(7, 2), "boxes that only grow place the pane");
        assert!(!b.place(7, 1), "a box older than the one that has the pane cannot move it");
        assert!(b.place(7, 2), "the box that has it can place it again");
        assert!(!b.park(7, 1), "box 1 letting go while box 2 has the pane leaves it there");
        assert!(!b.place(7, 1), "and box 1 places no more");
        assert!(b.park(7, 2) && !b.place(7, 2), "the box that has it takes it off the screen, once");
        assert!(b.place(7, 3));
        assert!(b.park(7, 0) && b.place(7, 1), "a screen just loaded starts its boxes again, whatever the clock said");
    }

    #[test]
    fn a_screen_that_went_away_cannot_place_or_park_the_pane_of_the_one_that_loaded_after() {
        let mut b = Boxes::default();
        assert!(!b.place(1, 1), "before any screen said it loaded, nothing places the pane");
        assert!(b.park(1, 0) && b.place(1, 5));
        assert!(b.park(2, 0), "the next load takes over");
        assert!(!b.place(1, 9) && !b.park(1, 9), "and the old one's late calls are ignored");
        assert!(b.place(2, 1) && !b.park(1, 1), "even a box number that would fit");
        assert!(b.park(2, 1));
        assert!(!b.park(1, 0) && b.place(2, 2), "a late start of the old screen does not take the pane back");
    }

    #[test]
    fn click_and_type_need_the_whole_address_of_a_page_the_agent_read() {
        assert!(bound(None).unwrap_err().starts_with("read the page first"));
        let long = format!("https://example.org/{}", "a".repeat(5_000));
        assert_eq!(bound(Some(&long)).unwrap(), long, "the address is kept whole, not cut like the one the agent gets");
    }

    #[tokio::test]
    async fn the_person_taking_the_pane_waits_for_an_action_in_flight() {
        let acting = tokio::sync::Mutex::new(());
        let busy = acting.lock().await;
        let mut taking = std::pin::pin!(when_idle(&acting, || "placed"));
        assert!(tokio::time::timeout(Duration::from_millis(50), &mut taking).await.is_err(), "it waits while the page is being changed");
        drop(busy);
        assert_eq!(taking.await, "placed", "and goes on once the action is over");
    }

    #[test]
    fn a_page_at_a_screen_size_is_drawn_as_that_screen_and_pictured_at_one_size() {
        for size in faber_engine::screens::SIZES {
            let side = f64::from(size.width.max(size.height)) * size.scale * look_scale(&size);
            assert!((side - LOOK_SIDE).abs() < 0.01, "{} at {}x is pictured {side} px long", size.name(), size.scale);
            let (screen, touch) = metrics(&size);
            assert_eq!(
                (screen["width"].as_u64(), screen["deviceScaleFactor"].as_f64(), screen["mobile"].as_bool()),
                (Some(u64::from(size.width)), Some(size.scale), Some(size.mobile()))
            );
            assert_eq!(touch["enabled"].as_bool(), Some(size.mobile()), "touch on phones and tablets only");
        }
    }

    #[test]
    fn the_actions_are_one_function_the_page_can_call() {
        let f = act_function();
        assert!(f.starts_with("(request) => {") && f.ends_with('}'), "{f}");
    }

    #[test]
    fn a_cookie_s_host_and_path_never_move_it_to_another_site() {
        let cookie = |host: &str, path: &str| firefox::Cookie {
            name: "sid".into(),
            value: "v".into(),
            host: host.into(),
            path: path.into(),
            secure: true,
            http_only: true,
            same_site: Some("Lax"),
            expires: 1.0,
        };
        assert_eq!(cookie_param(&cookie("mail.example", "/inbox")).unwrap()["url"], "https://mail.example/inbox");
        assert_eq!(cookie_param(&cookie(".example", "/")).unwrap()["domain"], ".example");
        for (host, path) in
            [("trusted.example", "@attacker.example/"), ("trusted.example", "/?x"), ("a.example@b.example", "/"), ("trusted.example", "")]
        {
            assert!(cookie_param(&cookie(host, path)).is_none(), "{host} {path}");
        }
    }

    #[test]
    fn only_web_pages_load_and_never_faber_s_own_origins() {
        let ok = |u: &str| allowed_url(u);
        assert!(ok("https://docs.rs/tokio") && ok("http://localhost:3000/") && ok("http://127.0.0.1:8080/"));
        for bad in [
            "http://tauri.localhost/",
            "https://ipc.localhost/",
            "http://x.localhost./",
            "file:///C:/",
            "tauri://localhost/",
            "data:text/html,x",
            "not an address",
        ] {
            assert!(!ok(bad), "{bad} must not load");
        }
        assert_eq!(ok("http://localhost:5173/"), !cfg!(debug_assertions), "the dev screen is Faber's own in a dev build");
    }
}
