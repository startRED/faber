//! What the person does with the pane: placing it over the screen, its tabs and history, the screen size and corners, and the
//! sign-ins they bring over from their own browser. The `site_*` and `cookie_*` methods answer the window commands of the same
//! names, so a shell's `Shell` implementation only forwards to them.

use super::rules::{
    CLEARING, LISTING, MAX_HELD, MAX_RADIUS, ROUND, SETTING, checked, cookie_param, fit_scale, metrics, unix_now, when_idle,
};
use super::{Held, Listing, Pane, Place, firefox};
use crate::Res;
use faber_engine::TabStep;
use faber_engine::browser::tabs::Tabs;
use faber_engine::screens::Viewport;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::time::Instant;

impl Pane {
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
            if !s.open || !s.boxes.place(load, box_id) {
                return false;
            }
            s.shown = true;
            s.handed_back = true;
            s.taken += 1;
            s.placed = Some((width, height));
            s.visible_place = Some(Place::Over { x, y, width, height });
            self.looking.send_replace(true);
            self.driver.place(self.view, Place::Over { x, y, width, height }).is_ok()
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
            self.runtime.spawn(async move { pane.round().await });
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
    pub(super) async fn show_size(&self) {
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
    pub(super) async fn round(&self) {
        let _one = self.rounding.lock().await;
        let (wanted, old, made) = {
            let s = crate::lock(&self.state);
            if !s.open || s.rounded == Some(s.round) {
                return;
            }
            (s.round, s.round_script.clone(), s.made)
        };
        let same = |s: &super::State| s.made == made && s.open;
        if let Some(identifier) = old {
            let _ = self.look_call(made, "Page.removeScriptToEvaluateOnNewDocument", json!({ "identifier": identifier })).await;
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
        let added = self
            .look_call(
                made,
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
            if !s.open {
                return;
            }
            self.park_view();
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
        let made = {
            let s = crate::lock(&self.state);
            (s.open && s.owner.as_deref() == Some(conversation)).then_some(s.made)
        };
        let Some(made) = made else { return (false, false) };
        let Ok(history) = self.look_call(made, "Page.getNavigationHistory", json!({})).await else { return (false, false) };
        let at = history["currentIndex"].as_i64().unwrap_or(0);
        let count = history["entries"].as_array().map_or(0, |e| e.len() as i64);
        (at > 0, at + 1 < count)
    }

    /// Whether the pane holds tab `tab` of `conversation` now.
    pub(super) fn holds(&self, conversation: &str, tab: &str) -> bool {
        let s = crate::lock(&self.state);
        s.open && s.owner.as_deref() == Some(conversation) && s.tab.as_deref() == Some(tab)
    }

    /// A conversation's tabs.
    pub(super) async fn tabs_of(&self, conversation: &str) -> Option<Tabs> {
        self.home.tabs(conversation.to_string()).await
    }

    /// Does `step` to a conversation's tabs in the engine; the strip hears of it.
    pub(super) async fn tab_step(&self, conversation: &str, step: TabStep) -> Option<(Tabs, Option<String>)> {
        let answer = self.home.tab(conversation.to_string(), step).await;
        self.home.tell(super::PaneEvent::Tabs(conversation.to_string()));
        answer
    }

    /// Loads `url` in tab `tab` for the person: the page is that conversation's from then on.
    async fn show_in(self: &Arc<Self>, conversation: &str, tab: Option<String>, url: &str) -> bool {
        // The page the agent left is usually the one the card names: it shows at once, even while a call of the agent's runs,
        // which then finds the person looking and stops before it acts.
        let (owner, held, same) = {
            let s = crate::lock(&self.state);
            (s.owner.clone(), s.tab.clone(), s.open && s.url.as_deref() == Some(url))
        };
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
    pub(super) async fn bring_in(&self, conversation: &str) -> usize {
        let cookies = {
            let mut s = crate::lock(&self.state);
            if s.brought || !s.open || s.owner.as_deref() != Some(conversation) {
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
    pub(super) async fn set_cookies(&self, cookies: &[firefox::Cookie]) -> usize {
        if !self.is_open() {
            return 0;
        }
        let deadline = Instant::now() + SETTING;
        let send = |method: &'static str, params: Value| {
            let left = deadline.saturating_duration_since(Instant::now());
            async move {
                match tokio::time::timeout(left, self.driver.call(self.view, method, params)).await {
                    Ok(Ok(answer)) => Some(answer),
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
        let home = self.home.clone();
        let (profile, sites) = tokio::task::spawn_blocking(move || {
            let profile = home.profiles().into_iter().find(|p| p.id == profile).ok_or("error.cookies.missing")?;
            home.sites(&profile, unix_now()).map(|sites| (profile.id, sites))
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
        let home = self.home.clone();
        let cookies = tokio::task::spawn_blocking(move || {
            let profile = home.profiles().into_iter().find(|p| p.id == profile).ok_or("error.cookies.missing")?;
            home.cookies(&profile, &hosts, unix_now())
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
    pub async fn forget(self: &Arc<Self>, conversation: &str) {
        let _turn = self.opening.lock().await;
        let page = {
            let mut s = crate::lock(&self.state);
            s.held.retain(|h| h.conversation != conversation);
            if !s.open || s.owner.as_deref() != Some(conversation) {
                return;
            }
            (s.url.clone(), s.tab.clone(), s.shown.then_some(s.visible_place).flatten())
        };
        self.close();
        let _behind = if let Some(behind) = &self.behind {
            let turn = behind.opening.lock().await;
            crate::lock(&behind.state).held.retain(|h| h.conversation != conversation);
            Some(turn)
        } else {
            None
        };
        if let Some(url) = page.0 {
            if self.load(&url, conversation, page.1.as_deref()).await.is_ok() {
                if let Some(place) = page.2 {
                    if self.driver.place(self.view, place).is_ok() {
                        crate::lock(&self.state).shown = true;
                        self.looking.send_replace(true);
                    }
                }
            }
        }
    }

    /// "Clear this conversation's browsing data", once the person said yes: its tabs, closed tabs and sign-ins go, and a pane
    /// holding its page closes with everything the page kept, so the next page starts clean.
    pub async fn clear(&self, conversation: &str) {
        // Held until the pane closed, so no page of the conversation loads while its data goes.
        let _turn = self.opening.lock().await;
        let held = {
            let mut s = crate::lock(&self.state);
            s.held.retain(|h| h.conversation != conversation);
            s.open && s.owner.as_deref() == Some(conversation)
        };
        if held {
            let _ = tokio::time::timeout(CLEARING, self.driver.clear(self.view)).await;
            self.close();
        }
        self.tab_step(conversation, TabStep::Clear).await;
    }

    /// A page that passes to another conversation gets a pane of its own: one conversation's history (and anything the
    /// person signed in to there) is never a Back away from another's agent. The old pane's private data goes now, while
    /// its page still runs, and its profile with it when the pane closes; the next pane is on a new one.
    pub(super) async fn fresh_for(&self, conversation: &str) {
        let clear = {
            let s = crate::lock(&self.state);
            if !s.owner.as_deref().is_some_and(|owner| owner != conversation) {
                return;
            }
            s.open && self.behind.is_some()
        };
        if clear {
            let _ = tokio::time::timeout(CLEARING, self.driver.clear(self.view)).await;
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
}

/// The window commands of the pane, with the arguments and answers of the matching `Shell` methods, so a shell's `Shell`
/// implementation is one line each.
impl Pane {
    pub async fn site_place(self: &Arc<Self>, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> Res<bool> {
        Ok(self.place(load, box_id, x, y, width, height).await)
    }

    pub async fn site_round(self: &Arc<Self>, radius: f64, color: [u8; 3]) -> Res<()> {
        self.set_round(radius, color).await;
        Ok(())
    }

    /// The screen sizes Faber's browser can draw its pages at (`faber_engine::screens::SIZES`, the agent's own list) and the one in
    /// use, by its `1920x1080` name; none is the pane's own. With `pick`, `size` becomes the pane's first: a name not in the
    /// list (or none) is the pane's own. The agent's next calls use it too.
    pub async fn site_size(self: &Arc<Self>, pick: bool, size: Option<String>) -> Res<Value> {
        if pick {
            self.set_size(size.as_deref().and_then(faber_engine::screens::named)).await;
        }
        let sizes: Vec<_> = faber_engine::screens::SIZES
            .iter()
            .map(|v| json!({ "name": v.name(), "screen": v.screen.name(), "width": v.width, "height": v.height }))
            .collect();
        Ok(json!({ "now": self.size().map(|v| v.name()), "sizes": sizes }))
    }

    pub async fn site_park(self: &Arc<Self>, load: u32, box_id: u64) -> Res<()> {
        self.park(load, box_id);
        Ok(())
    }

    pub async fn site_open(self: &Arc<Self>, id: String, url: String) -> Res<bool> {
        Ok(self.show_page(&id, &url).await)
    }

    pub async fn site_step(self: &Arc<Self>, id: String, step: String) -> Res<bool> {
        Ok(self.step(&id, &step).await)
    }

    pub async fn site_go(self: &Arc<Self>, id: String, url: String) -> Res<bool> {
        Ok(self.go(&id, &url).await)
    }

    /// What the person did to the tabs, answered as `{ tabs, tab }`; none for a step that names no tab it needs.
    pub async fn site_tab(self: &Arc<Self>, id: String, step: String, tab: Option<String>) -> Res<Option<Value>> {
        let step = match (step.as_str(), tab) {
            ("new", _) => TabStep::New,
            ("reopen", _) => TabStep::Reopen,
            ("bring", Some(tab)) => TabStep::Bring(tab),
            ("close", Some(tab)) => TabStep::Close(tab),
            ("closeOthers", Some(tab)) => TabStep::CloseOthers(tab),
            _ => return Ok(None),
        };
        Ok(self.tab(&id, step).await.map(|(tabs, tab)| json!({ "tabs": tabs, "tab": tab })))
    }

    pub async fn site_history(self: &Arc<Self>, id: String) -> Res<[bool; 2]> {
        let (back, forward) = self.history(&id).await;
        Ok([back, forward])
    }

    pub async fn site_clear(self: &Arc<Self>, id: String) -> Res<()> {
        self.clear(&id).await;
        Ok(())
    }

    /// The Firefox profiles whose sign-ins can come over: id (never a path), name and whether it is the one used last.
    pub async fn cookie_profiles(self: &Arc<Self>) -> Res<Value> {
        let home = self.home.clone();
        let found = tokio::task::spawn_blocking(move || home.profiles()).await.unwrap_or_default();
        Ok(Value::Array(found.into_iter().map(|p| json!({ "id": p.id, "name": p.name, "lastUsed": p.last_used })).collect()))
    }

    /// The sites with cookies in a profile, under a ticket the import names: no cookie crosses into the screen.
    pub async fn cookie_sites(self: &Arc<Self>, profile: String) -> Res<Value> {
        let (ticket, sites) = self.list_sites(profile).await.map_err(str::to_string)?;
        let sites: Vec<Value> = sites
            .into_iter()
            .map(|s| {
                let kind = s.kind.map(|k| match k {
                    firefox::Kind::Money => "money",
                    firefox::Kind::Email => "email",
                    firefox::Kind::SignIn => "signin",
                });
                json!({ "name": s.name, "cookies": s.cookies, "kind": kind })
            })
            .collect();
        Ok(json!({ "ticket": ticket, "sites": sites }))
    }

    pub async fn cookie_import(self: &Arc<Self>, id: String, ticket: String, sites: Vec<String>) -> Res<usize> {
        self.import(&id, &ticket, &sites).await.map_err(str::to_string)
    }

    pub async fn cookie_held(self: &Arc<Self>, id: String) -> Res<Vec<String>> {
        Ok(self.held_sites(&id))
    }

    pub async fn cookie_forget(self: &Arc<Self>, id: String) -> Res<()> {
        self.forget(&id).await;
        Ok(())
    }
}
