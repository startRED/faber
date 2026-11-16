//! What the agent does with the pane: open a page, read it, click, type, scroll, go back, and look at it at other screen
//! sizes, always by the words a person sees, and never while the person has the pane.

use super::rules::{
    CLICK_TRIES, DRAG_STEPS, FLOW_TIME, LOAD, MAX_MEMO, MAX_MEMOS, MOVED, NAVIGATES, PICTURE_SCALE, RESIZED, SENDS, SHOT_MAX, SHOT_PAD,
    STILL, WAIT_PERSON, act_function, bound, checked, goes_behind, look_scale, metrics,
};
use super::{PARKED_SIZE, Pane, PaneEvent, Unfinished};
use base64::Engine as _;
use faber_engine::browser::{Browser, Look, MAX_CUT, MAX_TEXT, MAX_TITLE, MAX_URL, Page, Request, Shot, Step, Target, Turn};
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
    async fn run_act(&self, request: Value, taken: u64) -> Result<(bool, Option<Turn>), String> {
        let value = self.act_value(request, taken).await?;
        Ok((value["ok"].as_bool() == Some(true), value["turn"].as_str().and_then(Turn::from_words)))
    }

    /// [`Self::run_act`], with what the page answered when it did it or stopped at the person's turn.
    async fn act_value(&self, mut request: Value, taken: u64) -> Result<Value, String> {
        request["mark"] = json!(self.mark);
        request["memo"] = self.memo();
        let changes = matches!(request["kind"].as_str(), Some("click" | "type" | "back"));
        let call = async |request: &Value| {
            let expression = format!("({})({request})", act_function());
            self.evaluate(&expression, taken, changes).await
        };
        let mut value = Value::Null;
        if request["kind"] != "click" || request["box"].is_boolean() {
            value = call(&request).await?;
        } else {
            // Before a click the control is on view and enabled (the page says so), and still: its box is read, a moment passes,
            // and the click goes only if the box is where it was. One that is still moving is waited for, a few times.
            for attempt in 0..CLICK_TRIES {
                let mut probe = request.clone();
                probe["probe"] = json!(true);
                value = call(&probe).await?;
                if value["ok"].as_bool() != Some(true) {
                    break;
                }
                tokio::time::sleep(STILL).await;
                request["rect"] = value["rect"].clone();
                value = call(&request).await?;
                if value["why"] != "moving" || attempt + 1 == CLICK_TRIES {
                    break;
                }
            }
        }
        if !value.is_object() {
            return Err("the page did not let Faber act on it".into());
        }
        let turn = value["turn"].as_str().and_then(Turn::from_words);
        if value["ok"].as_bool() == Some(true) || turn.is_some() {
            return Ok(value);
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
            Some("ref") => {
                "no control on the page has that number any more (it changed or went); read the page again for the current numbers".into()
            }
            Some("noref") => "read the page first: a number only means something from a read of this page".into(),
            Some("control") => format!("that point is a control ({}); click it by its number instead", quoted(&value["seen"])),
            Some("point") => "nothing is at that point; it is outside the page's view".into(),
            Some("hidden") => "that control is not on view; scroll to it or open what hides it".into(),
            Some("disabled") => "that control is disabled; do what enables it first (fill a field, tick a box)".into(),
            Some("moving") => "that control kept moving and never held still; try again once the page settles".into(),
            Some("option") => "that list has no option that reads so".into(),
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

    /// `quiet`: a step of a flow, which does its action and answers nothing; the flow's last read is the one the agent gets.
    async fn act_page(self: Arc<Self>, conversation: String, tab: Option<String>, request: Request, quiet: bool) -> Result<Page, String> {
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
        // A pane the person covered or parked keeps working: the page believes it has the focus, so its fields and scripts run.
        let _ = self.call("Emulation.setFocusEmulationEnabled", json!({ "enabled": true })).await;
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
                // An address the agent asks for is read whole, never as changes; its numbers go on from where they were.
                self.forget_read();
                None
            }
            _ if !self.is_open() => return Err("no page is open; open one with its address first".into()),
            _ if crate::lock(&self.state).owner.as_deref() != Some(conversation.as_str()) => {
                return Err("the page open in Faber's browser is not this conversation's; open yours again with its address".into());
            }
            Request::Read | Request::Controls | Request::Watch(_) | Request::Shot(_) => None,
            Request::At { x, y, to } => {
                self.touch();
                self.person_took_it(taken)?;
                self.point(*x, *y, *to, taken).await?;
                None
            }
            Request::Click(target) => {
                Some(json!({ "kind": "click", "target": words(target), "ref": number(target), "at": bound(reported.as_deref())? }))
            }
            Request::Type { target, text, submit } => Some(json!({
                "kind": "type", "target": words(target), "ref": number(target), "text": text, "submit": submit, "at": bound(reported.as_deref())?
            })),
            Request::Scroll { down } => Some(json!({ "kind": "scroll", "down": down })),
            Request::Back => Some(json!({ "kind": "back" })),
            Request::Size(size) => {
                crate::lock(&self.state).size = *size;
                None
            }
            Request::Sizes(_) => None,
            Request::Steps(_) => return Err("a flow runs as a whole, not inside another call".into()),
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
        } else if !matches!(request, Request::Read | Request::Controls | Request::Watch(_) | Request::Sizes(_) | Request::Shot(_))
            && refused.is_none()
        {
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if quiet {
            return Ok(Page::default());
        }
        let looks = match &request {
            Request::Sizes(sizes) => {
                self.touch();
                self.looks(sizes, taken).await?
            }
            _ => Vec::new(),
        };
        if let Request::Watch(seconds) = request {
            let mut news = self.noted.subscribe();
            if crate::lock(&self.state).seen.is_empty() {
                let _ = tokio::time::timeout(Duration::from_secs(seconds.into()), news.changed()).await;
            }
        }
        let (mut page, href) = self.read(refused, taken, matches!(request, Request::Controls)).await?;
        page.looks = looks;
        if let Request::Shot(shot) = &request
            && page.turn.is_none()
        {
            page.shot = self.shoot(shot, reported.as_deref(), taken).await?;
        }
        let mut s = crate::lock(&self.state);
        // The person took the pane after the last look: this answer is old, and their use is the next one's to tell.
        if s.taken != taken {
            return Err("the person took over Faber's browser; ask them, or try again once they go back".into());
        }
        // Taken only by an answer that reaches the agent: a call that failed leaves them for the next.
        let used = std::mem::take(&mut s.handed_back);
        let visited = std::mem::take(&mut s.visited);
        let seen = std::mem::take(&mut s.seen);
        if mine {
            (page.person, page.visited, page.seen) = (used, visited, seen);
        }
        s.reported = Some(href);
        Ok(page)
    }

    /// The page as it is: its address, title and text, and a picture of what is in view, none on a page that is the
    /// person's turn, where it could hold what they typed. `refused` is the turn an action just refused there. Answers, besides
    /// the page, its whole address, which stays in the shell.
    async fn read(&self, refused: Option<Turn>, taken: u64, controls: bool) -> Result<(Page, String), String> {
        // Cut inside the page, so a huge page never crosses into Faber whole; one more character tells the engine it was cut.
        // Its turn is looked at in the same step as its text, and what changes in the page from then on is counted. The page
        // answers its controls and text as the changes since the last read of this tab when it is the same page, and the numbers
        // it gave (`memo`), which Faber keeps for the next call.
        let request =
            json!({ "kind": "read", "watch": true, "view": true, "controls": controls, "max": MAX_TEXT + 1, "memo": self.memo() });
        let expression = format!(
            "(() => {{ const r = ({})({request}); \
             return {{ turn: r.turn, href: String(location.href), url: String(location.href).slice(0, {MAX_URL}), title: String(document.title).slice(0, {MAX_TITLE}), \
             text: r.view, memo: r.memo, diff: r.diff, x: scrollX, y: scrollY }}; }})()",
            act_function()
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
        self.keep_memo(&value["memo"]);
        let page = Page {
            url: text("url"),
            title: text("title"),
            text: text("text"),
            changes_only: value["diff"].as_bool() == Some(true),
            picture,
            turn,
            size,
            ..Page::default()
        };
        Ok((page, text("href")))
    }

    /// The key the agent's read of this pane's page is kept under: the conversation and its tab.
    fn memo_key(&self) -> String {
        let s = crate::lock(&self.state);
        format!("{}\0{}", s.owner.as_deref().unwrap_or_default(), s.tab.as_deref().unwrap_or_default())
    }

    /// What the last read of this tab left (null when none).
    fn memo(&self) -> Value {
        crate::lock(&self.memos).get(&self.memo_key()).cloned().unwrap_or(Value::Null)
    }

    /// Keeps what a read answered, if it is a memo and not too big; a bigger one keeps only its counter, so the numbers never
    /// repeat. At most [`MAX_MEMOS`] tabs keep one.
    fn keep_memo(&self, memo: &Value) {
        if !memo.is_object() {
            return;
        }
        let memo = if memo.to_string().len() > MAX_MEMO { json!({ "next": memo["next"] }) } else { memo.clone() };
        let mut all = crate::lock(&self.memos);
        let key = self.memo_key();
        if all.len() >= MAX_MEMOS && !all.contains_key(&key) {
            all.clear();
        }
        all.insert(key, memo);
    }

    /// The next read of this tab is whole, but its numbers go on from where they were.
    fn forget_read(&self) {
        let next = self.memo()["next"].clone();
        let mut all = crate::lock(&self.memos);
        all.insert(self.memo_key(), if next.is_null() { Value::Null } else { json!({ "next": next }) });
    }

    /// A click at a point, or a drag between two, with the mouse: only where the page has no control there (`act.js` says so
    /// for both ends), and never on a page that is the person's turn.
    async fn point(&self, x: u32, y: u32, to: Option<(u32, u32)>, taken: u64) -> Result<(), String> {
        for (px, py) in std::iter::once((x, y)).chain(to) {
            let request = json!({ "kind": "at", "x": px, "y": py });
            let value = self.evaluate(&format!("({})({request})", act_function()), taken, false).await?;
            if value["ok"].as_bool() != Some(true) {
                return Err(match (value["why"].as_str(), value["turn"].is_string()) {
                    (_, true) => "this page asks for a login, captcha or payment, which only the person does".into(),
                    (Some("control"), _) => {
                        let name =
                            value["seen"][0].as_str().unwrap_or_default().chars().filter(|c| !c.is_control()).take(40).collect::<String>();
                        format!("that point is a control (\"{name}\"); click it by its number instead")
                    }
                    _ => "nothing is at that point; it is outside the page's view".into(),
                });
            }
        }
        let mouse = |kind: &'static str, px: u32, py: u32, buttons: u32| json!({ "type": kind, "x": px, "y": py, "button": "left", "buttons": buttons, "clickCount": 1 });
        self.person_took_it(taken)?;
        self.call("Input.dispatchMouseEvent", mouse("mouseMoved", x, y, 0)).await?;
        self.call("Input.dispatchMouseEvent", mouse("mousePressed", x, y, 1)).await?;
        let end = to.unwrap_or((x, y));
        if to.is_some() {
            for step in 1..=DRAG_STEPS {
                let along = |from: u32, to: u32| {
                    (i64::from(from) + (i64::from(to) - i64::from(from)) * i64::from(step) / i64::from(DRAG_STEPS)) as u32
                };
                self.call("Input.dispatchMouseEvent", mouse("mouseMoved", along(x, end.0), along(y, end.1), 1)).await?;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
        self.call("Input.dispatchMouseEvent", mouse("mouseReleased", end.0, end.1, 0)).await?;
        Ok(())
    }

    /// The picture the agent asked for: the page in view with the numbers of the last read drawn on it (taken off again whatever
    /// happens), one control, or a part of the page enlarged. Only a JPEG while the call runs; none when the picture failed.
    async fn shoot(&self, shot: &Shot, reported: Option<&str>, taken: u64) -> Result<Option<Vec<u8>>, String> {
        let size = crate::lock(&self.state).size;
        let (width, height) = size.map_or(PARKED_SIZE, |s| (f64::from(s.width), f64::from(s.height)));
        let scroll = self.evaluate("({ x: scrollX, y: scrollY })", taken, false).await?;
        let at = |key: &str| scroll[key].as_f64().filter(|v| v.is_finite() && *v >= 0.0).unwrap_or(0.0).min(1e7);
        let act = |request: Value| format!("({})({request})", act_function());
        let clip = match shot {
            Shot::Labeled => {
                let labels = json!({ "kind": "label", "memo": self.memo(), "mark": self.mark });
                let _ = self.evaluate(&act(labels), taken, false).await;
                let clip = json!({ "x": at("x"), "y": at("y"), "width": width, "height": height, "scale": 1.0 });
                let picture = self.capture(clip).await;
                let _ = self.evaluate(&act(json!({ "kind": "unlabel", "mark": self.mark })), taken, false).await;
                self.person_took_it(taken)?;
                return Ok(picture);
            }
            Shot::Element(target) => {
                let request =
                    json!({ "kind": "click", "box": true, "target": words(target), "ref": number(target), "at": bound(reported)? });
                let found = self.act_value(request, taken).await?;
                if found["ok"].as_bool() != Some(true) {
                    return Err("the page asks for something only the person does there; no picture of it".into());
                }
                let edge = |i: usize| found["rect"][i].as_f64().unwrap_or(0.0);
                let left = (found["x"].as_f64().unwrap_or(0.0) + edge(0) - SHOT_PAD).max(0.0);
                let top = (found["y"].as_f64().unwrap_or(0.0) + edge(1) - SHOT_PAD).max(0.0);
                let (w, h) = ((edge(2) + 2.0 * SHOT_PAD).clamp(1.0, SHOT_MAX), (edge(3) + 2.0 * SHOT_PAD).clamp(1.0, SHOT_MAX));
                json!({ "x": left, "y": top, "width": w, "height": h, "scale": if w.max(h) <= 400.0 { 2.0 } else { 1.0 } })
            }
            Shot::Region { x, y, width, height } => {
                let (w, h) = (f64::from(*width).min(SHOT_MAX), f64::from(*height).min(SHOT_MAX));
                json!({ "x": at("x") + f64::from(*x), "y": at("y") + f64::from(*y), "width": w, "height": h, "scale": (1280.0 / w.max(h)).clamp(1.0, 4.0) })
            }
        };
        let picture = self.capture(clip).await;
        self.person_took_it(taken)?;
        Ok(picture)
    }

    /// A JPEG of `clip` (page pixels and a scale), none when the view could not draw it.
    async fn capture(&self, clip: Value) -> Option<Vec<u8>> {
        let shot = self.call("Page.captureScreenshot", json!({ "format": "jpeg", "quality": 70, "clip": clip })).await.ok()?;
        shot["data"].as_str().and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok())
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
        // A flow that moved midway is not run again from its first step.
        for _ in 1..if matches!(request, Request::Steps(_)) { 1 } else { SENDS } {
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
            pane.clone().act_page(conversation.to_string(), Some(tab.to_string()), Request::Open(url), true).await?;
        }
        if let Request::Steps(steps) = request {
            return pane.steps(conversation, tab, steps).await;
        }
        pane.act_page(conversation.to_string(), Some(tab.to_string()), request.clone(), false).await
    }

    /// A flow: each step in order, stopping at the first that fails or when [`FLOW_TIME`] is over, then one read of the page,
    /// with a line per step run. A step that fails is an answer, not an error: the agent gets the page as it stands.
    async fn steps(self: Arc<Self>, conversation: &str, tab: &str, steps: &[Step]) -> Result<Page, String> {
        let end = tokio::time::Instant::now() + FLOW_TIME;
        let mut log = Vec::new();
        for (n, step) in steps.iter().enumerate() {
            if tokio::time::Instant::now() >= end {
                log.push(format!("stopped before step {}: the flow took more than {} seconds", n + 1, FLOW_TIME.as_secs()));
                break;
            }
            let done = match step {
                Step::Act(request) => {
                    self.clone().act_page(conversation.to_string(), Some(tab.to_string()), request.clone(), true).await.map(drop)
                }
                Step::Wait { text, seconds } => self.wait_text(conversation, text.as_deref(), *seconds, end).await,
                Step::Assert { text, absent } => match self.has_text(conversation, text).await {
                    Ok(here) if here != *absent => Ok(()),
                    Ok(_) => Err(if *absent { "that text is still on the page" } else { "that text is not on the page" }.into()),
                    Err(e) => Err(e),
                },
            };
            match done {
                Ok(()) => log.push(format!("{}. {} - done", n + 1, step.label())),
                Err(why) => {
                    log.push(format!("{}. {} - failed: {why}", n + 1, step.label()));
                    break;
                }
            }
        }
        match self.clone().act_page(conversation.to_string(), Some(tab.to_string()), Request::Read, false).await {
            Ok(mut page) => {
                page.steps = log;
                Ok(page)
            }
            Err(e) => Err(format!("{e} (the flow so far: {})", log.join("; "))),
        }
    }

    /// Whether `text` is on the page now, as a person reads it; false for a page that is not this conversation's.
    async fn has_text(&self, conversation: &str, text: &str) -> Result<bool, String> {
        let taken = {
            let s = crate::lock(&self.state);
            if !s.open || s.owner.as_deref() != Some(conversation) {
                return Err("the page open in Faber's browser is not this conversation's; open yours again with its address".into());
            }
            s.taken
        };
        self.person_took_it(taken)?;
        let request = json!({ "kind": "has", "text": text });
        let value = self.evaluate(&format!("({})({request})", act_function()), taken, false).await?;
        Ok(value["ok"].as_bool() == Some(true))
    }

    /// Waits for `text` to be on the page, up to `seconds` and the end of the flow; without text, just waits.
    async fn wait_text(&self, conversation: &str, text: Option<&str>, seconds: u32, end: tokio::time::Instant) -> Result<(), String> {
        let until = (tokio::time::Instant::now() + Duration::from_secs(seconds.into())).min(end);
        let Some(text) = text else {
            tokio::time::sleep_until(until).await;
            return Ok(());
        };
        loop {
            if self.has_text(conversation, text).await? {
                return Ok(());
            }
            if tokio::time::Instant::now() >= until {
                return Err("that text did not appear in time".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
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

/// The words of a target, empty for a number.
fn words(target: &Target) -> &str {
    match target {
        Target::Words(w) => w,
        Target::Ref(_) => "",
    }
}

/// The number of a target, none for words.
fn number(target: &Target) -> Option<u32> {
    match target {
        Target::Ref(n) => Some(*n),
        Target::Words(_) => None,
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
