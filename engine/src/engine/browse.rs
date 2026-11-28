//! Faber's browser in a conversation (R161 step 7): when a project's agent calls the browser tool, the engine checks the
//! address, has the shell's browser open it, keeps its picture in the conversation's attachments (Faber's own data, never
//! the project folder, deleted with the conversation) and answers the call with the page's text.
use super::*;
use crate::browser;

/// The longest a page may take to open; the agent and its turn wait for it all that time.
const OPEN_TIMEOUT: Duration = Duration::from_secs(45);
/// A whole flow of steps: the host stops it at 120 seconds and answers; this is only the engine's last word.
const FLOW_TIMEOUT: Duration = Duration::from_secs(135);

impl Core {
    /// Whether a conversation offers the browser: a project's own agent in full access, where the shell has a browser. The
    /// modes that ask have no card for a page yet. Codex retains the tool definition across modes, so calls check this too.
    pub(super) fn offers_browser(&self, area: Area, permissions: Permissions) -> bool {
        area == Area::Project && permissions == Permissions::FullAccess && self.browser.is_some()
    }

    /// The agent called the browser. A request that cannot be done is answered at once with why, so the agent goes on.
    pub(super) fn browse(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(live) = self.live.get(conv) else { return };
        let sid = live.sid;
        let area = self.convs.get(conv).map_or(Area::Chat, |c| c.area);
        if !self.offers_browser(area, live.permissions) {
            let why = if self.browser.is_none() {
                "Faber's browser is not available here."
            } else {
                "Faber's browser is available only in project conversations with full access."
            };
            return self.answer_browse(conv, sid, &call, false, why.into(), Vec::new());
        }
        if input["action"].as_str().is_some_and(|action| action.starts_with("extensions_")) {
            return self.browse_extensions(conv, sid, call, input);
        }
        let opened = match (self.browser.clone(), browser::checked_request(&input)) {
            (None, _) => Err("Faber's browser is not available here.".to_string()),
            (_, Err(why)) => Err(format!("Faber's browser did not do it: {why}.")),
            (Some(b), Ok(request)) => Ok((b, request)),
        };
        let (b, mut request) = match opened {
            Ok(open) => open,
            Err(text) => return self.answer_browse(conv, sid, &call, false, text, Vec::new()),
        };
        if let browser::Request::Open(url) = &request
            && let Err(why) = browser::check_blocked_url(url, self.store.settings().blocked_hosts.as_deref().unwrap_or_default())
        {
            return self.answer_browse(conv, sid, &call, false, why.to_string(), Vec::new());
        }
        // A check at the sizes that matter, with none named: the ones of where the project runs, from its own files.
        let folder = matches!(&request, browser::Request::Sizes(sizes) if sizes.is_empty())
            .then(|| self.convs.get(conv).and_then(|c| self.cwd_of(c).ok()))
            .flatten();
        // The agent acts only in its own tab: a new page opens there (a new tab unless the one in front is blank), and every
        // other action needs the page it opened, never the one the person is on.
        let Some(tab) = (if matches!(request, browser::Request::App(_)) {
            Some(String::new())
        } else {
            self.agent_tab(conv, matches!(request, browser::Request::Open(_)))
        }) else {
            let text = "Faber's browser did not do it: no page of yours is open; open one with its address first.".to_string();
            return self.answer_browse(conv, sid, &call, false, text, Vec::new());
        };
        let part = self.convs.get(conv).and_then(|c| c.plan.as_ref()).and_then(|plan| {
            let mut active = plan.parts.iter().filter(|p| !p.removed && matches!(p.state, PartState::Now | PartState::Redo));
            let first = active.next()?;
            active.next().is_none().then(|| first.id.clone())
        });
        let (me, conv, data_dir) = (self.me.clone(), conv.to_string(), self.opts.data_dir.clone());
        self.opts.runtime.spawn(async move {
            let mut picked = None;
            let limit = if matches!(request, browser::Request::Steps(_)) { FLOW_TIMEOUT } else { OPEN_TIMEOUT };
            if let browser::Request::Sizes(sizes) = &mut request
                && sizes.is_empty()
            {
                let kind = match folder {
                    Some(folder) => {
                        tokio::task::spawn_blocking(move || crate::screens::kind_of(&folder)).await.unwrap_or(crate::screens::Kind::Web)
                    }
                    None => crate::screens::Kind::Web,
                };
                *sizes = crate::screens::sizes_of(kind.screens());
                picked = Some(kind);
            }
            let result = tokio::time::timeout(limit, b.act(conv.clone(), tab, request)).await;
            let Some(core) = me.upgrade() else { return };
            if !core.lock().unwrap_or_else(|e| e.into_inner()).accepts_browse(&conv, sid) {
                return;
            }
            let (ok, text, pictures, pending) = match result {
                Ok(Ok(mut page)) => {
                    page.picked = picked;
                    let call_for_stage = call.clone();
                    let (page, pending) = tokio::task::spawn_blocking(move || {
                        let pending = stage_browse_pictures(&data_dir, &call_for_stage, &page);
                        (page, pending)
                    })
                    .await
                    .unwrap_or_default();
                    // The agent sees each size's picture too (the owner, 2026-10-19), in the order of its lines; a page that is
                    // the person's turn has none.
                    (true, browser::result_text(&page), browser::look_pictures(&page), pending)
                }
                Ok(Err(why)) => (false, format!("Faber's browser did not do it: {why}."), Vec::new(), Vec::new()),
                Err(_) => (
                    false,
                    format!("The page took longer than {} seconds; it stays in Faber's browser, without its text.", limit.as_secs()),
                    Vec::new(),
                    Vec::new(),
                ),
            };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            core.finish_browse_pictures(&conv, sid, &call, part.as_deref(), pending, browser::MAX_PICTURES_BYTES);
            let text = if ok { text } else { browser::mask::mask(&text) };
            core.answer_browse(&conv, sid, &call, ok, text, pictures);
        });
    }

