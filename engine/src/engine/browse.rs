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
            let (ok, text, pictures) = match tokio::time::timeout(OPEN_TIMEOUT, b.act(conv.clone(), request)).await {
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

    /// Where a call's picture is kept, when the conversation exists and the picture was taken.
    pub(super) fn site_picture(&self, conv: &str, call: &str) -> Option<PathBuf> {
        self.store.get(conv)?;
        let path = self.opts.data_dir.join("attachments").join(conv).join(browser::picture_name(call)?);
        path.is_file().then_some(path)
    }
}

impl Engine {
    /// A conversation's tabs in Faber's browser (`browser::tabs`); none for one that is gone or never had a page.
    pub fn browser_tabs(&self, id: &str) -> Option<browser::tabs::Tabs> {
        self.with(|c| c.load(id)?.browser.clone())
    }

    /// A page finished loading in tab `tab` of conversation `id`'s browser (the front one when none), with its title when it is
    /// known: kept in the conversation's file, so the pane opens it again. A load that changes nothing writes nothing, and a
    /// page loading never moves the conversation in the sidebar.
    pub fn browser_seen(&self, id: &str, tab: Option<&str>, url: &str, title: Option<&str>) {
        self.with(|c| {
            let Some(conversation) = c.load(id) else { return };
            let tabs = conversation.browser.get_or_insert_default();
            let changed = tabs.record(tab, url, title);
            if tabs.tabs.is_empty() {
                conversation.browser = None;
            }
            if changed {
                c.save(id);
            }
        });
    }
}
