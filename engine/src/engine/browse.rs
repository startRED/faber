//! Faber's browser in a conversation (R161 step 7): when a project's agent calls the browser tool, the engine checks the
//! address, has the shell's browser open it, keeps its picture in the conversation's attachments (Faber's own data, never
//! the project folder, deleted with the conversation) and answers the call with the page's text.
use super::*;
use crate::browser;

/// The longest a page may take to open; the agent and its turn wait for it all that time.
const OPEN_TIMEOUT: Duration = Duration::from_secs(45);

impl Core {
    /// Whether a conversation offers the browser: a project's own agent in full access, where the shell has a browser. The
    /// modes that ask have no card for a page yet, and Codex runs its dynamic tools without one, so they do not offer it.
    pub(super) fn offers_browser(&self, area: Area, permissions: Permissions) -> bool {
        area == Area::Project && permissions == Permissions::FullAccess && self.browser.is_some()
    }

    /// The agent called the browser. An address that cannot open is answered at once with why, so the agent goes on.
    pub(super) fn browse(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        let opened = match (self.browser.clone(), browser::checked_url(&input)) {
            (None, _) => Err("Faber's browser is not available here.".to_string()),
            (_, Err(why)) => Err(format!("The page did not open: {why}.")),
            (Some(b), Ok(url)) => Ok((b, url)),
        };
        let (b, url) = match opened {
            Ok(open) => open,
            Err(text) => return self.answer_browse(conv, sid, &call, false, text),
        };
        let (me, conv, dir) = (self.me.clone(), conv.to_string(), self.opts.data_dir.join("attachments").join(conv));
        self.opts.runtime.spawn(async move {
            let (ok, text) = match tokio::time::timeout(OPEN_TIMEOUT, b.open(url)).await {
                Ok(Ok(page)) => {
                    // A picture that cannot be kept only leaves the card without one.
                    if let (Some(picture), Some(name)) = (&page.picture, browser::picture_name(&call)) {
                        let _ = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(name), picture));
                    }
                    (true, browser::result_text(&page))
                }
                Ok(Err(why)) => (false, format!("The page did not open: {why}.")),
                Err(_) => (false, "The page took longer than 45 seconds to load; it stays in Faber's browser, without its text.".into()),
            };
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_browse(&conv, sid, &call, ok, text);
        });
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_browse(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }

    /// Where a call's picture is kept, when the conversation exists and the picture was taken.
    pub(super) fn site_picture(&self, conv: &str, call: &str) -> Option<PathBuf> {
        self.store.get(conv)?;
        let path = self.opts.data_dir.join("attachments").join(conv).join(browser::picture_name(call)?);
        path.is_file().then_some(path)
    }
}