    fn accepts_browse(&self, conv: &str, sid: u64) -> bool {
        self.live.get(conv).is_some_and(|l| l.sid == sid) && self.convs.get(conv).is_some_and(|c| !c.stopped)
    }

    fn finish_browse_pictures(&mut self, conv: &str, sid: u64, call: &str, part: Option<&str>, pending: PendingPictures, max: u64) {
        if pending.is_empty() || !self.accepts_browse(conv, sid) {
            return;
        }
        let dir = self.opts.data_dir.join("attachments").join(conv);
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let Ok(max) = browser::tool_picture_budget(&dir, max) else { return };
        // Publish, link and trim together under Core's lock. Other captures are still outside this directory.
        let main = browser::picture_name(call);
        let mut kept = false;
        let mut primary = false;
        for (name, bytes, picture) in pending {
            if bytes <= max && picture.persist_noclobber(dir.join(&name)).is_ok() {
                kept = true;
                primary |= Some(&name) == main.as_ref();
            }
        }
        if !kept {
            return;
        }
        let parts = self.convs.get_mut(conv).and_then(|c| c.plan.as_mut()).map_or(&mut [][..], |p| p.parts.as_mut_slice());
        let mut changed = false;
        if primary
            && let Some(part) = part.and_then(|id| parts.iter_mut().find(|p| p.id == id))
            && !part.pictures.iter().any(|id| id == call)
        {
            part.pictures.push(call.to_string());
            if part.pictures.len() > 8 {
                part.pictures.remove(1);
            }
            changed = true;
        }
        changed |= browser::trim_pictures(&dir, max, parts);
        if changed {
            self.save(conv);
            (self.emit)(AppEvent::Plan { conversation_id: conv.to_string(), plan: self.shown_plan(conv) });
        }
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_browse(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String, pictures: Vec<String>) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply_with_pictures(call, ok, text, pictures);
        }
    }

    /// The tab the agent's call acts in: for a new page its own tab (made when it has none: `Tabs::agent_tab`), for anything
    /// else the tab it already has, none when it has none. The record is saved when it changed.
    fn agent_tab(&mut self, conv: &str, opens: bool) -> Option<String> {
        let c = self.load(conv)?;
        let before = c.browser.clone();
        let tabs = c.browser.get_or_insert_default();
        let tab = if opens { Some(tabs.agent_tab()) } else { tabs.agent.clone().filter(|id| tabs.get(id).is_some()) };
        if tabs.tabs.is_empty() && tabs.closed.is_empty() {
            c.browser = None;
        }
        if c.browser != before {
            self.save(conv);
        }
        tab
    }

    pub(super) fn has_part_picture(&self, conv: &str, call: &str) -> bool {
        browser::is_picture_call(call)
            && super::pictures::tool_picture_path(&self.opts.data_dir.join("attachments").join(conv), call, false).is_some()
    }
}

