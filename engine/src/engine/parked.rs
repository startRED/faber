//! Requests the CLI denied because nobody could answer them. Full access starts Claude Code with `--permission-prompts none`,
//! so a request that would have prompted is denied at once and comes back in the turn's result. It is kept on the reply
//! (`Message::parked`), shows in the running list as "needs you" until the next message, and is never allowed on its own.

use super::*;

impl Core {
    /// Keeps one denied request on the running turn. A request on a credential folder was refused for good (the deny rules,
    /// and the engine's own check on every request): it is no question for the person, so it is dropped, not parked.
    pub(super) fn park(&mut self, conv: &str, parked: Parked) {
        let cwd = self.convs.get(conv).and_then(|c| self.cwd_of(c).ok());
        let credentials =
            crate::permissions::touches_credentials(&parked.input, cwd.as_deref().and_then(Path::to_str), &|name| std::env::var(name).ok());
        if credentials {
            return;
        }
        if let Some(m) = self.turn(conv) {
            let list = m.parked.get_or_insert_default();
            if !list.iter().any(|p| p.tool_use_id == parked.tool_use_id) {
                list.push(parked);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionCommand;
    use serde_json::json;

    fn engine(dir: &Path) -> Engine {
        let opts = EngineOptions {
            data_dir: dir.into(),
            clis: BTreeMap::new(),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        Engine::new(opts, Arc::new(|_| {}))
    }

    /// A chat with one streaming reply on a live full-access session that counts every answer it is given.
    fn seed(engine: &Engine) -> (String, Arc<Mutex<usize>>) {
        let commands = Arc::new(Mutex::new(0));
        let counted = commands.clone();
        let session = Session::from_fn(move |cmd: SessionCommand| {
            if matches!(cmd, SessionCommand::Answer { .. }) {
                *counted.lock().unwrap() += 1;
            }
        });
        let id = engine.with(|c| {
            let id = c.blank(Area::Chat, None);
            let conv = c.convs.get_mut(&id).unwrap();
            conv.messages.push(Message::new("question".into(), Role::User, "question".into(), 1));
            let mut reply = Message::new("reply".into(), Role::Assistant, String::new(), 2);
            reply.vendor = Some(Vendor::Claude);
            reply.status = Some(Status::Streaming);
            conv.messages.push(reply);
            c.live.insert(
                id.clone(),
                Live {
                    cwd: PathBuf::new(),
                    vendor: Vendor::Claude,
                    model: None,
                    running: None,
                    effort: "medium".into(),
                    sid: 1,
                    session,
                    turn: Some("reply".into()),
                    break_pending: false,
                    answer_started: false,
                    used: 0,
                    cli: Cli { command: "claude".into(), prefix: vec![] },
                    resumed: false,
                    permissions: Permissions::FullAccess,
                    by_hand: true,
                    approved: false,
                    started: 0.0,
                    served: false,
                    steering: vec![],
                },
            );
            c.refresh_running();
            id
        });
        (id, commands)
    }

    fn done() -> EngineEvent {
        EngineEvent::Done {
            ok: true,
            reason: String::new(),
            cost_total_usd: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cache_write_1h_tokens: 0,
            incomplete: false,
        }
    }

    fn parked(id: &str, tool: &str, input: serde_json::Value) -> EngineEvent {
        EngineEvent::Parked { tool_use_id: id.into(), tool: tool.into(), input }
    }

    #[tokio::test]
    async fn a_denied_request_is_parked_survives_the_end_of_the_turn_and_needs_the_person() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, commands) = seed(&engine);
        let note = json!({ "file_path": "/work/note.txt", "content": "hi" });
        engine.with(|c| {
            c.handle(&id, 1, parked("toolu_1", "Write", note.clone()));
            c.handle(&id, 1, parked("toolu_1", "Write", note.clone()));
            c.handle(&id, 1, parked("toolu_2", "Bash", json!({ "command": "git push" })));
            c.handle(&id, 1, done());
        });
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        let tools: Vec<_> = reply.parked.as_ref().unwrap().iter().map(|p| (p.tool_use_id.as_str(), p.tool.as_str())).collect();
        assert_eq!(tools, [("toolu_1", "Write"), ("toolu_2", "Bash")], "kept in order, once each");
        assert_eq!(reply.status, Some(Status::Done));
        let items = engine.state().running.items;
        assert_eq!(items.len(), 1, "the finished turn stays listed");
        assert_eq!(items[0].state, RunState::NeedsYou);
        assert_eq!(items[0].parked.len(), 2);
        assert!(items[0].permissions.is_empty(), "there is no card to answer: the CLI already denied it");
        assert_eq!(*commands.lock().unwrap(), 0, "no answer went to the CLI: a parked request is never allowed");
    }

    #[tokio::test]
    async fn the_next_message_takes_the_conversation_off_the_needs_you_list_but_keeps_the_note() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, _) = seed(&engine);
        engine.with(|c| {
            c.handle(&id, 1, parked("toolu_1", "Write", json!({ "file_path": "/work/a" })));
            c.handle(&id, 1, done());
            c.convs.get_mut(&id).unwrap().messages.push(Message::new("later".into(), Role::User, "go ahead".into(), 9));
            c.refresh_running();
        });
        assert!(engine.state().running.items.is_empty());
        assert_eq!(engine.open(&id).unwrap().messages[1].parked.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_denied_request_on_a_credential_folder_is_never_parked() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, commands) = seed(&engine);
        engine.with(|c| {
            for (n, input) in [
                json!({ "file_path": "~/.claude/.credentials.json" }),
                json!({ "file_path": "~/.ssh/id_ed25519", "content": "x" }),
                json!({ "command": "cat ~/.codex/auth.json" }),
            ]
            .into_iter()
            .enumerate()
            {
                c.handle(&id, 1, parked(&format!("toolu_{n}"), "Read", input));
            }
            c.handle(&id, 1, done());
        });
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert!(reply.parked.is_none(), "{:?}", reply.parked);
        assert!(engine.state().running.items.is_empty(), "no needs-you note for what is refused for good");
        assert_eq!(*commands.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn the_cache_lifetime_of_a_turn_is_kept_on_its_reply() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, _) = seed(&engine);
        let before = now();
        engine.with(|c| {
            c.handle(&id, 1, EngineEvent::CacheWarm { secs: 3600 });
            c.handle(&id, 1, done());
        });
        let until = engine.open(&id).unwrap().messages.pop().unwrap().cache_warm_until.unwrap();
        assert!(until >= before + 3_600_000 && until <= now() + 3_600_000, "{until}");
        // A turn that reported no bucket leaves it unknown.
        let (other, _) = seed(&engine);
        engine.with(|c| c.handle(&other, 1, done()));
        assert_eq!(engine.open(&other).unwrap().messages.pop().unwrap().cache_warm_until, None);
    }
}
