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
//! never while the person is looking at the pane: it is theirs then, and the agent's call waits for them to go back
//! (`WAIT_PERSON`), then hears that they used it.
//!
//! The agent acts by the words a person sees (`act.js`, run inside the page); on a page that asks for a login, a captcha
//! or a payment it may read and scroll but never click or type.

pub(crate) mod firefox;
pub(crate) mod native;

use base64::Engine as _;
use faber_engine::TabStep;
use faber_engine::browser::tabs::Tabs;
use faber_engine::browser::{Browser, Look, MAX_CUT, MAX_TEXT, MAX_TITLE, MAX_URL, Page, Request, Turn};
use faber_engine::screens::Viewport;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::{NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewBuilder, WebviewUrl};

const LABEL: &str = "site";
/// The second pane: the agent's tab while the person has another tab of the same conversation in front. It is never shown,
/// shares the first pane's private profile (the same conversation's sign-ins) and closes with it.
const BEHIND_LABEL: &str = "site-agent";
/// Unused this long, the agent's pane behind closes: a second page lives only while the agent works out of view.
const BEHIND_IDLE: Duration = Duration::from_secs(60);
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
/// How long the agent's call waits for the person to hand the browser back.
const WAIT_PERSON: Duration = Duration::from_secs(30);
/// After a click, typing or going back, how long a new page has to start loading before the same page is read again.
const NAVIGATES: Duration = Duration::from_millis(1_500);
/// The agent's actions, run inside the page: the function `act.js` exports, which the screen's checks import as it is.
const ACT: &str = include_str!("act.js");

/// The page's rounded corners and thin scrollbars, drawn in a world of Faber's own on every document (`round`).
const ROUND: &str = include_str!("round.js");
/// How long a call for the page's corners may take before the corners wait for the next placing.
const LOOK_CALL: Duration = Duration::from_secs(3);
/// The roundest corner a page takes, in logical pixels: a look's card is never rounder.
const MAX_RADIUS: f64 = 32.0;

fn act_function() -> &'static str {
    ACT.split_once("export default ").map_or(ACT, |(_, f)| f.trim_end().trim_end_matches(';'))
}
/// The picture in the reply is this share of the page's size.
const PICTURE_SCALE: f64 = 0.375;
/// The longest side of a picture at a screen size of the agent's, in the picture's pixels, whatever the screen's density.
const LOOK_SIDE: f64 = 640.0;
/// How long a page has to lay itself out again at a new screen size before it is looked at.
const RESIZED: Duration = Duration::from_millis(400);
/// The least time between two writes of where the pane's page is: a page that changes its title in a loop writes the
/// conversation's file at most this often.
const KEEP_EVERY: Duration = Duration::from_secs(2);

/// The share of a page at `size` its picture takes: the capture counts each of the screen's device pixels, so a dense phone
/// screen is scaled down further than a PC's, and every picture's longest side is `LOOK_SIDE`.
fn look_scale(size: &Viewport) -> f64 {
    (LOOK_SIDE / (f64::from(size.width.max(size.height)) * size.scale)).min(1.0)
}

/// The DevTools parameters that draw the page at `size`, as that screen would: its width, height and density, and a phone's
/// or tablet's layout rules and touch.
fn metrics(size: &Viewport) -> (Value, Value) {
    let mobile = size.mobile();
    (
        json!({ "width": size.width, "height": size.height, "deviceScaleFactor": size.scale, "mobile": mobile }),
        json!({ "enabled": mobile, "maxTouchPoints": if mobile { 5 } else { 1 } }),
    )
}

/// How much a page drawn at `size` shrinks so all of it fits a box of `width` by `height` logical pixels; never past its real size.
fn fit_scale(size: &Viewport, width: f64, height: f64) -> f64 {
    (width / f64::from(size.width)).min(height / f64::from(size.height)).clamp(0.05, 1.0)
}

/// Whether the pane may load `url`: a web page, never one of Faber's own origins (nor, in a development build, the screen's
/// own dev server).
fn allowed(url: &Url) -> bool {
    let host = url.host_str().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let dev_screen = cfg!(debug_assertions) && host == "localhost" && url.port() == Some(5173);
    matches!(url.scheme(), "http" | "https") && !host.is_empty() && !host.ends_with(".localhost") && !dev_screen
}

pub struct Pane {
    app: AppHandle,
    label: &'static str,
    /// How long the pane may sit unused out of sight before it closes.
    idle: Duration,
    /// The agent's pane behind this one (`BEHIND_LABEL`); none on that pane itself.
    behind: Option<Arc<Pane>>,
    /// On the pane behind: the first pane, whose era it is bound to (`still_behind`).
    front_of: std::sync::OnceLock<std::sync::Weak<Pane>>,
    /// Never call a webview getter or await under this lock: `on_page_load` takes it on the window's thread.
    state: Mutex<State>,
    /// The pages that started and the last of them that finished, so an open waits for its own page and never for a late
    /// end of the one before.
    loads: tokio::sync::watch::Sender<(u64, u64)>,
    /// Held from a navigation to the last read of its page, so two calls never read each other's page.
    opening: tokio::sync::Mutex<()>,
    /// The person is looking at the pane: it is theirs until they go back.
    looking: tokio::sync::watch::Sender<bool>,
    /// Held while an action of the agent changes the page: the person takes the pane only when it is over.
    acting: tokio::sync::Mutex<()>,
    /// The random name `act.js` gives the ring it draws where the agent acted, so it removes its own ring and never a node of
    /// the page (each action runs in a world of its own and cannot keep the node itself).
    mark: String,
    /// Held while the page's corners change, so two placings never add the script twice.
    rounding: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct State {
    /// The corners the screen's card asks for (radius, colour), and the ones the page has with the DevTools script that
    /// draws them; a new pane has none until it is placed.
    round: (f64, [u8; 3]),
    rounded: Option<(f64, [u8; 3])>,
    round_script: Option<String>,
    /// Counts the panes made, so a look that finishes after its pane closed never counts for the next one.
    made: u64,
    /// On the first pane: counts its closes, before the pane behind closes with it. On the pane behind: the first pane's era
    /// and the conversation it works for, so a pane behind made while the first one closed never lives on beside another
    /// conversation's pages in the profile they share.
    era: u64,
    bound: Option<(u64, String)>,
    webview: Option<Webview>,
    shown: bool,
    used: Option<Instant>,
    /// An idle close is already waiting; one at a time, however often the pane is used.
    idle_watch: bool,
    /// The person looked at the pane since the agent's last call.
    handed_back: bool,
    /// The conversation whose page the pane holds: another conversation's agent never acts on it or reads it.
    owner: Option<String>,
    /// The tab of that conversation (`browser::tabs`) whose page the pane holds, none for a page from before tabs.
    tab: Option<String>,
    /// The pane (`made`) and the conversation whose tabs its pages are kept in (`browser_seen`): set when Faber loads a page in
    /// that pane, cleared when it closes, so a late page of a pane that closed is never kept in the conversation after it.
    keeps_for: Option<(u64, String)>,
    /// From a page Faber loads until it finishes: a title or history that comes meanwhile may still be the page before's.
    keep_waits: bool,
    /// The page Faber loads is another tab's than the one before: once it finishes, the history of the tab before goes, so
    /// Back never steps into another tab.
    new_history: bool,
    /// A write of where the page is runs (`keep_soon`), and another was asked meanwhile.
    keeping: bool,
    keep_again: Option<u64>,
    /// Which of the screen's boxes has the pane.
    boxes: Boxes,
    /// Counts the times the person took the pane, so a call that ran meanwhile knows, even if they already went back.
    taken: u64,
    /// The whole address the last answer to the owner reported (never the cut one the agent gets), which an action only runs on
    /// (the page may have moved since). None until the owner has read or opened a page: nothing is clicked or typed before.
    reported: Option<String>,
    /// The sites the person went to while the pane was theirs, since the agent's last call (`went_to`).
    visited: Vec<String>,
    /// Pages Faber loaded (the agent's, or a card's) that have not finished: that many next finishes are not sites the
    /// person went to. A load that never finishes leaves one more skipped: the list can fall short, never say more.
    skip_finish: u32,
    /// The screen size of the pane's pages, chosen by the agent (`Request::Size`) or the person (`set_size`), none for the
    /// pane's own; kept until it closes. The person sees it shrunk to fit the box, the agent at its real size.
    size: Option<Viewport>,
    /// The box the page was last placed over, in the screen's logical pixels: a screen size is fitted into it.
    placed: Option<(f64, f64)>,
    /// The page is drawn at a size of the agent's, not the pane's: the person taking the pane gets the pane's own back.
    emulated: bool,
    /// The size and scale the person's view was last drawn at (`show_size`), so placing the same box again sends nothing;
    /// the agent drawing its own size, or a new page, forgets it.
    fitted: Option<(Viewport, f64)>,
    /// The sign-ins the person brought over from their own browser, per conversation (`bring_in`), and whether this pane
    /// already has its owner's. Only in this process: never written anywhere, gone when Faber quits.
    held: Vec<Held>,
    brought: bool,
    /// The sites last listed for the person to pick from, which an import may only choose among.
    listing: Option<Listing>,
}

/// Sign-ins the person brought over in one conversation: its pages get them, no other conversation's do (R195, R308).
struct Held {
    conversation: String,
    sites: Vec<String>,
    cookies: Vec<firefox::Cookie>,
}

/// A list of sites shown to the person, by a random name the screen sends back with what they picked.
struct Listing {
    ticket: String,
    profile: String,
    sites: Vec<firefox::Site>,
    at: Instant,
}

/// How long the person has to pick sites from a list before it must be read again.
const LISTING: Duration = Duration::from_secs(5 * 60);
/// The longest the pane spends setting brought-over sign-ins, so a page the person or the agent waits for is never held up.
const SETTING: Duration = Duration::from_secs(8);
/// The longest a new pane waits for its private data to be cleared before it gives up and closes.
const CLEARING: Duration = Duration::from_secs(10);
/// At most this many conversations hold sign-ins at once; the oldest import goes first.
const MAX_HELD: usize = 8;

/// The screen's boxes the pane is placed over, named by numbers that only grow while the screen lives. A box that let the
/// pane go places it no more, and a box older than the one that has it cannot move or take it away. Each load of the screen
/// has an id of its own, chosen at random: a request of any other load (a screen that went away, its late calls) is ignored.
#[derive(Default)]
struct Boxes {
    load: Option<u32>,
    /// Loads that were the current one before: a late start of one of them never takes the pane back.
    retired: Vec<u32>,
    parked: u64,
    placed: u64,
}

impl Boxes {
    /// Whether box `id` of screen load `load` may place the pane.
    fn place(&mut self, load: u32, id: u64) -> bool {
        let fresh = self.load == Some(load) && id > self.parked && id >= self.placed;
        if fresh {
            self.placed = id;
        }
        fresh
    }

