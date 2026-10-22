//! Faber's browser in a conversation (R161 step 7): when a project's agent calls the browser tool, the engine checks the
//! address, has the shell's browser open it, keeps its picture in the conversation's attachments (Faber's own data, never
//! the project folder, deleted with the conversation) and answers the call with the page's text.
use super::*;
use crate::browser;

/// The longest a page may take to open; the agent and its turn wait for it all that time.
const OPEN_TIMEOUT: Duration = Duration::from_secs(45);

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
        let opened = match (self.browser.clone(), browser::checked_request(&input)) {
            (None, _) => Err("Faber's browser is not available here.".to_string()),
            (_, Err(why)) => Err(format!("Faber's browser did not do it: {why}.")),
            (Some(b), Ok(request)) => Ok((b, request)),
        };
        let (b, mut request) = match opened {
            Ok(open) => open,
            Err(text) => return self.answer_browse(conv, sid, &call, false, text, Vec::new()),
        };
        // A check at the sizes that matter, with none named: the ones of where the project runs, from its own files.
        let folder = matches!(&request, browser::Request::Sizes(sizes) if sizes.is_empty())
            .then(|| self.convs.get(conv).and_then(|c| self.cwd_of(c).ok()))
            .flatten();
        // The agent acts only in its own tab: a new page opens there (a new tab unless the one in front is blank), and every
        // other action needs the page it opened, never the one the person is on.
        let Some(tab) = self.agent_tab(conv, matches!(request, browser::Request::Open(_))) else {
            let text = "Faber's browser did not do it: no page of yours is open; open one with its address first.".to_string();
            return self.answer_browse(conv, sid, &call, false, text, Vec::new());
        };
        let (me, conv, dir) = (self.me.clone(), conv.to_string(), self.opts.data_dir.join("attachments").join(conv));
        self.opts.runtime.spawn(async move {
            let mut picked = None;
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
            let (ok, text, pictures) = match tokio::time::timeout(OPEN_TIMEOUT, b.act(conv.clone(), tab, request)).await {
                Ok(Ok(mut page)) => {
                    page.picked = picked;
                    // A picture that cannot be kept only leaves the card without one. Each size's picture is the call's
                    // with the size's place after it (`<call>-s1`), so the screen asks for it like any other.
                    let pictures = std::iter::once((call.clone(), page.picture.as_ref()))
                        .chain(page.looks.iter().enumerate().map(|(n, look)| (format!("{call}-s{}", n + 1), look.picture.as_ref())));
                    let mut kept = false;
                    for (id, picture) in pictures {
                        if let (Some(picture), Some(name)) = (picture, browser::picture_name(&id)) {
                            kept |= std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(name), picture)).is_ok();
                        }
                    }
                    if kept {
                        browser::trim_pictures(&dir, browser::MAX_PICTURES_BYTES);
                    }
                    // The agent sees each size's picture too (the owner, 2026-10-19), in the order of its lines; a page that is
                    // the person's turn has none.
                    (true, browser::result_text(&page), browser::look_pictures(&page))
                }
                Ok(Err(why)) => (false, format!("Faber's browser did not do it: {why}."), Vec::new()),
                Err(_) => {
                    (false, "The page took longer than 45 seconds; it stays in Faber's browser, without its text.".into(), Vec::new())
                }
            };
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_browse(&conv, sid, &call, ok, text, pictures);
        });
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

    /// Where a call's picture is kept, when the conversation exists and the picture was taken.
    pub(super) fn site_picture(&self, conv: &str, call: &str) -> Option<PathBuf> {
        self.store.get(conv)?;
        let path = self.opts.data_dir.join("attachments").join(conv).join(browser::picture_name(call)?);
        path.is_file().then_some(path)
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