/// All browser picture writers hold Core's lock so the budget and conversation deletion cannot race a write.
pub(super) fn keep_picture(dir: &Path, name: &str, picture: &[u8], tool_picture: bool) -> std::io::Result<bool> {
    use std::io::Write as _;
    if picture.is_empty() || !browser::picture_room(dir, name, picture.len() as u64, browser::MAX_PICTURES_BYTES, tool_picture)? {
        return Ok(false);
    }
    let answers = dir.join(browser::ANSWER_PICTURES_DIR);
    let dir = if tool_picture { dir } else { answers.as_path() };
    std::fs::create_dir_all(dir)?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(picture)?;
    file.persist(dir.join(name))?;
    Ok(true)
}

type PendingPictures = Vec<(String, u64, tempfile::TempPath)>;

/// Write the JPEG bytes off-lock on the same volume, outside the directory trimming scans.
fn stage_browse_pictures(dir: &Path, call: &str, page: &browser::Page) -> PendingPictures {
    use std::io::Write as _;
    let mut pending = Vec::new();
    if !browser::is_picture_call(call) || browser::picture_name(call).is_none() {
        return pending;
    }
    let stage = |id: &str, bytes: &[u8]| {
        let name = browser::picture_name(id)?;
        if bytes.is_empty() {
            return None;
        }
        let mut picture = tempfile::NamedTempFile::new_in(dir).ok()?;
        picture.write_all(bytes).ok()?;
        Some((name, bytes.len() as u64, picture.into_temp_path()))
    };
    let mut primary = page.shot.as_deref().or(page.picture.as_deref()).and_then(|picture| stage(call, picture));
    for (n, look) in page.looks.iter().enumerate() {
        if let Some(picture) = &look.picture
            && let Some(size) = stage(&format!("{call}-s{}", n + 1), picture)
        {
            pending.push(size);
            // A sizes-only capture has one representative under the call id, independent of its size cards.
            if primary.is_none() {
                primary = stage(call, picture);
            }
        }
    }
    pending.extend(primary);
    pending
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use crate::engine::answer::tests::{engine, seed};

    fn with_part() -> serde_json::Value {
        serde_json::json!({"parts":[{"id":"a","area":"Search","name":"Find it","state":"now"}]})
    }

    fn page(bytes: u8) -> browser::Page {
        browser::Page { picture: Some(vec![bytes; 100]), ..Default::default() }
    }

    #[tokio::test]
    async fn plan_parts_a_replaced_or_stopped_session_cannot_save_or_trim_captures() {
        let root = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&root);
        let id = seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let dir = root.path().join("attachments").join(&id);
        let pending = stage_browse_pictures(root.path(), "late", &page(1));
        engine.with(|core| {
            core.live.get_mut(&id).unwrap().sid = 2;
            core.write_progress(&id, crate::progress::PLAN_TOOL, &with_part()).unwrap();
            core.finish_browse_pictures(&id, 2, "current", Some("a"), stage_browse_pictures(root.path(), "current", &page(2)), 200);
            std::fs::write(dir.join("site-unlinked.jpg"), [9; 100]).unwrap();
            core.finish_browse_pictures(&id, 1, "late", Some("a"), pending, 0);
            core.convs.get_mut(&id).unwrap().stopped = true;
            core.finish_browse_pictures(&id, 2, "stopped", Some("a"), stage_browse_pictures(root.path(), "stopped", &page(3)), 0);
            assert_eq!(core.convs[&id].plan.as_ref().unwrap().parts[0].pictures, ["current"]);
        });
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        assert!(dir.join("site-unlinked.jpg").is_file(), "stale calls cannot run trim");
        assert_eq!(std::fs::read(engine.site_picture(&id, "current").unwrap()).unwrap(), vec![2; 100]);
    }

    #[tokio::test]
    async fn plan_parts_one_completion_cannot_trim_another_pending_capture() {
        let root = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&root);
        let id = seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let later = stage_browse_pictures(root.path(), "later", &page(2));
        let first = stage_browse_pictures(root.path(), "first", &page(1));
        engine.with(|core| {
            core.write_progress(&id, crate::progress::PLAN_TOOL, &with_part()).unwrap();
            core.finish_browse_pictures(&id, 1, "first", Some("a"), first, 100);
            assert!(
                super::super::pictures::picture_path(&root.path().join("attachments").join(&id), "later").is_none(),
                "a pending capture is still outside the trim directory"
            );
            core.finish_browse_pictures(&id, 1, "later", Some("a"), later, 100);
            assert_eq!(core.convs[&id].plan.as_ref().unwrap().parts[0].pictures, ["first", "later"]);
        });
        assert_eq!(std::fs::read(engine.site_picture(&id, "later").unwrap()).unwrap(), vec![2; 100]);
    }

    #[tokio::test]
    async fn plan_parts_browser_captures_share_the_budget_with_answer_pictures() {
        let root = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&root);
        let id = seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let dir = root.path().join("attachments").join(&id);
        let answers = dir.join(browser::ANSWER_PICTURES_DIR);
        std::fs::create_dir_all(&answers).unwrap();
        let answer = answers.join("before-answer-1.jpg");
        std::fs::write(&answer, [9; 60]).unwrap();
        std::fs::write(dir.join("site-unlinked.jpg"), [8; 60]).unwrap();

        engine.with(|core| {
            core.write_progress(&id, crate::progress::PLAN_TOOL, &with_part()).unwrap();
            let capture = |id| stage_browse_pictures(root.path(), id, &page(1));
            core.finish_browse_pictures(&id, 1, "too-large", Some("a"), capture("too-large"), 159);
            core.finish_browse_pictures(&id, 1, "fits", Some("a"), capture("fits"), 160);
            assert_eq!(core.convs[&id].plan.as_ref().unwrap().parts[0].pictures, ["fits"]);
        });
        assert!(!dir.join("site-unlinked.jpg").exists());
        assert!(engine.site_picture(&id, "too-large").is_none());
        assert!(engine.site_picture(&id, "fits").is_some());
        assert_eq!(std::fs::read(answer).unwrap(), [9; 60]);
    }

    #[tokio::test]
    async fn plan_parts_link_exact_picture_ids_and_keep_legacy_cards_readable() {
        let root = crate::workspace::TestRoot::new();
        let (engine, _) = engine(&root);
        let id = seed(&engine, Session::from_fn(|_| {}), &[("turn", &[])]);
        let dir = root.path().join("attachments").join(&id);
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("site-ab.jpg");
        std::fs::write(&legacy, [1; 100]).unwrap();
        engine.with(|core| {
            core.write_progress(&id, crate::progress::PLAN_TOOL, &with_part()).unwrap();
            let before = |call| serde_json::json!({"parts":[{"id":"a","before":call}]});
            assert!(
                core.write_progress(&id, crate::progress::PLAN_TOOL, &before("a.b")).unwrap_err().starts_with(crate::progress::BAD_BEFORE)
            );
            core.write_progress(&id, crate::progress::PLAN_TOOL, &before("ab")).unwrap();
            core.finish_browse_pictures(&id, 1, "a.b", Some("a"), stage_browse_pictures(root.path(), "a.b", &page(2)), 100);
            assert_eq!(core.convs[&id].plan.as_ref().unwrap().parts[0].pictures, ["a.b"]);
        });
        assert_eq!(std::fs::read(&legacy).unwrap(), vec![1; 100]);
        assert_eq!(std::fs::read(engine.site_picture(&id, "a.b").unwrap()).unwrap(), vec![2; 100]);
    }

    #[test]
    fn plan_parts_size_variants_fit_for_the_longest_call_id() {
        let dir = tempfile::tempdir().unwrap();
        let call = "x".repeat(120);
        let page = browser::Page {
            looks: vec![browser::Look {
                size: crate::screens::named("phone").unwrap(),
                picture: Some(vec![1]),
                wider: 0,
                cut: vec![],
                turn: None,
            }],
            ..Default::default()
        };
        let pending = stage_browse_pictures(dir.path(), &call, &page);
        assert_eq!(pending.len(), 2, "a sizes-only call has both the size image and its representative");
    }
}

