//! Task kits are selected in code before a native session starts.
use super::*;

const RESOURCE_PREFIX: &str =
    "Current catalog resources for this session, replacing earlier catalog selections and paths (within current permissions):\n";

const EMPTY_RESOURCES: &str =
    "No catalog skill instructions apply to this session. Earlier catalog skill selections and paths no longer apply.";

pub(super) fn resource_digest(resources: Option<&str>) -> String {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    if let Some(resources) = resources {
        hash.update(RESOURCE_PREFIX);
        hash.update(resources);
        hash.update("\n\n");
    }
    crate::hex(&hash.finalize())
}

pub(super) fn resume_resources(
    resources: Option<String>,
    saved: Option<&(String, Vec<String>, Option<String>)>,
    resume: Option<&String>,
    saved_digest: Option<&String>,
) -> Option<String> {
    if resources.is_none()
        && saved.is_some_and(|(native, _, _)| Some(native) == resume)
        && saved_digest.is_none_or(|old| old != &resource_digest(None))
    {
        Some(EMPTY_RESOURCES.into())
    } else {
        resources
    }
}

pub(super) fn session_tool_identity(vendor: Vendor, delivery: &crate::tools::Delivery) -> Option<String> {
    use sha2::Digest;
    if vendor == Vendor::Codex {
        return delivery.server_identity();
    }
    let snapshot = delivery.identity()?;
    let servers: Vec<_> = delivery.servers.iter().map(|s| (&s.name, &s.command, &s.args, &s.env)).collect();
    let launch = serde_json::to_vec(&(snapshot, delivery.plugin.is_some(), servers)).ok()?;
    Some(crate::hex(&sha2::Sha256::digest(launch)))
}

pub(super) fn claude_resources(area: Area, delivery: &crate::tools::Delivery) -> Option<String> {
    match area {
        Area::Chat => delivery.skill_instructions(),
        Area::Project => delivery.project_skill_descriptions(Vendor::Claude),
    }
}

/// Digest of Faber's own guide as a session was told of it (its line or its text), kept apart from the kit's resources: a
/// kit that changes never sends it again, and a guide that changes never sends the kit again.
pub(super) fn guide_digest(note: Option<&str>) -> String {
    use sha2::Digest;
    crate::hex(&sha2::Sha256::digest(format!("faber-guide\0{}", note.unwrap_or_default())))
}

/// What a live session knows of Faber's own guide (R18), apart from the kit's resources.
#[derive(Default)]
pub(super) struct LiveGuide {
    /// The line (or text) that tells this session of the guide; none where Claude Code loads it as a skill.
    pub note: Option<String>,
    /// The digest of `note` once the session was told, so a kit that changes never sends it again.
    pub digest: Option<String>,
    /// Goes ahead of the next ordinary message.
    pub pending: Option<String>,
    /// The line reaches this session in a user message (Claude Code, a resumed Codex thread), which a compaction can drop;
    /// a new Codex thread has it in its instructions, which a compaction keeps.
    pub in_input: bool,
}

impl LiveGuide {
    /// A session that starts or resumes. A new session, one saved before the guide existed, one told another line (a new screen
    /// language, a moved folder) and one compacted since are told once. Claude Code and a resumed Codex thread take the line
    /// with their first input; a new Codex thread took it with its instructions (`thread/start`), and a resumed one keeps
    /// the instructions it had.
    pub(super) fn start(
        note: Option<String>,
        mode: SessionStart,
        first_input: bool,
        saved: Option<&(String, Vec<String>, Option<String>)>,
        resume: Option<&String>,
        saved_digest: Option<&String>,
    ) -> Self {
        let digest = guide_digest(note.as_deref());
        let told = saved.is_some_and(|(native, _, _)| Some(native) == resume) && saved_digest == Some(&digest);
        let (pending, digest) = match &note {
            _ if mode == SessionStart::Compact => (None, saved_digest.cloned()),
            Some(note) if first_input && !told => (Some(note.clone()), None),
            _ => (None, Some(digest)),
        };
        LiveGuide { in_input: first_input && note.is_some(), note, digest, pending }
    }

    /// The CLI compacted: a line that went in a user message is a summary now, so the next message carries it again. True
    /// when it will, so the saved digest is dropped until it does.
    pub(super) fn compacted(&mut self) -> bool {
        if self.in_input {
            self.digest = None;
            self.pending = self.note.clone();
        }
        self.in_input
    }

    fn take(&mut self) -> Option<String> {
        let note = self.pending.take()?;
        self.digest = Some(guide_digest(Some(&note)));
        Some(note)
    }
}

pub(super) fn resources_changed(
    saved: Option<&(String, Vec<String>, Option<String>)>,
    resume: Option<&String>,
    ids: &[String],
    identity: &Option<String>,
    saved_digest: Option<&String>,
    digest: &str,
) -> bool {
    !(saved.is_some_and(|(native, old_ids, old_identity)| Some(native) == resume && old_ids == ids && old_identity == identity)
        && saved_digest.is_some_and(|old| old == digest))
}

pub(super) fn codex_resources(area: Area, delivery: &crate::tools::Delivery) -> Option<String> {
    match (delivery.project_skill_descriptions(Vendor::Codex).filter(|_| area == Area::Project), delivery.skill_instructions()) {
        (Some(index), Some(body)) => Some(format!("{index}\n\n{body}")),
        (index, body) => index.or(body),
    }
}

pub(super) fn with_resources(text: String, resources: Option<String>) -> String {
    match resources {
        Some(resources) => format!("{RESOURCE_PREFIX}{resources}\n\n{text}"),
        None => text,
    }
}

/// A first input: the guide's line, then the changed catalog resources, then the words. A slash command goes alone, where
/// Claude Code reads it, and what waits goes with the next ordinary message.
pub(super) fn first_input(text: String, resources: &mut Option<String>, guide: &mut LiveGuide) -> String {
    if text.starts_with('/') {
        return text;
    }
    let text = with_resources(text, resources.take());
    match guide.take() {
        Some(note) => format!("{note}\n\n{text}"),
        None => text,
    }
}

pub(super) fn record_native_kit(conversation: &mut Conversation, native: String, live: Option<&Live>, store: Option<&crate::tools::Store>) {
    let Some(live) = live else { return };
    if let Some(card) = &live.start_card {
        conversation.session_start_cards.insert(live.vendor, (native.clone(), card.clone()));
    } else {
        conversation.session_start_cards.remove(&live.vendor);
    }
    let mut ids = live.tool_kit.ids.clone();
    if live.vendor == Vendor::Codex
        && let Some((_, old_ids, _)) = conversation.session_kits.get(&live.vendor)
        && let Some(store) = store
    {
        let current = store.server_identity_for(old_ids, Area::Project, Permissions::FullAccess, &[]);
        if current != "servers:none" && live.kit_identity.as_deref() == Some(current.as_str()) {
            ids.extend(store.catalog_server_ids(old_ids));
            ids.sort();
            ids.dedup();
        }
    }
    conversation.session_kits.insert(live.vendor, (native, ids, live.kit_identity.clone()));
    // shortcut: the kit's digest is saved when the session starts, even if its resources wait behind a `/` first message, so a
    // process that ends before the next ordinary message never resends them; upgrade by saving it when the text goes out, as the guide's is.
    if let Some(digest) = &live.kit_resource_digest {
        conversation.session_resource_digests.insert(live.vendor, digest.clone());
    } else {
        conversation.session_resource_digests.remove(&live.vendor);
    }
    if let Some(digest) = &live.guide.digest {
        conversation.session_guide_digests.insert(live.vendor, digest.clone());
    } else {
        conversation.session_guide_digests.remove(&live.vendor);
    }
}

impl Core {
    pub(super) fn session_tool_report(
        &mut self,
        conv: &str,
        reply: &str,
        mut kit: crate::tools::kits::Report,
    ) -> Option<crate::tools::kits::Report> {
        let previous = self.convs.get(conv).and_then(|c| {
            c.messages.iter().rev().filter(|m| m.id != reply).filter_map(|m| m.tool_kit.as_ref()).find(|k| !k.ids.is_empty())
        });
        if previous.is_some_and(|old| old.ids == kit.ids) {
            kit.ids.clear();
        }
        // Like notes-seen, a durable marker belongs to the app, not a conversation or editable setting.
        if kit.suggested.is_some()
            && std::fs::OpenOptions::new().write(true).create_new(true).open(self.opts.data_dir.join("catalog-hint-seen")).is_err()
        {
            kit.suggested = None;
        }
        (!kit.ids.is_empty() || kit.suggested.is_some() || kit.error.is_some()).then_some(kit)
    }

    pub(super) fn catalog_store(&self) -> crate::tools::Store {
        if let Some(store) = &self.catalog_store {
            return store.clone();
        }
        unreachable!("the engine retains its catalog store")
    }

    pub(super) fn choose_session_tools(
        &mut self,
        conv: &str,
        vendor: Vendor,
        permissions: Permissions,
        cwd: &Path,
    ) -> (crate::tools::Delivery, crate::tools::kits::Report) {
        let c = &self.convs[conv];
        let area = c.area;
        let project = c.project_id.clone();
        let request = c.messages.iter().rev().find(|m| m.role == Role::User).map(|m| m.text.clone()).unwrap_or_default();
        let mut kit = self.session_tools(vendor, area, permissions, project.as_deref(), &request, cwd);
        kit.0.guide = self.guide_for(vendor, area, permissions);
        kit
    }

    /// The language the screen shows: the choice in Settings, else the system's.
    pub(super) fn screen_language(&self) -> crate::guide::Language {
        crate::guide::Language::resolve(self.store.settings().language.as_deref(), &crate::guide::system_languages())
    }

