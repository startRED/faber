//! What the agent does with the pane: open a page, read it, click, type, scroll, go back, and look at it at other screen
//! sizes, always by the words a person sees, and never while the person has the pane.

use super::rules::{
    LOAD, MOVED, NAVIGATES, PICTURE_SCALE, RESIZED, SENDS, WAIT_PERSON, act_function, bound, checked, goes_behind, look_scale, metrics,
};
use super::{PARKED_SIZE, Pane, PaneEvent, Unfinished};
use base64::Engine as _;
use faber_engine::browser::{Browser, Look, MAX_CUT, MAX_TEXT, MAX_TITLE, MAX_URL, Page, Request, Turn};
use faber_engine::screens::Viewport;
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

impl Pane {
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
                    self.behind.is_some() && goes_behind(s.open, s.owner.as_deref(), s.tab.as_deref(), &conversation, tab, front.as_deref())
                }
                _ => !(s.open && s.owner.as_deref() == Some(conversation.as_str()) && s.tab.as_deref() == Some(tab)),
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
            _ if !self.is_open() => return Err("no page is open; open one with its address first".into()),
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
            None => (PARKED_SIZE.0, PARKED_SIZE.1, PICTURE_SCALE),
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

    /// The agent's call on its own tab: a page it left in a tab no pane holds now loads again from the tab's record first.
    async fn agent_act(self: Arc<Self>, conversation: String, tab: String, request: Request) -> Result<Page, String> {
        let _busy = Busy::new(self.home.clone(), conversation.clone(), tab.clone());
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

/// Which pane the agent's call on tab `tab` goes to: the one that holds it; else the first pane, unless that one holds
/// another tab of the same conversation, which is the person's and stays as it is: the pane behind takes the agent's then.
fn agent_pane(main: &Arc<Pane>, conversation: &str, tab: &str, front: Option<&str>) -> Arc<Pane> {
    let Some(behind) = main.behind.clone() else { return main.clone() };
    let (person_s, era) = {
        let s = crate::lock(&main.state);
        (goes_behind(s.open, s.owner.as_deref(), s.tab.as_deref(), conversation, tab, front), s.era)
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

/// Tells the screen the agent works in a tab, and that it stopped once the call ends, however it ends.
struct Busy {
    home: Arc<dyn super::Home>,
    conversation: String,
    tab: String,
}

impl Busy {
    fn new(home: Arc<dyn super::Home>, conversation: String, tab: String) -> Busy {
        home.tell(PaneEvent::Busy { conversation: conversation.clone(), tab: tab.clone(), busy: true });
        Busy { home, conversation, tab }
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        self.home.tell(PaneEvent::Busy { conversation: self.conversation.clone(), tab: self.tab.clone(), busy: false });
    }
}

/// A task that ends when the engine stops waiting for it, so a page the engine gave up on stops being read.
struct Abort(tokio::task::JoinHandle<Result<Page, String>>);

impl Drop for Abort {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The engine's view of the pane.
pub struct PaneBrowser(pub Arc<Pane>);

impl Browser for PaneBrowser {
    fn act(&self, conversation: String, tab: String, request: Request) -> Pin<Box<dyn Future<Output = Result<Page, String>> + Send>> {
        let pane = self.0.clone();
        // The call runs as a task of its own on the pane's runtime, so the engine giving up on it is what ends it.
        Box::pin(async move {
            let mut task = Abort(pane.runtime.spawn(pane.clone().agent_act(conversation, tab, request)));
            (&mut task.0).await.map_err(|e| e.to_string())?
        })
    }
}
