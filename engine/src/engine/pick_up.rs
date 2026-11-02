//! Picking up a session the person started with a CLI in a terminal (parity F5): the CLI lists its own sessions of the
//! project folder, and one becomes a project conversation whose next message resumes that session with the CLI's own
//! resume. Faber never reads the session's files; the CLI keeps its history and Faber shows only what is said from here.
use super::*;
use crate::model::CliSession;

impl Core {
    /// The project conversation that goes on with `session`: the one already made for it (archived too: it says so and
    /// offers Unarchive), or a new one in the project folder (never a separate copy, since the session's history belongs to
    /// the folder it ran in).
    pub(super) fn pick_up(&mut self, project_id: &str, session: CliSession) -> Result<String, String> {
        self.check_project_folder(Some(project_id))?;
        let holds = |c: &Conversation| c.project_id.as_deref() == Some(project_id) && c.sessions.get(&session.vendor) == Some(&session.id);
        if let Some(id) = self.convs.values().find(|c| holds(c)).map(|c| c.id.clone()) {
            return Ok(id);
        }
        for id in self.store.mentioning(&session.id) {
            if self.store.get(&id).is_some_and(|c| holds(&c)) {
                return Ok(id);
            }
        }
        let id = self.blank(Area::Project, Some(project_id.to_string()));
        // The model the session used, while the account still offers it (or before the account's list has come), else the
        // account's default: the picker shows a Codex model, never Auto, which could send the next message to the other CLI.
        let model = match session.vendor {
            Vendor::Codex => {
                let known = |m: &String| self.codex_models.is_empty() || self.codex_models.contains(m);
                session.model.filter(known).or_else(|| self.codex_recommended.clone()).or_else(|| self.codex_models.first().cloned())
            }
            // The picker names Claude's models by family.
            Vendor::Claude => session.model.as_deref().and_then(crate::budget::family_of).map(String::from),
        };
        let c = self.convs.get_mut(&id).expect("new conversation");
        c.title = session.title.chars().take(60).collect();
        c.sessions.insert(session.vendor, session.id);
        c.choice = Some(crate::router::Choice { vendor: session.vendor.name().into(), model, effort: None });
        self.save(&id);
        self.list_changed();
        Ok(id)
    }

    /// The first message of a picked-up session goes on with that session's CLI: Auto would start the other one, which
    /// knows nothing of it.
    pub(super) fn own_cli(&mut self, req: SendRequest) -> SendRequest {
        if req.choice.vendor != "auto" {
            return req;
        }
        let clis: Vec<Vendor> = self.opts.clis.keys().copied().collect();
        let Some(c) = req.conversation_id.as_deref().and_then(|id| self.load(id)) else { return req };
        if !c.messages.is_empty() {
            return req;
        }
        let Some(vendor) = c.sessions.keys().copied().find(|v| clis.contains(v)) else { return req };
        let own = c.choice.clone().filter(|ch| ch.vendor == vendor.name());
        SendRequest { choice: own.unwrap_or(crate::router::Choice { vendor: vendor.name().into(), model: None, effort: None }), ..req }
    }
}

impl Engine {
    /// The sessions started in a terminal in project `project_id`'s folder, newest first, of each connected CLI: Codex's
    /// from its own list (one short-lived process), Claude Code's from its history files of that folder
    /// (`crate::claude_sessions`), since it has no list to ask.
    pub async fn cli_sessions(&self, project_id: &str, timeout: Duration) -> Result<Vec<CliSession>, String> {
        let (folder, codex, claude, spawn, windows) = self.with(|c| {
            c.check_project_folder(Some(project_id))?;
            let project = c.project_of(Some(project_id)).ok_or("error.projectGone")?;
            let clis = &c.opts.clis;
            Ok::<_, String>((
                project.path,
                clis.get(&Vendor::Codex).cloned(),
                clis.contains_key(&Vendor::Claude),
                c.opts.spawn.clone(),
                c.opts.windows,
            ))
        })?;
        // One deadline for both lists, which are read side by side.
        let deadline = tokio::time::Instant::now() + timeout;
        // A read still stuck on a stalled network folder is not joined by another: each would hold a thread.
        static READING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        use std::sync::atomic::Ordering::SeqCst;
        let claude = (claude && READING.compare_exchange(false, true, SeqCst, SeqCst).is_ok()).then(|| {
            let folder = PathBuf::from(&folder);
            tokio::task::spawn_blocking(move || {
                let list = crate::claude_sessions::config_dir(&|k| std::env::var(k).ok())
                    .map(|config| crate::claude_sessions::list_claude_sessions(&config, &folder, 30))
                    .unwrap_or_default();
                READING.store(false, SeqCst);
                list
            })
        });
        let mut list = match codex {
            Some(cli) => {
                let prefix = cli.prefix;
                let spawn: crate::proc::Spawner =
                    Arc::new(move |bin, args, cwd| spawn(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
                crate::codex::list_codex_threads(&cli.command, Path::new(&folder), windows, spawn, timeout).await
            }
            None => vec![],
        };
        if let Some(read) = claude {
            list.extend(tokio::time::timeout_at(deadline, read).await.ok().and_then(Result::ok).unwrap_or_default());
        }
        list.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
        Ok(list)
    }
}