    /// Whether box `id` letting go takes the pane off the screen. Box 0 is a screen just loaded: it makes `load` the one that
    /// counts and forgets every box.
    fn park(&mut self, load: u32, id: u64) -> bool {
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

/// Closes the pane when dropped while armed: an action of the agent cut off before its end.
struct Unfinished<'a> {
    pane: &'a Pane,
    armed: bool,
}

impl Drop for Unfinished<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.pane.close();
        }
    }
}

/// Runs `then` once no action of the agent is changing the page, and keeps further ones out while it runs.
async fn when_idle<T>(acting: &tokio::sync::Mutex<()>, then: impl FnOnce() -> T) -> T {
    let _idle = acting.lock().await;
    then()
}

/// The address as the pane loads it, or why it never does.
fn checked(url: &str) -> Result<Url, String> {
    let parsed: Url = url.parse().map_err(|_| "the address is not valid".to_string())?;
    if !allowed(&parsed) {
        return Err("only web pages open in Faber's browser".into());
    }
    Ok(parsed)
}

/// The full address the owner last read, or the sentence that sends the agent to read first.
fn bound(reported: Option<&str>) -> Result<&str, String> {
    reported.ok_or_else(|| "read the page first; click and type only run on a page you have read".to_string())
}

/// A cookie as the DevTools call sets it: one stored for a domain keeps it (its subdomains get it too), one stored for its
/// host alone is set by address, so it stays that host's. None for a host or path that is not one: the address is built
/// from both, and a path like `@other.example/` would move the cookie to another site.
fn cookie_param(cookie: &firefox::Cookie) -> Option<Value> {
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

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Test builds only: where the pane last closed and which of its page processes failed, the latest eight, for a probe's
/// failure text: a getter that lost its page says why.
#[cfg(feature = "test-hooks")]
static TEST_ENDINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(feature = "test-hooks")]
fn test_ending(what: String) {
    let mut endings = crate::lock(&TEST_ENDINGS);
    if endings.len() == 8 {
        endings.remove(0);
    }
    endings.push(what);
}

#[cfg(feature = "test-hooks")]
fn test_probe_error(error: impl std::fmt::Display, field: &str, before: (u64, u64, Option<&str>), now: (u64, u64, Option<&str>)) -> String {
    // The native getter ran outside the state lock; a close or replacement invalidates its captured handle.
    let endings = crate::lock(&TEST_ENDINGS).join("; ");
    if before != now {
        return format!("the test browser closed ({endings})");
    }
    let (era, made, owner) = before;
    format!("{error}; test_site_probe {field}: era {era}->{}, made {made}->{}, owner {owner:?}->{:?} ({endings})", now.0, now.1, now.2)
}

impl Pane {
    pub fn new(app: AppHandle) -> Arc<Pane> {
        let behind = Pane::made(app.clone(), BEHIND_LABEL, BEHIND_IDLE, None);
        let main = Pane::made(app, LABEL, IDLE, Some(behind.clone()));
        let _ = behind.front_of.set(Arc::downgrade(&main));
        main
    }

