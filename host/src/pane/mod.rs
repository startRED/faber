//! Faber's own browser pane (R161 step 7, R459): the rules and the state, over a [`Driver`] any shell implements. The agent
//! opens a page in it through the engine's browser tool; the person sees it when they open it from the reply, placed over the
//! running column. Away from the screen it is parked outside the window, never hidden, so its pictures stay fresh, and it
//! closes once nobody has used it for a while.
//!
//! Only http and https pages load in it, never Faber's own `*.localhost` origins; it opens no new windows, downloads
//! nothing and keeps no cookies or logins once it closes (a private profile per pane), so one project's agent never reads a
//! session another conversation or the person left there. One page opens at a time, and never while the person is looking at
//! the pane: it is theirs then, and the agent's call waits for them to go back ([`rules::WAIT_PERSON`]), then hears that they
//! used it.
//!
//! The agent acts by the words a person sees (`act.js`, run inside the page); on a page that asks for a login, a captcha or a
//! payment it may read and scroll but never click or type.
//!
//! A shell in another process makes its [`Driver`] with [`relay::Relay`], which orders its views over the wire.
//!
//! A shell makes a [`Pane`] over its [`Driver`] and a [`Home`], hands [`PaneBrowser`] to the engine, feeds the views' events to
//! [`Pane::event`], and answers the window commands (`site_*`, `cookie_*`) with the methods of the same names.

mod agent;
mod driver;
pub mod firefox;
mod home;
mod lifecycle;
mod person;
pub mod relay;
mod rules;
#[cfg(test)]
mod tests;

pub use agent::PaneBrowser;
pub use driver::{Driver, PARKED, PARKED_SIZE, Place, View, ViewEvent};
pub use home::{EngineHome, Home, PaneEvent};
pub use relay::Relay;
pub use rules::{allowed, allowed_url};

use faber_engine::screens::Viewport;
use rules::Boxes;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::time::Instant;

/// One view's side of the pane: the first one the person sees, and the agent's behind it.
pub struct Pane {
    driver: Arc<dyn Driver>,
    home: Arc<dyn Home>,
    /// Where the pane's own background work (idle close, keeping the page, redraws) runs.
    runtime: Handle,
    view: View,
    /// How long the pane may sit unused out of sight before it closes.
    idle: Duration,
    /// The agent's pane behind this one; none on that pane itself.
    behind: Option<Arc<Pane>>,
    /// On the pane behind: the first pane, whose era it is bound to (`still_behind`).
    front_of: OnceLock<Weak<Pane>>,
    /// Never await under this lock: `event` takes it on the shell's thread.
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
    /// The view is open, on this private profile (the first pane names it, the pane behind takes it).
    open: bool,
    profile: Option<String>,
    /// The address of the page, as the view last said.
    url: Option<String>,
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
    /// The pane (`made`) and the conversation whose tabs its pages are kept in: set when Faber loads a page in that pane,
    /// cleared when it closes, so a late page of a pane that closed is never kept in the conversation after it.
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
    /// Last visible rectangle, for reopening a page after its profile is discarded.
    visible_place: Option<Place>,
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

impl Pane {
    /// The first pane, with the agent's pane behind it.
    pub fn new(driver: Arc<dyn Driver>, home: Arc<dyn Home>, runtime: Handle) -> Arc<Pane> {
        let behind = Pane::made(&driver, &home, &runtime, View::Behind, rules::BEHIND_IDLE, None);
        let main = Pane::made(&driver, &home, &runtime, View::Main, rules::IDLE, Some(behind.clone()));
        let _ = behind.front_of.set(Arc::downgrade(&main));
        main
    }

    fn made(
        driver: &Arc<dyn Driver>,
        home: &Arc<dyn Home>,
        runtime: &Handle,
        view: View,
        idle: Duration,
        behind: Option<Arc<Pane>>,
    ) -> Arc<Pane> {
        Arc::new(Pane {
            driver: driver.clone(),
            home: home.clone(),
            runtime: runtime.clone(),
            view,
            idle,
            behind,
            front_of: OnceLock::new(),
            state: Mutex::default(),
            loads: tokio::sync::watch::channel((0, 0)).0,
            opening: tokio::sync::Mutex::new(()),
            looking: tokio::sync::watch::channel(false).0,
            acting: tokio::sync::Mutex::new(()),
            rounding: tokio::sync::Mutex::new(()),
            mark: format!("{:016x}", std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), 0u8)),
        })
    }

    /// A profile name no pane has had before.
    fn new_profile() -> String {
        format!("faber-pane-{}", uuid::Uuid::new_v4())
    }
}
