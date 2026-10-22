//! Requests the CLI denied because nobody could answer them. Full access starts Claude Code with `--permission-prompts none`,
//! so a request that would have prompted is denied at once and comes back in the turn's result. It is kept on the reply
//! (`Message::parked`) as safe metadata only, shows in the running list as "needs you" until the next message, and is never
//! allowed on its own.

use super::*;
use serde_json::Value;

/// At most this many requests are kept per turn; the rest are only counted (`Message::parked_more`).
pub(super) const MAX_PARKED: usize = 20;
/// And at most this many bytes of metadata in all, so a turn that is denied a hundred things stays a few KB.
pub(super) const MAX_PARKED_BYTES: usize = 4096;
const MAX_LABEL: usize = 160;

fn cut(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// What may be kept of a denied request: its kind and a short label, never the request itself. A command line, a body or a
/// URL query can carry a token or an Authorization header, so none of them is stored. A file is named by its path and a page by
/// its host (no scheme, user, port, path or query); everything else has no label.
pub(super) fn describe(tool: &str, input: &Value) -> (&'static str, String) {
    let text = |keys: &[&str]| keys.iter().find_map(|k| input[*k].as_str()).unwrap_or_default();
    match tool {
        "Bash" | "PowerShell" | "command" => ("command", String::new()),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "Read" | "fileChange" => {
            ("file", cut(text(&["file_path", "path", "notebook_path"]), MAX_LABEL))
        }
        "WebFetch" => {
            let url = text(&["url"]);
            let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
            ("page", cut(host.split(':').next().unwrap_or_default(), MAX_LABEL))
        }
        _ => ("other", String::new()),
    }
}

impl Core {
    /// Keeps one denied request on the running turn, as metadata. A request on a credential folder was refused for good (the deny
    /// rules, and the engine's own check on every request): it is no question for the person, so it is dropped, not parked.
    pub(super) fn park(&mut self, conv: &str, tool_use_id: String, tool: String, input: Value) {
        let cwd = self.convs.get(conv).and_then(|c| self.cwd_of(c).ok());
        let credentials =
            crate::permissions::touches_credentials(&input, cwd.as_deref().and_then(Path::to_str), &|name| std::env::var(name).ok());
        if credentials {
            return;
        }
        let (kind, label) = describe(&tool, &input);
        let item = Parked { tool_use_id, tool: cut(&tool, 64), kind: kind.into(), label, at: now() };
        if let Some(m) = self.turn(conv) {
            let list = m.parked.get_or_insert_default();
            if list.iter().any(|p| p.tool_use_id == item.tool_use_id) {
                return;
            }
            let size = |p: &Parked| p.tool_use_id.len() + p.tool.len() + p.kind.len() + p.label.len();
            if list.len() >= MAX_PARKED || list.iter().map(size).sum::<usize>() + size(&item) > MAX_PARKED_BYTES {
                *m.parked_more.get_or_insert(0) += 1;
            } else {
                list.push(item);
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
                    tool_kit: Default::default(),
                    kit_identity: None,
                    kit_resource_digest: None,
                    kit_pending: false,
                    prepared: false,
                    kit_resources: None,
                    group_files: None,
                    group_context: None,
                    cwd: PathBuf::new(),
                    visual_root: None,
                    vendor: Vendor::Claude,
                    model: None,
                    running: None,
                    capabilities: Default::default(),
                    ended: None,
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
                    tuning: Default::default(),
                    research: None,
                    retiring: false,
                    was_idle: false,
                    runs: Permissions::FullAccess,
                    by_hand: true,
                    approved: false,
                    started: 0.0,
                    served: false,
                    unsaved_session: None,
                    steering: vec![],
                    held: None,
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
    async fn the_cache_lifetime_counts_from_the_start_of_the_request_not_from_the_result() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let until_of = |secs: u64, started_ms: Option<u64>| {
            let (id, _) = seed(&engine);
            engine.with(|c| {
                c.handle(&id, 1, EngineEvent::CacheWarm { secs, started_ms });
                c.handle(&id, 1, done());
            });
            engine.open(&id).unwrap().messages.pop().unwrap().cache_warm_until
        };
        let at = now();
        // A request that began a moment ago keeps its whole lifetime, less the small margin for the first word.
        let fresh = until_of(3600, Some(at)).unwrap();
        assert!(fresh >= at + 3_600_000 - WARM_MARGIN_MS && fresh <= at + 3_600_000, "{fresh}");
        // A six-minute answer on a five-minute cache is already cold when its result arrives: nothing left to show.
        let slow = until_of(300, Some(at - 360_000)).unwrap();
        assert!(slow < at, "{slow} vs {at}");
        // Without the start of the request the expiry is unknown, so no label is stored at all.
        assert_eq!(until_of(3600, None), None);
        let (other, _) = seed(&engine);
        engine.with(|c| c.handle(&other, 1, done()));
        assert_eq!(engine.open(&other).unwrap().messages.pop().unwrap().cache_warm_until, None);
    }

    #[tokio::test]
    async fn a_parked_request_keeps_safe_metadata_never_the_command_or_any_input() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, _) = seed(&engine);
        engine.with(|c| {
            c.handle(
                &id,
                1,
                parked("t1", "Bash", json!({ "command": "curl -H 'Authorization: Bearer sk-SECRET-1' https://api.example.com" })),
            );
            c.handle(&id, 1, parked("t2", "Write", json!({ "file_path": "/work/notes.txt", "content": "token=sk-SECRET-2" })));
            c.handle(&id, 1, parked("t3", "WebFetch", json!({ "url": "https://user:sk-SECRET-3@example.com:8443/a/b?key=sk-SECRET-4#x" })));
            c.handle(&id, 1, parked("t4", "mcp__x__send", json!({ "body": "sk-SECRET-5" })));
            c.handle(&id, 1, done());
            // A safe timestamp may contain the same digits as a denied URL's port.
            c.convs.get_mut(&id).unwrap().messages.last_mut().unwrap().parked.as_mut().unwrap()[0].at = 8443;
        });
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        let kept: Vec<_> = reply.parked.as_ref().unwrap().iter().map(|p| (p.tool.as_str(), p.kind.as_str(), p.label.as_str())).collect();
        assert_eq!(
            kept,
            [
                ("Bash", "command", ""),
                ("Write", "file", "/work/notes.txt"),
                ("WebFetch", "page", "example.com"),
                ("mcp__x__send", "other", "")
            ]
        );
        assert!(reply.parked.as_ref().unwrap().iter().all(|p| p.at > 0));
        // Neither the saved conversation, nor the reply sent to the screen, nor the running list holds any of it.
        let saved = std::fs::read_to_string(dir.path().join("conversations").join(format!("{id}.json"))).unwrap_or_default();
        let wire =
            [serde_json::to_string(&reply).unwrap(), serde_json::to_string(&engine.state().running.items).unwrap(), saved].join("\n");
        for secret in ["sk-SECRET", "Bearer", "Authorization", "curl", "example.com:8443", "key="] {
            assert!(!wire.contains(secret), "{secret} leaked: {wire}");
        }
        assert!(wire.contains("8443"), "the regression fixture includes a legitimate timestamp, not a URL port");
    }

    #[tokio::test]
    async fn a_flood_of_denials_is_capped_in_count_and_size_and_the_rest_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, _) = seed(&engine);
        let long = format!("/work/{}", "d".repeat(500));
        engine.with(|c| {
            for n in 0..100 {
                c.handle(&id, 1, parked(&format!("t{n}"), "Write", json!({ "file_path": long })));
            }
            c.handle(&id, 1, done());
        });
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        let kept = reply.parked.as_ref().unwrap();
        assert!(kept.len() <= MAX_PARKED, "{}", kept.len());
        assert_eq!(kept.len() as u32 + reply.parked_more.unwrap(), 100, "nothing is lost from the count");
        assert!(serde_json::to_string(kept).unwrap().len() <= 2 * MAX_PARKED_BYTES);
        let item = &engine.state().running.items[0];
        assert_eq!((item.parked.len(), item.parked_more), (kept.len(), reply.parked_more.unwrap()));
        assert_eq!(item.state, RunState::NeedsYou);
        // Twenty short ones are all kept: the size cap is what stopped the long ones.
        let (small, _) = seed(&engine);
        engine.with(|c| {
            for n in 0..25 {
                c.handle(&small, 1, parked(&format!("s{n}"), "Bash", json!({ "command": "x" })));
            }
            c.handle(&small, 1, done());
        });
        let reply = engine.open(&small).unwrap().messages.pop().unwrap();
        assert_eq!((reply.parked.unwrap().len(), reply.parked_more), (MAX_PARKED, Some(5)));
    }

    #[tokio::test]
    async fn a_cli_that_refuses_a_flag_loses_the_newer_flags_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let engine = engine(dir.path());
        let (id, _) = seed(&engine);
        let known = crate::claude::ClaudeFlags { autocompact: true, permission_prompts: true, auto_mode: true, plugin_dir: false };
        engine.with(|c| c.claude_flags = known);
        engine.with(|c| {
            c.handle(&id, 1, EngineEvent::Error { message: "Claude Code stopped (exit 1). error: something else".into(), fatal: true })
        });
        assert_eq!(engine.with(|c| c.claude_flags), known, "any other failure keeps them");
        let (id, _) = seed(&engine);
        engine.with(|c| {
            c.handle(
                &id,
                1,
                EngineEvent::Error {
                    message: "Claude Code stopped (exit 1). error: unknown option '--permission-prompts'".into(),
                    fatal: true,
                },
            )
        });
        assert_eq!(engine.with(|c| c.claude_flags), Default::default());
    }
}
