//! Making, loading, keeping and closing the pane's view, and what the view tells back.

use super::rules::{CALL, KEEP_EVERY, LOOK_CALL, MOVED, allowed, checked, keeps_now, went_to};
use super::{Making, Pane, PaneEvent, Place, View, ViewEvent};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use url::Url;

impl Pane {
    pub(super) fn is_open(&self) -> bool {
        crate::lock(&self.state).open
    }

    /// Whether this is the pane the person sees; the agent's pane behind is never on the screen.
    fn is_front(&self) -> bool {
        self.view == View::Main
    }

    /// Loads `url` in the pane for `conversation`, making the pane first if it is closed. Answers the count of started pages
    /// to wait past. A pane is made blank, on a private profile of its own that no earlier pane had (so one conversation's
    /// sign-ins never reach the next one's, R308), and takes the sign-ins `conversation` holds before its first page.
    pub(super) async fn load(self: &Arc<Self>, url: &str, conversation: &str, tab: Option<&str>) -> Result<u64, String> {
        let parsed = checked(url)?;
        let before = self.loads.borrow().0;
        // A page Faber loads is the agent's or a card's, never one the person went to.
        crate::lock(&self.state).skip_finish += 1;
        self.touch();
        if self.is_open() {
            {
                let mut s = crate::lock(&self.state);
                (s.keeps_for, s.keep_waits) = (Some((s.made, conversation.to_string())), true);
                if s.tab.as_deref() != tab {
                    s.new_history = true;
                }
                s.tab = tab.map(str::to_string);
                s.url = Some(parsed.to_string());
            }
            if self.driver.load(self.view, parsed.to_string()).await.is_ok() {
                return Ok(before);
            }
            // A pane left from a window that closed and came back: it is made again.
            self.close();
        }
        // The pane behind lives on the first pane's profile, which holds this same conversation's pages.
        let profile = match self.front_of.get() {
            None => Pane::new_profile(),
            Some(main) => {
                let main = main.upgrade().ok_or(MOVED)?;
                let profile = crate::lock(&main.state).profile.clone();
                profile.ok_or(MOVED)?
            }
        };
        tokio::time::timeout(CALL, self.driver.open(self.view, profile.clone())).await.map_err(|_| "Faber's browser did not open")??;
        let made = {
            let mut s = crate::lock(&self.state);
            s.open = true;
            s.profile = Some(profile);
            s.url = None;
            s.made += 1;
            (s.keeps_for, s.keep_waits) = (Some((s.made, conversation.to_string())), true);
            s.tab = tab.map(str::to_string);
            s.made
        };
        // Stored first, then checked: a first pane that closes after this closes this view too, and one that closed before is
        // seen here.
        if !self.still_behind() {
            self.close();
            return Err(MOVED.into());
        }
        // Until its first page, a pane whose making stops (an error, or the call that made it given up) closes.
        let making = Making { pane: self, made, done: false };
        let held = {
            let mut s = crate::lock(&self.state);
            s.brought = true;
            s.held.iter().find(|h| h.conversation == conversation).map(|h| h.cookies.clone())
        };
        if let Some(cookies) = held {
            self.set_cookies(&cookies).await;
        }
        if crate::lock(&self.state).made != made || !self.is_open() || !self.still_behind() {
            return Err("the browser closed".into());
        }
        let before = self.loads.borrow().0;
        {
            let mut s = crate::lock(&self.state);
            s.keep_waits = true;
            s.url = Some(parsed.to_string());
        }
        self.driver.load(self.view, parsed.to_string()).await?;
        making.finish();
        Ok(before)
    }

    /// What the view says. A report from a view that closed is ignored, so a shell that is a moment late changes nothing.
    pub fn event(self: &Arc<Self>, view: View, event: ViewEvent) {
        let pane = match view {
            View::Main => Some(self.clone()),
            View::Behind => self.behind.clone(),
        };
        if let Some(pane) = pane {
            pane.heard(event);
        }
    }