/// What the person does to the tabs of a conversation's browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabStep {
    New,
    Bring(String),
    Close(String),
    CloseOthers(String),
    Reopen,
    /// The page of a reply's card: its own tab when one has it, else the blank one in front, else a new one.
    Show(String),
    /// "Clear this conversation's browsing data": every tab and every closed one go.
    Clear,
}

impl Engine {
    /// Does `step` to conversation `id`'s tabs and answers them with the tab it concerns (the new, reopened or shown one, or
    /// the one in front after a close); none when the conversation is gone or the step names no tab of it. Closing the
    /// last tab leaves none, and the screen closes the pane.
    pub fn browser_tab(&self, id: &str, step: TabStep) -> Option<(browser::tabs::Tabs, Option<String>)> {
        self.with(|c| {
            let conversation = c.load(id)?;
            let tabs = conversation.browser.get_or_insert_default();
            let (known, tab) = match &step {
                TabStep::New => (true, Some(tabs.new_tab())),
                TabStep::Bring(tab) => (tabs.bring(tab), Some(tab.clone())),
                TabStep::Close(tab) => (tabs.close(tab), tabs.front.clone()),
                TabStep::CloseOthers(tab) => (tabs.close_others(tab), Some(tab.clone())),
                TabStep::Reopen => {
                    let tab = tabs.reopen();
                    (tab.is_some(), tab)
                }
                TabStep::Clear => {
                    *tabs = browser::tabs::Tabs::default();
                    (true, None)
                }
                TabStep::Show(url) => {
                    let tab = browser::tabs::kept_url(url).map(|url| tabs.show(&url));
                    (tab.is_some(), tab)
                }
            };
            let answer = tabs.clone();
            if tabs.tabs.is_empty() && tabs.closed.is_empty() {
                conversation.browser = None;
            }
            if !known {
                return None;
            }
            c.save(id);
            Some((answer, tab))
        })
    }

    /// A conversation's tabs in Faber's browser (`browser::tabs`); none for one that is gone or never had a page.
    pub fn browser_tabs(&self, id: &str) -> Option<browser::tabs::Tabs> {
        self.with(|c| c.load(id)?.browser.clone())
    }

    /// A page finished loading in tab `tab` of conversation `id`'s browser (the front one when none), with its title when it is
    /// known: kept in the conversation's file, so the pane opens it again. A load that changes nothing writes nothing, and a
    /// page loading never moves the conversation in the sidebar. Answers whether the tabs changed.
    pub fn browser_seen(&self, id: &str, tab: Option<&str>, url: &str, title: Option<&str>) -> bool {
        self.with(|c| {
            let Some(conversation) = c.load(id) else { return false };
            let tabs = conversation.browser.get_or_insert_default();
            let changed = tabs.record(tab, url, title);
            if tabs.tabs.is_empty() && tabs.closed.is_empty() {
                conversation.browser = None;
            }
            if changed {
                c.save(id);
            }
            changed
        })
    }
}
