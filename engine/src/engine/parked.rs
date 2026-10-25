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

const RANK: [&str; 5] = ["delete", "history", "publish", "install", "network"];
/// Words that only lead to the program that runs: wrappers that run the program after them, and the shell's own keywords.
const LEADS: [&str; 17] =
    ["$", "!", "sudo", "env", "exec", "nohup", "time", "timeout", "nice", "command", "xargs", "then", "do", "else", "if", "while", "until"];
const SHELLS: [&str; 6] = ["bash", "sh", "zsh", "pwsh", "powershell", "cmd"];

/// The parts of a command line and their words, the way a shell splits them: quotes keep a word whole and are dropped, and `;`,
/// `&`, `|`, a new line, brackets and backticks end a part outside quotes. The lines of a heredoc are text, not commands, so
/// everything after the line that opens one is left out.
fn parts(command: &str) -> Vec<Vec<String>> {
    let (mut parts, mut part, mut word) = (vec![], vec![], String::new());
    let (mut quote, mut heredoc) = (None, false);
    let mut chars = command.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => word.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() || matches!(ch, ';' | '&' | '|' | '(' | ')' | '{' | '}' | '`') => {
                if !word.is_empty() {
                    part.push(std::mem::take(&mut word));
                }
                if ch == ' ' || ch == '\t' {
                    continue;
                }
                if !part.is_empty() {
                    parts.push(std::mem::take(&mut part));
                }
                if ch == '\n' && heredoc {
                    return parts;
                }
            }
            None => {
                heredoc |= ch == '<' && chars.peek() == Some(&'<');
                word.push(ch);
            }
        }
    }
    if !word.is_empty() {
        part.push(word);
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// What a command would have done, as one of Faber's own words, read from the program of each of its parts and the words that
/// make it act (`git push`, `gh pr merge`, `npm install`). The most serious one wins; "" when none fits. The answer is always one
/// of the fixed words above, so nothing of the command itself is ever kept.
pub(super) fn does(command: &str) -> &'static str {
    let rank = |word: &str| RANK.iter().position(|r| *r == word).unwrap_or(RANK.len());
    let mut best = RANK.len();
    for words in parts(command) {
        let words: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();
        // Wrappers, `VAR=x`, flags and the numbers of `timeout 5` or `nice -n 5` lead to the program that runs.
        let at =
            words.iter().position(|w| !(w.contains('=') || w.starts_with('-') || LEADS.contains(&w.as_str()) || w.parse::<f64>().is_ok()));
        let Some(at) = at else { continue };
        let head = &words[at];
        let program = head.rsplit(['/', '\\']).next().unwrap_or(head).trim_end_matches(".exe");
        let mut rest: Vec<&str> = words[at + 1..].iter().map(String::as_str).collect();
        if SHELLS.contains(&program) {
            // `bash -lc "<script>"` or `cmd /c <line>`: what follows the flags is a command line of its own.
            let script = rest.iter().skip_while(|w| w.starts_with('-') || **w == "/c").copied().collect::<Vec<_>>().join(" ");
            best = best.min(rank(does(&script)));
            continue;
        }
        if program == "git" {
            // git's own options come before its command: `-C <folder>` and `-c <key=value>` take a word each.
            while let Some(first) = rest.first() {
                let skip = if *first == "-c" { 2 } else { usize::from(first.starts_with('-')) };
                if skip == 0 {
                    break;
                }
                rest.drain(..skip.min(rest.len()));
            }
        }
        let sub = rest.first().copied().unwrap_or_default();
        let then = rest.get(1).copied().unwrap_or_default();
        let has = |flag: &str| rest.contains(&flag);
        let dry = has("-n") || has("--dry-run");
        let found = match (program, sub) {
            ("rm" | "rmdir" | "del" | "erase" | "rd" | "unlink" | "shred" | "remove-item" | "ri", _) => "delete",
            ("find", _) if has("-delete") => "delete",
            ("git", "clean") if !dry => "delete",
            ("git", "rm") if !dry && !has("--cached") => "delete",
            ("git", "reset") if has("--hard") => "history",
            ("git", "push") if dry => continue,
            ("git", "push") if rest.iter().any(|w| *w == "-f" || w.starts_with("--force")) => "history",
            ("git", "rebase" | "filter-branch" | "filter-repo") => "history",
            ("git", "push") | ("npm" | "pnpm" | "yarn" | "cargo", "publish") => "publish",
            ("gh", "pr" | "issue" | "release" | "repo")
                if matches!(
                    then,
                    "create"
                        | "merge"
                        | "edit"
                        | "close"
                        | "comment"
                        | "ready"
                        | "reopen"
                        | "delete"
                        | "upload"
                        | "review"
                        | "fork"
                        | "archive"
                        | "rename"
                ) =>
            {
                "publish"
            }
            ("npm" | "pnpm" | "yarn" | "bun", "install" | "i" | "add" | "ci")
            | (
                "pip" | "pip3" | "cargo" | "go" | "gem" | "winget" | "choco" | "scoop" | "apt" | "apt-get" | "brew",
                "install" | "upgrade" | "add",
            ) => "install",
            ("curl" | "wget" | "invoke-webrequest" | "iwr" | "invoke-restmethod" | "irm", _)
            | ("git", "clone" | "fetch" | "pull")
            | ("gh", "api" | "run") => "network",
            _ => continue,
        };
        best = best.min(rank(found));
    }
    RANK.get(best).copied().unwrap_or_default()
}