    fn heard(self: &Arc<Self>, event: ViewEvent) {
        let made = {
            let s = crate::lock(&self.state);
            if !s.open {
                return;
            }
            s.made
        };
        match event {
            ViewEvent::Gone => self.close(),
            ViewEvent::Titled => self.keep_soon(made),
            ViewEvent::Moved { url } => {
                let mut s = crate::lock(&self.state);
                s.url = Some(url.clone());
                if s.shown {
                    if let Ok(page) = Url::parse(&url) {
                        went_to(&mut s.visited, &page);
                    }
                }
                drop(s);
                self.keep_soon(made);
                if self.is_front() {
                    self.home.tell(PaneEvent::Page(url));
                }
            }
            ViewEvent::Started { url } => {
                self.loads.send_modify(|(started, _)| *started += 1);
                crate::lock(&self.state).url = Some(url);
                // The thin line under the address field, while the page the person sees loads.
                if self.is_front() {
                    self.home.tell(PaneEvent::Loading(true));
                }
            }
            ViewEvent::Finished { url } => {
                self.loads.send_modify(|(started, finished)| *finished = *started);
                let (redraw, forget) = {
                    let mut s = crate::lock(&self.state);
                    s.keep_waits = false;
                    let forget = std::mem::take(&mut s.new_history);
                    if s.skip_finish > 0 {
                        s.skip_finish -= 1;
                    } else if s.shown {
                        // A page that finished while the person had the pane is one they went to: the agent hears its site.
                        if let Ok(page) = Url::parse(&url) {
                            went_to(&mut s.visited, &page);
                        }
                    }
                    // A new page may have left the screen size behind (another process draws it): the person's view is drawn
                    // at it again rather than trusted.
                    s.fitted = None;
                    s.url = Some(url.clone());
                    (s.size.is_some() && *self.looking.borrow(), forget)
                };
                if forget {
                    let pane = self.clone();
                    self.runtime.spawn(async move {
                        let _ = pane.look_call(made, "Page.resetNavigationHistory", json!({})).await;
                    });
                }
                self.keep_soon(made);
                if redraw {
                    let pane = self.clone();
                    self.runtime.spawn(async move { pane.show_size().await });
                }
                // The screen names the page it shows by where it is now, never by the card it was opened from.
                if self.is_front() {
                    self.home.tell(PaneEvent::Loading(false));
                    self.home.tell(PaneEvent::Page(url));
                }
            }
        }
    }

