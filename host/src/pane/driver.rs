//! What a shell gives the browser pane: the views only a window can make. Every rule (which pages load, who may use the
//! pane, when it closes, where it waits) stays in the host; the shell opens, moves, loads and asks, and reports what the
//! views tell it.

use crate::{BoxFuture, Res};
use serde_json::Value;

/// Where a view waits while nobody looks at it, outside the window, where it keeps drawing so its pictures stay fresh.
pub const PARKED: (f64, f64) = (-20_000.0, -20_000.0);
/// The size a view has while parked, and the size the agent's pages load at: a common laptop window, in logical pixels.
pub const PARKED_SIZE: (f64, f64) = (1280.0, 800.0);

/// The two views the pane has. They share one private profile: the second is the agent's tab while the person has another
/// tab of the same conversation in front, never shown, and it closes with the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    /// The one the person sees, placed over the running column.
    Main,
    /// The agent's tab behind.
    Behind,
}

impl View {
    /// The name the view has in the shell's own tables.
    pub fn label(self) -> &'static str {
        match self {
            View::Main => "site",
            View::Behind => "site-agent",
        }
    }
}

/// Where a view is, in the screen's logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    /// At [`PARKED`], [`PARKED_SIZE`] large.
    Parked,
    /// Over the box the screen shows it in.
    Over { x: f64, y: f64, width: f64, height: f64 },
}

/// What a view tells the host. A shell feeds them to [`Pane::event`](super::Pane::event) in the order they happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewEvent {
    /// A page started to load (the main frame), at this address.
    Started { url: String },
    /// A page finished loading, at the address it ended up.
    Finished { url: String },
    /// The main frame moved within the same document.
    Moved { url: String },
    /// The page's title changed.
    Titled,
    /// The view is gone: its window closed, or its page process died. The pane closes and opens fresh next time.
    Gone,
}

/// The windows of the pane, implemented by a shell. Any call on a view that is not open fails.
///
/// What the shell must keep, since the host's rules rest on it:
/// - A view opens at `about:blank`, parked, and reports nothing for that first page.
/// - Its profile is private and only in memory: no cookie, login, storage or cache reaches the disk, and what it holds is
///   gone once the last view opened on that profile closes. The host names a profile once, per pane it makes, and never
///   gives the same name again, so a pane made later starts clean.
/// - Only web pages load, and the shell asks [`allowed_url`](super::allowed_url) for every navigation of the main frame,
///   whoever starts it (the page, a redirect, the back button) and cancels one that is not. A view opens no new window and
///   starts no download: both are refused without a word to the host.
/// - Calls on one view reach it in the order they were made, as soon as they are polled; the host relies on that when it
///   checks a pane and calls it in one step.
/// - After `close` returns, nothing more is reported for that view.
pub trait Driver: Send + Sync + 'static {
    /// Opens `view`, blank and parked, on the private profile `profile`; the second view is given the first's profile.
    fn open(&self, view: View, profile: String) -> BoxFuture<'_, Res<()>>;
    /// Closes `view` and everything its page holds. Does nothing for a view that is not open.
    fn close(&self, view: View);
    /// Moves `view`. Cheap: it never waits for the page.
    fn place(&self, view: View, place: Place) -> Res<()>;
    /// Starts loading `url` in `view`. The page's end comes later as [`ViewEvent::Finished`].
    fn load(&self, view: View, url: String) -> BoxFuture<'_, Res<()>>;
    /// One DevTools Protocol command on `view`'s page, answered with its result.
    fn call(&self, view: View, method: &'static str, params: Value) -> BoxFuture<'_, Res<Value>>;
    /// Clears every cookie, storage and cache of `view`'s profile, so the page signs out of everything.
    fn clear(&self, view: View) -> BoxFuture<'_, Res<()>>;
}