/// What may be kept of a denied request: its kind, a short label and, for a command, what it would have done; never the request
/// itself. A command line, a body or a URL query can carry a token or an Authorization header, so none of them is stored. A file
/// is named by its path and a page by its host (no scheme, user, port, path or query); everything else has no label.
pub(super) fn describe(tool: &str, input: &Value) -> (&'static str, String, &'static str) {
    let text = |keys: &[&str]| keys.iter().find_map(|k| input[*k].as_str()).unwrap_or_default();
    match tool {
        "Bash" | "PowerShell" | "command" => {
            // Codex gives the command as a list of words.
            let line = match &input["command"] {
                Value::Array(words) => words.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
                _ => text(&["command", "cmd"]).to_owned(),
            };
            ("command", String::new(), does(&line))
        }
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "Read" | "fileChange" => {
            ("file", cut(text(&["file_path", "path", "notebook_path"]), MAX_LABEL), "")
        }
        "WebFetch" => {
            let url = text(&["url"]);
            let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
            let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
            let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
            ("page", cut(host.split(':').next().unwrap_or_default(), MAX_LABEL), "")
        }
        _ => ("other", String::new(), ""),
    }
}

impl Engine {
    /// The person read the held-back requests of a reply. They stay on it as a record, and leave the "needs you" list until
    /// another request is held back.
    pub fn parked_seen(&self, id: &str, turn_id: &str) -> Result<(), String> {
        self.with(|c| {
            let m = c
                .load(id)
                .and_then(|conv| conv.messages.iter_mut().find(|m| m.id == turn_id))
                .filter(|m| m.parked.as_ref().is_some_and(|p| !p.is_empty()) || m.parked_more.unwrap_or(0) > 0)
                .ok_or("error.permission.stale")?;
            m.parked_seen = Some(true);
            c.save(id);
            c.emit_message(id, turn_id);
            c.refresh_running();
            Ok(())
        })
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
        let (kind, label, does) = describe(&tool, &input);
        let item = Parked { tool_use_id, tool: cut(&tool, 64), kind: kind.into(), label, does: does.into(), at: now() };
        if let Some(m) = self.turn(conv) {
            let list = m.parked.get_or_insert_default();
            if list.iter().any(|p| p.tool_use_id == item.tool_use_id) {
                return;
            }
            // A new one needs the person again, even after they said they read the earlier ones.
            m.parked_seen = None;
            let size = |p: &Parked| p.tool_use_id.len() + p.tool.len() + p.kind.len() + p.label.len() + p.does.len();
            if list.len() >= MAX_PARKED || list.iter().map(size).sum::<usize>() + size(&item) > MAX_PARKED_BYTES {
                *m.parked_more.get_or_insert(0) += 1;
            } else {
                list.push(item);
            }
            // The conversation needs the person now, not only once the turn ends.
            self.refresh_running();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::SessionCommand;
    use serde_json::json;

    fn engine(dir: &crate::workspace::TestRoot) -> Engine {
        let opts = EngineOptions {
            data_dir: dir.path().into(),
            #[cfg(windows)]
            chat_dir: dir.work().into(),
            clis: BTreeMap::new(),
            spawn: Arc::new(|_, _, _| Err(std::io::Error::other("no process in this test"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(60),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        };
        Engine::new(opts, Arc::new(|_| {})).unwrap()
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
                    guide: Default::default(),
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
                    settled: false,
                    open_tools: Vec::new(),
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
    async fn got_it_takes_the_conversation_off_the_needs_you_list_until_another_request_is_held_back() {
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
        let (id, _) = seed(&engine);
        assert_eq!(engine.parked_seen(&id, "reply").unwrap_err(), "error.permission.stale", "nothing held back yet");
        engine.with(|c| c.handle(&id, 1, parked("toolu_1", "Bash", json!({ "command": "rm -rf ./build" }))));
        assert_eq!(engine.state().running.items[0].state, RunState::NeedsYou);
        assert!(engine.parked_seen(&id, "another-reply").is_err());
        engine.parked_seen(&id, "reply").unwrap();
        let item = &engine.state().running.items[0];
        assert_eq!((item.state, item.parked.len()), (RunState::Working, 0), "still at work, no longer needs the person");
        let reply = engine.open(&id).unwrap().messages.pop().unwrap();
        assert_eq!((reply.parked.as_ref().unwrap().len(), reply.parked_seen), (1, Some(true)), "the note stays on the reply");
        engine.with(|c| c.handle(&id, 1, done()));
        assert!(engine.state().running.items.is_empty(), "a finished turn whose notes were read leaves the list");
        let (id, _) = seed(&engine);
        engine.with(|c| c.handle(&id, 1, parked("toolu_1", "Bash", json!({ "command": "x" }))));
        engine.parked_seen(&id, "reply").unwrap();
        engine.with(|c| c.handle(&id, 1, parked("toolu_2", "Bash", json!({ "command": "y" }))));
        assert_eq!(engine.state().running.items[0].parked.len(), 2, "a new one needs the person again, with the earlier ones");
    }

    #[tokio::test]
    async fn got_it_never_hides_a_question_still_waiting_and_works_on_a_count_alone() {
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
        let (id, _) = seed(&engine);
        engine.with(|c| {
            c.handle(&id, 1, parked("toolu_1", "Bash", json!({ "command": "x" })));
            c.turn(&id).unwrap().permissions =
                Some(vec![Permission { request_id: "r1".into(), tool: "Bash".into(), input: json!({}), epoch: None }]);
        });
        engine.parked_seen(&id, "reply").unwrap();
        engine.with(|c| c.refresh_running());
        let item = &engine.state().running.items[0];
        assert_eq!((item.state, item.parked.len(), item.permissions.len()), (RunState::NeedsYou, 0, 1), "the question still waits");
        let (id, _) = seed(&engine);
        engine.with(|c| {
            c.turn(&id).unwrap().parked_more = Some(3);
            c.refresh_running();
        });
        engine.parked_seen(&id, "reply").unwrap();
        let item = engine.state().running.items.into_iter().find(|i| i.conversation_id == id).unwrap();
        assert_eq!((item.state, item.parked_more), (RunState::Working, 0));
    }

    #[test]
    fn a_command_is_told_by_what_it_would_have_done_and_the_most_serious_part_wins() {
        for (command, word) in [
            ("mkdir -p /tmp/x && cd /tmp/x && rm -rf ./* && gh api repos/o/r/actions/runs/1/artifacts", "delete"),
            ("Remove-Item -Recurse -Force C:\\work\\out", "delete"),
            ("git clean -fdx", "delete"),
            ("git reset --hard origin/main", "history"),
            ("git push --force-with-lease origin HEAD", "history"),
            ("cd repo; git push origin HEAD:refs/heads/queue/x", "publish"),
            ("gh pr merge 12 --squash", "publish"),
            ("FOO=1 npm install left-pad", "install"),
            ("sudo apt-get install jq", "install"),
            ("curl -H 'Authorization: Bearer sk-SECRET' https://api.example.com | jq .", "network"),
            ("bash -lc \"git fetch origin\"", "network"),
            ("cat notes.txt | grep rm", ""),
            ("git commit -m \"tidy; rm old files\"", ""),
            ("cat > notes.md <<'EOF'\nrm -rf everything\ncurl https://example.com\nEOF", ""),
            ("git -C repo reset --hard && curl https://example.com", "history"),
            ("git -c user.name=x push -f", "history"),
            ("timeout 5 rm -rf out", "delete"),
            ("find . -name '*.tmp' -delete", "delete"),
            ("ls | xargs rm", "delete"),
            ("cmd /c del out.txt", "delete"),
            ("git rm --cached secret.txt", ""),
            ("git clean -n", ""),
            ("git push --dry-run", ""),
            ("gh pr view 12", ""),
            ("gh pr checks 12 && gh issue list", ""),
            ("brew list", ""),
            ("winget search node", ""),
            ("gh release create v1", "publish"),
            ("echo git push", ""),
            ("cargo test", ""),
            ("", ""),
        ] {
            assert_eq!(does(command), word, "{command}");
        }
        let (kind, label, word) = describe("command", &json!({ "command": ["bash", "-lc", "rm -rf out"] }));
        assert_eq!((kind, label.as_str(), word), ("command", "", "delete"), "Codex gives the words as a list");
    }

    #[tokio::test]
    async fn the_next_message_takes_the_conversation_off_the_needs_you_list_but_keeps_the_note() {
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
        let does: Vec<_> = reply.parked.as_ref().unwrap().iter().map(|p| p.does.as_str()).collect();
        assert_eq!(does, ["network", "", "", ""], "only Faber's own word for what the command would have done");
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
        let dir = crate::workspace::TestRoot::new();
        let engine = engine(&dir);
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
