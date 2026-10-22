//! The agent's read-only toolbox (`crate::toolbox`): `quota` answers from the readings the engine already holds, `catalog` from
//! the shipped catalog and the store's ledger. Neither starts a request, so both are offered to a project's own agent in
//! any permission mode.
use super::*;
use crate::toolbox;

impl Core {
    pub(super) fn toolbox_call(&mut self, conv: &str, call: String, tool: &'static str, input: serde_json::Value) {
        let Some(sid) = self.live.get(conv).map(|l| l.sid) else { return };
        if tool == toolbox::QUOTA_TOOL {
            let text = toolbox::quota(&self.usage(), self.secs());
            return self.answer_toolbox(conv, sid, &call, true, text);
        }
        // The ledger is a small file and a few checks of the store's folders: read away from the lock.
        let store = self.catalog_store();
        let (me, conv) = (self.me.clone(), conv.to_string());
        let read = self.opts.runtime.spawn_blocking(move || {
            let installed = store.installed();
            let entries: Vec<_> = crate::tools::catalog()
                .iter()
                .map(|item| toolbox::Entry {
                    item,
                    plugin: store.plugin_definition(&item.id).is_some(),
                    installed: installed.contains_key(&item.id),
                    on: installed.contains_key(&item.id) && store.enabled(&item.id),
                })
                .collect();
            toolbox::catalog(&entries, &input)
        });
        self.opts.runtime.spawn(async move {
            let (ok, text) = read.await.unwrap_or_else(|_| (false, "Faber could not read the catalog.".into()));
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
        });
    }

    /// The answer goes back only to the process that asked: a conversation that was stopped or moved on drops it.
    fn answer_toolbox(&self, conv: &str, sid: u64, call: &str, ok: bool, text: String) {
        if let Some(l) = self.live.get(conv).filter(|l| l.sid == sid) {
            l.session.helper_reply(call, ok, text);
        }
    }
}