    /// Keeps where pane `made`'s page is now, with its title, in the tabs of the conversation it loads for, so the pane opens it
    /// again after it closed, another conversation or a restart. One write at a time, in order, at most once per `KEEP_EVERY`;
    /// the last change always gets its write.
    pub(super) fn keep_soon(self: &Arc<Self>, made: u64) {
        {
            let mut s = crate::lock(&self.state);
            if s.keep_waits || keeps_now(&s.keeps_for, made, s.open).is_none() {
                return;
            }
            if std::mem::replace(&mut s.keeping, true) {
                s.keep_again = Some(made);
                return;
            }
        }
        let pane = self.clone();
        self.runtime.spawn(async move {
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
    /// and written only while the same pane still keeps for the same conversation. Only web pages are kept.
    async fn keep_now(&self, made: u64) {
        let _turn = self.opening.lock().await;
        let held = {
            let s = crate::lock(&self.state);
            // A page Faber loads has not started: the history still shows the page before, which is another tab's.
            if s.keep_waits {
                return;
            }
            keeps_now(&s.keeps_for, made, s.open).map(|c| (c, s.tab.clone()))
        };
        let Some((conversation, tab)) = held else { return };
        let Ok(history) = self.look_call(made, "Page.getNavigationHistory", json!({})).await else { return };
        let entry = usize::try_from(history["currentIndex"].as_i64().unwrap_or(-1)).ok().and_then(|at| history["entries"].get(at));
        let Some(url) = entry.and_then(|e| e["url"].as_str()).and_then(|u| Url::parse(u).ok()).filter(allowed) else { return };
        // A page without a title shows its address as one; the tab then keeps none.
        let title = entry.and_then(|e| e["title"].as_str()).filter(|t| !t.is_empty() && *t != url.as_str()).map(str::to_string);
        {
            let s = crate::lock(&self.state);
            if keeps_now(&s.keeps_for, made, s.open).as_ref() != Some(&conversation) || s.tab != tab {
                return;
            }
        }
        self.keep_page(conversation, tab, url.to_string(), Some(title.unwrap_or_default())).await;
    }

    pub(super) async fn keep_page(&self, conversation: String, tab: Option<String>, url: String, title: Option<String>) {
        let changed = self.home.seen(conversation.clone(), tab, url, title).await;
        // The strip shows the new title or address.
        if changed {
            self.home.tell(PaneEvent::Tabs(conversation));
        }
    }

    pub(super) fn touch(self: &Arc<Self>) {
        let mut s = crate::lock(&self.state);
        s.used = Some(Instant::now());
        if std::mem::replace(&mut s.idle_watch, true) {
            return;
        }
        let pane = self.clone();
        self.runtime.spawn(async move {
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
                        (s.profile, s.url) = (None, None);
                        Err(std::mem::take(&mut s.open))
                    } else {
                        Ok(left.max(Duration::from_secs(1)))
                    }
                };
                match wait {
                    Ok(wait) => tokio::time::sleep(wait).await,
                    Err(was_open) => {
                        if was_open {
                            pane.driver.close(pane.view);
                        }
                        // The pane behind never outlives this one.
                        if let Some(behind) = &pane.behind {
                            behind.close();
                        }
                        break;
                    }
                }
            }
        });
    }

    /// Closes the view and forgets whose page it held; the pane behind closes with it.
    pub fn close(&self) {
        let was_open = {
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
            s.profile = None;
            s.url = None;
            std::mem::take(&mut s.open)
        };
        self.looking.send_replace(false);
        if was_open {
            self.driver.close(self.view);
        }
        if let Some(behind) = &self.behind {
            behind.close();
        }
    }

    /// One DevTools call, with a deadline: a page that holds it longer closes with the pane, which opens fresh next time.
    pub(super) async fn call(&self, method: &'static str, params: Value) -> Result<Value, String> {
        if !self.is_open() {
            return Err("the browser closed".into());
        }
        let Ok(answer) = tokio::time::timeout(CALL, self.driver.call(self.view, method, params)).await else {
            self.close();
            return Err("the page stopped answering".into());
        };
        answer
    }

    /// A DevTools call for a look only, on pane `made`: one made since never gets it, and a page that holds it gives up after
    /// `LOOK_CALL` and keeps its pane, unlike [`Pane::call`].
    pub(super) async fn look_call(&self, made: u64, method: &'static str, params: Value) -> Result<Value, String> {
        {
            let s = crate::lock(&self.state);
            if !s.open || s.made != made {
                return Err("the browser closed".into());
            }
        }
        // Called at once, with no wait since the check: the view takes calls in order.
        tokio::time::timeout(LOOK_CALL, self.driver.call(self.view, method, params)).await.map_err(|_| "the page is busy".to_string())?
    }

    /// Whether this pane may hold a page now: always for the first pane; for the pane behind, only while the first pane is
    /// open on the same conversation in the era it was bound to.
    pub(super) fn still_behind(&self) -> bool {
        let Some(main) = self.front_of.get() else { return true };
        let Some((era, conversation)) = crate::lock(&self.state).bound.clone() else { return false };
        let Some(main) = main.upgrade() else { return false };
        let s = crate::lock(&main.state);
        s.open && s.era == era && s.owner.as_deref() == Some(conversation.as_str())
    }

    /// Moves the view out of the window, back at the size the agent's pages load at.
    pub(super) fn park_view(&self) {
        let _ = self.driver.place(self.view, Place::Parked);
    }
}
