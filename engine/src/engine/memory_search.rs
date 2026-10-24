//! `memory_search` of a project's agent (`crate::memory_search`): the glue. Under the lock it only clones the search handle and
//! reads the project's folder; the search, the kept decisions and the notes list (file reads, up to five seconds) run on a blocking
//! worker, and the answer goes back only to the process that asked.
use super::*;
use crate::{memory_search, toolbox};

impl Core {
    pub(super) fn memory_search_call(&mut self, conv: &str, sid: u64, call: String, input: &serde_json::Value) {
        let query = match memory_search::query(input) {
            Ok(query) => query,
            Err(text) => return self.answer_toolbox(conv, sid, &call, false, text.into()),
        };
        // Re-read here: Codex keeps the tool list of a thread, so being offered the tool proves nothing about the area now.
        let project = self.convs.get(conv).filter(|c| c.area == Area::Project).and_then(|c| c.project_id.clone());
        let Some((project, folder)) = project.and_then(|id| self.project_of(Some(&id)).map(|p| (p.id, p.path))) else {
            return self.answer_toolbox(conv, sid, &call, false, toolbox::MEMORY_NOT_HERE.into());
        };
        let (search, kept) = (self.store.search(), self.store.kept_decisions_reader(&project));
        let (me, conv) = (self.me.clone(), conv.to_string());
        let asking = conv.clone();
        let read = self.opts.runtime.spawn_blocking(move || {
            let found = search.query_project(&query, &project, &asking, memory_search::ASKED);
            let list = crate::project::memory::list_faber(Path::new(&folder)).map(|mut list| {
                let records = kept(&list);
                crate::project::memory::mark_kept(&mut list, &records);
                list
            });
            memory_search::answer(&query, &found, &list, now())
        });
        self.opts.runtime.spawn(async move {
            let (ok, text) = match read.await {
                Ok(text) => (true, text),
                Err(_) => (false, "Faber could not search the project's memory.".into()),
            };
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
        });
    }
}