    /// How Faber's own guide (R18) reaches a session, the same for every message of it: never to Faber's own internal requests.
    pub(super) fn guide_for(&self, vendor: Vendor, area: Area, permissions: Permissions) -> crate::guide::Guide {
        use crate::guide::Guide;
        let language = self.screen_language();
        if permissions == Permissions::Isolated {
            return Guide::Off;
        }
        // Every session can read a file Faber points to, so the folder is written for all of them; only without it do the words go.
        match crate::guide::plugin(&self.opts.data_dir, language) {
            Some(dir) if vendor == Vendor::Claude && area == Area::Project => Guide::Plugin { dir, language },
            Some(dir) => Guide::Index { dir, language },
            None => Guide::Text(language),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionStart {
    Prepare,
    Turn,
    Compact,
}

/// Native resumes can retain their original instructions. Changed context starts a fresh native session with Faber's history.
pub(super) fn session_card(
    current: Option<String>,
    saved: Option<&(String, String)>,
    native: Option<&String>,
    mode: SessionStart,
) -> (Option<String>, bool) {
    let previous = saved.filter(|(id, card)| {
        Some(id) == native && card.len() <= 6 * 1024 && card.starts_with("```faber-start-card\n") && card.ends_with("```")
    });
    if mode == SessionStart::Compact {
        return (previous.map(|(_, card)| card.clone()), false);
    }
    let reset = native.is_some() && current.is_some() && previous.map(|(_, card)| card) != current.as_ref();
    (current, reset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Item, Store as ToolStore, digest};
    use serde_json::{Value, json};
    use std::io::Write;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    #[test]
    fn project_start_card_resume_binds_the_native_id_and_compaction_never_adopts_new_words() {
        let old = "```faber-start-card\nOLD\n```".to_string();
        let new = "```faber-start-card\nNEW\n```".to_string();
        let native = "native".to_string();
        let saved = (native.clone(), old.clone());
        for mode in [SessionStart::Prepare, SessionStart::Turn] {
            assert_eq!(session_card(Some(old.clone()), Some(&saved), Some(&native), mode), (Some(old.clone()), false));
            assert_eq!(session_card(Some(new.clone()), Some(&saved), Some(&native), mode), (Some(new.clone()), true));
            for unknown in [None, Some(&("other-native".into(), old.clone())), Some(&(native.clone(), "x".repeat(6145)))] {
                assert!(session_card(Some(old.clone()), unknown, Some(&native), mode).1);
            }
        }
        assert_eq!(session_card(Some(new), Some(&saved), Some(&native), SessionStart::Compact), (Some(old), false));
        assert_eq!(session_card(None, None, Some(&native), SessionStart::Compact), (None, false));
    }

    type Arguments = Arc<Mutex<Vec<(String, Vec<String>)>>>;
    type Messages = Arc<Mutex<Vec<(String, Value)>>>;

    fn fake() -> (Spawner, Arguments, Messages) {
        fake_reply(false)
    }

    fn fake_reply(first_word: bool) -> (Spawner, Arguments, Messages) {
        fake_backend(first_word, None)
    }

    fn fake_backend(first_word: bool, effective: Option<Messages>) -> (Spawner, Arguments, Messages) {
        let args: Arguments = Arc::default();
        let messages: Messages = Arc::default();
        let (seen_args, seen_messages) = (args.clone(), messages.clone());
        let spawn: Spawner = Arc::new(move |bin, argv, _| {
            seen_args.lock().unwrap().push((bin.into(), argv.to_vec()));
            let (stdin, incoming) = tokio::io::duplex(65536);
            let (mut outgoing, stdout) = tokio::io::duplex(65536);
            let (bin, messages, effective) = (bin.to_string(), seen_messages.clone(), effective.clone());
            tokio::spawn(async move {
                let mut threads = BTreeMap::<String, String>::new();
                let mut lines = BufReader::new(incoming).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let msg: Value = serde_json::from_str(&line).unwrap();
                    messages.lock().unwrap().push((bin.clone(), msg.clone()));
                    if first_word && msg["type"] == "user" {
                        let words = json!({"type":"stream_event", "parent_tool_use_id":null,
                            "event":{"type":"content_block_delta", "delta":{"type":"text_delta", "text":"first word"}}});
                        if outgoing.write_all(format!("{words}\n").as_bytes()).await.is_err() {
                            break;
                        }
                    }
                    let result = match msg["method"].as_str() {
                        Some("initialize") => Some(json!({})),
                        Some("config/read") => Some(json!({"config":{}})),
                        Some("thread/start" | "thread/resume") if effective.is_some() => {
                            let id = msg["params"]["threadId"].as_str().map(str::to_owned).unwrap_or_else(|| format!("kit-{}", msg["id"]));
                            // A loaded native thread ignores later resume instruction overrides.
                            threads
                                .entry(id.clone())
                                .or_insert_with(|| msg["params"]["developerInstructions"].as_str().unwrap_or_default().into());
                            Some(json!({"thread":{"id":id}}))
                        }
                        Some("thread/start" | "thread/resume") => Some(json!({"thread":{"id":format!("kit-{}", msg["id"])}})),
                        Some("turn/start") => {
                            if let Some(effective) = &effective {
                                let id = msg["params"]["threadId"].as_str().unwrap();
                                let text = msg["params"]["input"][0]["text"].as_str().unwrap();
                                let resources = threads.get_mut(id).expect("turns use an already loaded thread");
                                if text.starts_with("Current catalog resources for this session,") {
                                    *resources = text.rsplit_once("\n\n").unwrap().0.into();
                                }
                                effective.lock().unwrap().push((id.into(), json!(resources)));
                            }
                            Some(json!({"turn":{"id":"turn"}}))
                        }
                        _ => None,
                    };
                    if let Some(result) = result {
                        let answer = json!({"id":msg["id"],"result":result});
                        if outgoing.write_all(format!("{answer}\n").as_bytes()).await.is_err() {
                            break;
                        }
                        if msg["method"] == "turn/start" {
                            let started = json!({"method":"turn/started", "params":{
                                "threadId":msg["params"]["threadId"], "turn":{"id":"turn"}}});
                            if outgoing.write_all(format!("{started}\n").as_bytes()).await.is_err() {
                                break;
                            }
                            if first_word {
                                let words = json!({"method":"item/agentMessage/delta", "params":{
                                    "threadId":msg["params"]["threadId"], "turnId":"turn", "itemId":"answer", "delta":"first word"}});
                                if outgoing.write_all(format!("{words}\n").as_bytes()).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            });
            Ok(crate::proc::Proc { stdin: Box::new(stdin), stdout: Box::new(stdout), stderr: None, child: None, contained: None })
        });
        (spawn, args, messages)
    }

    #[test]
    fn resource_digest_tracks_the_exact_delivered_prefix_even_when_native_ids_match() {
        use sha2::Digest;
        let resources = "Catalog skill debug (supporting files: old-path):\ndebug instructions";
        let prefix = with_resources(String::new(), Some(resources.into()));
        let digest = resource_digest(Some(resources));
        assert_eq!(digest, crate::hex(&sha2::Sha256::digest(prefix.as_bytes())));
        assert_ne!(resource_digest(None), resource_digest(Some("")), "an empty prefix differs from no prefix");
        let native = "native-history".to_string();
        let ids = vec!["debug".to_string()];
        let identity = Some("servers:none".to_string());
        let saved = (native.clone(), ids.clone(), identity.clone());
        assert!(!resources_changed(Some(&saved), Some(&native), &ids, &identity, Some(&digest), &digest));
        let changed = resource_digest(Some(&resources.replace("old-path", "current-path")));
        assert!(resources_changed(Some(&saved), Some(&native), &ids, &identity, Some(&digest), &changed));
        assert!(resources_changed(Some(&saved), Some(&native), &ids, &identity, None, &digest), "older metadata refreshes once");
    }

    #[test]
    fn empty_resource_replacement_distinguishes_fresh_known_empty_and_unknown_native_history() {
        let native = "native-history".to_string();
        let saved = (native.clone(), Vec::new(), Some("servers:none".into()));
        let never_had_resources = resource_digest(None);
        let cleared_resources = resource_digest(Some(EMPTY_RESOURCES));
        assert!(resume_resources(None, None, None, None).is_none(), "a fresh thread needs no empty marker");
        assert!(
            resume_resources(None, None, Some(&native), None).is_none(),
            "external native history has no earlier Faber catalog selection"
        );
        assert!(
            resume_resources(None, Some(&saved), Some(&native), Some(&never_had_resources)).is_none(),
            "known empty native history needs no marker"
        );
        assert_eq!(
            resume_resources(None, Some(&saved), Some(&native), None).as_deref(),
            Some(EMPTY_RESOURCES),
            "unknown older history is cleared once"
        );
        assert_eq!(
            resume_resources(None, Some(&saved), Some(&native), Some(&cleared_resources)).as_deref(),
            Some(EMPTY_RESOURCES),
            "the stable marker keeps its accepted digest"
        );
    }

    fn catalog(dir: &Path) -> ToolStore {
        catalog_version(dir, "")
    }

    #[tokio::test]
    async fn tools_in_use_are_the_saved_kit_of_the_ai_at_work_with_each_job_group() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, _) = fake();
        let engine = engine(&dir, spawn);
        assert!(engine.tools_in_use("kit").is_empty(), "an unknown conversation has none");
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            let c = core.convs.get_mut("kit").unwrap();
            c.messages[1].vendor = Some(Vendor::Claude);
            c.session_kits.insert(Vendor::Claude, ("claude-native".into(), vec!["systematic-debugging".into(), "gone".into()], None));
            c.session_kits.insert(Vendor::Codex, ("codex-native".into(), vec!["design-taste-frontend".into()], None));
        });
        let pair = |id: &str, job: &str| (id.to_string(), job.to_string());
        assert_eq!(
            engine.tools_in_use("kit"),
            [pair("systematic-debugging", "debugging"), pair("gone", "")],
            "the newest reply's AI; an id the catalog no longer has keeps no job"
        );
        engine.with(|core| core.convs.get_mut("kit").unwrap().messages[1].vendor = Some(Vendor::Codex));
        assert_eq!(engine.tools_in_use("kit"), [pair("design-taste-frontend", "design")], "a switch of AI shows that AI's own kit");
        engine.with(|core| {
            core.convs.get_mut("kit").unwrap().session_kits.insert(Vendor::Codex, ("codex-native".into(), vec![], None));
        });
        assert!(engine.tools_in_use("kit").is_empty(), "a session that started with an empty kit has none");
        engine.shutdown();
    }

    #[tokio::test]
    async fn catalog_hint_is_once_for_the_app_and_survives_settings_changes_and_reopening() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, _) = fake();
        let engine = engine(&dir, spawn);
        let missing = || crate::tools::kits::Report { suggested: Some("systematic-debugging".into()), ..Default::default() };
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            let first = core.session_tool_report("kit", "reply", missing()).unwrap();
            assert_eq!(first.suggested.as_deref(), Some("systematic-debugging"));
            let mut second = core.convs["kit"].clone();
            second.id = "other".into();
            core.convs.insert(second.id.clone(), second);
            assert!(core.session_tool_report("other", "reply", missing()).is_none(), "a new conversation does not repeat the hint");
            core.save_settings(Settings { language: Some("de".into()), ..Default::default() }).unwrap();
            assert!(folder.join("catalog-hint-seen").is_file(), "the marker persists independently of ordinary settings patches");
            core.store = Store::new(&folder);
            assert!(
                core.session_tool_report("kit", "reply", missing()).is_none(),
                "a freshly opened settings store retains the once-only hint"
            );
        });
        assert!(args.lock().unwrap().is_empty(), "showing a hint starts no process");
        engine.shutdown();
    }

    fn catalog_version(dir: &Path, version: &str) -> ToolStore {
        catalog_items(dir, version, &[("debug", "debugging"), ("design", "design")])
    }

    fn catalog_items(dir: &Path, version: &str, entries: &[(&str, &str)]) -> ToolStore {
        let entries: Vec<_> = entries.iter().map(|&(id, job)| (id, job, version)).collect();
        catalog_items_versions(dir, &entries)
    }

    fn catalog_items_versions(dir: &Path, entries: &[(&str, &str, &str)]) -> ToolStore {
        let mut items = Vec::new();
        let mut archives = Vec::new();
        for &(id, job, version) in entries {
            let body = format!("---\nname: {id}\n---\n{id}_KIT_INSTRUCTIONS{version}");
            let skill = tempfile::tempdir().unwrap();
            std::fs::write(skill.path().join("SKILL.md"), &body).unwrap();
            let item = Item {
                id: id.into(),
                job: job.into(),
                license: "MIT".into(),
                repo: "fixture/skills".into(),
                commit: "1234".into(),
                path: format!("skills/{id}"),
                digest: digest(skill.path()).unwrap(),
                server: None,
                chat_safe: false,
                cli_name: None,
                tier: Default::default(),
            };
            let archive = dir.join(format!("{id}.zip"));
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
            zip.start_file(format!("fixture-1234/{}/SKILL.md", item.path), zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(body.as_bytes()).unwrap();
            zip.finish().unwrap();
            items.push(item);
            archives.push(archive);
        }
        let catalog = ToolStore::with_catalog(dir, items.clone());
        for (item, archive) in items.iter().zip(archives) {
            catalog.install_from(item, &archive).unwrap();
        }
        prepare(&catalog, dir);
        catalog
    }

    fn server_catalog(dir: &Path, version: &str) -> ToolStore {
        server_catalog_scope(dir, version, false)
    }

    fn server_catalog_scope(dir: &Path, version: &str, chat_safe: bool) -> ToolStore {
        let body = format!("console.log('{version}');");
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("server.js"), &body).unwrap();
        let item = Item {
            id: "browser".into(),
            job: "measure".into(),
            license: "MIT".into(),
            repo: String::new(),
            commit: version.into(),
            path: String::new(),
            digest: digest(source.path()).unwrap(),
            chat_safe,
            cli_name: None,
            tier: Default::default(),
            server: Some(crate::tools::Server {
                package: "browser-fixture".into(),
                version: "1.0.0".into(),
                entry: "server.js".into(),
                args: vec![],
                env: BTreeMap::new(),
            }),
        };
        let path = dir.join(format!("server-{version}.tgz"));
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(&path).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, "package/server.js", body.as_bytes()).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        let mut items = vec![item.clone()];
        for (id, job) in [("debug", "debugging"), ("design", "design")] {
            let path = dir.join("tools/plugin/skills").join(id);
            if path.is_dir() {
                items.push(Item {
                    id: id.into(),
                    job: job.into(),
                    license: "MIT".into(),
                    repo: "fixture/skills".into(),
                    commit: "1234".into(),
                    path: format!("skills/{id}"),
                    digest: digest(&path).unwrap(),
                    server: None,
                    chat_safe: false,
                    cli_name: None,
                    tier: Default::default(),
                });
            }
        }
        let store = ToolStore::with_catalog(dir, items).with_node("C:/node.exe".into());
        store.install_from(&item, &path).unwrap();
        store.prepare_kit_for(Vendor::Codex, "Measure lighthouse performance", Some(dir), Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        store
    }

    fn prepare(store: &ToolStore, folder: &Path) {
        // Preparing one task must cache resources, never that task's selection or permission scope.
        store.prepare_kit_for(Vendor::Codex, "Debug this failure", Some(folder), Area::Chat, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
    }

    fn engine(root: &crate::workspace::TestRoot, spawn: Spawner) -> Engine {
        let dir = root.path().canonicalize().unwrap();
        let dir = dir.as_path();
        let engine = Engine::new(
            EngineOptions {
                data_dir: dir.into(),
                #[cfg(windows)]
                chat_dir: dir.parent().unwrap().join("work"),
                clis: [Vendor::Claude, Vendor::Codex]
                    .into_iter()
                    .map(|vendor| (vendor, Cli { command: vendor.name().into(), prefix: vec![] }))
                    .collect(),
                spawn,
                windows: false,
                claude_env_billed: false,
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tokio::runtime::Handle::current(),
                ask: None,
            },
            Arc::new(|_| {}),
        )
        .unwrap();
        engine.with(|core| {
            core.synchronous_kit_checks = true;
            core.store.save_settings(&Settings { permissions: Some(Permissions::FullAccess), ..Default::default() }).unwrap();
            core.claude_flags.plugin_dir = true;
            core.catalog_store = Some(catalog(dir));
            core.codex_models = crate::route::PROFILES.iter().filter(|p| p.vendor == Vendor::Codex).map(|p| p.model.into()).collect();
            for vendor in [Vendor::Claude, Vendor::Codex] {
                core.ledger.of_mut(vendor).billing = Billing::Subscription;
            }
        });
        engine
    }

    fn conversation(core: &mut Core, area: Area, project: Option<&str>, folder: &Path, task: &str) {
        let mut c: Conversation = serde_json::from_value(json!({
            "id":"kit", "title":"Kit", "area":area, "projectId":project, "workingFolder":folder,
            "createdAt":1, "updatedAt":1, "messages":[]
        }))
        .unwrap();
        c.messages.push(Message::new("user".into(), Role::User, task.into(), 1));
        c.messages.push(Message::new("reply".into(), Role::Assistant, String::new(), 1));
        core.convs.insert("kit".into(), c);
    }

    fn pick(vendor: Vendor) -> Pick {
        Pick { vendor, model: None, effort: "medium".into(), reason: "reason.manual".into(), pace: None }
    }

    async fn checked_signal(signal: &std::sync::mpsc::Receiver<()>) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while signal.try_recv().is_err() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the off-lock launch check did not reach its gate");
    }

    async fn input_with(messages: &Messages, needle: &str) {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !messages.lock().unwrap().iter().any(|(_, message)| message.to_string().contains(needle)) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the user turn was not sent");
    }

    #[tokio::test]
    async fn production_launch_check_leaves_other_chats_responsive_and_composer_warm_cannot_replace_send() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let (started, entered) = std::sync::mpsc::channel();
        let (finished, ended) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug the failure");
            core.convs.get_mut("kit").unwrap().working_folder = None;
            let mut peer = core.convs["kit"].clone();
            peer.id = "peer".into();
            core.convs.insert("peer".into(), peer);
            core.launch("peer", &pick(Vendor::Claude)).unwrap();
            core.synchronous_kit_checks = false;
            core.kit_check_hook = Some((started, gate.clone(), finished));
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "KIT_TURN".into(), vec![], true).unwrap();
            assert!(!core.live.contains_key("kit"));
        });
        checked_signal(&entered).await;
        engine.with(|core| {
            core.rewind.captured_for_test("reply");
            core.start("peer", "reply", &pick(Vendor::Claude), "PEER_TURN".into(), vec![], true).unwrap();
            assert_eq!(core.kit_check_gate.available_permits(), 0, "hash buffers have one off-lock owner");
        });
        input_with(&messages, "PEER_TURN").await;
        for vendor in ["codex", "claude"] {
            engine.warm(WarmRequest {
                conversation_id: Some("kit".into()),
                area: Area::Chat,
                project_id: None,
                choice: Choice { vendor: vendor.into(), model: None, effort: None },
            });
        }
        assert_eq!(args.lock().unwrap().len(), 1, "the waiting reply starts no CLI and keeps its continuation");
        gate.wait();
        checked_signal(&ended).await;
        input_with(&messages, "KIT_TURN").await;
        engine.with(|core| {
            assert!(core.kit_checks.is_empty());
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_start_card_isolated_driver_path_excludes_internal_questions() {
        const CARD: &str = "```faber-start-card\nISOLATED_APPROVED_CARD\n```";
        for aside in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let delivery = crate::tools::Delivery { start_card: Some(CARD.into()), ..Default::default() };
            let instructions = aside.then(|| "Internal question only".to_string());
            let launch = ClaudeLaunch {
                helper: Default::default(),
                browse: false,
                conversations: false,
                verify: false,
                progress: false,
                area: Area::Project,
                model: None,
                effort: None,
                resume: None,
                user_hooks: false,
                permissions: Permissions::Isolated,
                instructions: instructions.clone(),
                research: None,
                group_instructions: None,
                user_name: None,
                flags: Default::default(),
                tuning: Default::default(),
                tools: delivery.clone(),
            };
            let argv = crate::claude::claude_args(&launch);
            assert_eq!(argv.iter().any(|arg| arg.contains(CARD)), !aside);
            let session = engine.with(|core| {
                let cli = core.opts.clis[&Vendor::Codex].clone();
                core.codex_server(Area::Project, false, Permissions::Isolated, &cli).open_with_tools(
                    CodexThread {
                        helper: Default::default(),
                        browse: false,
                        conversations: false,
                        verify: false,
                        progress: false,
                        cwd: folder.clone(),
                        model: None,
                        effort: None,
                        resume: None,
                        instructions,
                        research: None,
                        user_name: None,
                        group_instructions: None,
                        permissions: Permissions::Isolated,
                        leave_plan: false,
                        tuning: Default::default(),
                    },
                    Arc::new(|_| {}),
                    delivery,
                )
            });
            session.send(UserTurn::text("ISOLATED_CARD_TURN"));
            input_with(&messages, "ISOLATED_CARD_TURN").await;
            let seen = messages.lock().unwrap();
            assert!(seen.iter().any(|(_, message)| message["method"] == "config/read"));
            let opened = seen.iter().find(|(_, message)| message["method"] == "thread/start").unwrap();
            assert_eq!(opened.1["params"]["developerInstructions"].as_str().is_some_and(|words| words.contains(CARD)), !aside);
            drop(seen);
            session.close();
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn project_start_card_selects_latest_beyond_memory_listing_window() {
        use crate::project::memory::KeptDecision;
        let dir = crate::workspace::TestRoot::new();
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().canonicalize().unwrap();
        std::fs::create_dir(folder.join(".faber")).unwrap();
        let approved = "- NEWEST_PERSON_KEEP";
        let mut text = (0..4097).map(|n| format!("- Earlier {n}. [by AI]\n")).collect::<String>();
        text.push_str(&format!("{approved} [by person]\n"));
        assert!(text.len() < crate::project::memory::MAX_BYTES);
        std::fs::write(folder.join(".faber/decisions.md"), text).unwrap();
        let display = crate::project::memory::list_faber(&folder).unwrap();
        assert!(display.incomplete);
        assert_eq!(display.files[0].entries.len(), 4096, "the Memory display window remains bounded");
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            core.store.keep_decision(&project.id, KeptDecision::of(approved), std::slice::from_ref(&project.id)).unwrap();
            conversation(core, Area::Project, Some(&project.id), &folder, "CARD_NEWEST_TURN");
            core.synchronous_kit_checks = false;
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "CARD_NEWEST_TURN".into(), vec![], true).unwrap();
        });
        input_with(&messages, "CARD_NEWEST_TURN").await;
        let observed = args.lock().unwrap();
        let prompt = observed.iter().flat_map(|(_, argv)| argv.windows(2)).find(|pair| pair[0] == "--append-system-prompt").unwrap();
        assert!(prompt[1].contains("NEWEST_PERSON_KEEP"), "the newest kept decision must not be hidden by Memory's display window");
        drop(observed);
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_start_card_stale_file_reference_leaves_the_card_without_losing_approval() {
        project_start_card_stale_file_reference_captured(Vendor::Claude).await;
    }

    #[tokio::test]
    async fn project_start_card_stale_file_reference_codex_preserves_approval() {
        project_start_card_stale_file_reference_captured(Vendor::Codex).await;
    }

    async fn project_start_card_stale_file_reference_captured(vendor: Vendor) {
        use crate::project::memory::Edit;
        let dir = crate::workspace::TestRoot::new();
        let work = tempfile::tempdir().unwrap();
        let folder = work.path().canonicalize().unwrap();
        std::fs::create_dir(folder.join(".faber")).unwrap();
        std::fs::create_dir(folder.join("src")).unwrap();
        std::fs::write(folder.join("src/lib.rs"), "pub fn public_api() {}\n").unwrap();
        let text =
            "- STALE_FILE_DECISION preserve `src/lib.rs`. [by AI]\n- KEEP_COMMAND_TEXT use `cargo test` and `setting.name`. [by AI]\n";
        std::fs::write(folder.join(".faber/decisions.md"), text).unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        let list = engine.project_memory(&project.id).unwrap();
        let file = list.files.iter().find(|file| file.path == ".faber/decisions.md").unwrap();
        for entry in &file.entries {
            engine
                .edit_project_memory(
                    &project.id,
                    &Edit {
                        path: file.path.clone(),
                        revision: file.revision.clone(),
                        start: entry.start,
                        end: entry.end,
                        text: None,
                        keep: true,
                    },
                )
                .unwrap();
        }
        let approval = engine.with(|core| core.store.kept_decisions(&project.id));
        assert_eq!(approval.len(), 2);
        std::fs::remove_file(folder.join("src/lib.rs")).unwrap();
        engine.with(|core| {
            conversation(core, Area::Project, Some(&project.id), &folder, "STALE_REFERENCE_TURN");
            core.synchronous_kit_checks = false;
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(vendor), "STALE_REFERENCE_TURN".into(), vec![], true).unwrap();
        });
        input_with(&messages, "STALE_REFERENCE_TURN").await;
        let prompt = if vendor == Vendor::Claude {
            args.lock()
                .unwrap()
                .iter()
                .find_map(|(_, argv)| argv.windows(2).find(|pair| pair[0] == "--append-system-prompt").map(|pair| pair[1].clone()))
                .unwrap()
        } else {
            messages
                .lock()
                .unwrap()
                .iter()
                .find_map(|(_, message)| {
                    (message["method"] == "thread/start").then(|| message["params"]["developerInstructions"].as_str().unwrap().to_string())
                })
                .unwrap()
        };
        assert!(
            !prompt.contains("STALE_FILE_DECISION"),
            "a kept decision whose recorded file disappeared must leave the actual driver card"
        );
        assert!(prompt.contains("KEEP_COMMAND_TEXT"), "commands and settings are never guessed to be missing files");
        assert_eq!(std::fs::read_to_string(folder.join(".faber/decisions.md")).unwrap(), text);
        assert_eq!(engine.with(|core| core.store.kept_decisions(&project.id)), approval);
        engine.shutdown();
    }

    async fn project_start_card_captured(vendor: Vendor) {
        use crate::project::memory::KeptDecision;
        let mut previous_card = None::<String>;
        for state in ["fresh", "matching", "legacy", "changed", "wrong-native", "revoked"] {
            let resumed = state != "fresh";
            let dir = crate::workspace::TestRoot::new();
            let work = tempfile::tempdir().unwrap();
            let folder = work.path().canonicalize().unwrap();
            std::fs::create_dir(folder.join(".faber")).unwrap();
            std::fs::write(folder.join("package.json"), r#"{"description":"UNTRUSTED_DESCRIPTION","scripts":{"test":"node --test"}}"#)
                .unwrap();
            let approved = "- Preserve public exports.";
            std::fs::write(
                folder.join(".faber/decisions.md"),
                format!("{approved} [by person]\n- FORGED_PERSON [by person]\n- UNREVIEWED_AI [by AI]\n"),
            )
            .unwrap();
            std::fs::write(folder.join(".faber/taste.md"), "UNAPPROVED_TASTE [by person]\n").unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            engine.with(|core| {
                core.store.keep_decision(&project.id, KeptDecision::of(approved), std::slice::from_ref(&project.id)).unwrap();
                conversation(core, Area::Project, Some(&project.id), &folder, "CARD_CAPTURE_TURN");
                if resumed {
                    let c = core.convs.get_mut("kit").unwrap();
                    c.sessions.insert(vendor, "native-history".into());
                    if state != "legacy" {
                        let id = if state == "wrong-native" { "another-native" } else { "native-history" };
                        let card =
                            if state == "changed" { "```faber-start-card\nOLD_CARD\n```".into() } else { previous_card.clone().unwrap() };
                        c.session_start_cards.insert(vendor, (id.into(), card));
                    }
                    let old_user = Message::new("earlier-user".into(), Role::User, "VERBATIM_OLD_REQUEST".into(), 0);
                    let old_reply = Message::new("earlier-reply".into(), Role::Assistant, "Earlier answer".into(), 0);
                    c.messages.splice(0..0, [old_user, old_reply]);
                }
                if state == "revoked" {
                    std::fs::write(
                        folder.join(".faber/decisions.md"),
                        "- Preserve public exports.\n  But remove the public API. [by person]\n",
                    )
                    .unwrap();
                }
                core.synchronous_kit_checks = false;
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(vendor), "CARD_CAPTURE_TURN".into(), vec![], true).unwrap();
                assert!(!core.live.contains_key("kit"), "source reads must precede spawning outside Core");
            });
            input_with(&messages, "CARD_CAPTURE_TURN").await;
            let prompt = if vendor == Vendor::Claude {
                args.lock()
                    .unwrap()
                    .iter()
                    .find_map(|(_, argv)| argv.windows(2).find(|p| p[0] == "--append-system-prompt").map(|p| p[1].clone()))
                    .expect("the project persona is sent")
            } else {
                messages
                    .lock()
                    .unwrap()
                    .iter()
                    .find_map(|(_, message)| {
                        (message["method"] == if state == "matching" { "thread/resume" } else { "thread/start" })
                            .then(|| message["params"]["developerInstructions"].as_str().unwrap_or_default().to_string())
                    })
                    .expect("the project thread is opened")
            };
            if resumed && state != "matching" {
                assert!(!args.lock().unwrap().iter().any(|(_, argv)| argv.iter().any(|arg| arg == "--resume")));
                assert!(
                    !messages.lock().unwrap().iter().any(|(_, message)| message["method"] == "thread/resume"),
                    "a legacy native session without a saved card starts fresh with history"
                );
            }
            let card = prompt.split_once("```faber-start-card\n").expect("the real driver sends the project start card").1;
            assert_eq!(card.contains("Preserve public exports."), state != "revoked", "exact current approved content only: {state}");
            for denied in ["UNTRUSTED_DESCRIPTION", "FORGED_PERSON", "UNREVIEWED_AI", "UNAPPROVED_TASTE"] {
                assert!(!prompt.contains(denied), "unapproved repository prose must stay out: {denied}");
            }
            assert_eq!(prompt.matches("```faber-start-card\n").count(), 1);
            assert!(card.split_once("```").unwrap().0.len() + "```faber-start-card\n```".len() <= 6 * 1024);
            let sent = format!("```faber-start-card\n{}```", card.split_once("```").unwrap().0);
            if resumed && state != "matching" {
                assert!(
                    messages.lock().unwrap().iter().any(|(_, message)| message.to_string().contains("VERBATIM_OLD_REQUEST")),
                    "native restart preserves Faber's previous requests: {state}"
                );
            }
            engine.with(|core| {
                let sid = core.live["kit"].sid;
                core.handle("kit", sid, EngineEvent::Started { session_id: "native-history".into(), model: "fixture".into() });
                core.handle("kit", sid, EngineEvent::TurnStarted);
                assert_eq!(core.convs["kit"].session_start_cards.get(&vendor), Some(&("native-history".into(), sent.clone())));
            });
            engine.shutdown();
            if state == "fresh" {
                previous_card = Some(sent);
            }
        }
    }

    #[tokio::test]
    async fn project_start_card_claude_actual_start_and_resume() {
        project_start_card_captured(Vendor::Claude).await;
    }

    #[tokio::test]
    async fn project_start_card_codex_actual_start_and_resume() {
        project_start_card_captured(Vendor::Codex).await;
    }

    #[tokio::test]
    async fn project_facts_current_working_copy_selects_kit_before_first_spawn() {
        let dir = crate::workspace::TestRoot::new();
        let original = tempfile::tempdir().unwrap();
        let working = tempfile::tempdir().unwrap();
        std::fs::write(original.path().join("package.json"), r#"{"dependencies":{"expo":"1"}}"#).unwrap();
        std::fs::write(working.path().join("package.json"), r#"{"dependencies":{"stripe":"1"}}"#).unwrap();
        let folder = working.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&original.path().to_string_lossy()).unwrap();
        engine.with(|core| {
            core.catalog_store = Some(catalog_items(dir.path(), "", &[("stripe-best-practices", "payments"), ("debug", "debugging")]));
            conversation(core, Area::Project, Some(&project.id), &folder, "Fix the checkout payment bug");
            core.convs.get_mut("kit").unwrap().copy = Some(crate::project::copy::WorkCopy {
                path: folder.to_string_lossy().into_owned(),
                branch: "fixture".into(),
                root: original.path().to_string_lossy().into_owned(),
                sub: String::new(),
                base: None,
            });
            core.synchronous_kit_checks = false;
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "FACTS_KIT_TURN".into(), vec![], true).unwrap();
            assert!(!core.live.contains_key("kit"));
        });
        assert!(args.lock().unwrap().is_empty(), "selection must finish before spawning the session");
        input_with(&messages, "FACTS_KIT_TURN").await;
        engine.with(|core| {
            assert!(
                core.live["kit"].tool_kit.ids.contains(&"stripe-best-practices".into()),
                "the actual working copy supplies dependency evidence on the first launch"
            );
            assert!(core.live["kit"].tool_kit.ids.contains(&"debug".into()), "unconstrained matching resources remain eligible");
            assert!(core.kit_checks.is_empty());
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_facts_empty_kit_still_selects_outside_the_core_lock() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, _) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            conversation(core, Area::Project, Some(&project.id), &folder, "What is two plus two?");
            core.catalog_store = Some(ToolStore::with_catalog(&folder, vec![]));
            core.synchronous_kit_checks = false;
            let result = core.checked_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            assert!(
                matches!(result, Err(ref error) if error == mcp_servers::CHECK_PENDING),
                "even an empty selection needs the off-lock project facts check"
            );
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn project_facts_leave_chat_warming_unchanged() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let sid = engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            core.convs.get_mut("kit").unwrap().working_folder = None;
            core.warm_launch("kit", &pick(Vendor::Claude));
            assert!(core.live["kit"].prepared);
            core.live["kit"].sid
        });
        engine.with(|core| {
            core.synchronous_kit_checks = false;
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "FACTS_WARM_TURN".into(), vec![], true).unwrap();
        });
        input_with(&messages, "FACTS_WARM_TURN").await;
        engine.with(|core| assert_eq!(core.live["kit"].sid, sid, "Chat still reuses its prepared process"));
        assert_eq!(args.lock().unwrap().len(), 1);
        engine.shutdown();
    }

    #[tokio::test]
    async fn production_launch_check_cancels_stopped_turns_and_rejects_tampering_before_spawn() {
        for change in ["stop", "tamper", "switch"] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let (started, entered) = std::sync::mpsc::channel();
            let (finished, ended) = std::sync::mpsc::channel();
            let gate = Arc::new(std::sync::Barrier::new(2));
            let source = engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug the failure");
                core.synchronous_kit_checks = false;
                core.kit_check_hook = Some((started, gate.clone(), finished));
                core.rewind.captured_for_test("reply");
                let source = core
                    .catalog_store()
                    .ready_kit_for(Vendor::Codex, "Debug", None, Area::Chat, Permissions::FullAccess, &[])
                    .0
                    .skills
                    .unwrap()
                    .join("debug/SKILL.md");
                core.start("kit", "reply", &pick(Vendor::Claude), "CHECKED_TURN".into(), vec![], true).unwrap();
                source
            });
            checked_signal(&entered).await;
            if change == "stop" {
                engine.interrupt("kit");
            } else if change == "switch" {
                engine.with(|core| core.catalog_store().set_enabled("debug", false).unwrap());
            } else {
                std::fs::write(source, "TAMPERED_INSTRUCTIONS").unwrap();
            }
            gate.wait();
            checked_signal(&ended).await;
            if change == "stop" {
                tokio::task::yield_now().await;
                assert!(args.lock().unwrap().is_empty());
                engine.with(|core| assert!(core.kit_checks.is_empty() && !core.live.contains_key("kit")));
            } else {
                input_with(&messages, "CHECKED_TURN").await;
                engine.with(|core| {
                    assert!(core.live["kit"].tool_kit.ids.is_empty());
                    let expected = (change == "tamper").then_some("error.tools.reinstall");
                    assert_eq!(core.live["kit"].tool_kit.error.as_deref(), expected);
                    assert_eq!(core.catalog_store().item_status("debug").as_deref(), expected);
                });
                assert!(!messages.lock().unwrap().iter().any(|(_, message)| message.to_string().contains("TAMPERED_INSTRUCTIONS")));
                if change == "tamper" {
                    engine.with(|core| {
                        core.retire("kit").unwrap().session.close();
                        let c = core.convs.get_mut("kit").unwrap();
                        c.messages.extend([
                            Message::new("design-request".into(), Role::User, "Design a layout".into(), 3),
                            Message::new("design-reply".into(), Role::Assistant, String::new(), 4),
                        ]);
                        core.rewind.captured_for_test("design-reply");
                        core.start("kit", "design-reply", &pick(Vendor::Claude), "HEALTHY_DESIGN_TURN".into(), vec![], true).unwrap();
                    });
                    input_with(&messages, "HEALTHY_DESIGN_TURN").await;
                    engine.with(|core| {
                        assert_eq!(core.live["kit"].tool_kit.ids, ["design"]);
                        assert!(
                            core.live["kit"].tool_kit.error.is_none(),
                            "a prior launch's omitted bad item does not warn on later healthy work"
                        );
                        assert_eq!(core.catalog_store().item_status("debug").as_deref(), Some("error.tools.reinstall"));
                    });
                }
            }
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn production_launch_checks_bound_hashing_to_one_worker() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let (first, entered_first) = std::sync::mpsc::channel();
        let (finished_first, ended_first) = std::sync::mpsc::channel();
        let gate_first = Arc::new(std::sync::Barrier::new(2));
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug the failure");
            let mut second = core.convs["kit"].clone();
            second.id = "second".into();
            core.convs.insert("second".into(), second);
            core.synchronous_kit_checks = false;
            core.kit_check_hook = Some((first, gate_first.clone(), finished_first));
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "FIRST_CHECK".into(), vec![], true).unwrap();
        });
        checked_signal(&entered_first).await;
        let (second, entered_second) = std::sync::mpsc::channel();
        let (finished_second, ended_second) = std::sync::mpsc::channel();
        let gate_second = Arc::new(std::sync::Barrier::new(2));
        engine.with(|core| {
            core.kit_check_hook = Some((second, gate_second.clone(), finished_second));
            core.rewind.captured_for_test("reply");
            core.start("second", "reply", &pick(Vendor::Claude), "SECOND_CHECK".into(), vec![], true).unwrap();
        });
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }
        assert!(entered_second.try_recv().is_err(), "the second worker cannot allocate a hash buffer before the first finishes");
        engine.with(|core| assert_eq!(core.kit_checks.len(), 2, "both pending conversations remain reachable"));
        gate_first.wait();
        checked_signal(&ended_first).await;
        checked_signal(&entered_second).await;
        input_with(&messages, "FIRST_CHECK").await;
        gate_second.wait();
        checked_signal(&ended_second).await;
        input_with(&messages, "SECOND_CHECK").await;
        engine.shutdown();
    }

    #[tokio::test]
    async fn core_keeps_a_session_kit_until_retirement_and_rechecks_switches_for_the_next_one() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for area in [Area::Chat, Area::Project] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, _) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                engine.with(|core| {
                    conversation(core, area, Some(&project.id), &folder, "Debug this failure");
                    let assembly_started = std::time::Instant::now();
                    let (measured, _) = core.catalog_store().kit("Debug this failure", Some(&folder), area, Permissions::FullAccess, &[]);
                    let assembly_time = assembly_started.elapsed();
                    let bytes = measured.skill_instructions().unwrap().len();
                    assert!(args.lock().unwrap().is_empty(), "assembling a kit adds no process");
                    println!(
                        "kit assembly {vendor:?}/{area:?}: {} us, {bytes} instruction bytes, 0 added processes",
                        assembly_time.as_micros()
                    );
                    drop(measured);
                    core.launch("kit", &pick(vendor)).unwrap();
                    let first = core.live["kit"].sid;
                    let expected = vec!["debug"];
                    assert_eq!(core.live["kit"].tool_kit.ids, expected);
                    core.catalog_store().set_enabled("debug", false).unwrap();
                    core.convs.get_mut("kit").unwrap().messages[0].text = "Design a layout".into();
                    core.launch("kit", &pick(vendor)).unwrap();
                    assert_eq!(core.live["kit"].sid, first);
                    assert_eq!(args.lock().unwrap().len(), 1, "reusing a session adds no process");
                    assert_eq!(core.live["kit"].tool_kit.ids, expected, "mid-session tasks and catalog switches do not change its kit");
                    core.retire("kit").unwrap().session.close();
                    core.launch("kit", &pick(vendor)).unwrap();
                    assert_ne!(core.live["kit"].sid, first);
                    assert!(!core.live["kit"].tool_kit.ids.iter().any(|id| id == "debug"), "disabled tools never join a new session");
                    assert!(core.live["kit"].tool_kit.ids.iter().all(|id| id == "design"), "a not-yet-ready replacement may be empty");
                });
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn moving_chat_to_a_project_discards_its_native_kit_and_picks_for_the_project_turn() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(vendor, "previous-chat-session".into());
                c.session_kits.insert(vendor, ("previous-chat-session".into(), vec!["debug".into()], Some("servers:none".into())));
                core.save("kit");
            });
            let moved = engine.move_to_project("kit", &project.id).unwrap();
            assert!(moved.sessions.is_empty() && moved.session_kits.is_empty());
            engine.with(|core| {
                core.convs.get_mut("kit").unwrap().messages.push(Message::new(
                    "project-task".into(),
                    Role::User,
                    "Design a landing page".into(),
                    2,
                ));
                core.launch("kit", &pick(vendor)).unwrap();
                assert_eq!(core.live["kit"].tool_kit.ids, ["design"]);
                assert!(!core.live["kit"].resumed);
            });
            if vendor == Vendor::Claude {
                assert!(!args.lock().unwrap()[0].1.iter().any(|arg| arg == "--resume"));
            } else {
                until_message(&messages, "thread/start").await;
                assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/resume"));
            }
            engine.shutdown();
        }
    }

    async fn until_message(messages: &Messages, method: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(msg) = messages.lock().unwrap().iter().find_map(|(_, msg)| (msg["method"] == method).then(|| msg.clone())) {
                    break msg;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("no {method} on fake wire: {:?}", messages.lock().unwrap()))
    }

    async fn claude_input(messages: &Messages) -> String {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(text) = messages.lock().unwrap().iter().find_map(|(bin, msg)| {
                    (bin == "claude" && msg["type"] == "user").then(|| msg["message"]["content"].as_str().unwrap_or_default().to_string())
                }) {
                    break text;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the current catalog resources must reach the Claude user input")
    }

    /// The folders a Claude Code launch was handed with `--plugin-dir`.
    fn plugin_dirs(args: &Arguments) -> Vec<PathBuf> {
        let recorded = args.lock().unwrap();
        let (_, argv) = recorded.iter().find(|(bin, _)| bin == "claude").expect("a Claude Code launch");
        argv.windows(2).filter(|a| a[0] == "--plugin-dir").map(|a| PathBuf::from(&a[1])).collect()
    }

    fn guide_skill(plugins: &[PathBuf]) -> Option<String> {
        plugins.iter().find_map(|dir| std::fs::read_to_string(dir.join("skills/guide/SKILL.md")).ok())
    }

    /// The rules a Claude Code launch may use with no card (`--allowedTools`, each rule its own argument).
    fn allowed_tools(args: &Arguments) -> Vec<String> {
        let recorded = args.lock().unwrap();
        let (_, argv) = recorded.iter().find(|(bin, _)| bin == "claude").expect("a Claude Code launch");
        argv.iter()
            .position(|a| a == "--allowedTools")
            .map(|at| argv[at + 1..].iter().take_while(|a| !a.starts_with("--")).cloned().collect())
            .unwrap_or_default()
    }

    /// The deny rules in the settings a Claude Code launch was handed.
    fn denied_tools(args: &Arguments) -> Vec<String> {
        let recorded = args.lock().unwrap();
        let (_, argv) = recorded.iter().find(|(bin, _)| bin == "claude").expect("a Claude Code launch");
        let settings = argv.windows(2).find(|a| a[0] == "--settings").expect("settings");
        let settings: Value = serde_json::from_str(&settings[1]).unwrap();
        settings["permissions"]["deny"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect()
    }

    /// The rule path of a file the way Claude Code reads it: `//c/dir/file` on Windows, `/dir/file` elsewhere.
    fn rule_path(path: &Path) -> String {
        crate::permissions::claude_rule_path(&crate::project::git::native_path(path))
    }

    fn guide_file(folder: &Path) -> PathBuf {
        folder.join("guide-plugin/skills/guide/SKILL.md")
    }

    fn set_screen(core: &mut Core, language: &str, permissions: Permissions) {
        core.store
            .save_settings(&Settings { language: Some(language.into()), permissions: Some(permissions), ..Default::default() })
            .unwrap();
    }

    /// Marks the saved session as told of the guide as it stands, so a test about the kit sees no line ahead of its words.
    fn told_of_guide(core: &mut Core, vendor: Vendor, area: Area) {
        let digest = guide_digest(guide_note(core, vendor, area).as_deref());
        core.convs.get_mut("kit").unwrap().session_guide_digests.insert(vendor, digest);
    }

    /// The line a session of this vendor and area is told of the guide.
    fn guide_note(core: &Core, vendor: Vendor, area: Area) -> Option<String> {
        core.guide_for(vendor, area, Permissions::Ask).note()
    }

    /// Faber's own guide (R18) reaches every fresh session, whatever the first message says, and lazily wherever a file can be
    /// read: Claude Code in Projects loads it as a plugin of its own (nothing rides on the turn), Claude Code in Chat and
    /// Codex carry one line that points at the file, in the language the screen shows.
    #[tokio::test]
    async fn faber_guide_is_a_line_or_a_skill_in_every_fresh_session_never_the_whole_text() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for area in [Area::Chat, Area::Project] {
                for task in ["How do I switch the theme?", "Debug this failing test"] {
                    let dir = crate::workspace::TestRoot::new();
                    let folder = dir.path().canonicalize().unwrap();
                    let (spawn, args, messages) = fake();
                    let engine = engine(&dir, spawn);
                    let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                    let note = engine.with(|core| {
                        set_screen(core, "de", Permissions::Ask);
                        conversation(core, area, (area == Area::Project).then_some(project.id.as_str()), &folder, task);
                        core.rewind.captured_for_test("reply");
                        core.start("kit", "reply", &pick(vendor), task.into(), vec![], true).unwrap();
                        guide_note(core, vendor, area)
                    });
                    let which = format!("{vendor:?} {area:?} {task}");
                    assert_eq!(
                        std::fs::read_to_string(guide_file(&folder)).unwrap(),
                        crate::guide::skill_file(crate::guide::Language::De),
                        "{which}: the file holds the guide in the screen's language"
                    );
                    let read = format!("Read({})", rule_path(&guide_file(&folder)));
                    match (vendor, area) {
                        (Vendor::Claude, Area::Chat) => {
                            let text = claude_input(&messages).await;
                            let note = note.unwrap();
                            assert!(text.starts_with(&note) && text.ends_with(task), "{which}: {text}");
                            assert!(note.contains("German") && note.contains("SKILL.md") && note.len() < 1000, "{which}: {note}");
                            assert!(!note.contains("# Frontend"), "{which}: skill bodies remain lazy");
                            assert!(!text.contains("Selbst weitermachen"), "{which}: the table stays in the file");
                            assert!(plugin_dirs(&args).is_empty(), "{which}: Chat has no Skill tool to load a plugin");
                            assert!(
                                allowed_tools(&args).contains(&read),
                                "{which}: reading the file needs no card: {:?}",
                                allowed_tools(&args)
                            );
                        }
                        (Vendor::Claude, Area::Project) => {
                            let text = claude_input(&messages).await;
                            assert!(
                                note.is_none() && !text.contains("Faber's own guide"),
                                "{which}: the CLI lists the skill itself: {text}"
                            );
                            let plugins = plugin_dirs(&args);
                            assert_eq!(plugins.len(), 2, "{which}: the catalog's plugin and the guide's: {plugins:?}");
                            assert_eq!(guide_skill(&plugins), Some(crate::guide::skill_file(crate::guide::Language::De)), "{which}");
                            assert!(!text.contains("faber-guide"), "{which}: the kit's index never lists it");
                            assert_eq!(
                                allowed_tools(&args),
                                ["Skill(faber-guide:guide)", "Skill(faber-guide:frontend-craft)", "Skill(faber-guide:frontend-proof)"],
                                "{which}"
                            );
                        }
                        (Vendor::Codex, _) => {
                            let thread = until_message(&messages, "thread/start").await;
                            let instructions = thread["params"]["developerInstructions"].as_str().unwrap();
                            let note = note.unwrap();
                            assert!(instructions.contains(&note) && !instructions.contains("Selbst weitermachen"), "{which}");
                            let turn = until_message(&messages, "turn/start").await;
                            assert!(!turn["params"]["input"][0]["text"].as_str().unwrap().contains("Faber's own guide"), "{which}");
                            assert!(
                                core_guide_digest(&engine, Vendor::Codex).is_some(),
                                "{which}: a new thread is told once, with its instructions"
                            );
                        }
                    }
                    engine.shutdown();
                }
            }
        }
    }

    fn core_guide_digest(engine: &Engine, vendor: Vendor) -> Option<String> {
        engine.with(|core| core.live["kit"].guide.digest.clone().filter(|_| core.live["kit"].vendor == vendor))
    }

    /// Where a file cannot be read the whole text goes (the folder is not Faber's to hand out), in the screen's language only; and
    /// an older Claude Code with no plugin folder reads the file like Chat does.
    #[tokio::test]
    async fn faber_guide_falls_back_to_text_without_its_folder_and_to_a_line_in_an_older_claude_code() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        std::fs::write(folder.join("guide-plugin"), "not a folder of Faber's").unwrap();
        let (spawn, args, messages) = fake();
        let first = engine(&dir, spawn);
        first.with(|core| {
            set_screen(core, "de", Permissions::Ask);
            conversation(core, Area::Chat, None, &folder, "Debug this failing test");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "Debug this failing test".into(), vec![], true).unwrap();
        });
        let text = claude_input(&messages).await;
        assert!(text.contains("Faber guide.") && text.contains("Selbst weitermachen") && !text.contains("Continuar sozinho"), "{text}");
        assert!(text.contains("The screen is shown in German"), "{text}");
        assert!(allowed_tools(&args).iter().all(|rule| !rule.starts_with("Read(")), "no file, no rule: {:?}", allowed_tools(&args));
        first.shutdown();

        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            set_screen(core, "fr", Permissions::Ask);
            core.claude_flags.plugin_dir = false;
            conversation(core, Area::Project, Some(&project.id), &folder, "Debug this failing test");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "Debug this failing test".into(), vec![], true).unwrap();
        });
        let text = claude_input(&messages).await;
        assert!(text.starts_with("Faber's own guide: ") && text.contains("French") && !text.contains("Continuer seul"), "{text}");
        assert!(plugin_dirs(&args).is_empty());
        assert!(allowed_tools(&args).contains(&format!("Read({})", rule_path(&guide_file(&folder)))));
        engine.shutdown();
    }

    /// The allow rules exist only where a card would stop the guide, and the guide's folder is closed to Edit and Write in every
    /// mode that has the guide.
    #[tokio::test]
    async fn faber_guide_is_allowed_without_a_card_in_guarded_modes_only_and_its_folder_is_closed_to_writes() {
        for permissions in [Permissions::Ask, Permissions::Plan, Permissions::FullAccess] {
            for area in [Area::Chat, Area::Project] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, messages) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                engine.with(|core| {
                    set_screen(core, "en", permissions);
                    conversation(core, area, (area == Area::Project).then_some(project.id.as_str()), &folder, "Debug this failing test");
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(Vendor::Claude), "Debug this failing test".into(), vec![], true).unwrap();
                });
                claude_input(&messages).await;
                let which = format!("{permissions:?} {area:?}");
                let allowed = allowed_tools(&args);
                let expected = match (permissions.guarded(), area) {
                    (false, _) => vec![],
                    (true, Area::Chat) => vec![format!("Read({})", rule_path(&guide_file(&folder)))],
                    (true, Area::Project) => vec!["Skill(faber-guide:guide)".to_string()],
                };
                assert!(
                    expected.iter().all(|rule| allowed.contains(rule)) && (!expected.is_empty() || allowed.is_empty()),
                    "{which}: {allowed:?}"
                );
                if permissions.guarded() {
                    for name in ["frontend-craft", "frontend-proof"] {
                        let rule = match area {
                            Area::Chat => format!("Read({})", rule_path(&folder.join(format!("guide-plugin/skills/{name}/SKILL.md")))),
                            Area::Project => format!("Skill(faber-guide:{name})"),
                        };
                        assert!(allowed.contains(&rule), "{which}: first-party skill needs no extra card: {allowed:?}");
                    }
                }
                let folder_rule = rule_path(&folder.join("guide-plugin"));
                let denied = denied_tools(&args);
                for tool in ["Edit", "Write"] {
                    assert!(denied.contains(&format!("{tool}({folder_rule}/**)")), "{which}: {tool} stays closed: {denied:?}");
                }
                engine.shutdown();
            }
        }
    }

    /// A session that resumes is told of the guide when it was never told, or told another line, and not when it was: the
    /// guide has its own digest, so a kit that changes never sends it again and a guide that changes never sends the kit.
    #[tokio::test]
    async fn a_resumed_claude_session_is_told_of_the_guide_by_its_own_digest_apart_from_the_kit() {
        for (guide_saved, kit_current, language) in
            [(Some(true), true, "en"), (Some(true), false, "en"), (Some(false), true, "en"), (None, true, "en"), (Some(true), true, "de")]
        {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let note = engine.with(|core| {
                set_screen(core, "en", Permissions::Ask);
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                let (delivery, report) = core.choose_session_tools("kit", Vendor::Claude, Permissions::Ask, &folder);
                let identity = session_tool_identity(Vendor::Claude, &delivery);
                let kit_digest = resource_digest(claude_resources(Area::Chat, &delivery).as_deref());
                let saved_note = delivery.guide.note();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Claude, "native-history".into());
                c.session_kits.insert(Vendor::Claude, ("native-history".into(), report.ids, identity));
                c.session_resource_digests.insert(Vendor::Claude, if kit_current { kit_digest } else { "stale".into() });
                match guide_saved {
                    Some(true) => c.session_guide_digests.insert(Vendor::Claude, guide_digest(saved_note.as_deref())),
                    Some(false) => c.session_guide_digests.insert(Vendor::Claude, "stale".into()),
                    None => None,
                };
                set_screen(core, language, Permissions::Ask);
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Claude), "Debug this failure".into(), vec![], true).unwrap();
                guide_note(core, Vendor::Claude, Area::Chat).unwrap()
            });
            let text = claude_input(&messages).await;
            let which = format!("saved {guide_saved:?}, kit current {kit_current}, screen {language}");
            let told = guide_saved != Some(true) || language != "en";
            assert_eq!(text.starts_with(&note), told, "{which}: {text}");
            assert_eq!(text.contains(RESOURCE_PREFIX), !kit_current, "{which}: {text}");
            if !told && kit_current {
                assert_eq!(text, "Debug this failure", "{which}");
            }
            let saved = engine.with(|core| core.convs["kit"].session_guide_digests.get(&Vendor::Claude).cloned());
            assert_eq!(saved, Some(guide_digest(Some(&note))), "{which}: what the session was told is what is saved");
            engine.shutdown();
        }
    }

    /// Codex's thread keeps the instructions it started with: a resumed thread gets no guide line with them, and gets one with
    /// its first input only when it was never told (an older thread) or told another line; a new thread was told at its start.
    #[tokio::test]
    async fn a_resumed_codex_thread_is_told_of_the_guide_in_its_first_input_only_when_it_was_not_told() {
        for told in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let note = engine.with(|core| {
                set_screen(core, "es", Permissions::Ask);
                conversation(core, Area::Chat, None, &folder, "Debug this failing test");
                let (delivery, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::Ask, &folder);
                let identity = session_tool_identity(Vendor::Codex, &delivery);
                let note = delivery.guide.note().unwrap();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "native-history".into());
                c.session_kits.insert(Vendor::Codex, ("native-history".into(), report.ids, identity));
                if told {
                    c.session_guide_digests.insert(Vendor::Codex, guide_digest(Some(&note)));
                }
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failing test".into(), vec![], true).unwrap();
                note
            });
            let thread = until_message(&messages, "thread/resume").await;
            assert!(!thread["params"]["developerInstructions"].as_str().unwrap_or_default().contains("Faber's own guide"), "told={told}");
            let turn = until_message(&messages, "turn/start").await;
            let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
            assert_eq!(sent.starts_with(&note), !told, "told={told}: {sent}");
            engine.shutdown();
        }
    }

    /// A first message that is a slash command goes alone, where Claude Code reads it; the line waits for the next ordinary
    /// message. After a compaction the summary replaced the first input, so the line goes again, and its saved digest is gone
    /// until it does. Codex keeps its thread's instructions through a compaction.
    #[tokio::test]
    async fn the_guide_line_waits_for_an_ordinary_message_and_returns_after_a_compaction() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let first = engine(&dir, spawn);
        let note = first.with(|core| {
            set_screen(core, "en", Permissions::Ask);
            conversation(core, Area::Chat, None, &folder, "/compact");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "/compact".into(), vec![], true).unwrap();
            guide_note(core, Vendor::Claude, Area::Chat).unwrap()
        });
        assert_eq!(claude_input(&messages).await, "/compact", "a slash command is not preceded by anything");
        first.with(|core| {
            assert_eq!(core.live["kit"].guide.pending.as_deref(), Some(note.as_str()));
            assert!(core.live["kit"].guide.digest.is_none() && core.convs["kit"].session_guide_digests.is_empty(), "not told yet");
            let l = core.live.get_mut("kit").unwrap();
            let next = first_input("How do I switch the theme?".into(), &mut l.kit_resources, &mut l.guide);
            assert!(next.starts_with(&note) && next.ends_with("How do I switch the theme?"), "{next}");
            assert!(l.guide.pending.is_none() && l.guide.digest.is_some(), "told now");
            let again = first_input("And the language?".into(), &mut l.kit_resources, &mut l.guide);
            assert_eq!(again, "And the language?", "told once");

            let sid = core.live["kit"].sid;
            core.convs.get_mut("kit").unwrap().session_guide_digests.insert(Vendor::Claude, "told".into());
            core.handle("kit", sid, EngineEvent::Compacted { after: Some(1000) });
            assert_eq!(core.live["kit"].guide.pending.as_deref(), Some(note.as_str()), "the summary lost the line");
            assert!(core.convs["kit"].session_guide_digests.is_empty(), "a restart before the next message tells it too");
            let l = core.live.get_mut("kit").unwrap();
            assert!(first_input("Next".into(), &mut l.kit_resources, &mut l.guide).starts_with(&note));
        });
        first.shutdown();

        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            set_screen(core, "en", Permissions::Ask);
            conversation(core, Area::Chat, None, &folder, "Debug this failing test");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failing test".into(), vec![], true).unwrap();
        });
        until_message(&messages, "turn/start").await;
        engine.with(|core| {
            let sid = core.live["kit"].sid;
            core.handle("kit", sid, EngineEvent::Compacted { after: Some(1000) });
            assert!(core.live["kit"].guide.pending.is_none(), "Codex's thread instructions are not part of what it compacts");
        });
        engine.shutdown();
    }

    /// A Codex thread told by its first input (one from before the guide, resumed) loses that message to a compaction, so the
    /// line goes again, as for Claude Code; a thread that has it in its instructions keeps it (the test above).
    #[tokio::test]
    async fn a_resumed_codex_thread_told_by_its_input_is_told_again_after_a_compaction() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let note = engine.with(|core| {
            set_screen(core, "en", Permissions::Ask);
            conversation(core, Area::Chat, None, &folder, "Debug this failing test");
            core.convs.get_mut("kit").unwrap().sessions.insert(Vendor::Codex, "native-history".into());
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failing test".into(), vec![], true).unwrap();
            guide_note(core, Vendor::Codex, Area::Chat).unwrap()
        });
        let turn = until_message(&messages, "turn/start").await;
        assert!(turn["params"]["input"][0]["text"].as_str().unwrap().starts_with(&note));
        engine.with(|core| {
            let sid = core.live["kit"].sid;
            core.convs.get_mut("kit").unwrap().session_guide_digests.insert(Vendor::Codex, "told".into());
            core.handle("kit", sid, EngineEvent::Compacted { after: None });
            assert_eq!(core.live["kit"].guide.pending.as_deref(), Some(note.as_str()), "the first input is a summary now");
            assert!(core.convs["kit"].session_guide_digests.is_empty());
        });
        engine.shutdown();
    }

    /// The screen language the guide names is Faber's own language setting, the one the screen shows; the system's only
    /// decides when Faber has none (or one it has no words for).
    #[tokio::test]
    async fn the_guide_names_the_language_of_fabers_own_setting_before_the_systems() {
        let dir = crate::workspace::TestRoot::new();
        let (spawn, _, _) = fake();
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            use crate::guide::Language;
            for (code, language) in
                [("en", Language::En), ("pt-BR", Language::PtBr), ("es", Language::Es), ("fr", Language::Fr), ("de", Language::De)]
            {
                set_screen(core, code, Permissions::Ask);
                assert_eq!(core.screen_language(), language, "{code}");
                let note = guide_note(core, Vendor::Claude, Area::Chat).unwrap();
                assert!(note.ends_with(&format!("The screen is shown in {}.", language.name())), "{code}: {note}");
            }
            core.store.save_settings(&Settings { language: None, ..Default::default() }).unwrap();
            assert_eq!(
                core.screen_language(),
                Language::resolve(None, &crate::guide::system_languages()),
                "with no language of Faber's own, the system's decides"
            );
        });
        engine.shutdown();
    }

    /// The guide is Faber's, not the catalog's: no id in a kit's report, nothing installed, nothing under the catalog's folder,
    /// and never in a request of Faber's own (`Isolated`).
    #[tokio::test]
    async fn faber_guide_is_no_catalog_item_and_never_reaches_an_internal_request() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, _) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            conversation(core, Area::Project, Some(&project.id), &folder, "Debug this failing test");
            let (delivery, report) = core.choose_session_tools("kit", Vendor::Claude, Permissions::FullAccess, &folder);
            let crate::guide::Guide::Plugin { dir: plugin, .. } = &delivery.guide else { panic!("{:?}", delivery.guide) };
            assert_eq!(report.ids, ["debug"], "the kit's choice is the catalog's alone");
            assert!(core.catalog_store().installed().keys().all(|id| !id.contains("guide")));
            let root = delivery.tools_root().unwrap();
            assert!(!plugin.starts_with(&root), "the guide is not kept in the catalog's store");
            for (vendor, area) in [(Vendor::Claude, Area::Chat), (Vendor::Codex, Area::Project)] {
                assert_eq!(core.guide_for(vendor, area, Permissions::Isolated), crate::guide::Guide::Off);
                assert_eq!(core.choose_session_tools("kit", vendor, Permissions::Isolated, &folder).0.guide, crate::guide::Guide::Off);
            }
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn claude_chat_receives_bounded_instructions_in_restricted_input_without_external_reads() {
        for permissions in [Permissions::Ask, Permissions::FullAccess] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                core.store.save_settings(&Settings { permissions: Some(permissions), ..Default::default() }).unwrap();
                conversation(core, Area::Chat, None, &folder, "Debug this failing test");
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Claude), "Debug this failing test".into(), vec![], true).unwrap();
                assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
            });
            let text = claude_input(&messages).await;
            let text = crate::guide::without_note(&text);
            assert!(text.contains("debug_KIT_INSTRUCTIONS"));
            assert!(!text.contains("design_KIT_INSTRUCTIONS"));
            assert!(!text.contains("SKILL.md"), "Chat never needs a catalog path outside its working folder");
            assert!(text.len() < crate::tools::kits::CONTEXT_BUDGET + "Debug this failing test".len());
            let recorded = args.lock().unwrap();
            let argv = &recorded[0].1;
            assert!(argv.windows(2).any(|a| a == ["--input-format", "stream-json"]));
            let mut files = vec![format!("Read({})", rule_path(&guide_file(&folder)))];
            files.extend(
                ["frontend-craft", "frontend-proof"]
                    .map(|name| format!("Read({})", rule_path(&folder.join(format!("guide-plugin/skills/{name}/SKILL.md"))))),
            );
            assert!(
                !argv.iter().any(|a| a == "--plugin-dir" || a == "--add-dir" || (a.starts_with("Read(") && !files.contains(a))),
                "only Faber's original instruction files are readable without a card"
            );
            drop(recorded);
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn resumed_claude_projects_receive_current_index_with_native_history_intact() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            conversation(core, Area::Project, Some(&project.id), &folder, "Design a landing page");
            core.convs.get_mut("kit").unwrap().sessions.insert(Vendor::Claude, "native-history".into());
            core.catalog_store().set_enabled("debug", false).unwrap();
            prepare(&core.catalog_store(), &folder);
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Claude), "Design a landing page".into(), vec![], true).unwrap();
        });
        let text = claude_input(&messages).await;
        assert!(text.contains("replacing earlier catalog selections and paths"));
        assert!(text.contains("faber:design;"));
        assert!(!text.contains("faber:debug;"));
        assert!(!text.contains("design_KIT_INSTRUCTIONS"), "Projects keep descriptions-only native loading");
        assert!(text.contains("native Skill tool"));
        assert!(!text.contains("; Read ") && !text.contains("SKILL.md"), "a loaded native plugin never asks for an outside Read");
        let recorded = args.lock().unwrap();
        let argv = &recorded[0].1;
        assert!(argv.windows(2).any(|a| a == ["--resume", "native-history"]));
        let plugin = argv.windows(2).find(|a| a[0] == "--plugin-dir").unwrap();
        assert!(Path::new(&plugin[1]).join("skills/design/SKILL.md").is_file());
        assert!(!argv.windows(2).filter(|a| a[0] == "--append-system-prompt").any(|a| a[1].contains("faber:design;")));
        drop(recorded);
        engine.shutdown();
    }

    #[tokio::test]
    async fn claude_native_resume_sends_resources_only_when_the_saved_kit_changes() {
        for area in [Area::Chat, Area::Project] {
            for change in ["unchanged", "ids", "snapshot", "missing-digest"] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, messages) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                let task = if change == "ids" { "Design a landing page" } else { "Debug this failure" };
                engine.with(|core| {
                    conversation(core, area, Some(&project.id), &folder, "Debug this failure");
                    let (delivery, report) = core.choose_session_tools("kit", Vendor::Claude, Permissions::FullAccess, &folder);
                    let identity = session_tool_identity(Vendor::Claude, &delivery);
                    let digest = resource_digest(claude_resources(area, &delivery).as_deref());
                    drop(delivery);
                    let c = core.convs.get_mut("kit").unwrap();
                    c.sessions.insert(Vendor::Claude, "native-history".into());
                    c.session_kits.insert(Vendor::Claude, ("native-history".into(), report.ids, identity));
                    if change != "missing-digest" {
                        c.session_resource_digests.insert(Vendor::Claude, digest);
                    }
                    c.messages.iter_mut().find(|m| m.role == Role::User).unwrap().text = task.into();
                    told_of_guide(core, Vendor::Claude, area);
                    if change == "snapshot" {
                        core.catalog_store = Some(catalog_version(&folder, "_UPDATED"));
                    }
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(Vendor::Claude), task.into(), vec![], true).unwrap();
                });
                let text = claude_input(&messages).await;
                if change == "unchanged" {
                    assert_eq!(text, task, "{area:?}: an idle native resume spends no repeated catalog prefix");
                } else {
                    assert!(
                        text.contains("replacing earlier catalog selections and paths") && text.ends_with(task),
                        "{area:?}/{change}: {text}"
                    );
                    if area == Area::Chat && change == "snapshot" {
                        assert!(text.contains("debug_KIT_INSTRUCTIONS_UPDATED"));
                    }
                    if area == Area::Project {
                        assert!(text.contains("native Skill tool") && !text.contains("SKILL.md"));
                    }
                }
                assert!(args.lock().unwrap()[0].1.windows(2).any(|a| a == ["--resume", "native-history"]), "native history is preserved");
                engine.shutdown();
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn claude_warmed_native_resume_sends_only_changed_resources() {
        for change in ["unchanged", "ids", "snapshot"] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let task = if change == "ids" { "Design a landing page" } else { "Debug the next failure too" };
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug the failure");
                let (delivery, report) = core.choose_session_tools("kit", Vendor::Claude, Permissions::FullAccess, &folder);
                let identity = session_tool_identity(Vendor::Claude, &delivery);
                let digest = resource_digest(claude_resources(Area::Chat, &delivery).as_deref());
                let c = core.convs.get_mut("kit").unwrap();
                c.working_folder = None;
                c.sessions.insert(Vendor::Claude, "native-history".into());
                c.session_kits.insert(Vendor::Claude, ("native-history".into(), report.ids, identity));
                c.session_resource_digests.insert(Vendor::Claude, digest);
                c.messages.last_mut().unwrap().status = Some(Status::Done);
                told_of_guide(core, Vendor::Claude, Area::Chat);
                core.launch("kit", &pick(Vendor::Claude)).unwrap();
                let sid = core.live["kit"].sid;
                core.handle("kit", sid, EngineEvent::Started { session_id: "native-history".into(), model: "fixture".into() });
                core.handle("kit", sid, EngineEvent::TurnStarted);
                core.close_when_idle("kit");
            });
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_secs(120)).await;
            for _ in 0..1000 {
                if engine.with(|core| !core.live.contains_key("kit")) {
                    break;
                }
                tokio::task::yield_now().await;
            }
            engine.with(|core| {
                assert!(!core.live.contains_key("kit"), "the preceding native process retired during idle");
                core.warm_launch("kit", &pick(Vendor::Claude));
                assert!(core.live["kit"].prepared && core.live["kit"].resumed);
                let c = core.convs.get_mut("kit").unwrap();
                c.messages.iter_mut().find(|m| m.role == Role::User).unwrap().text = task.into();
                if change == "snapshot" {
                    core.catalog_store = Some(catalog_version(&folder, "_UPDATED"));
                }
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Claude), task.into(), vec![], true).unwrap();
            });
            let text = claude_input(&messages).await;
            if change == "unchanged" {
                assert_eq!(text, task, "returning to a warmed native session does not repeat its saved instructions");
            } else {
                assert!(text.contains("replacing earlier catalog selections and paths") && text.ends_with(task), "{change}: {text}");
                assert!(text.contains(if change == "ids" { "design_KIT_INSTRUCTIONS" } else { "debug_KIT_INSTRUCTIONS_UPDATED" }));
            }
            let recorded = args.lock().unwrap();
            assert_eq!(recorded.len(), if change == "snapshot" { 3 } else { 2 });
            assert!(recorded.iter().all(|(_, argv)| argv.windows(2).any(|part| part == ["--resume", "native-history"])));
            drop(recorded);
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn claude_projects_and_helpers_choose_discovery_from_actual_plugin_support() {
        for helper in [false, true] {
            for plugin_dir in [false, true] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, messages) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                engine.with(|core| {
                    core.claude_flags.plugin_dir = plugin_dir;
                    conversation(core, Area::Project, Some(&project.id), &folder, "Design a landing page");
                    if helper {
                        core.launch("kit", &pick(Vendor::Codex)).unwrap();
                        core.start_helper("kit", "child".into(), json!({"title":"Design", "brief":"Design a landing page"}));
                        assert!(core.helpers.contains_key("child"));
                    } else {
                        core.rewind.captured_for_test("reply");
                        core.start("kit", "reply", &pick(Vendor::Claude), "Design a landing page".into(), vec![], true).unwrap();
                    }
                });
                let text = claude_input(&messages).await;
                assert!(text.contains("faber:design;") && text.contains("faber:debug;"), "both paths discover all enabled skills");
                assert!(!text.contains("design_KIT_INSTRUCTIONS"), "Projects keep instruction bodies lazy");
                assert_eq!(text.contains("native Skill tool"), plugin_dir, "helper={helper}, plugin_dir={plugin_dir}: {text}");
                assert_eq!(text.contains("SKILL.md"), !plugin_dir, "an older CLI must receive a usable Read path");
                if !plugin_dir {
                    assert!(text.contains("Read "));
                }
                let recorded = args.lock().unwrap();
                let (_, argv) = recorded.iter().find(|(bin, _)| bin == "claude").unwrap();
                assert_eq!(argv.iter().any(|arg| arg == "--plugin-dir"), plugin_dir);
                drop(recorded);
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn codex_projects_discover_oversize_enabled_skills_while_chat_uses_the_next_fitting_item() {
        for area in [Area::Chat, Area::Project] {
            for resumed in [false, true] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, _, messages) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                engine.with(|core| {
                    let version = "X".repeat(crate::tools::kits::CONTEXT_BUDGET + 100);
                    core.catalog_store =
                        Some(catalog_items_versions(&folder, &[("design-large", "design", &version), ("design-small", "design", "")]));
                    conversation(core, area, Some(&project.id), &folder, "Design a landing page");
                    if resumed {
                        core.convs.get_mut("kit").unwrap().sessions.insert(Vendor::Codex, "native-history".into());
                    }
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(Vendor::Codex), "Design a landing page".into(), vec![], true).unwrap();
                    assert_eq!(core.live["kit"].tool_kit.ids, ["design-small"], "the oversize item never falsely appears as selected");
                });
                let thread = until_message(&messages, if resumed { "thread/resume" } else { "thread/start" }).await;
                let text = if resumed {
                    until_message(&messages, "turn/start").await["params"]["input"][0]["text"].as_str().unwrap().to_string()
                } else {
                    thread["params"]["developerInstructions"].as_str().unwrap().to_string()
                };
                // Faber's guide line rides apart from the kit, with the instructions of a new thread or ahead of a resumed first input.
                let text = text
                    .split(
                        "

",
                    )
                    .filter(|part| !part.starts_with("Faber's own guide: "))
                    .collect::<Vec<_>>()
                    .join(
                        "

",
                    );
                assert!(text.contains("design-small_KIT_INSTRUCTIONS"));
                assert!(!text.contains("design-large_KIT_INSTRUCTIONS"));
                assert_eq!(
                    text.contains("faber:design-large;"),
                    area == Area::Project,
                    "only Projects gets the full enabled description index"
                );
                if area == Area::Chat {
                    assert!(text.len() < crate::tools::kits::CONTEXT_BUDGET + 512);
                } else {
                    assert!(text.contains("SKILL.md"), "a project can lazily read the oversize skill from its checked snapshot");
                }
                if resumed {
                    assert_eq!(thread["params"]["threadId"], "native-history");
                }
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn codex_skill_boundary_resumes_native_history_and_delivers_full_selected_instructions() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            let c = core.convs.get_mut("kit").unwrap();
            c.sessions.insert(Vendor::Codex, "old-native-thread".into());
            c.messages.insert(0, Message::new("earlier-user".into(), Role::User, "Keep the earlier requirement".into(), 1));
            c.messages.insert(1, Message::new("earlier-answer".into(), Role::Assistant, "Earlier answer retained".into(), 1));
            c.messages[1].tool_kit = Some(crate::tools::kits::Report { ids: vec!["design".into()], ..Default::default() });
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
        });
        let thread = until_message(&messages, "thread/resume").await;
        let turn = until_message(&messages, "turn/start").await;
        assert_eq!(thread["params"]["threadId"], "old-native-thread");
        assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
        let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
        assert!(sent.contains("debug_KIT_INSTRUCTIONS") && sent.ends_with("Debug this failure"), "{sent}");
        assert!(!sent.contains("Earlier answer retained") && !sent.contains("Earlier conversation"), "native history is not replayed");
        assert!(!sent.contains("design_KIT_INSTRUCTIONS"));
        assert_eq!(args.lock().unwrap().len(), 1, "new native threads share the existing app-server process");
        engine.shutdown();
    }

    #[tokio::test]
    async fn loaded_codex_project_refreshes_resources_after_switching_back_with_the_same_kit() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let effective: Messages = Arc::default();
        let (spawn, args, messages) = fake_backend(true, Some(effective.clone()));
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        let old_resources = engine.with(|core| {
            let store = core.catalog_store();
            store.set_enabled("design", false).unwrap();
            prepare(&store, &folder);
            conversation(core, Area::Project, Some(&project.id), &folder, "Debug this failure");
            let (delivery, _) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            let resources = codex_resources(Area::Project, &delivery).unwrap();
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "BEFORE_CATALOG_CHANGE".into(), vec![], true).unwrap();
            resources
        });
        input_with(&messages, "BEFORE_CATALOG_CHANGE").await;
        tokio::time::timeout(Duration::from_secs(3), async {
            while !engine.with(|core| {
                let c = &core.convs["kit"];
                c.sessions.contains_key(&Vendor::Codex)
                    && c.session_kits.contains_key(&Vendor::Codex)
                    && c.session_resource_digests.contains_key(&Vendor::Codex)
            }) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the accepted turn saves its actual native thread and kit");
        let (native, saved_kit, server) = engine.with(|core| {
            let native = core.convs["kit"].sessions[&Vendor::Codex].clone();
            let saved_kit = core.convs["kit"].session_kits[&Vendor::Codex].clone();
            let mut peer = core.convs["kit"].clone();
            peer.id = "peer".into();
            peer.sessions.clear();
            peer.session_kits.clear();
            core.convs.insert("peer".into(), peer);
            core.launch("peer", &pick(Vendor::Codex)).unwrap();
            let server = core.codex.values().next().unwrap().clone();
            core.retire("kit").unwrap().session.close();
            core.launch("kit", &pick(Vendor::Claude)).unwrap();
            (native, saved_kit, server)
        });
        let current_resources = engine.with(|core| {
            let store = core.catalog_store();
            store.set_enabled("design", true).unwrap();
            prepare(&store, &folder);
            let (delivery, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            assert_eq!(report.ids, saved_kit.1, "the unrelated skill never changes the selected kit");
            assert_eq!(delivery.server_identity(), saved_kit.2, "the native server identity is unchanged");
            let resources = codex_resources(Area::Project, &delivery).unwrap();
            core.retire("kit").unwrap().session.close();
            core.convs.get_mut("kit").unwrap().messages.push(Message::new("after".into(), Role::Assistant, String::new(), 2));
            core.rewind.captured_for_test("after");
            core.start("kit", "after", &pick(Vendor::Codex), "AFTER_CATALOG_CHANGE".into(), vec![], true).unwrap();
            assert!(core.live["kit"].resumed);
            assert!(core.live.contains_key("peer") && server.is_alive(), "the peer keeps the shared backend loaded across the switch");
            resources
        });
        input_with(&messages, "AFTER_CATALOG_CHANGE").await;
        assert_ne!(old_resources, current_resources, "the discovery index and snapshot paths changed");
        assert!(current_resources.contains("faber:design;") && !old_resources.contains("faber:design;"));
        let sent = {
            let recorded = messages.lock().unwrap();
            let resumed = recorded.iter().find(|(_, msg)| msg["method"] == "thread/resume").unwrap();
            assert_eq!(resumed.1["params"]["threadId"], native, "native history survives the switch");
            recorded.iter().rev().find(|(_, msg)| msg["method"] == "turn/start").unwrap().1["params"]["input"][0]["text"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(args.lock().unwrap().iter().filter(|(bin, _)| bin == "codex").count(), 1, "both native threads share the same backend");
        {
            let effective = effective.lock().unwrap();
            let resources = effective.iter().rev().find(|(id, _)| id == &native).unwrap().1.as_str().unwrap();
            assert!(
                resources.contains(&current_resources),
                "a loaded thread must see the current index and paths even when resume overrides are ignored: {resources}"
            );
            assert!(!resources.contains(&old_resources), "the previous resource paths must be replaced");
        }
        assert!(
            sent.contains(&current_resources) && sent.ends_with("AFTER_CATALOG_CHANGE"),
            "changed resources reach turn/start as a prefix"
        );
        let digest = resource_digest(Some(&current_resources));
        tokio::time::timeout(Duration::from_secs(3), async {
            while !engine.with(|core| core.convs["kit"].session_resource_digests.get(&Vendor::Codex) == Some(&digest)) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("accepting the resumed turn persists its delivered resource digest");
        engine.with(|core| {
            assert_eq!(core.convs["kit"].session_kits[&Vendor::Codex], saved_kit);
            core.retire("kit").unwrap().session.close();
            core.convs.get_mut("kit").unwrap().messages.push(Message::new("unchanged".into(), Role::Assistant, String::new(), 3));
            core.rewind.captured_for_test("unchanged");
            core.start("kit", "unchanged", &pick(Vendor::Codex), "UNCHANGED_CATALOG".into(), vec![], true).unwrap();
        });
        input_with(&messages, "UNCHANGED_CATALOG").await;
        let recorded = messages.lock().unwrap();
        let turn = &recorded.iter().rev().find(|(_, msg)| msg["method"] == "turn/start").unwrap().1;
        assert_eq!(
            turn["params"]["input"][0]["text"], "UNCHANGED_CATALOG",
            "a later unchanged native resume spends no repeated resource prefix"
        );
        drop(recorded);
        engine.shutdown();
    }

    #[tokio::test]
    async fn loaded_codex_thread_clears_removed_resources_once_and_keeps_native_history() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let effective: Messages = Arc::default();
        let (spawn, args, messages) = fake_backend(true, Some(effective.clone()));
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "WITH_RESOURCES".into(), vec![], true).unwrap();
        });
        input_with(&messages, "WITH_RESOURCES").await;
        tokio::time::timeout(Duration::from_secs(3), async {
            while !engine.with(|core| {
                let c = &core.convs["kit"];
                c.sessions.contains_key(&Vendor::Codex)
                    && c.session_kits.contains_key(&Vendor::Codex)
                    && c.session_resource_digests.contains_key(&Vendor::Codex)
            }) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the first accepted turn persists its native history and resources");
        let (native, old_digest, server) = engine.with(|core| {
            let c = &core.convs["kit"];
            let native = c.sessions[&Vendor::Codex].clone();
            let old_digest = c.session_resource_digests[&Vendor::Codex].clone();
            let mut peer = c.clone();
            peer.id = "peer".into();
            peer.sessions.clear();
            peer.session_kits.clear();
            peer.session_resource_digests.clear();
            core.convs.insert("peer".into(), peer);
            core.launch("peer", &pick(Vendor::Codex)).unwrap();
            let server = core.codex.values().next().unwrap().clone();
            core.retire("kit").unwrap().session.close();
            let store = core.catalog_store();
            for id in ["debug", "design"] {
                store.set_enabled(id, false).unwrap();
            }
            prepare(&store, &folder);
            core.convs.get_mut("kit").unwrap().messages.push(Message::new("empty".into(), Role::Assistant, String::new(), 2));
            core.rewind.captured_for_test("empty");
            core.start("kit", "empty", &pick(Vendor::Codex), "NO_RESOURCES".into(), vec![], true).unwrap();
            assert!(core.live["kit"].resumed && core.live["kit"].tool_kit.ids.is_empty());
            assert!(core.live.contains_key("peer") && server.is_alive(), "the native backend stays loaded through its peer");
            (native, old_digest, server)
        });
        input_with(&messages, "NO_RESOURCES").await;
        let sent =
            messages.lock().unwrap().iter().rev().find(|(_, msg)| msg["method"] == "turn/start").unwrap().1["params"]["input"][0]["text"]
                .as_str()
                .unwrap()
                .to_string();
        assert!(sent.starts_with(RESOURCE_PREFIX), "a loaded thread must receive an explicit empty resource replacement: {sent}");
        let empty = sent.strip_prefix(RESOURCE_PREFIX).unwrap().strip_suffix("\n\nNO_RESOURCES").unwrap();
        assert_eq!(empty, EMPTY_RESOURCES, "the replacement explicitly clears the old catalog instructions");
        let digest = resource_digest(Some(empty));
        assert_ne!(digest, old_digest);
        tokio::time::timeout(Duration::from_secs(3), async {
            while !engine.with(|core| core.convs["kit"].session_resource_digests.get(&Vendor::Codex) == Some(&digest)) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("accepting the empty replacement persists its exact delivered digest");
        {
            let effective = effective.lock().unwrap();
            let resources = effective.iter().rev().find(|(id, _)| id == &native).unwrap().1.as_str().unwrap();
            assert!(
                resources.contains(empty) && !resources.contains("debug_KIT_INSTRUCTIONS"),
                "the loaded thread clears earlier resources despite ignoring resume overrides"
            );
        }
        engine.with(|core| {
            assert_eq!(core.convs["kit"].sessions[&Vendor::Codex], native);
            assert_eq!(core.convs["kit"].session_kits[&Vendor::Codex].0, native);
            assert!(core.convs["kit"].session_kits[&Vendor::Codex].1.is_empty());
            core.retire("kit").unwrap().session.close();
            core.convs.get_mut("kit").unwrap().messages.push(Message::new("still-empty".into(), Role::Assistant, String::new(), 3));
            core.rewind.captured_for_test("still-empty");
            core.start("kit", "still-empty", &pick(Vendor::Codex), "STILL_NO_RESOURCES".into(), vec![], true).unwrap();
            assert!(core.live["kit"].resumed && server.is_alive());
        });
        input_with(&messages, "STILL_NO_RESOURCES").await;
        let recorded = messages.lock().unwrap();
        let turn = &recorded.iter().rev().find(|(_, msg)| msg["method"] == "turn/start").unwrap().1;
        assert_eq!(turn["params"]["input"][0]["text"], "STILL_NO_RESOURCES", "the explicit empty marker is delivered only once");
        let resumed: Vec<_> = recorded.iter().filter(|(_, msg)| msg["method"] == "thread/resume").collect();
        assert_eq!(resumed.len(), 2);
        assert!(resumed.iter().all(|(_, msg)| msg["params"]["threadId"] == native), "both resumes keep the original native history");
        drop(recorded);
        assert_eq!(args.lock().unwrap().iter().filter(|(bin, _)| bin == "codex").count(), 1, "resource clearing keeps the shared backend");
        engine.shutdown();
    }

    #[tokio::test]
    async fn codex_native_resume_sends_resources_only_when_the_saved_kit_changes() {
        for area in [Area::Chat, Area::Project] {
            for change in ["unchanged", "contents", "ids", "native-id", "missing-record", "missing-digest"] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, _, messages) = fake();
                let engine = engine(&dir, spawn);
                let project = engine.add_project(&folder.to_string_lossy()).unwrap();
                let task = if change == "ids" { "Design a landing page" } else { "Debug this failure" };
                engine.with(|core| {
                    conversation(core, area, Some(&project.id), &folder, "Debug this failure");
                    let (delivery, report) = core.catalog_store().ready_kit_for(
                        Vendor::Codex,
                        "Debug this failure",
                        Some(&folder),
                        area,
                        Permissions::FullAccess,
                        &[],
                    );
                    let (delivery, damaged, io_error) = delivery.verified_for_launch(Vendor::Codex);
                    assert!(damaged.is_empty() && !io_error, "{area:?}/{change}: the prior verified resource fixture is ready");
                    let digest = resource_digest(codex_resources(area, &delivery).as_deref());
                    let c = core.convs.get_mut("kit").unwrap();
                    c.sessions.insert(Vendor::Codex, "native-history".into());
                    if change != "missing-record" {
                        let native = if change == "native-id" { "another-thread" } else { "native-history" };
                        c.session_kits.insert(Vendor::Codex, (native.into(), report.ids, delivery.server_identity()));
                        if change != "missing-digest" {
                            c.session_resource_digests.insert(Vendor::Codex, digest);
                        }
                    }
                    c.messages[0].text = task.into();
                    let saved = serde_json::to_vec(c).unwrap();
                    core.convs.insert("kit".into(), serde_json::from_slice(&saved).unwrap());
                    told_of_guide(core, Vendor::Codex, area);
                    if change == "contents" {
                        core.catalog_store = Some(catalog_version(&folder, "_UPDATED"));
                    }
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(Vendor::Codex), task.into(), vec![], true).unwrap();
                    assert!(core.live["kit"].resumed);
                    assert_eq!(
                        core.live["kit"].tool_kit.ids,
                        [if change == "ids" { "design" } else { "debug" }],
                        "{area:?}/{change}: the current resource fixture is ready"
                    );
                });
                let thread = until_message(&messages, "thread/resume").await;
                assert_eq!(thread["params"]["threadId"], "native-history");
                let instructions = thread["params"]["developerInstructions"].as_str().unwrap();
                if area == Area::Chat {
                    assert!(
                        !instructions.contains("supporting files:"),
                        "resuming Chat retains bounded instruction bodies without file paths"
                    );
                }
                assert_eq!(instructions.contains("debug_KIT_INSTRUCTIONS_UPDATED"), change == "contents");
                assert_eq!(instructions.contains("SKILL.md"), area == Area::Project, "{area:?}/{change}: {thread}");
                let turn = until_message(&messages, "turn/start").await;
                let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
                if change == "unchanged" {
                    assert_eq!(sent, task, "{area:?}: a restored native kit spends no repeated catalog prefix");
                } else {
                    assert!(
                        sent.contains("replacing earlier catalog selections and paths") && sent.ends_with(task),
                        "{area:?}/{change}: {sent}"
                    );
                    assert!(sent.contains(match change {
                        "ids" => "design_KIT_INSTRUCTIONS",
                        "contents" => "debug_KIT_INSTRUCTIONS_UPDATED",
                        _ => "debug_KIT_INSTRUCTIONS",
                    }));
                    assert_eq!(crate::guide::without_note(sent).contains("SKILL.md"), area == Area::Project);
                }
                assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn identical_codex_kits_resume_native_history_and_compact_never_opens_an_empty_thread() {
        for compact in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "saved-native-thread".into());
                let last = c.messages.last_mut().unwrap();
                last.vendor = Some(Vendor::Codex);
                last.status = Some(Status::Done);
                last.tool_kit = Some(crate::tools::kits::Report { ids: vec!["debug".into()], ..Default::default() });
                if compact {
                    core.catalog_store().set_enabled("debug", false).unwrap();
                    core.compact_now("kit").unwrap();
                } else {
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
                }
                assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
            });
            let resumed = until_message(&messages, "thread/resume").await;
            assert_eq!(resumed["params"]["threadId"], "saved-native-thread");
            if compact {
                until_message(&messages, "thread/compact/start").await;
            } else {
                let turn = until_message(&messages, "turn/start").await;
                let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
                assert!(!sent.contains("Earlier conversation"), "native history is never replayed as text");
                assert!(sent.ends_with("Debug this failure"));
            }
            assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
            assert_eq!(args.lock().unwrap().len(), 1);
            engine.shutdown();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn code_pick_never_calls_a_model_or_delays_dispatch() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let clock = tokio::time::Instant::now();
        engine.with(|core| {
            core.codex_models.clear();
            core.warm_router();
            let spare = core.spare.as_ref().expect("Auto's Claude router is prepared").asked.clone();
            core.opts.ask = Some(Arc::new(|_| panic!("kit selection must never call a model")));
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
            assert_eq!(core.live["kit"].tool_kit.method, "code");
            assert!(Arc::ptr_eq(&core.spare.as_ref().expect("kit selection leaves Auto's spare alone").asked, &spare));
            assert!(spare.lock().unwrap().is_none());
            assert_eq!(clock.elapsed(), Duration::ZERO, "selection and queueing have no asynchronous wait");
        });
        until_message(&messages, "turn/start").await;
        assert_eq!(args.lock().unwrap().len(), 2, "the already prepared router and the conversation only");
        engine.shutdown();
    }

    #[tokio::test]
    async fn cold_snapshot_preparation_never_holds_the_engine_lock_or_delays_a_turn() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let catalog = engine.with(|core| {
            core.store.save_settings(&Settings { permissions: Some(Permissions::Plan), ..Default::default() }).unwrap();
            // The worker below holds the store lock; the session's own preparation must stay detached.
            core.synchronous_kit_checks = false;
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            core.catalog_store().clear_prepared_for_test();
            core.catalog_store()
        });
        let (held, gate_held) = std::sync::mpsc::channel();
        let (release, released) = std::sync::mpsc::channel();
        let gate = std::thread::spawn(move || {
            let _guard = crate::tools::lock();
            held.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(2));
        });
        gate_held.recv_timeout(Duration::from_secs(2)).unwrap();
        let (entered, worker_entered) = std::sync::mpsc::channel();
        let root = folder.clone();
        let prepared = catalog.clone();
        let worker = tokio::task::spawn_blocking(move || {
            entered.send(()).unwrap();
            catalog.prepare_kit_for(Vendor::Codex, "Debug this failure", Some(&root), Area::Chat, Permissions::Plan, &[]);
        });
        worker_entered.recv_timeout(Duration::from_secs(2)).unwrap();
        let started = std::time::Instant::now();
        engine.with(|core| {
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
            assert!(core.live["kit"].tool_kit.ids.is_empty(), "an unready scope starts without waiting for its snapshot");
        });
        assert!(started.elapsed() < Duration::from_secs(1), "synchronous snapshot I/O must not delay dispatch");
        engine.with(|core| core.tick += 1);
        until_message(&messages, "turn/start").await;
        assert!(!worker.is_finished(), "the turn reaches the CLI while snapshot disk preparation remains blocked");
        release.send(()).unwrap();
        gate.join().unwrap();
        worker.await.unwrap();
        prepared.wait_prepared_for_test();
        engine.with(|core| {
            assert!(core.live["kit"].tool_kit.ids.is_empty(), "readiness never mutates the running session");
            core.synchronous_kit_checks = true;
            core.retire("kit").unwrap().session.close();
            core.launch("kit", &pick(Vendor::Codex)).unwrap();
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"], "the prepared kit joins only the next session");
        });
        engine.shutdown();
    }

    #[tokio::test]
    #[ignore = "manual release-build fake-CLI first-word measurement"]
    async fn release_first_word_with_a_blank_warmed_claude_catalog_session() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, _) = fake_reply(true);
        let engine = engine(&dir, spawn);
        let store = if let Some(root) = std::env::var_os("FABER_KIT_BENCH_ROOT") {
            ToolStore::new(Path::new(&root))
        } else {
            engine.with(|core| core.catalog_store())
        };
        store.prepare_kit_for(Vendor::Claude, "", None, Area::Chat, Permissions::Ask, &[]);
        store.wait_prepared_for_test();
        engine.with(|core| {
            core.catalog_store = Some(store.clone());
            conversation(core, Area::Chat, None, &folder, "");
            for vendor in [Vendor::Claude, Vendor::Codex] {
                let checked = std::time::Instant::now();
                let (delivery, report) = core.session_tools(
                    vendor,
                    Area::Project,
                    Permissions::FullAccess,
                    None,
                    "Design a website and measure devtools performance",
                    &folder,
                );
                let (delivery, damaged, io_failed) = delivery.verified_for_launch(vendor);
                assert!(damaged.is_empty() && !io_failed);
                assert!(report.error.is_none());
                eprintln!(
                    "release full-folder launch check {vendor:?}/Project: {} us; {} selected ids; {} servers",
                    checked.elapsed().as_micros(),
                    report.ids.len(),
                    delivery.servers.len()
                );
            }
            let c = core.convs.get_mut("kit").unwrap();
            c.messages.clear();
            c.working_folder = None;
            core.warm_launch("kit", &pick(Vendor::Claude));
        });
        let started = std::time::Instant::now();
        engine.with(|core| {
            core.convs.get_mut("kit").unwrap().messages.extend([
                Message::new("sent".into(), Role::User, "Debug this failing test".into(), 1),
                Message::new("answer".into(), Role::Assistant, String::new(), 2),
            ]);
            core.rewind.captured_for_test("answer");
            core.start("kit", "answer", &pick(Vendor::Claude), "Debug this failing test".into(), vec![], true).unwrap();
        });
        let dispatch = started.elapsed();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !engine.with(|core| core.convs["kit"].messages.last().unwrap().text.contains("first word")) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(args.lock().unwrap().len(), 1, "the blank warmed process must serve the sent task");
        eprintln!(
            "release warmed Claude fake CLI ({} installed items): dispatch={} us, first_word={} us, 1 reused process",
            store.installed().len(),
            dispatch.as_micros(),
            started.elapsed().as_micros()
        );
        engine.shutdown();
    }

    #[tokio::test]
    #[ignore = "manual release-build fake-CLI first-word measurement"]
    async fn release_first_word_with_ready_and_unready_catalog_kits() {
        for ready in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, _) = fake_reply(true);
            let engine = engine(&dir, spawn);
            let store = if let Some(root) = std::env::var_os("FABER_KIT_BENCH_ROOT") {
                ToolStore::new(Path::new(&root))
            } else {
                engine.with(|core| core.catalog_store())
            };
            store.prepare_kit_for(Vendor::Codex, "Debug this failure", Some(&folder), Area::Chat, Permissions::FullAccess, &[]);
            store.wait_prepared_for_test();
            let installed = store.installed().len();
            engine.with(|core| {
                core.catalog_store = Some(store);
                if !ready {
                    core.store.save_settings(&Settings { permissions: Some(Permissions::Plan), ..Default::default() }).unwrap();
                    core.catalog_store().clear_prepared_for_test();
                }
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                core.rewind.captured_for_test("reply");
            });
            let started = std::time::Instant::now();
            engine.with(|core| {
                core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
                assert_eq!(core.live["kit"].tool_kit.ids.is_empty(), !ready);
            });
            let dispatch = started.elapsed();
            tokio::time::timeout(Duration::from_secs(5), async {
                while !engine.with(|core| core.convs["kit"].messages.last().unwrap().text.contains("first word")) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            eprintln!(
                "release fake CLI ({installed} installed catalog items), ready={ready}: dispatch={} us, first_word={} us, 0 model wait, 0 picker processes",
                dispatch.as_micros(),
                started.elapsed().as_micros()
            );
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn terminal_and_pre_update_threads_without_a_kit_record_resume_an_empty_pick() {
        for old_history in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Go on");
                core.catalog_store().set_enabled("debug", false).unwrap();
                core.catalog_store().set_enabled("design", false).unwrap();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "external-native-thread".into());
                if old_history {
                    let mut old = Message::new("old".into(), Role::Assistant, "Saved before kits".into(), 0);
                    old.vendor = Some(Vendor::Codex);
                    c.messages.insert(0, old);
                }
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Codex), "Go on".into(), vec![], true).unwrap();
                assert!(core.live["kit"].resumed);
            });
            let resumed = until_message(&messages, "thread/resume").await;
            assert_eq!(resumed["params"]["threadId"], "external-native-thread");
            let turn = until_message(&messages, "turn/start").await;
            // Nothing records what it was told, so it is told of Faber's guide once, ahead of the person's words.
            let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
            assert!(sent.starts_with("Faber's own guide: ") && crate::guide::without_note(sent) == "Go on", "{sent}");
            assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn picked_up_node_terminal_with_ponytail_resumes_and_receives_full_skill_instructions() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        std::fs::write(folder.join("package.json"), "{}").unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            core.catalog_store = Some(catalog_items(&folder, "", &[("ponytail", "lean")]));
            let id = core
                .pick_up(
                    &project.id,
                    CliSession {
                        vendor: Vendor::Codex,
                        id: "terminal-thread".into(),
                        title: "Terminal".into(),
                        model: None,
                        updated_at: 1,
                    },
                )
                .unwrap();
            let mut c = core.convs.remove(&id).unwrap();
            c.id = "kit".into();
            assert!(c.session_kits.is_empty(), "terminal pickup has no Faber kit metadata");
            c.messages.push(Message::new("user".into(), Role::User, "Go on".into(), 1));
            c.messages.push(Message::new("reply".into(), Role::Assistant, String::new(), 1));
            core.convs.insert("kit".into(), c);
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Go on".into(), vec![], true).unwrap();
            assert!(core.live["kit"].resumed);
            assert_eq!(core.live["kit"].tool_kit.ids, ["ponytail"]);
        });
        assert_eq!(until_message(&messages, "thread/resume").await["params"]["threadId"], "terminal-thread");
        let turn = until_message(&messages, "turn/start").await;
        let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
        assert!(sent.contains("ponytail_KIT_INSTRUCTIONS") && sent.ends_with("Go on"));
        assert!(!sent.contains("Earlier conversation") && !sent.contains("history"));
        assert!(!messages.lock().unwrap().iter().any(|(_, m)| m["method"] == "thread/start"));
        engine.shutdown();
    }

    #[tokio::test]
    async fn unrelated_catalog_switch_and_new_skill_pick_preserve_the_native_thread() {
        for (change_task, remove) in [(false, false), (false, true), (true, false)] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                let (delivery, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "keep-native-thread".into());
                c.session_kits.insert(Vendor::Codex, ("keep-native-thread".into(), report.ids, delivery.server_identity()));
                c.session_resource_digests.insert(Vendor::Codex, resource_digest(codex_resources(Area::Chat, &delivery).as_deref()));
                if change_task {
                    c.messages[0].text = "Design a layout".into();
                } else if remove {
                    core.catalog_store().remove("design").unwrap();
                } else {
                    core.catalog_store().set_enabled("design", false).unwrap();
                }
                prepare(&core.catalog_store(), &folder);
                core.rewind.captured_for_test("reply");
                let request = if change_task { "Design a layout" } else { "Debug this failure" };
                core.start("kit", "reply", &pick(Vendor::Codex), request.into(), vec![], true).unwrap();
                assert!(core.live["kit"].resumed);
                assert_eq!(core.live["kit"].tool_kit.ids, [if change_task { "design" } else { "debug" }]);
            });
            let resumed = until_message(&messages, "thread/resume").await;
            assert_eq!(resumed["params"]["threadId"], "keep-native-thread");
            let turn = until_message(&messages, "turn/start").await;
            let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
            assert_eq!(sent.contains("design_KIT_INSTRUCTIONS"), change_task);
            assert!(!sent.contains("debug_KIT_INSTRUCTIONS"), "unchanged Chat instruction text needs no repeated prefix");
            assert_eq!(sent.contains("replacing earlier catalog selections and paths"), change_task);
            assert_eq!(resumed["params"]["developerInstructions"].as_str().unwrap().contains("debug_KIT_INSTRUCTIONS"), !change_task);
            assert!(!sent.contains("Earlier conversation"));
            assert!(!messages.lock().unwrap().iter().any(|(_, m)| m["method"] == "thread/start"));
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn restarting_with_an_unready_or_unmatched_server_kit_preserves_native_context_and_its_record() {
        for cold in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake_reply(true);
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            let identity = engine.with(|core| {
                core.catalog_store = Some(server_catalog(&folder, "before"));
                conversation(core, Area::Project, Some(&project.id), &folder, "Measure lighthouse performance");
                let (tools, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
                let identity = tools.server_identity().unwrap();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "browser-native-thread".into());
                let mut ids = report.ids;
                ids.push("retired-skill".into());
                c.session_kits.insert(Vendor::Codex, ("browser-native-thread".into(), ids, Some(identity.clone())));
                c.messages[0].text = "Go on".into();
                if cold {
                    core.catalog_store().clear_prepared_for_test();
                }
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Codex), "Go on".into(), vec![], true).unwrap();
                assert!(core.live["kit"].resumed, "readiness and an unmatched continuation never discard native context");
                assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&identity));
                assert!(core.live["kit"].tool_kit.ids.is_empty(), "the report never claims old tools matched this task");
                identity
            });
            assert_eq!(until_message(&messages, "thread/resume").await["params"]["threadId"], "browser-native-thread");
            until_message(&messages, "turn/start").await;
            tokio::time::timeout(Duration::from_secs(2), async {
                while !engine.with(|core| core.convs["kit"].messages.last().unwrap().text.contains("first word")) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("accepted native turns persist the effective server identity");
            let store = engine.with(|core| core.catalog_store());
            store.prepare_kit_for(Vendor::Codex, "Go on", Some(&folder), Area::Project, Permissions::FullAccess, &[]);
            store.wait_prepared_for_test();
            engine.with(|core| {
                let (_, ids, saved_identity) = &core.convs["kit"].session_kits[&Vendor::Codex];
                assert_eq!(ids, &["browser"], "the native record keeps server ids separately from this task's visible report");
                assert_eq!(saved_identity.as_ref(), Some(&identity));
                core.retire("kit").unwrap().session.close();
                core.launch("kit", &pick(Vendor::Codex)).unwrap();
                assert!(core.live["kit"].resumed, "the next ready miss cannot flip the identity kept during a cold start");
                assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&identity));
                core.retire("kit").unwrap().session.close();
                core.catalog_store().set_enabled("browser", false).unwrap();
            });
            store.prepare_kit_for(Vendor::Codex, "Go on", Some(&folder), Area::Project, Permissions::FullAccess, &[]);
            store.wait_prepared_for_test();
            engine.with(|core| {
                core.launch("kit", &pick(Vendor::Codex)).unwrap();
                assert!(!core.live["kit"].resumed, "a person's explicit server switch still revokes the old configuration");
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn a_resumed_continuation_refreshes_server_paths_after_a_skill_snapshot_changes() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        let (old_script, identity, store) = engine.with(|core| {
            core.catalog_store = Some(server_catalog(&folder, "before"));
            conversation(core, Area::Project, Some(&project.id), &folder, "Go on");
            let store = core.catalog_store();
            let (tools, report) = store.ready_kit_for(
                Vendor::Codex,
                "Measure lighthouse performance",
                Some(&folder),
                Area::Project,
                Permissions::FullAccess,
                &[],
            );
            let identity = tools.server_identity().unwrap();
            let c = core.convs.get_mut("kit").unwrap();
            c.sessions.insert(Vendor::Codex, "unchanged-browser-thread".into());
            c.session_kits.insert(Vendor::Codex, ("unchanged-browser-thread".into(), report.ids, Some(identity.clone())));
            store.set_enabled("debug", false).unwrap();
            (tools.servers[0].args[0].clone(), identity, store)
        });
        store.prepare_kit_for(Vendor::Codex, "Go on", Some(&folder), Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        engine.with(|core| {
            core.launch("kit", &pick(Vendor::Codex)).unwrap();
            assert!(core.live["kit"].resumed, "a skill switch changes the snapshot, never the native server configuration");
            assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&identity));
            assert!(core.live["kit"].tool_kit.ids.is_empty());
        });
        let resumed = until_message(&messages, "thread/resume").await;
        let script = resumed["params"]["config"]["mcp_servers.faber-browser"]["args"][0].as_str().unwrap();
        assert_ne!(script, old_script, "the native resume replaces the old snapshot path");
        assert!(Path::new(script).is_file(), "the refreshed server script exists in the current verified snapshot");
        assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
        engine.shutdown();
    }

    #[tokio::test]
    async fn explicitly_revoked_servers_never_resume_while_the_replacement_snapshot_is_pending() {
        for action in ["disable", "remove", "project", "catalog"] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            engine.with(|core| {
                core.catalog_store = Some(server_catalog(&folder, "before"));
                conversation(core, Area::Project, Some(&project.id), &folder, "Go on");
                let (tools, report) = core.catalog_store().ready_kit_for(
                    Vendor::Codex,
                    "Measure lighthouse performance",
                    Some(&folder),
                    Area::Project,
                    Permissions::FullAccess,
                    &[],
                );
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "revoked-native-thread".into());
                c.session_kits.insert(Vendor::Codex, ("revoked-native-thread".into(), report.ids, tools.server_identity()));
                match action {
                    "disable" => core.catalog_store().set_enabled("browser", false).unwrap(),
                    "remove" => core.catalog_store().remove("browser").unwrap(),
                    "catalog" => core.catalog_store = Some(ToolStore::with_catalog(&folder, vec![])),
                    _ => {
                        let mut projects = core.store.projects();
                        projects.iter_mut().find(|p| p.id == project.id).unwrap().disabled_mcp_servers.insert("faber-browser".into());
                        core.store.save_projects(&projects).unwrap();
                    }
                }
                core.catalog_store = Some(core.catalog_store().with_node("C:/unprepared-node.exe".into()));
                let disabled = core.project_disabled_servers(Some(&project.id));
                assert!(
                    !core
                        .catalog_store()
                        .ready_kit_for(Vendor::Codex, "Go on", Some(&folder), Area::Project, Permissions::FullAccess, &disabled)
                        .0
                        .catalog_ready
                );
                core.launch("kit", &pick(Vendor::Codex)).unwrap();
                assert!(!core.live["kit"].resumed, "{action} must revoke a known server before any background preparation finishes");
            });
            until_message(&messages, "thread/start").await;
            assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/resume"));
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn only_a_changed_selected_server_replaces_the_native_thread() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake_reply(true);
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        engine.with(|core| {
            core.catalog_store = Some(server_catalog(&folder, "before"));
            conversation(core, Area::Project, Some(&project.id), &folder, "Measure lighthouse performance");
            let (tools, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            assert_eq!(report.ids, ["browser"]);
            let c = core.convs.get_mut("kit").unwrap();
            c.sessions.insert(Vendor::Codex, "old-server-thread".into());
            c.session_kits.insert(Vendor::Codex, ("old-server-thread".into(), report.ids, tools.server_identity()));
            c.messages.insert(0, Message::new("earlier".into(), Role::User, "Keep this requirement".into(), 1));
            c.messages.iter_mut().rev().find(|m| m.role == Role::User).unwrap().text = "Go on".into();
            core.catalog_store = Some(server_catalog(&folder, "after"));
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "Go on".into(), vec![], true).unwrap();
            assert!(!core.live["kit"].resumed);
        });
        until_message(&messages, "thread/start").await;
        let turn = until_message(&messages, "turn/start").await;
        let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
        assert!(sent.contains("Keep this requirement"), "a genuine server reset carries the saved history");
        assert!(!messages.lock().unwrap().iter().any(|(_, m)| m["method"] == "thread/resume"));
        tokio::time::timeout(Duration::from_secs(2), async {
            while !engine.with(|core| core.convs["kit"].messages.last().unwrap().text.contains("first word")) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        engine.with(|core| {
            assert_eq!(
                core.convs["kit"].session_kits[&Vendor::Codex].1,
                ["browser"],
                "restored effective servers are recorded even after a real reset"
            );
            core.retire("kit").unwrap().session.close();
            core.launch("kit", &pick(Vendor::Codex)).unwrap();
            assert!(core.live["kit"].resumed, "a trusted server update resets once, never on every continuation");
        });
        until_message(&messages, "thread/resume").await;
        engine.shutdown();
    }

    #[tokio::test]
    async fn saved_empty_native_kit_overrides_old_reports_and_never_applies_to_another_thread() {
        for unknown_native_id in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                core.catalog_store().set_enabled("debug", false).unwrap();
                core.catalog_store().set_enabled("design", false).unwrap();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, if unknown_native_id { "another-native-thread" } else { "old-native-thread" }.into());
                c.session_kits.insert(Vendor::Codex, ("old-native-thread".into(), vec![], None));
                let last = c.messages.last_mut().unwrap();
                last.vendor = Some(Vendor::Codex);
                last.status = Some(Status::Done);
                last.tool_kit = Some(crate::tools::kits::Report { ids: vec!["debug".into()], ..Default::default() });
                let saved = serde_json::to_vec(c).unwrap();
                let loaded: Conversation = serde_json::from_slice(&saved).unwrap();
                assert_eq!(loaded.session_kits[&Vendor::Codex], ("old-native-thread".into(), vec![], None));
                core.convs.insert("kit".into(), loaded);
                assert!(
                    !core.launch("kit", &pick(Vendor::Codex)).unwrap(),
                    "an unknown native kit is empty too; neither an empty saved kit nor a historical report replaces native context"
                );
                assert!(core.live["kit"].tool_kit.ids.is_empty());
            });
            {
                let resumed = until_message(&messages, "thread/resume").await;
                assert_eq!(resumed["params"]["threadId"], if unknown_native_id { "another-native-thread" } else { "old-native-thread" });
                assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
                engine.with(|core| {
                    core.retire("kit").unwrap().session.close();
                    core.catalog_store().set_enabled("debug", true).unwrap();
                    prepare(&core.catalog_store(), &folder);
                    assert!(!core.launch("kit", &pick(Vendor::Codex)).unwrap(), "adding skills preserves native context");
                    assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
                });
                assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
            }
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn codex_native_context_survives_trusted_skill_updates() {
        for changed in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let old_identity = engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                let (delivery, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
                let identity = delivery.server_identity().expect("verified kit has a server identity");
                drop(delivery);
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "saved-native-thread".into());
                c.session_kits.insert(Vendor::Codex, ("saved-native-thread".into(), report.ids, Some(identity.clone())));
                let (delivery, _) = core.checked_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder).unwrap();
                let digest = resource_digest(codex_resources(Area::Chat, &delivery).as_deref());
                core.convs.get_mut("kit").unwrap().session_resource_digests.insert(Vendor::Codex, digest);
                told_of_guide(core, Vendor::Codex, Area::Chat);
                if changed {
                    core.catalog_store = Some(catalog_version(&folder, "_UPDATED"));
                }
                assert!(!core.launch("kit", &pick(Vendor::Codex)).unwrap(), "skill content updates preserve native context");
                assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
                assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&identity));
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Codex), "Debug this failure".into(), vec![], true).unwrap();
                identity
            });
            let resumed = until_message(&messages, "thread/resume").await;
            assert_eq!(resumed["params"]["developerInstructions"].as_str().unwrap().contains("debug_KIT_INSTRUCTIONS_UPDATED"), changed);
            let turn = until_message(&messages, "turn/start").await;
            let sent = turn["params"]["input"][0]["text"].as_str().unwrap();
            if changed {
                assert!(
                    sent.starts_with(RESOURCE_PREFIX)
                        && sent.contains("debug_KIT_INSTRUCTIONS_UPDATED")
                        && sent.ends_with("Debug this failure")
                );
            } else {
                assert_eq!(sent, "Debug this failure", "unchanged resources need no repeated prefix");
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    if engine.with(|core| core.convs["kit"].session_kits[&Vendor::Codex].0 != "saved-native-thread") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            })
            .await
            .expect("native turn acceptance must persist its catalog identity");
            engine.with(|core| {
                let (_, ids, saved_identity) = &core.convs["kit"].session_kits[&Vendor::Codex];
                assert_eq!(ids, &["debug"]);
                assert_eq!(saved_identity, &core.live["kit"].kit_identity, "accepted native turns persist the verified kit identity");
                assert_eq!(saved_identity.as_ref(), Some(&old_identity));
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn compacted_native_kit_does_not_reuse_old_contents_after_a_trusted_update() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug this failure");
            let (delivery, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            let old_identity = delivery.server_identity().unwrap();
            let digest = resource_digest(codex_resources(Area::Chat, &delivery).as_deref());
            drop(delivery);
            let c = core.convs.get_mut("kit").unwrap();
            c.sessions.insert(Vendor::Codex, "native-before-update".into());
            c.session_kits.insert(Vendor::Codex, ("native-before-update".into(), report.ids, Some(old_identity.clone())));
            c.session_resource_digests.insert(Vendor::Codex, digest.clone());
            core.catalog_store = Some(catalog_version(&folder, "_UPDATED"));
            assert!(
                !core.launch_session("kit", &pick(Vendor::Codex), SessionStart::Compact).unwrap(),
                "compaction resumes the native context even after its catalog files changed"
            );
            let compact_sid = core.live["kit"].sid;
            assert!(core.live["kit"].resumed && core.live["kit"].prepared);
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
            assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&old_identity));
            core.handle("kit", compact_sid, EngineEvent::Started { session_id: "native-before-update".into(), model: "fixture".into() });
            assert_eq!(
                core.convs["kit"].session_resource_digests[&Vendor::Codex],
                digest,
                "compaction preserves the delivered resource digest"
            );
            assert!(!core.launch_session("kit", &pick(Vendor::Codex), SessionStart::Turn).unwrap());
            assert_ne!(core.live["kit"].sid, compact_sid);
            assert!(core.live["kit"].resumed);
            assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
            assert_eq!(core.live["kit"].kit_identity.as_ref(), Some(&old_identity));
            assert!(
                core.live["kit"].kit_resources.as_deref().unwrap().contains("debug_KIT_INSTRUCTIONS_UPDATED"),
                "compaction never marks the updated resources as already delivered"
            );
        });
        let resumed = until_message(&messages, "thread/resume").await;
        assert!(resumed["params"]["developerInstructions"].as_str().unwrap().contains("debug_KIT_INSTRUCTIONS_UPDATED"));
        engine.shutdown();
    }

    #[tokio::test]
    async fn claude_blank_or_previous_task_warmup_reuses_the_process_and_reports_the_sent_task() {
        for earlier in [None, Some("Design a landing page")] {
            for task in ["Debug this failing test", "Meu app trava ao abrir"] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, messages) = fake();
                let engine = engine(&dir, spawn);
                let sid = engine.with(|core| {
                    conversation(core, Area::Chat, None, &folder, earlier.unwrap_or_default());
                    let c = core.convs.get_mut("kit").unwrap();
                    c.working_folder = None;
                    if earlier.is_none() {
                        c.messages.clear();
                    }
                    core.warm_launch("kit", &pick(Vendor::Claude));
                    assert!(core.live["kit"].prepared);
                    let expected = if earlier.is_some() { vec!["design".to_string()] } else { Vec::new() };
                    assert_eq!(core.live["kit"].tool_kit.ids, expected);
                    core.live["kit"].sid
                });
                engine.with(|core| {
                    core.convs.get_mut("kit").unwrap().messages.push(Message::new("sent".into(), Role::User, task.into(), 1));
                    core.convs.get_mut("kit").unwrap().messages.push(Message::new("answer".into(), Role::Assistant, String::new(), 2));
                    core.rewind.captured_for_test("answer");
                    core.start("kit", "answer", &pick(Vendor::Claude), task.into(), vec![], true).unwrap();
                    assert_eq!(core.live["kit"].sid, sid, "task-picked skill ids cannot discard a warmed process");
                    assert_eq!(core.live["kit"].tool_kit.ids, ["debug"]);
                    assert_eq!(core.convs["kit"].messages.last().unwrap().tool_kit.as_ref().unwrap().ids, ["debug"]);
                });
                assert_eq!(args.lock().unwrap().len(), 1, "Send creates no second process");
                let text = claude_input(&messages).await;
                assert!(text.contains("debug_KIT_INSTRUCTIONS"));
                assert!(!text.contains("design_KIT_INSTRUCTIONS"), "warm-up must deliver the sent task's instructions");
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn a_known_corrupt_server_revokes_native_configuration_even_while_preparation_is_unavailable() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, _) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        let store = server_catalog(&folder, "one");
        engine.with(|core| {
            core.catalog_store = Some(store.clone());
            conversation(core, Area::Project, Some(&project.id), &folder, "Measure lighthouse performance");
            let (tools, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
            let c = core.convs.get_mut("kit").unwrap();
            c.sessions.insert(Vendor::Codex, "old-browser-thread".into());
            c.session_kits.insert(Vendor::Codex, ("old-browser-thread".into(), report.ids, tools.server_identity()));
            c.messages[0].text = "Go on".into();
        });
        std::fs::write(folder.join("tools/servers/browser/server.js"), "corrupted server").unwrap();
        store.set_enabled("debug", false).unwrap();
        store.prepare_kit_for(Vendor::Codex, "Measure performance", Some(&folder), Area::Project, Permissions::FullAccess, &[]);
        store.wait_prepared_for_test();
        assert_eq!(store.checked_statuses().get("browser").map(String::as_str), Some("error.tools.reinstall"));
        store.clear_prepared_for_test();
        engine.with(|core| {
            core.launch("kit", &pick(Vendor::Codex)).unwrap();
            assert!(!core.live["kit"].resumed, "a known invalid server is a revocation, not a harmless readiness miss");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_changed_retained_server_cannot_reenter_a_resumed_native_thread() {
        for damage in ["source", "snapshot", "unreadable"] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, _) = fake();
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            engine.with(|core| {
                core.catalog_store = Some(server_catalog(&folder, "one"));
                conversation(core, Area::Project, Some(&project.id), &folder, "Measure lighthouse performance");
                let (tools, report) = core.choose_session_tools("kit", Vendor::Codex, Permissions::FullAccess, &folder);
                let script = tools.servers[0].args[0].clone();
                let c = core.convs.get_mut("kit").unwrap();
                c.sessions.insert(Vendor::Codex, "saved-browser-thread".into());
                c.session_kits.insert(Vendor::Codex, ("saved-browser-thread".into(), report.ids, tools.server_identity()));
                c.messages[0].text = "Go on".into();
                if damage == "snapshot" {
                    std::fs::remove_file(&script).unwrap();
                }
                if damage == "unreadable" {
                    std::fs::rename(Path::new(&script).parent().unwrap(), folder.join("hidden-server")).unwrap();
                } else {
                    std::fs::write(script, "changed server code").unwrap();
                }
                core.launch("kit", &pick(Vendor::Codex)).unwrap();
                assert!(!core.live["kit"].resumed, "a retained server discovered damaged at this launch revokes the saved configuration");
                assert_eq!(core.live["kit"].kit_identity.as_deref(), Some("servers:none"));
                let expected = if damage == "source" { "error.tools.reinstall" } else { "error.tools.snapshot" };
                assert_eq!(core.live["kit"].tool_kit.error.as_deref(), Some(expected));
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn claude_warmup_still_restarts_for_an_actual_server_configuration_change() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, _) = fake();
        let engine = engine(&dir, spawn);
        engine.with(|core| {
            core.catalog_store = Some(server_catalog_scope(&folder, "one", true));
            conversation(core, Area::Chat, None, &folder, "");
            let c = core.convs.get_mut("kit").unwrap();
            c.working_folder = None;
            c.messages.clear();
            core.warm_launch("kit", &pick(Vendor::Claude));
            let warm_sid = core.live["kit"].sid;
            core.convs.get_mut("kit").unwrap().messages.push(Message::new(
                "sent".into(),
                Role::User,
                "Measure lighthouse performance".into(),
                1,
            ));
            core.launch("kit", &pick(Vendor::Claude)).unwrap();
            assert_ne!(core.live["kit"].sid, warm_sid, "a prepared process without a requested server cannot supply it");
            assert_eq!(core.live["kit"].tool_kit.ids, ["browser"]);
        });
        assert_eq!(args.lock().unwrap().len(), 2);
        engine.shutdown();
    }

    #[tokio::test]
    async fn installed_catalog_items_do_not_disable_warm_process_preparation() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for (off, guarded) in [(false, false), (true, false), (false, true)] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, args, messages) = fake();
                let engine = engine(&dir, spawn);
                engine.with(|core| {
                    conversation(core, Area::Chat, None, &folder, "Debug this failure");
                    core.convs.get_mut("kit").unwrap().working_folder = None;
                    if off {
                        core.catalog_store().set_enabled("debug", false).unwrap();
                        core.catalog_store().set_enabled("design", false).unwrap();
                    }
                    if guarded {
                        core.store.save_settings(&Settings { permissions: Some(Permissions::Ask), ..Default::default() }).unwrap();
                    }
                    core.warm_launch("kit", &pick(vendor));
                    if vendor == Vendor::Claude {
                        assert!(core.live["kit"].prepared);
                    } else {
                        assert!(!core.codex.is_empty());
                    }
                });
                for _ in 0..1000 {
                    if !args.lock().unwrap().is_empty() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
                assert_eq!(args.lock().unwrap().len(), 1, "a catalog install never suppresses preparation");
                engine.with(|core| {
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(vendor), "Debug this failure".into(), vec![], true).unwrap();
                    assert_eq!(args.lock().unwrap().len(), 1, "sending the warm kit adds no cold process");
                });
                if vendor == Vendor::Codex {
                    until_message(&messages, "turn/start").await;
                }
                assert_eq!(args.lock().unwrap().len(), 1);
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn kit_reports_appear_only_at_nonempty_session_boundaries() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            for empty in [false, true] {
                let dir = crate::workspace::TestRoot::new();
                let folder = dir.path().canonicalize().unwrap();
                let (spawn, _, _) = fake();
                let engine = engine(&dir, spawn);
                engine.with(|core| {
                    if empty {
                        core.catalog_store().set_enabled("debug", false).unwrap();
                        core.catalog_store().set_enabled("design", false).unwrap();
                    }
                    conversation(core, Area::Chat, None, &folder, "Debug this failure");
                    core.rewind.captured_for_test("reply");
                    core.start("kit", "reply", &pick(vendor), "Debug this failure".into(), vec![], true).unwrap();
                    assert_eq!(core.convs["kit"].messages.last().unwrap().tool_kit.is_some(), !empty);
                    core.convs.get_mut("kit").unwrap().messages.push(Message::new("second".into(), Role::Assistant, String::new(), 2));
                    core.rewind.captured_for_test("second");
                    core.start("kit", "second", &pick(vendor), "Again".into(), vec![], true).unwrap();
                    assert!(core.convs["kit"].messages.last().unwrap().tool_kit.is_none(), "ordinary turns never repeat a kit report");
                });
                engine.shutdown();
            }
        }
    }

    #[tokio::test]
    async fn idle_reopen_does_not_repeat_an_unchanged_conversations_kit_report() {
        for vendor in [Vendor::Claude, Vendor::Codex] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, _) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(vendor), "Debug this failure".into(), vec![], true).unwrap();
                assert!(core.convs["kit"].messages.last().unwrap().tool_kit.is_some());
                core.retire("kit").unwrap().session.close();
                core.convs.get_mut("kit").unwrap().messages.push(Message::new("second".into(), Role::Assistant, String::new(), 2));
                core.rewind.captured_for_test("second");
                core.start("kit", "second", &pick(vendor), "Again".into(), vec![], true).unwrap();
                assert!(core.convs["kit"].messages.last().unwrap().tool_kit.is_none());
            });
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn abandoned_codex_preparation_closes_on_switch_or_cleanup_without_stopping_a_peer() {
        for (switch, peer) in [(true, false), (false, false), (true, true), (false, true)] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            let server = engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug this failure");
                core.convs.get_mut("kit").unwrap().working_folder = None;
                core.warm_launch("kit", &pick(Vendor::Codex));
                let server = core.codex.values().next().unwrap().clone();
                if peer {
                    let mut other = core.convs["kit"].clone();
                    other.id = "peer".into();
                    core.convs.insert("peer".into(), other);
                    core.launch("peer", &pick(Vendor::Codex)).unwrap();
                }
                if switch {
                    core.warm_launch("kit", &pick(Vendor::Claude));
                } else {
                    assert!(core.retire("kit").is_none());
                }
                assert!(!core.codex_prepared.contains_key("kit"));
                server
            });
            if peer {
                until_message(&messages, "thread/start").await;
                assert!(server.is_alive(), "abandoning a prepared conversation preserves another native thread");
                engine.with(|core| assert!(core.live.contains_key("peer")));
            } else {
                tokio::time::timeout(Duration::from_secs(2), async {
                    while server.is_alive() {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                    }
                })
                .await
                .expect("an unused prepared process must stop");
            }
            engine.shutdown();
        }
    }

    #[tokio::test(start_paused = true)]
    async fn unused_codex_preparation_expires_without_a_native_thread() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, _, messages) = fake();
        let engine = engine(&dir, spawn);
        let server = engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Go on");
            core.convs.get_mut("kit").unwrap().working_folder = None;
            core.warm_launch("kit", &pick(Vendor::Codex));
            core.codex.values().next().unwrap().clone()
        });
        until_message(&messages, "initialize").await;
        tokio::time::advance(Duration::from_secs(120)).await;
        for _ in 0..1000 {
            if !server.is_alive() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!server.is_alive());
        engine.with(|core| assert!(core.codex_prepared.is_empty()));
        assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg["method"] == "thread/start"));
        engine.shutdown();
    }

    #[tokio::test]
    async fn a_debugging_preparation_serves_a_new_design_task_without_preparing_that_task() {
        for request in ["Create a website", "Crie um site"] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, _, messages) = fake();
            let engine = engine(&dir, spawn);
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, request);
                core.rewind.captured_for_test("reply");
                core.start("kit", "reply", &pick(Vendor::Codex), request.into(), vec![], true).unwrap();
                assert_eq!(core.live["kit"].tool_kit.ids, ["design"]);
            });
            let opened = until_message(&messages, "thread/start").await;
            let instructions = opened["params"]["developerInstructions"].as_str().unwrap();
            assert!(instructions.contains("design_KIT_INSTRUCTIONS"));
            assert!(!instructions.contains("debug_KIT_INSTRUCTIONS"));
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn core_helpers_deliver_their_own_tasks_kit_to_the_other_vendor() {
        for caller in [Vendor::Claude, Vendor::Codex] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let project = engine.add_project(&folder.to_string_lossy()).unwrap();
            engine.with(|core| {
                conversation(core, Area::Project, Some(&project.id), &folder, "Debug this failure");
                core.launch("kit", &pick(caller)).unwrap();
                let expected = vec!["debug"];
                assert_eq!(core.live["kit"].tool_kit.ids, expected);
                core.start_helper("kit", "child".into(), json!({"title":"Design the page", "brief":"Design a landing page"}));
                assert!(core.helpers.contains_key("child"), "the helper must launch, not merely compute a prospective selection");
                assert_eq!(core.live["kit"].tool_kit.ids, expected);
            });
            let child = crate::helper::other(caller);
            if child == Vendor::Claude {
                let text = claude_input(&messages).await;
                assert!(text.contains("faber:design;") && text.contains("faber:debug;"));
                assert!(!text.contains("design_KIT_INSTRUCTIONS"), "helpers keep native descriptions-only discovery");
                let recorded = args.lock().unwrap();
                let (_, argv) = recorded.iter().find(|(bin, _)| bin == "claude").unwrap();
                let plugin = argv.windows(2).find(|a| a[0] == "--plugin-dir").unwrap();
                assert!(Path::new(&plugin[1]).join("skills/debug/SKILL.md").is_file());
                assert!(Path::new(&plugin[1]).join("skills/design/SKILL.md").is_file(), "Claude loads descriptions of every enabled skill");
            } else {
                let instructions = tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        if let Some(instructions) = messages.lock().unwrap().iter().find_map(|(bin, msg)| {
                            (bin == "codex" && msg["method"] == "thread/start")
                                .then(|| msg["params"]["developerInstructions"].as_str().unwrap_or_default().to_string())
                        }) {
                            break instructions;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .expect("the helper's thread was not opened");
                assert!(instructions.contains("design_KIT_INSTRUCTIONS"));
                assert!(!instructions.contains("debug_KIT_INSTRUCTIONS"));
            }
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn production_pending_compaction_queues_send_and_stop_clears_its_busy_state() {
        for stop in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, messages) = fake();
            let engine = engine(&dir, spawn);
            let (started, entered) = std::sync::mpsc::channel();
            let (finished, ended) = std::sync::mpsc::channel();
            let gate = Arc::new(std::sync::Barrier::new(2));
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug the failure");
                let c = core.convs.get_mut("kit").unwrap();
                c.working_folder = None;
                c.sessions.insert(Vendor::Claude, "native-history".into());
                let reply = c.messages.last_mut().unwrap();
                reply.vendor = Some(Vendor::Claude);
                reply.status = Some(Status::Done);
                core.synchronous_kit_checks = false;
                core.kit_check_hook = Some((started, gate.clone(), finished));
                core.compact_now("kit").unwrap();
            });
            checked_signal(&entered).await;
            let pending_busy = engine.with(|core| core.convs["kit"].compacting && core.compacts.contains_key("kit"));
            let sent = if stop {
                engine.interrupt("kit");
                Ok(String::new())
            } else {
                engine.with(|core| {
                    core.send(
                        SendRequest {
                            conversation_id: Some("kit".into()),
                            area: Area::Chat,
                            project_id: None,
                            text: "QUEUED_AFTER_COMPACT".into(),
                            images: vec![],
                            files: vec![],
                            choice: Choice { vendor: "claude".into(), model: None, effort: None },
                        },
                        None,
                    )
                })
            };
            let queued = engine.with(|core| core.convs["kit"].queued.len());
            gate.wait();
            checked_signal(&ended).await;
            assert!(pending_busy, "pending compaction owns the work slot before verification returns");
            sent.unwrap();
            if stop {
                engine.with(|core| {
                    assert!(!core.convs["kit"].compacting && !core.compacts.contains_key("kit"));
                    assert!(!core.live.contains_key("kit") && core.kit_checks.is_empty());
                });
                assert!(args.lock().unwrap().is_empty(), "Stop before admission starts no native process");
            } else {
                assert_eq!(queued, 1, "Send queues behind the pending native compaction");
                input_with(&messages, "/compact ").await;
                assert!(!messages.lock().unwrap().iter().any(|(_, msg)| msg.to_string().contains("QUEUED_AFTER_COMPACT")));
                engine.with(|core| {
                    let sid = core.live["kit"].sid;
                    core.handle(
                        "kit",
                        sid,
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
                        },
                    );
                });
                input_with(&messages, "QUEUED_AFTER_COMPACT").await;
                assert!(args.lock().unwrap()[0].1.windows(2).any(|part| part == ["--resume", "native-history"]));
                engine.with(|core| assert!(!core.convs["kit"].compacting && core.convs["kit"].queued.is_empty()));
            }
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn production_stopping_a_pending_warmed_codex_send_releases_its_unused_process() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let (started, entered) = std::sync::mpsc::channel();
        let (finished, ended) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        let server = engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug the failure");
            core.convs.get_mut("kit").unwrap().working_folder = None;
            core.warm_launch("kit", &pick(Vendor::Codex));
            let server = core.codex.values().next().unwrap().clone();
            core.synchronous_kit_checks = false;
            core.kit_check_hook = Some((started, gate.clone(), finished));
            core.rewind.captured_for_test("reply");
            core.start("kit", "reply", &pick(Vendor::Codex), "CANCELLED_SEND".into(), vec![], true).unwrap();
            server
        });
        checked_signal(&entered).await;
        engine.interrupt("kit");
        gate.wait();
        checked_signal(&ended).await;
        tokio::time::timeout(Duration::from_secs(3), async {
            while server.is_alive() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the unused warmed process must retire after Stop");
        assert_eq!(args.lock().unwrap().len(), 1, "verification adds no process");
        assert!(
            !messages.lock().unwrap().iter().any(|(_, message)| matches!(message["method"].as_str(), Some("thread/start" | "turn/start")))
        );
        engine.with(|core| assert!(core.kit_checks.is_empty() && core.codex_prepared.is_empty() && !core.live.contains_key("kit")));
        engine.shutdown();
    }

    #[tokio::test]
    async fn production_helper_rejection_after_verification_releases_its_pending_slot_and_lease() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, _) = fake();
        let engine = engine(&dir, spawn);
        let project = engine.add_project(&folder.to_string_lossy()).unwrap();
        let (started, entered) = std::sync::mpsc::channel();
        let (finished, ended) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        engine.with(|core| {
            conversation(core, Area::Project, Some(&project.id), &folder, "Debug the failure");
            core.launch("kit", &pick(Vendor::Claude)).unwrap();
            core.synchronous_kit_checks = false;
            core.kit_check_hook = Some((started, gate.clone(), finished));
            core.start_helper("kit", "rejected-child".into(), json!({"title":"Design", "brief":"Design a landing page"}));
        });
        checked_signal(&entered).await;
        let child_cli = engine.with(|core| core.opts.clis.remove(&Vendor::Codex).unwrap());
        gate.wait();
        checked_signal(&ended).await;
        tokio::time::timeout(Duration::from_secs(3), async {
            while engine.with(|core| !core.kit_checks.is_empty()) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a helper rejected by the current connection gate releases its checked result");
        assert_eq!(args.lock().unwrap().len(), 1, "only the parent CLI started");
        engine.with(|core| {
            assert!(core.helpers.is_empty());
            core.opts.clis.insert(Vendor::Codex, child_cli);
            core.synchronous_kit_checks = true;
            for call in ["first-child", "second-child"] {
                core.start_helper("kit", call.into(), json!({"title":"Design", "brief":"Design a landing page"}));
            }
            assert_eq!(core.helpers.len(), 2, "a rejected pending helper consumes neither of the two slots");
        });
        engine.shutdown();
    }

    #[tokio::test]
    async fn production_discarding_a_pending_warm_conversation_releases_its_snapshot_lease() {
        for draft in [true, false] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, _) = fake();
            let engine = engine(&dir, spawn);
            let (started, entered) = std::sync::mpsc::channel();
            let (finished, ended) = std::sync::mpsc::channel();
            let gate = Arc::new(std::sync::Barrier::new(2));
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug the failure");
                core.convs.get_mut("kit").unwrap().working_folder = None;
                if draft {
                    core.draft = Some("kit".into());
                    core.convs.get_mut("kit").unwrap().messages.clear();
                }
                core.synchronous_kit_checks = false;
                core.kit_check_hook = Some((started, gate.clone(), finished));
                core.warm_launch("kit", &pick(Vendor::Claude));
            });
            checked_signal(&entered).await;
            let removed = engine.with(|core| {
                if draft {
                    core.drop_draft();
                    Ok(())
                } else {
                    core.remove("kit")
                }
            });
            gate.wait();
            checked_signal(&ended).await;
            removed.unwrap();
            engine.with(|core| assert!(core.kit_checks.is_empty() && !core.convs.contains_key("kit") && !core.live.contains_key("kit")));
            assert!(args.lock().unwrap().is_empty(), "discarded warm-up never starts a late process");
            engine.shutdown();
        }
    }

    #[tokio::test]
    async fn production_failed_compact_check_sends_the_queued_turn_through_a_new_check() {
        let dir = crate::workspace::TestRoot::new();
        let folder = dir.path().canonicalize().unwrap();
        let (spawn, args, messages) = fake();
        let engine = engine(&dir, spawn);
        let (started, entered) = std::sync::mpsc::channel();
        let (finished, ended) = std::sync::mpsc::channel();
        let gate = Arc::new(std::sync::Barrier::new(2));
        engine.with(|core| {
            conversation(core, Area::Chat, None, &folder, "Debug the failure");
            let c = core.convs.get_mut("kit").unwrap();
            c.working_folder = None;
            c.sessions.insert(Vendor::Claude, "native-history".into());
            let reply = c.messages.last_mut().unwrap();
            reply.vendor = Some(Vendor::Claude);
            reply.status = Some(Status::Done);
            core.synchronous_kit_checks = false;
            core.kit_check_hook = Some((started, gate.clone(), finished));
            core.compact_now("kit").unwrap();
        });
        checked_signal(&entered).await;
        let sent = engine.with(|core| {
            let sent = core.send(
                SendRequest {
                    conversation_id: Some("kit".into()),
                    area: Area::Chat,
                    project_id: None,
                    text: "QUEUED_AFTER_CHECK_FAILURE".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: "claude".into(), model: None, effort: None },
                },
                None,
            );
            let token = core.kit_checks["kit"].token;
            // Exercise the actual JoinError recovery while its old worker is still parked.
            core.failed_kit_check("kit", token);
            sent
        });
        let no_spawn_before_verification = args.lock().unwrap().is_empty();
        gate.wait();
        checked_signal(&ended).await;
        sent.unwrap();
        assert!(no_spawn_before_verification, "the failed preflight never launches native resources");
        input_with(&messages, "QUEUED_AFTER_CHECK_FAILURE").await;
        assert!(!messages.lock().unwrap().iter().any(|(_, message)| message.to_string().contains("/compact ")));
        assert_eq!(args.lock().unwrap().len(), 1);
        assert!(args.lock().unwrap()[0].1.windows(2).any(|part| part == ["--resume", "native-history"]));
        engine.with(|core| assert!(!core.convs["kit"].compacting && core.convs["kit"].queued.is_empty() && core.kit_checks.is_empty()));
        engine.shutdown();
    }

    #[tokio::test]
    async fn production_quota_exhausted_during_launch_check_recovers_the_original_reply() {
        for already_recovering in [false, true] {
            let dir = crate::workspace::TestRoot::new();
            let folder = dir.path().canonicalize().unwrap();
            let (spawn, args, _) = fake();
            let engine = engine(&dir, spawn);
            let (started, entered) = std::sync::mpsc::channel();
            let (finished, ended) = std::sync::mpsc::channel();
            let gate = Arc::new(std::sync::Barrier::new(2));
            engine.with(|core| {
                conversation(core, Area::Chat, None, &folder, "Debug the failure");
                core.convs.get_mut("kit").unwrap().working_folder = None;
                core.opts.clis.remove(&Vendor::Codex);
                core.synchronous_kit_checks = false;
                core.kit_check_hook = Some((started, gate.clone(), finished));
                core.rewind.captured_for_test("reply");
                let pick = pick(if already_recovering { Vendor::Codex } else { Vendor::Claude });
                core.transfers.insert(
                    "kit".into(),
                    recovery::Transfer {
                        reply: "reply".into(),
                        pick: pick.clone(),
                        request: "Debug the failure".into(),
                        text: "Debug the failure".into(),
                        images: vec![],
                        by_hand: false,
                        approved: None,
                        retried: false,
                        moving: false,
                        check: "glance",
                    },
                );
                if already_recovering {
                    core.recover("kit", false).unwrap();
                    assert!(core.transfers["kit"].retried, "the fallback has consumed the reply's one recovery attempt");
                } else {
                    core.dispatch("kit", "reply", &pick, "Debug the failure".into(), vec![], false).unwrap();
                }
            });
            checked_signal(&entered).await;
            let until = engine.with(|core| {
                let until = core.secs() + 600.0;
                core.ledger.of_mut(Vendor::Claude).reach = crate::budget::Reach::Out(Some(until));
                until
            });
            gate.wait();
            checked_signal(&ended).await;
            tokio::time::timeout(Duration::from_secs(3), async {
                while engine.with(|core| !core.kit_checks.is_empty()) {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("the checked turn must pass through quota recovery");
            engine.with(|core| {
                let reply = core.convs["kit"].messages.last().unwrap();
                assert_eq!(reply.id, "reply", "recovery keeps the original reply and history");
                if already_recovering {
                    assert_eq!(reply.status, Some(Status::Error));
                    assert_eq!(reply.error.as_deref(), Some("error.moveFailed"), "recovery keeps its existing one-attempt limit");
                    assert!(reply.wait_until.is_none() && !core.transfers.contains_key("kit"));
                } else {
                    assert_eq!(reply.status, Some(Status::Streaming));
                    assert_eq!(reply.reason.as_deref(), Some("reason.waiting"));
                    assert_eq!(reply.wait_until, Some(until));
                    assert!(reply.error.is_none(), "a quota change during checking follows dispatch recovery instead of failing the reply");
                    assert_eq!(core.transfers["kit"].reply, "reply");
                }
                assert!(!core.live.contains_key("kit"));
            });
            assert!(args.lock().unwrap().is_empty(), "an exhausted vendor starts no process after verification");
            engine.shutdown();
        }
    }
}