    fn made(app: AppHandle, label: &'static str, idle: Duration, behind: Option<Arc<Pane>>) -> Arc<Pane> {
        Arc::new(Pane {
            app,
            label,
            idle,
            behind,
            front_of: std::sync::OnceLock::new(),
            state: Mutex::default(),
            loads: tokio::sync::watch::channel((0, 0)).0,
            opening: tokio::sync::Mutex::new(()),
            looking: tokio::sync::watch::channel(false).0,
            acting: tokio::sync::Mutex::new(()),
            rounding: tokio::sync::Mutex::new(()),
            mark: format!("{:016x}", std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), 0u8)),
        })
    }

    fn webview(&self) -> Option<Webview> {
        crate::lock(&self.state).webview.clone()
    }

    /// Loads `url` in the pane for `conversation`, making the pane first if it is closed. Answers the count of started pages
    /// to wait past. A pane is made blank: before its first page it clears all the private data a closed one may still share
    /// (cookies, every site's storage; one conversation's sign-ins never reach the next one's, R308), waits until WebView2
    /// says it is gone, and takes the sign-ins `conversation` holds.
    async fn load(self: &Arc<Self>, url: &str, conversation: &str, tab: Option<&str>) -> Result<u64, String> {
        let parsed = checked(url)?;
        let before = self.loads.borrow().0;
        // A page Faber loads is the agent's or a card's, never one the person went to.
        crate::lock(&self.state).skip_finish += 1;
        self.touch();
        if let Some(webview) = self.webview() {
            {
                let mut s = crate::lock(&self.state);
                (s.keeps_for, s.keep_waits) = (Some((s.made, conversation.to_string())), true);
                if s.tab.as_deref() != tab {
                    s.new_history = true;
                }
                s.tab = tab.map(str::to_string);
            }
            if webview.navigate(parsed.clone()).is_ok() {
                return Ok(before);
            }
            // A pane left from a window that closed and came back: it is made again.
            self.close();
        }
        let window = self.app.get_window("main").ok_or("Faber's window is closed")?;
        let (loads, app, pane) = (self.loads.clone(), self.app.clone(), Arc::downgrade(self));
        let starting = self.app.clone();
        // Which pane this is (`State::made`), known once it is made: a late page of a pane that closed is never kept.
        let mine = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (titled, titled_mine, loaded_mine) = (Arc::downgrade(self), mine.clone(), mine.clone());
        let blank = Url::parse("about:blank").map_err(|e| e.to_string())?;
        // Only the pane the person sees tells the screen where its page is; the agent's pane behind is never on it.
        let front = self.behind.is_some();
        let builder = WebviewBuilder::new(self.label, WebviewUrl::External(blank))
            .incognito(true)
            .on_navigation(allowed)
            .on_new_window(|_, _| NewWindowResponse::Deny)
            .on_download(|_, _| false)
            .on_document_title_changed(move |_, _| {
                if let Some(pane) = titled.upgrade() {
                    pane.keep_soon(titled_mine.load(std::sync::atomic::Ordering::Relaxed));
                }
            })
            .on_page_load(move |_, payload| match payload.event() {
                PageLoadEvent::Started => {
                    loads.send_modify(|(started, _)| *started += 1);
                    // The thin line under the address field, while the page the person sees loads.
                    if front {
                        let _ = starting.emit_to("main", "faber:site-loading", true);
                    }
                }
                PageLoadEvent::Finished => {
                    loads.send_modify(|(started, finished)| *finished = *started);
                    // A page that finished while the person had the pane is one they went to: the agent hears its site next time.
                    if let Some(pane) = pane.upgrade() {
                        let (redraw, forget) = {
                            let mut s = crate::lock(&pane.state);
                            s.keep_waits = false;
                            let forget = std::mem::take(&mut s.new_history).then(|| s.webview.clone()).flatten();
                            if s.skip_finish > 0 {
                                s.skip_finish -= 1;
                            } else if s.shown {
                                went_to(&mut s.visited, payload.url());
                            }
                            // A new page may have left the screen size behind (another process draws it): the person's
                            // view is drawn at it again rather than trusted.
                            s.fitted = None;
                            (s.size.is_some() && *pane.looking.borrow(), forget)
                        };
                        if let Some(webview) = forget {
                            tauri::async_runtime::spawn(async move {
                                let _ = look_call(&webview, "Page.resetNavigationHistory", json!({})).await;
                            });
                        }
                        pane.keep_soon(loaded_mine.load(std::sync::atomic::Ordering::Relaxed));
                        if redraw {
                            tauri::async_runtime::spawn(async move { pane.show_size().await });
                        }
                    }
                    // The screen names the page it shows by where it is now, never by the card it was opened from.
                    if front {
                        let _ = app.emit_to("main", "faber:site-loading", false);
                        let _ = app.emit_to("main", "faber:site", payload.url().as_str());
                    }
                }
            });
        // Reuse the host's environment: WebView2 rejects differing options in a shared browser process.
        let (ready, waiting) = tokio::sync::oneshot::channel();
        self.app
            .get_webview("main")
            .ok_or("Faber's screen is closed")?
            .with_webview(move |host| {
                let _ = ready.send(builder.with_environment(host.environment()));
            })
            .map_err(|e| e.to_string())?;
        let builder = tokio::time::timeout(CALL, waiting)
            .await
            .map_err(|_| "Faber's browser environment did not answer")?
            .map_err(|_| "Faber's screen closed before the browser opened")?;
        let webview = window.add_child(builder, PARKED, SIZE).map_err(|e| e.to_string())?;
        #[cfg(feature = "test-hooks")]
        native::test_process_failed(&webview, test_ending);
        // The browser's keys pressed on the page are the screen's: the page is a native view, so they never reach it there.
        if front {
            let app = self.app.clone();
            native::keys(&webview, move |key| {
                let _ = app.emit_to("main", "faber:site-key", key);
            });
        }
        let made = {
            let mut s = crate::lock(&self.state);
            s.webview = Some(webview.clone());
            s.made += 1;
            (s.keeps_for, s.keep_waits) = (Some((s.made, conversation.to_string())), true);
            s.tab = tab.map(str::to_string);
            mine.store(s.made, std::sync::atomic::Ordering::Relaxed);
            s.made
        };
        // Stored first, then checked: a first pane that closes after this closes this webview too, and one that closed before
        // is seen here.
        if !self.still_behind() {
            self.close();
            return Err(MOVED.into());
        }
        // Until its first page, a pane whose making stops (an error, or the call that made it given up) closes.
        let making = Making { pane: self, made, done: false };
        // The pane behind shares the first pane's profile, which holds this same conversation's pages: clearing it would sign
        // the person's tab out.
        if self.behind.is_some() && !clear_new(&webview).await {
            // Never a page on data that may be another conversation's: the next open makes a new pane and tries again.
            return Err("Faber's browser could not start fresh; try again".into());
        }
        let held = {
            let mut s = crate::lock(&self.state);
            s.brought = true;
            s.held.iter().find(|h| h.conversation == conversation).map(|h| h.cookies.clone())
        };
        if let Some(cookies) = held {
            self.set_cookies(&cookies).await;
        }
        if crate::lock(&self.state).made != made || !self.still_behind() {
            return Err("the browser closed".into());
        }
        let before = self.loads.borrow().0;
        crate::lock(&self.state).keep_waits = true;
        webview.navigate(parsed).map_err(|e| e.to_string())?;
        making.finish();
        Ok(before)
    }

    /// Keeps where pane `made`'s page is now, with its title, in the tabs of the conversation it loads for, so the pane opens it
    /// again after it closed, another conversation or a restart. One write at a time, in order, at most once per `KEEP_EVERY`;
    /// the last change always gets its write.
    fn keep_soon(self: &Arc<Self>, made: u64) {
        {
            let mut s = crate::lock(&self.state);
            if s.keep_waits || keeps_now(&s.keeps_for, made, s.webview.is_some()).is_none() {
                return;
            }
            if std::mem::replace(&mut s.keeping, true) {
                s.keep_again = Some(made);
                return;
            }
        }
        let pane = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut made = made;
            loop {
                pane.keep_now(made).await;
                tokio::time::sleep(KEEP_EVERY).await;
                let mut s = crate::lock(&pane.state);
                match s.keep_again.take() {
                    Some(next) => made = next,
                    None => {
                        s.keeping = false;
                        break;
                    }
                }
            }
        });
    }

    /// One write: the address and title are read together from the page's history, so they always belong to the same page,
    /// and written only while the same pane still keeps for the same conversation. Only web pages are kept, off the window's
    /// thread.
    async fn keep_now(&self, made: u64) {
        let _turn = self.opening.lock().await;
        let held = {
            let s = crate::lock(&self.state);
            // A page Faber loads has not started: the history still shows the page before, which is another tab's.
            if s.keep_waits {
                return;
            }
            keeps_now(&s.keeps_for, made, s.webview.is_some()).zip(s.webview.clone()).map(|(c, w)| (c, w, s.tab.clone()))
        };
        let Some((conversation, webview, tab)) = held else { return };
        let Ok(history) = look_call(&webview, "Page.getNavigationHistory", json!({})).await else { return };
        let entry = usize::try_from(history["currentIndex"].as_i64().unwrap_or(-1)).ok().and_then(|at| history["entries"].get(at));
        let Some(url) = entry.and_then(|e| e["url"].as_str()).and_then(|u| Url::parse(u).ok()).filter(allowed) else { return };
        // A page without a title shows its address as one; the tab then keeps none.
        let title = entry.and_then(|e| e["title"].as_str()).filter(|t| !t.is_empty() && *t != url.as_str()).map(str::to_string);
        {
            let s = crate::lock(&self.state);
            if keeps_now(&s.keeps_for, made, s.webview.is_some()).as_ref() != Some(&conversation) || s.tab != tab {
                return;
            }
        }
        self.keep_page(conversation, tab, url.to_string(), Some(title.unwrap_or_default())).await;
    }

    async fn keep_page(&self, conversation: String, tab: Option<String>, url: String, title: Option<String>) {
        let app = self.app.clone();
        let changed = tauri::async_runtime::spawn_blocking(move || {
            let faber = app.try_state::<crate::Faber>()?;
            faber.engine.browser_seen(&conversation, tab.as_deref(), &url, title.as_deref()).then_some(conversation)
        })
        .await;
        // The strip shows the new title or address.
        if let Ok(Some(conversation)) = changed {
            let _ = self.app.emit_to("main", "faber:site-tabs", conversation);
        }
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
                    let left = s.used.map_or(Duration::ZERO, |u| pane.idle.saturating_sub(u.elapsed()));
                    if left.is_zero() && !s.shown {
                        s.idle_watch = false;
                        s.visited.clear();
                        // The next pane is a new page: it gets its corners again, and its screen size drawn again.
                        (s.rounded, s.round_script) = (None, None);
                        (s.emulated, s.fitted) = (false, None);
                        s.brought = false;
                        (s.keeps_for, s.tab) = (None, None);
                        s.era += 1;
                        s.bound = None;
                        #[cfg(feature = "test-hooks")]
                        test_ending("closed when idle".into());
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
                        // The pane behind never outlives this one: a new pane clears the profile they share.
                        if let Some(behind) = &pane.behind {
                            behind.close();
                        }
                        break;
                    }
                }
            }
        });
    }

    #[track_caller]
    pub fn close(&self) {
        #[cfg(feature = "test-hooks")]
        test_ending(format!("closed at {}", std::panic::Location::caller()));
        let webview = {
            let mut s = crate::lock(&self.state);
            s.shown = false;
            s.reported = None;
            s.visited.clear();
            (s.size, s.emulated, s.fitted) = (None, false, None);
            (s.rounded, s.round_script) = (None, None);
            s.brought = false;
            (s.keeps_for, s.tab) = (None, None);
            s.era += 1;
            s.bound = None;
            s.webview.take()
        };
        self.looking.send_replace(false);
        if let Some(webview) = webview {
            let _ = webview.close();
        }
        if let Some(behind) = &self.behind {
            behind.close();
        }
    }

    /// Shows the pane over the screen's own rectangle for it, in the page's logical pixels. `box_id` names the screen's box,
    /// newer ones larger; a box that already let the pane go places it no more.
    /// The person taking the pane waits for an action of the agent in flight (bounded by `CALL`), so the page never changes
    /// under their hands and the call finds, when it goes on, that they took it.
    pub async fn place(self: &Arc<Self>, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> bool {
        if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) || width < 1.0 || height < 1.0 {
            return false;
        }
        let placed = when_idle(&self.acting, || {
            // The check and the move are one step under the lock, so a park that comes between cannot be undone by this box.
            let mut s = crate::lock(&self.state);
            let Some(webview) = s.webview.clone() else { return false };
            if !s.boxes.place(load, box_id) {
                return false;
            }
            s.shown = true;
            s.handed_back = true;
            s.taken += 1;
            s.placed = Some((width, height));
            self.looking.send_replace(true);
            webview.set_size(LogicalSize { width, height }).is_ok() && webview.set_position(LogicalPosition { x, y }).is_ok()
        })
        .await;
        // The person sees the page at the pane's screen size shrunk into the box, or at the box's own size when none is chosen;
        // the agent's next call draws it at the real size again.
        if placed {
            self.show_size().await;
        }
        // The corners are a look: the page shows at once, and a page slow to take them never holds the placing up.
        if placed {
            let pane = self.clone();
            tauri::async_runtime::spawn(async move { pane.round().await });
        }
        self.touch();
        placed
    }

    /// The screen size of the pane's pages; none is the pane's own.
    pub fn size(&self) -> Option<Viewport> {
        crate::lock(&self.state).size
    }

    /// The person picked the screen size the pane's pages are drawn at (none: the pane's own). The agent's next calls use it too.
    pub async fn set_size(self: &Arc<Self>, size: Option<Viewport>) {
        crate::lock(&self.state).size = size;
        if *self.looking.borrow() {
            self.show_size().await;
        }
        self.touch();
    }

    /// Draws the page for the person: at the pane's screen size, scaled down so all of it fits the box (never up), or at the
    /// box's own size when there is none. Under `acting`, so a check at several sizes never changes the size under their hands.
    async fn show_size(&self) {
        let _acting = self.acting.lock().await;
        let (size, placed, emulated, fitted) = {
            let s = crate::lock(&self.state);
            (s.size, s.placed, s.emulated, s.fitted)
        };
        match (size, placed) {
            (Some(size), Some((width, height))) => {
                let scale = fit_scale(&size, width, height);
                if emulated && fitted == Some((size, scale)) {
                    return;
                }
                // Touch changes only with the size: a new scale of the same size (a drag of the column) sends one call.
                let same_size = emulated && fitted.is_some_and(|(drawn, _)| drawn == size);
                let (mut screen, touch) = metrics(&size);
                screen["scale"] = json!(scale);
                {
                    let mut s = crate::lock(&self.state);
                    s.emulated = true;
                    s.fitted = None;
                }
                if self.call("Emulation.setDeviceMetricsOverride", screen).await.is_ok() {
                    if !same_size {
                        let _ = self.call("Emulation.setTouchEmulationEnabled", touch).await;
                    }
                    crate::lock(&self.state).fitted = Some((size, scale));
                }
            }
            _ if emulated && self.call("Emulation.clearDeviceMetricsOverride", json!({})).await.is_ok() => {
                let _ = self.call("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })).await;
                let mut s = crate::lock(&self.state);
                s.emulated = false;
                s.fitted = None;
            }
            _ => {}
        }
    }

    /// The card around the pane changed its corners or colour (a look, a corner setting): the page follows.
    pub async fn set_round(self: &Arc<Self>, radius: f64, color: [u8; 3]) {
        let radius = if radius.is_finite() { radius.clamp(0.0, MAX_RADIUS) } else { 0.0 };
        crate::lock(&self.state).round = (radius, color);
        self.round().await;
    }

    /// Draws the corners the screen asked for over the page (`round.js`), on its document and every next one, once per
    /// change; the old script goes first. A call that fails leaves the page as it was, and the next placing tries again.
    /// Every call goes to the pane the look began on: one made meanwhile never gets a script it does not know of.
    async fn round(&self) {
        let _one = self.rounding.lock().await;
        let (wanted, old, made, webview) = {
            let s = crate::lock(&self.state);
            let Some(webview) = s.webview.clone() else { return };
            if s.rounded == Some(s.round) {
                return;
            }
            (s.round, s.round_script.clone(), s.made, webview)
        };
        let same = |s: &State| s.made == made && s.webview.is_some();
        if let Some(identifier) = old {
            let _ = look_call(&webview, "Page.removeScriptToEvaluateOnNewDocument", json!({ "identifier": identifier })).await;
            let mut s = crate::lock(&self.state);
            if !same(&s) {
                return;
            }
            s.round_script = None;
        }
        let (radius, [r, g, b]) = wanted;
        let source = ROUND
            .replace("__ID__", &format!("faber-round-{}", self.mark))
            .replace("__R__", &format!("{radius}"))
            .replace("__C__", &format!("{r}, {g}, {b}"));
        let added = look_call(
            &webview,
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": source, "worldName": "faber-round", "runImmediately": true }),
        )
        .await;
        if let Ok(identifier) = added.as_ref().map(|a| a["identifier"].as_str().map(str::to_string)) {
            let mut s = crate::lock(&self.state);
            // A pane made since has none of this: it gets its own.
            if same(&s) {
                (s.rounded, s.round_script) = (Some(wanted), identifier);
            }
        }
    }

    /// Takes the pane off the screen, back at the size the agent's pages load at, for the screen's box `box_id` and every
    /// older one of screen load `load`, unless a newer box has it. Box 0 says the screen was just loaded and no box is left.
    pub fn park(self: &Arc<Self>, load: u32, box_id: u64) {
        {
            let mut s = crate::lock(&self.state);
            if !s.boxes.park(load, box_id) {
                return;
            }
            s.shown = false;
            self.looking.send_replace(false);
            let Some(webview) = &s.webview else { return };
            let _ = webview.set_position(PARKED);
            let _ = webview.set_size(SIZE);
        }
        self.touch();
    }

    /// Loads a page the person asked for from a conversation's reply. The page is that conversation's from then on.
    pub async fn show_page(self: &Arc<Self>, conversation: &str, url: &str) -> bool {
        // An address the pane never loads leaves another conversation's page and tabs as they were.
        if checked(url).is_err() {
            return false;
        }
        let tab = self.tab_step(conversation, TabStep::Show(url.to_string())).await.and_then(|(_, tab)| tab);
        self.show_in(conversation, tab, url).await
    }

    /// The person loads `url` in the tab in front (an address they typed): the tab keeps it.
    pub async fn go(self: &Arc<Self>, conversation: &str, url: &str) -> bool {
        // An address too long for its tab to keep would load and then reopen on the page before.
        if checked(url).is_err() || url.chars().count() > faber_engine::browser::MAX_URL {
            return false;
        }
        let front = self.tabs_of(conversation).await.and_then(|tabs| tabs.front);
        let tab = match front {
            Some(front) => Some(front),
            None => self.tab_step(conversation, TabStep::Show(url.to_string())).await.and_then(|(_, tab)| tab),
        };
        self.show_in(conversation, tab, url).await
    }

    /// The person did `step` to the conversation's tabs: the tab in front then shows, its page loaded again from its record
    /// when the pane holds another (a blank one shows only the address field). A tab the agent's pane behind held comes to
    /// this one, which loads it, and the pane behind closes. Answers the tabs and the tab the step concerns.
    pub async fn tab(self: &Arc<Self>, conversation: &str, step: TabStep) -> Option<(Tabs, Option<String>)> {
        let answer = self.tab_step(conversation, step).await?;
        if let Some(behind) = self.behind.as_ref() {
            let held = crate::lock(&behind.state).tab.clone();
            if held.is_some_and(|held| behind.holds(conversation, &held) && answer.0.get(&held).is_none()) {
                behind.close();
            }
        }
        let front = answer.0.front_tab().cloned();
        if let Some(front) = front.filter(|t| !t.url.is_empty() && !self.holds(conversation, &t.id)) {
            self.show_in(conversation, Some(front.id.clone()), &front.url).await;
        }
        Some(answer)
    }

    /// Whether Back and Forward can go anywhere on the page the person sees of `conversation`: (back, forward).
    pub async fn history(&self, conversation: &str) -> (bool, bool) {
        let webview = {
            let s = crate::lock(&self.state);
            s.webview.clone().filter(|_| s.owner.as_deref() == Some(conversation))
        };
        let Some(webview) = webview else { return (false, false) };
        let Ok(history) = look_call(&webview, "Page.getNavigationHistory", json!({})).await else { return (false, false) };
        let at = history["currentIndex"].as_i64().unwrap_or(0);
        let count = history["entries"].as_array().map_or(0, |e| e.len() as i64);
        (at > 0, at + 1 < count)
    }

    /// Whether the pane holds tab `tab` of `conversation` now.
    fn holds(&self, conversation: &str, tab: &str) -> bool {
        let s = crate::lock(&self.state);
        s.webview.is_some() && s.owner.as_deref() == Some(conversation) && s.tab.as_deref() == Some(tab)
    }

    /// A conversation's tabs, read off the window's thread.
    async fn tabs_of(&self, conversation: &str) -> Option<Tabs> {
        let (app, conversation) = (self.app.clone(), conversation.to_string());
        tauri::async_runtime::spawn_blocking(move || app.try_state::<crate::Faber>()?.engine.browser_tabs(&conversation)).await.ok()?
    }

    /// Does `step` to a conversation's tabs in the engine, off the window's thread; the strip hears of it.
    async fn tab_step(&self, conversation: &str, step: TabStep) -> Option<(Tabs, Option<String>)> {
        let (app, id) = (self.app.clone(), conversation.to_string());
        let answer =
            tauri::async_runtime::spawn_blocking(move || app.try_state::<crate::Faber>()?.engine.browser_tab(&id, step)).await.ok()?;
        let _ = self.app.emit_to("main", "faber:site-tabs", conversation);
        answer
    }

    /// Loads `url` in tab `tab` for the person: the page is that conversation's from then on.
    async fn show_in(self: &Arc<Self>, conversation: &str, tab: Option<String>, url: &str) -> bool {
        // The page the agent left is usually the one the card names: it shows at once, even while a call of the agent's runs,
        // which then finds the person looking and stops before it acts.
        let (owner, held) = {
            let s = crate::lock(&self.state);
            (s.owner.clone(), s.tab.clone())
        };
        let same = self.webview().and_then(|w| w.url().ok()).is_some_and(|now| now.as_str() == url);
        if same && owner.as_deref() == Some(conversation) && (tab.is_none() || held == tab) {
            return true;
        }
        if let Some(behind) = self.behind.as_ref().filter(|b| tab.as_deref().is_some_and(|t| b.holds(conversation, t))) {
            behind.close();
        }
        let _turn = self.opening.lock().await;
        self.fresh_for(conversation).await;
        // A pane already there takes the conversation's sign-ins before the page asks for anything.
        self.bring_in(conversation).await;
        let loaded = self.load(url, conversation, tab.as_deref()).await;
        #[cfg(feature = "test-hooks")]
        if let Err(error) = &loaded {
            eprintln!("test browser load failed: {error}");
        }
        let shown = loaded.is_ok();
        if shown {
            {
                let mut s = crate::lock(&self.state);
                if s.owner.as_deref() != Some(conversation) {
                    s.visited.clear();
                }
                s.owner = Some(conversation.to_string());
                s.reported = None;
            }
            // Remember an accepted address before another tab or a quick reopen can replace the live page.
            self.keep_page(conversation.to_string(), tab, url.to_string(), None).await;
        }
        shown
    }

    /// Puts the sign-ins the person brought over in `conversation` into the pane, once per pane and only while its page is
    /// that conversation's. Answers how many cookies the pane took.
    async fn bring_in(&self, conversation: &str) -> usize {
        let cookies = {
            let mut s = crate::lock(&self.state);
            if s.brought || s.webview.is_none() || s.owner.as_deref() != Some(conversation) {
                return 0;
            }
            let Some(held) = s.held.iter().find(|h| h.conversation == conversation) else { return 0 };
            let cookies = held.cookies.clone();
            s.brought = true;
            cookies
        };
        self.set_cookies(&cookies).await
    }

    /// Sets `cookies` in the pane on view, a batch at a time, for at most `SETTING` in all, every call included: a call
    /// still running then is given up, and the pane keeps what it took. Answers how many it took.
    async fn set_cookies(&self, cookies: &[firefox::Cookie]) -> usize {
        let Some(webview) = self.webview() else { return 0 };
        let deadline = Instant::now() + SETTING;
        let send = |method: &'static str, params: Value| {
            let left = deadline.saturating_duration_since(Instant::now());
            let webview = webview.clone();
            async move {
                let answer = tokio::time::timeout(left, native::devtools(&webview, method, params.to_string())).await;
                match answer {
                    Ok(Ok(Ok(json))) => serde_json::from_str::<Value>(&json).ok(),
                    _ => None,
                }
            }
        };
        let mut set = 0;
        for batch in cookies.chunks(200) {
            if Instant::now() >= deadline {
                break;
            }
            let params: Vec<Value> = batch.iter().filter_map(cookie_param).collect();
            if send("Network.setCookies", json!({ "cookies": params })).await.is_some() {
                set += params.len();
                continue;
            }
            // The browser refuses a whole batch for one cookie it does not like: the rest go one by one.
            for one in params {
                if Instant::now() >= deadline {
                    break;
                }
                if send("Network.setCookie", one).await.is_some_and(|a| a["success"].as_bool() != Some(false)) {
                    set += 1;
                }
            }
        }
        set
    }

    /// The sites of a Firefox profile that hold cookies, for the person to pick from: names, counts and kinds, never a
    /// cookie. Kept under a new ticket, which the import has to name, for `LISTING`.
    pub async fn list_sites(&self, profile: String) -> Result<(String, Vec<firefox::Site>), &'static str> {
        let (profile, sites) = tokio::task::spawn_blocking(move || {
            let profile = firefox::profiles().into_iter().find(|p| p.id == profile).ok_or("error.cookies.missing")?;
            firefox::sites(&profile, unix_now()).map(|sites| (profile.id, sites))
        })
        .await
        .map_err(|_| "error.cookies.locked")??;
        let ticket = uuid::Uuid::new_v4().to_string();
        let listing = Listing { ticket: ticket.clone(), profile, sites: sites.clone(), at: Instant::now() };
        crate::lock(&self.state).listing = Some(listing);
        Ok((ticket, sites))
    }

    /// Brings the person's sign-ins on the `picked` sites of listing `ticket` into `conversation`'s pages: the page on view
    /// now loads again with them, and every page the conversation opens later gets them, until Faber quits or the person
    /// forgets them. No other conversation's pages ever get them. Answers how many sites came over.
    pub async fn import(&self, conversation: &str, ticket: &str, picked: &[String]) -> Result<usize, &'static str> {
        // The list stays until an import from it is done, so one that failed (Firefox still open) can be tried again.
        let (profile, sites, hosts) = {
            let s = crate::lock(&self.state);
            let listing = s.listing.as_ref().filter(|l| l.ticket == ticket && l.at.elapsed() < LISTING).ok_or("error.cookies.stale")?;
            let sites: Vec<&firefox::Site> = listing.sites.iter().filter(|site| picked.contains(&site.name)).collect();
            if sites.is_empty() {
                return Err("error.cookies.stale");
            }
            let hosts: Vec<String> = sites.iter().flat_map(|site| site.hosts.iter().cloned()).collect();
            (listing.profile.clone(), sites.iter().map(|site| site.name.clone()).collect::<Vec<_>>(), hosts)
        };
        let cookies = tokio::task::spawn_blocking(move || {
            let profile = firefox::profiles().into_iter().find(|p| p.id == profile).ok_or("error.cookies.missing")?;
            firefox::cookies(&profile, &hosts, unix_now())
        })
        .await
        .map_err(|_| "error.cookies.locked")??;
        if cookies.is_empty() {
            return Err("error.cookies.none");
        }
        // After the agent's call in flight, so its page never changes under it.
        let _turn = self.opening.lock().await;
        let count = sites.len();
        {
            let mut s = crate::lock(&self.state);
            let mut held = match s.held.iter().position(|h| h.conversation == conversation) {
                Some(at) => s.held.remove(at),
                None => Held { conversation: conversation.to_string(), sites: Vec::new(), cookies: Vec::new() },
            };
            held.cookies.retain(|old| !cookies.iter().any(|new| (&new.host, &new.name, &new.path) == (&old.host, &old.name, &old.path)));
            held.cookies.extend(cookies);
            held.sites.retain(|site| !sites.contains(site));
            held.sites.extend(sites);
            s.held.push(held);
            if s.held.len() > MAX_HELD {
                s.held.remove(0);
            }
            // Another conversation's live page keeps what it has; this one's takes the new ones.
            if s.owner.as_deref() == Some(conversation) {
                s.brought = false;
            }
            if s.listing.as_ref().is_some_and(|l| l.ticket == ticket) {
                s.listing = None;
            }
        }
        if self.bring_in(conversation).await > 0 {
            let _ = self.call("Page.reload", json!({})).await;
        }
        Ok(count)
    }

    /// The sites whose sign-ins `conversation` holds, for the screen to name.
    pub fn held_sites(&self, conversation: &str) -> Vec<String> {
        crate::lock(&self.state).held.iter().find(|h| h.conversation == conversation).map(|h| h.sites.clone()).unwrap_or_default()
    }

    /// Forgets the sign-ins brought over in `conversation`. Its page on view is signed out of everything, not only of the
    /// cookies that came over: sites renew their sessions with new cookies and keep more in their storage.
    pub async fn forget(&self, conversation: &str) {
        let _turn = self.opening.lock().await;
        let webview = {
            let mut s = crate::lock(&self.state);
            s.held.retain(|h| h.conversation != conversation);
            if s.owner.as_deref() != Some(conversation) {
                return;
            }
            s.webview.clone()
        };
        let Some(webview) = webview else { return };
        let cleared = tokio::time::timeout(CLEARING, native::clear_all(&webview)).await;
        if matches!(cleared, Ok(Ok(Ok(())))) {
            let _ = self.call("Page.reload", json!({})).await;
        } else {
            // Never a page still signed in after the person said to forget: it closes, and the next one starts clean.
            self.close();
        }
    }

    /// "Clear this conversation's browsing data", once the person said yes: its tabs, closed tabs and sign-ins go, and a pane
    /// holding its page closes with everything the page kept, so the next page starts clean.
    pub async fn clear(&self, conversation: &str) {
        // Held until the pane closed, so no page of the conversation loads while its data goes.
        let _turn = self.opening.lock().await;
        let webview = {
            let mut s = crate::lock(&self.state);
            s.held.retain(|h| h.conversation != conversation);
            s.webview.clone().filter(|_| s.owner.as_deref() == Some(conversation))
        };
        if let Some(webview) = webview {
            let _ = tokio::time::timeout(CLEARING, native::clear_all(&webview)).await;
            self.close();
        }
        self.tab_step(conversation, TabStep::Clear).await;
    }

    /// A page that passes to another conversation gets a pane of its own: one conversation's history (and anything the
    /// person signed in to there) is never a Back away from another's agent. The old pane's private data goes now, while
    /// its page still runs, and all of it again before the new pane's first page (`load`), since private panes made one
    /// after another may share it.
    async fn fresh_for(&self, conversation: &str) {
        let webview = {
            let s = crate::lock(&self.state);
            if !s.owner.as_deref().is_some_and(|owner| owner != conversation) {
                return;
            }
            s.webview.clone()
        };
        if let Some(webview) = webview.filter(|_| self.behind.is_some()) {
            let _ = tokio::time::timeout(CLEARING, native::clear_all(&webview)).await;
        }
        self.close();
    }

    /// The person goes back, forward or loads the page again from the pane's head, only on the page of the conversation
    /// they look at. The history holds only pages the pane already let in, one conversation's own since `fresh_for`; a step
    /// that changes the document passes `allowed` again (one inside the same document cannot change the site), and the
    /// steps are the person's own going somewhere, so the agent hears of them like any other page they went to.
    pub async fn step(self: &Arc<Self>, conversation: &str, step: &str) -> bool {
        let _turn = self.opening.lock().await;
        {
            // Checked once the turn is ours: another conversation may have opened its page while this step waited, and a
            // pane out of sight (a step's picture stands in for it) is not the person's to move.
            let s = crate::lock(&self.state);
            if s.owner.as_deref() != Some(conversation) || !s.shown {
                return false;
            }
        }
        self.touch();
        match step {
            "reload" => self.call("Page.reload", json!({})).await.is_ok(),
            "back" | "forward" => {
                let Ok(history) = self.call("Page.getNavigationHistory", json!({})).await else {
                    return false;
                };
                let at = history["currentIndex"].as_i64().unwrap_or(-1) + if step == "back" { -1 } else { 1 };
                let entry = usize::try_from(at).ok().and_then(|at| history["entries"].get(at));
                let Some(id) = entry.and_then(|e| e["id"].as_i64()) else {
                    return false;
                };
                self.call("Page.navigateToHistoryEntry", json!({ "entryId": id })).await.is_ok()
            }
            _ => false,
        }
    }

    /// Test builds only: observe the child through safe native getters and fixed private DevTools requests.
    #[cfg(feature = "test-hooks")]
    pub async fn test_probe(&self, capture: bool) -> Result<Value, String> {
        let (webview, shown, owner, era, made) = {
            let s = crate::lock(&self.state);
            (s.webview.clone(), s.shown, s.owner.clone(), s.era, s.made)
        };
        let Some(webview) = webview else { return Ok(json!({ "shown": false })) };
        // These query the actual native child, not the rectangle the screen last requested.
        let position = webview.position().map_err(|e| {
            let now = crate::lock(&self.state);
            test_probe_error(e, "position", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
        })?;
        let size = webview.size().map_err(|e| {
            let now = crate::lock(&self.state);
            test_probe_error(e, "size", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
        })?;
        let scale = webview.window().scale_factor().map_err(|e| {
            let now = crate::lock(&self.state);
            test_probe_error(e, "scale_factor", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
        })?;
        let page = self.call("Runtime.evaluate", json!({
            "expression": "({url:location.href,title:document.title,text:(document.body?.innerText||'').slice(0,2000),width:innerWidth,height:innerHeight})",
            "returnByValue": true
        })).await?;
        let history = self.call("Page.getNavigationHistory", json!({})).await?;
        let private = tokio::time::timeout(CALL, native::test_private(&webview))
            .await
            .map_err(|_| "the test private-mode query timed out")?
            .map_err(|_| "the test browser closed")?
            .map_err(|e| {
                let now = crate::lock(&self.state);
                test_probe_error(e, "private", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
            })?;
        let mut answer = json!({
            "shown": shown, "owner": owner, "url": webview.url().map_err(|e| {
                let now = crate::lock(&self.state);
                test_probe_error(e, "url", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
            })?.as_str(),
            "x": position.x, "y": position.y, "width": size.width, "height": size.height,
            "scale": scale, "page": page["result"]["value"], "history": history, "private": private,
            "hostMinimized": webview.window().is_minimized().map_err(|e| {
                let now = crate::lock(&self.state);
                test_probe_error(e, "is_minimized", (era, made, owner.as_deref()), (now.era, now.made, now.owner.as_deref()))
            })?
        });
        if capture {
            let shot = self.call("Page.captureScreenshot", json!({"format":"png", "captureBeyondViewport":false})).await?;
            answer["picture"] = shot["data"].clone();
        }
        Ok(answer)
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

    /// A world of Faber's own inside the page: it shares the page's document but none of its scripts, so a page cannot change
    /// what Faber's code calls or what it answers.
    async fn world(&self) -> Result<i64, String> {
        let tree = self.call("Page.getFrameTree", json!({})).await?;
        let frame = tree["frameTree"]["frame"]["id"].as_str().ok_or("the page has no frame")?.to_string();
        let world = self.call("Page.createIsolatedWorld", json!({ "frameId": frame, "worldName": "faber" })).await?;
        world["executionContextId"].as_i64().ok_or_else(|| "the page has no world for Faber".to_string())
    }

    /// Evaluates `expression` in the world `context`.
    ///
    /// `taken` is how many times the person had taken the pane when the agent's call began: if that moved, the call stops just
    /// before the page runs anything. An evaluation that `changes` the page holds `acting` from that look to its end, so the
    /// person's taking waits for it.
    async fn evaluate_in(&self, context: i64, expression: &str, taken: u64, changes: bool) -> Result<Value, String> {
        let _acting = if changes { Some(self.acting.lock().await) } else { None };
        self.person_took_it(taken)?;
        // A call the engine gave up on drops this mid-evaluation: the pane closes before `acting` lets the person in, so an
        // action still running in the page never meets the person's hands.
        let mut unfinished = Unfinished { pane: self, armed: changes };
        let done = self.call("Runtime.evaluate", json!({ "expression": expression, "returnByValue": true, "contextId": context })).await;
        unfinished.armed = false;
        Ok(done?["result"]["value"].clone())
    }

    async fn evaluate(&self, expression: &str, taken: u64, changes: bool) -> Result<Value, String> {
        let context = self.world().await?;
        self.evaluate_in(context, expression, taken, changes).await
    }

    /// Takes the browser for the agent: waits until the person is not looking at it, at most `WAIT_PERSON`, without holding
    /// it meanwhile, so the person's own clicks on other cards never wait for the agent.
    async fn take(&self) -> Result<tokio::sync::MutexGuard<'_, ()>, String> {
        let deadline = tokio::time::Instant::now() + WAIT_PERSON;
        let mut looking = self.looking.subscribe();
        loop {
            if tokio::time::timeout_at(deadline, looking.wait_for(|on| !*on)).await.is_err() {
                return Err("the person is using Faber's browser; ask them, or try again once they go back".into());
            }
            let turn = self.opening.lock().await;
            if !*self.looking.borrow() {
                return Ok(turn);
            }
        }
    }

    /// The person took the browser while the agent's call ran, and may have gone back already: the call stops there, with no
    /// picture of what they did.
    fn person_took_it(&self, taken: u64) -> Result<(), String> {
        if *self.looking.borrow() || crate::lock(&self.state).taken != taken {
            return Err("the person took over Faber's browser; ask them, or try again once they go back".into());
        }
        Ok(())
    }

    /// Runs `act.js` on the page and answers what it said: whether it did it, and what is the person's turn there. Its
    /// reasons are codes; the words are Faber's, and the page's own words in them are quoted.
    async fn run_act(&self, mut request: Value, taken: u64) -> Result<(bool, Option<Turn>), String> {
        request["mark"] = json!(self.mark);
        let changes = matches!(request["kind"].as_str(), Some("click" | "type" | "back"));
        let value = self.evaluate(&format!("({})({request})", act_function()), taken, changes).await?;
        if !value.is_object() {
            return Err("the page did not let Faber act on it".into());
        }
        let turn = value["turn"].as_str().and_then(Turn::from_words);
        if value["ok"].as_bool() == Some(true) || turn.is_some() {
            return Ok((value["ok"].as_bool() == Some(true), turn));
        }
        let quoted = |list: &Value| -> String {
            let words = list.as_array().into_iter().flatten().filter_map(Value::as_str).take(5);
            words
                .map(|w| format!("\"{}\"", w.chars().filter(|c| !c.is_control()).take(60).collect::<String>()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        Err(match value["why"].as_str() {
            Some("click") => "nothing on view to click reads so".into(),
            Some("field") => "no field on view reads so".into(),
            Some("nofield") => "the page has no field on view to type in".into(),
            Some("submit") => "say which field to type in when you also press Enter".into(),
            Some("moved") => format!("the page changed to {} since you read it; read it again first", quoted(&value["seen"])),
            Some("many") => format!("several different things read so ({}); say the whole words of one", quoted(&value["seen"])),
            _ => "the page did not let Faber act on it".into(),
        })
    }

    /// Draws the page at `size` (none: the pane's own), unless the person took the pane since the call began: under `acting`,
    /// so their taking it waits for the change and then gets their own view back (`place`).
    async fn emulate(&self, size: Option<Viewport>, taken: u64) -> Result<(), String> {
        let _acting = self.acting.lock().await;
        self.person_took_it(taken)?;
        // The agent's drawing replaces the person's fitted one: their next look draws theirs again.
        crate::lock(&self.state).fitted = None;
        let Some(size) = size else {
            if crate::lock(&self.state).emulated {
                self.call("Emulation.clearDeviceMetricsOverride", json!({})).await?;
                self.call("Emulation.setTouchEmulationEnabled", json!({ "enabled": false })).await?;
                crate::lock(&self.state).emulated = false;
            }
            return Ok(());
        };
        let (screen, touch) = metrics(&size);
        crate::lock(&self.state).emulated = true;
        self.call("Emulation.setDeviceMetricsOverride", screen).await?;
        self.call("Emulation.setTouchEmulationEnabled", touch).await?;
        Ok(())
    }

    /// The page at each of `sizes`, a picture and whether it fits each, then back at the size it had. A page that is the
    /// person's turn stops the check there, with no picture.
    async fn looks(&self, sizes: &[Viewport], taken: u64) -> Result<Vec<Look>, String> {
        let looked = self.each_size(sizes, taken).await;
        // Back at the size it had, whether or not every size was looked at; one the person took gets the pane's own (`place`).
        let size = crate::lock(&self.state).size;
        let back = self.emulate(size, taken).await;
        let looks = looked?;
        back?;
        tokio::time::sleep(RESIZED).await;
        Ok(looks)
    }

    async fn each_size(&self, sizes: &[Viewport], taken: u64) -> Result<Vec<Look>, String> {
        let mut looks = Vec::new();
        for size in sizes {
            self.emulate(Some(*size), taken).await?;
            tokio::time::sleep(RESIZED).await;
            let expression = format!(
                "(() => {{ const act = {}; const turn = act({{ kind: 'read', watch: true }}).turn; const fits = act({{ kind: 'fits' }}); \
                 return {{ turn: fits.turn || turn, wider: fits.wider, cut: fits.cut, x: scrollX, y: scrollY }}; }})()",
                act_function()
            );
            let context = self.world().await?;
            let value = self.evaluate_in(context, &expression, taken, false).await?;
            let turn = value["turn"].as_str().and_then(Turn::from_words);
            let wider = value["wider"].as_u64().unwrap_or(0).min(100_000) as u32;
            let cut: Vec<String> =
                value["cut"].as_array().into_iter().flatten().filter_map(Value::as_str).take(MAX_CUT).map(str::to_string).collect();
            let (picture, turn) = match turn {
                Some(turn) => (None, Some(turn)),
                None => self.picture(context, &value, Some(size), taken).await?,
            };
            looks.push(Look { size: *size, picture, wider, cut, turn });
            if turn.is_some() {
                break;
            }
        }
        Ok(looks)
    }

    async fn act_page(self: Arc<Self>, conversation: String, tab: Option<String>, request: Request) -> Result<Page, String> {
        let _turn = self.take().await?;
        // The person may have brought another tab to the front, or closed the pane, while this call waited for its turn.
        if !self.still_behind() {
            self.close();
            return Err(MOVED.into());
        }
        if let Some(tab) = tab.as_deref() {
            let front = if self.behind.is_some() { self.tabs_of(&conversation).await.and_then(|t| t.front) } else { None };
            let s = crate::lock(&self.state);
            let moved = match request {
                Request::Open(_) => {
                    self.behind.is_some()
                        && goes_behind(s.webview.is_some(), s.owner.as_deref(), s.tab.as_deref(), &conversation, tab, front.as_deref())
                }
                _ => !(s.webview.is_some() && s.owner.as_deref() == Some(conversation.as_str()) && s.tab.as_deref() == Some(tab)),
            };
            if moved {
                return Err(MOVED.into());
            }
        }
        let (mine, taken, reported) = {
            let s = crate::lock(&self.state);
            // What the person did on another conversation's page is never this agent's to hear.
            (s.owner.as_deref() == Some(conversation.as_str()), s.taken, s.reported.clone())
        };
        let mut loads = self.loads.subscribe();
        let mut before = self.loads.borrow().0;
        let action = match &request {
            Request::Open(url) => {
                checked(url)?;
                // Under `acting`, so the person cannot take the pane between this look and the load: a page they took is never
                // closed or moved under their hands.
                let _acting = self.acting.lock().await;
                self.person_took_it(taken)?;
                self.fresh_for(&conversation).await;
                self.bring_in(&conversation).await;
                before = self.load(url, &conversation, tab.as_deref()).await?;
                {
                    let mut s = crate::lock(&self.state);
                    if !mine {
                        s.visited.clear();
                    }
                    s.owner = Some(conversation.clone());
                    s.reported = None;
                }
                None
            }
            _ if self.webview().is_none() => return Err("no page is open; open one with its address first".into()),
            _ if crate::lock(&self.state).owner.as_deref() != Some(conversation.as_str()) => {
                return Err("the page open in Faber's browser is not this conversation's; open yours again with its address".into());
            }
            Request::Read => None,
            Request::Click(target) => Some(json!({ "kind": "click", "target": target, "at": bound(reported.as_deref())? })),
            Request::Type { target, text, submit } => {
                Some(json!({ "kind": "type", "target": target, "text": text, "submit": submit, "at": bound(reported.as_deref())? }))
            }
            Request::Scroll { down } => Some(json!({ "kind": "scroll", "down": down })),
            Request::Back => Some(json!({ "kind": "back" })),
            Request::Size(size) => {
                crate::lock(&self.state).size = *size;
                None
            }
            Request::Sizes(_) => None,
        };
        // The size the agent chose holds for every call, after the person had the pane and on a page just opened; a check at
        // several sizes that was cut off (an error, the engine's deadline) left another drawn, which goes here.
        let (size, emulated) = {
            let s = crate::lock(&self.state);
            (s.size, s.emulated)
        };
        if size.is_some() || emulated {
            self.touch();
            self.emulate(size, taken).await?;
        }
        let mut navigated = matches!(request, Request::Open(_));
        // The turn the action itself refused: the page is the same and stays the person's, whatever the next look says.
        let mut refused = None;
        if let Some(action) = action {
            self.touch();
            self.person_took_it(taken)?;
            let (done, turn) = self.run_act(action, taken).await?;
            refused = turn.filter(|_| !done);
            // An action that starts a new page waits for it; one that does not reads the same page again.
            if refused.is_none() {
                navigated = tokio::time::timeout(NAVIGATES, loads.wait_for(|(started, _)| *started > before)).await.is_ok();
            }
        }
        if navigated {
            // A page that never says it finished is read as it is once the time is up.
            let _ = tokio::time::timeout(LOAD, loads.wait_for(|(_, finished)| *finished > before)).await;
            // Pages that draw themselves after the load get a moment.
            tokio::time::sleep(Duration::from_millis(600)).await;
        } else if matches!(request, Request::Size(_)) {
            tokio::time::sleep(RESIZED).await;
        } else if !matches!(request, Request::Read | Request::Sizes(_)) && refused.is_none() {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let looks = match &request {
            Request::Sizes(sizes) => {
                self.touch();
                self.looks(sizes, taken).await?
            }
            _ => Vec::new(),
        };
        let (mut page, href) = self.read(refused, taken).await?;
        page.looks = looks;
        let mut s = crate::lock(&self.state);
        // The person took the pane after the last look: this answer is old, and their use is the next one's to tell.
        if s.taken != taken {
            return Err("the person took over Faber's browser; ask them, or try again once they go back".into());
        }
        // Taken only by an answer that reaches the agent: a call that failed leaves them for the next.
        let used = std::mem::take(&mut s.handed_back);
        let visited = std::mem::take(&mut s.visited);
        if mine {
            (page.person, page.visited) = (used, visited);
        }
        s.reported = Some(href);
        Ok(page)
    }

    /// The page as it is: its address, title and text, and a picture of what is in view, none on a page that is the
    /// person's turn, where it could hold what they typed. `refused` is the turn an action just refused there. Answers, besides
    /// the page, its whole address, which stays in the shell.
    async fn read(&self, refused: Option<Turn>, taken: u64) -> Result<(Page, String), String> {
        // Cut inside the page, so a huge page never crosses into Faber whole; one more character tells the engine it was cut.
        // Its turn is looked at in the same step as its text, and what changes in the page from then on is counted.
        let expression = format!(
            "(() => {{ const turn = ({})({{ kind: 'read', watch: true }}).turn; const t = document.body ? document.body.innerText : ''; \
             return {{ turn, href: String(location.href), url: String(location.href).slice(0, {MAX_URL}), title: String(document.title).slice(0, {MAX_TITLE}), \
             text: String(t).slice(0, {}), x: scrollX, y: scrollY }}; }})()",
            act_function(),
            MAX_TEXT + 1
        );
        let context = self.world().await?;
        let value = self.evaluate_in(context, &expression, taken, false).await?;
        let text = |key: &str| value[key].as_str().unwrap_or_default().to_string();
        let mut turn = refused.or_else(|| Turn::from_words(&text("turn")));
        let size = crate::lock(&self.state).size;
        let mut picture = None;
        if turn.is_none() {
            (picture, turn) = self.picture(context, &value, size.as_ref(), taken).await?;
        }
        self.person_took_it(taken)?;
        let page = Page { url: text("url"), title: text("title"), text: text("text"), picture, turn, size, ..Page::default() };
        Ok((page, text("href")))
    }

    /// A picture of what is in view at `size` (none: the pane's own), from where `seen` says the page is scrolled to, after a
    /// look that started watching the page (`kind: 'read', watch: true`). A login or a payment that came up while it was
    /// taken is in it, and so is anything the page changed meanwhile (it cannot be told from here what it was): the picture
    /// is dropped, and the turn the page shows then is answered. A page that went to another document took the world with it.
    async fn picture(
        &self,
        context: i64,
        seen: &Value,
        size: Option<&Viewport>,
        taken: u64,
    ) -> Result<(Option<Vec<u8>>, Option<Turn>), String> {
        let at = |key: &str| seen[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0).min(1e7);
        let (width, height, scale) = match size {
            Some(size) => (f64::from(size.width), f64::from(size.height), look_scale(size)),
            None => (SIZE.width, SIZE.height, PICTURE_SCALE),
        };
        self.person_took_it(taken)?;
        let mut picture = self
            .call(
                "Page.captureScreenshot",
                json!({
                    "format": "jpeg",
                    "quality": 60,
                    "clip": { "x": at("x"), "y": at("y"), "width": width, "height": height, "scale": scale }
                }),
            )
            .await
            .ok()
            .and_then(|shot| shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()));
        let settle = format!("({})({{ kind: 'settle' }})", act_function());
        let mut turn = None;
        match self.evaluate_in(context, &settle, taken, false).await {
            Ok(after) => {
                turn = after["turn"].as_str().and_then(Turn::from_words);
                if turn.is_some() || after["changed"].as_bool() != Some(false) {
                    picture = None;
                }
            }
            Err(_) if self.person_took_it(taken).is_ok() => picture = None,
            Err(e) => return Err(e),
        }
        Ok((picture, turn))
    }
}

/// The most sites the agent hears the person went to: the latest of them.
const MAX_VISITED: usize = 5;

/// A pane being made: closed if its making stops before its first page, unless a newer one took its place.
struct Making<'a> {
    pane: &'a Pane,
    made: u64,
    done: bool,
}

impl Making<'_> {
    fn finish(mut self) {
        self.done = true;
    }
}

impl Drop for Making<'_> {
    fn drop(&mut self) {
        if !self.done && crate::lock(&self.pane.state).made == self.made {
            self.pane.close();
        }
    }
}

/// Clears all the private data of a pane just made, which panes made one after another may share, and answers whether
/// WebView2 said it went within `CLEARING`; the browser may need a moment before it answers.
async fn clear_new(webview: &Webview) -> bool {
    let deadline = Instant::now() + CLEARING;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(left, native::clear_all(webview)).await {
            Ok(Ok(Ok(()))) => break,
            Err(_) => return false,
            Ok(_) if Instant::now() + Duration::from_millis(100) >= deadline => return false,
            Ok(_) => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    // Cookies a second time, through the page's own channel, which answers only once its store has none.
    let left = deadline.saturating_duration_since(Instant::now());
    let cleared = tokio::time::timeout(left, native::devtools(webview, "Network.clearBrowserCookies", "{}".into())).await;
    matches!(cleared, Ok(Ok(Ok(_))))
}

/// A DevTools call for a look only, on the pane it was meant for: a page that holds it gives up after `LOOK_CALL` and keeps
/// its pane, unlike `Pane::call`.
async fn look_call(webview: &Webview, method: &str, params: Value) -> Result<Value, String> {
    let answer = tokio::time::timeout(LOOK_CALL, native::devtools(webview, method, params.to_string()))
        .await
        .map_err(|_| "the page is busy".to_string())?;
    let json = answer.map_err(|_| "the browser closed".to_string())??;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

/// Notes a site the person went to, for the agent and so for its vendor: only the site's own name (`docs.github.com` is
/// `github.com`, since a subdomain can be a token or a tenant), only for web pages with a plain name, never a local address
/// or an intranet name, once for pages in a row on the same site, the latest few kept. A page that finishes after the person
/// left the pane is not counted: the list can fall short, never say more.
fn went_to(visited: &mut Vec<String>, url: &Url) {
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
fn keeps_now(keeps_for: &Option<(u64, String)>, made: u64, open: bool) -> Option<String> {
    keeps_for.as_ref().filter(|(pane, _)| open && *pane == made && made != 0).map(|(_, conversation)| conversation.clone())
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

/// Tells the screen the agent works in a tab, and that it stopped once the call ends, however it ends.
struct Busy {
    app: AppHandle,
    conversation: String,
    tab: String,
}

impl Busy {
    fn new(app: AppHandle, conversation: String, tab: String) -> Busy {
        let _ = app.emit_to("main", "faber:site-busy", json!({ "conversation": conversation, "tab": tab, "busy": true }));
        Busy { app, conversation, tab }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        let _ = self.app.emit_to("main", "faber:site-busy", json!({ "conversation": self.conversation, "tab": self.tab, "busy": false }));
    }
}

/// Which pane the agent's call on tab `tab` goes to: the one that holds it; else the first pane, unless that one holds
/// another tab of the same conversation, which is the person's and stays as it is: the pane behind takes the agent's then.
fn agent_pane(main: &Arc<Pane>, conversation: &str, tab: &str, front: Option<&str>) -> Arc<Pane> {
    let Some(behind) = main.behind.clone() else { return main.clone() };
    let (person_s, era) = {
        let s = crate::lock(&main.state);
        (goes_behind(s.webview.is_some(), s.owner.as_deref(), s.tab.as_deref(), conversation, tab, front), s.era)
    };
    if !person_s {
        return main.clone();
    }
    // Bound to this era of the first pane and this conversation: a pane behind bound to another goes first.
    let bound = Some((era, conversation.to_string()));
    if crate::lock(&behind.state).bound != bound {
        behind.close();
        crate::lock(&behind.state).bound = bound;
    }
    behind
}

/// Whether the agent's call on `tab` goes to the pane behind: the first pane holds another tab of its conversation, the
/// person's, and the agent's tab is not the one in front (a blank tab in front that became the agent's shows its page).
fn goes_behind(open: bool, owner: Option<&str>, held: Option<&str>, conversation: &str, tab: &str, front: Option<&str>) -> bool {
    holds_another(open, owner, held, conversation, tab) && front != Some(tab)
}

/// Whether a pane with a page of `owner`'s tab `held` holds another tab of `conversation` than `tab`: the person's, which the
/// agent's call never loads over. Another conversation's page is not: the pane passes to the agent's conversation as before.
fn holds_another(open: bool, owner: Option<&str>, held: Option<&str>, conversation: &str, tab: &str) -> bool {
    open && owner == Some(conversation) && held.is_some_and(|held| held != tab)
}

/// The agent's call found the pane it was sent to holding something else once its turn came: it is sent again.
const MOVED: &str = "the person moved Faber's browser to another tab meanwhile; try again";
/// How many times the agent's call is sent before it gives up.
const SENDS: usize = 3;

impl Pane {
    /// Whether this pane may hold a page now: always for the first pane; for the pane behind, only while the first pane is
    /// open on the same conversation in the era it was bound to.
    fn still_behind(&self) -> bool {
        let Some(main) = self.front_of.get() else { return true };
        let Some((era, conversation)) = crate::lock(&self.state).bound.clone() else { return false };
        let Some(main) = main.upgrade() else { return false };
        let s = crate::lock(&main.state);
        s.webview.is_some() && s.era == era && s.owner.as_deref() == Some(conversation.as_str())
    }

    /// The agent's call on its own tab: a page it left in a tab no pane holds now loads again from the tab's record first.
    async fn agent_act(self: Arc<Self>, conversation: String, tab: String, request: Request) -> Result<Page, String> {
        let _busy = Busy::new(self.app.clone(), conversation.clone(), tab.clone());
        for _ in 1..SENDS {
            match self.agent_send(&conversation, &tab, &request).await {
                Err(e) if e == MOVED => continue,
                done => return done,
            }
        }
        self.agent_send(&conversation, &tab, &request).await
    }

    async fn agent_send(self: &Arc<Self>, conversation: &str, tab: &str, request: &Request) -> Result<Page, String> {
        let tabs = self.tabs_of(conversation).await;
        let pane = agent_pane(self, conversation, tab, tabs.as_ref().and_then(|t| t.front.as_deref()));
        if !matches!(request, Request::Open(_)) && !pane.holds(conversation, tab) {
            let url = tabs.as_ref().and_then(|tabs| tabs.get(tab).map(|t| t.url.clone())).filter(|u| !u.is_empty());
            let url = url.ok_or("no page of yours is open; open one with its address first")?;
            pane.clone().act_page(conversation.to_string(), Some(tab.to_string()), Request::Open(url)).await?;
        }
        pane.act_page(conversation.to_string(), Some(tab.to_string()), request.clone()).await
    }
}

impl Browser for PaneBrowser {
    fn act(&self, conversation: String, tab: String, request: Request) -> Pin<Box<dyn Future<Output = Result<Page, String>> + Send>> {
        let pane = self.0.clone();
        // Making the webview waits for the window's thread, so it runs off the engine's.
        Box::pin(async move {
            let mut task = Abort(tauri::async_runtime::spawn(pane.agent_act(conversation, tab, request)));
            (&mut task.0).await.map_err(|e| e.to_string())?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "test-hooks")]
    #[test]
    fn test_probe_preserves_a_native_failure_from_the_current_generation() {
        // The endings are the whole test process's: only their place is fixed here.
        let text = test_probe_error("failed to receive message from webview", "size", (0, 1, Some("a")), (0, 1, Some("a")));
        assert!(
            text.starts_with(
                r#"failed to receive message from webview; test_site_probe size: era 0->0, made 1->1, owner Some("a")->Some("a") ("#
            ),
            "{text}"
        );
    }

    #[cfg(feature = "test-hooks")]
    #[test]
    fn test_probe_reports_a_closed_generation_instead_of_a_native_getter_failure() {
        let owner = Some("5f1c0000-0000-4000-8000-0000000000b1");
        for field in ["position", "size", "scale_factor", "private", "url", "is_minimized"] {
            for now in [(1, 1, owner), (0, 2, owner), (0, 1, Some("other")), (0, 1, None)] {
                let text = test_probe_error("failed to receive message from webview", field, (0, 1, owner), now);
                assert!(text.starts_with("the test browser closed ("), "{field}, generation {now:?}: {text}");
            }
        }
    }

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

    #[test]
    fn the_person_taking_the_pane_waits_for_an_action_in_flight() {
        tauri::async_runtime::block_on(async {
            let acting = tokio::sync::Mutex::new(());
            let busy = acting.lock().await;
            let mut taking = std::pin::pin!(when_idle(&acting, || "placed"));
            assert!(
                tokio::time::timeout(Duration::from_millis(50), &mut taking).await.is_err(),
                "it waits while the page is being changed"
            );
            drop(busy);
            assert_eq!(taking.await, "placed", "and goes on once the action is over");
        });
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
