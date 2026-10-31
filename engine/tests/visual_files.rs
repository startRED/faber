use crate::support;
use faber_engine::codex::{CodexTurnState, parse_codex_notification};
use faber_engine::events::EngineEvent;
use faber_engine::model::{Message, Role, ToolUse, Vendor};
use faber_engine::visual::{CONVERSATION_BYTES, FILE_BYTES, TURN_BYTES, TURN_FILES, VisualKind, capture, detect, pin};
use serde_json::{Value, json};
use std::path::Path;

#[tokio::test]
async fn dispatched_cli_turn_captures_file_versions_in_the_checked_plain_working_folder() {
    use faber_engine::cli::Cli;
    use faber_engine::model::{Area, Status};
    use faber_engine::router::Choice;
    use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    for vendor in Vendor::RUNNABLE {
        for area in [Area::Chat, Area::Project] {
            let data = support::DataFolder::new();
            let folder = support::work_folder();
            let plain = faber_engine::permissions::checked_folder(folder.path()).unwrap();
            #[cfg(windows)]
            assert!(!plain.to_string_lossy().starts_with(r"\\?\"), "exercise the engine's actual plain launch spelling");
            let mut recording = support::fixture(vendor.name(), "chat");
            let end = recording
                .iter()
                .position(|entry| {
                    !entry.input
                        && match vendor {
                            Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
                            Vendor::Claude => entry.msg["type"] == "result",
                            Vendor::Codex => entry.msg["method"] == "turn/completed",
                        }
                })
                .unwrap();
            recording.truncate(end + 1);
            let outputs: Vec<Value> = serde_json::from_str(match vendor {
                Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
                Vendor::Claude => include_str!("../../tests/fixtures/visual-files/claude.json"),
                Vendor::Codex => include_str!("../../tests/fixtures/visual-files/codex.json"),
            })
            .unwrap();
            let start = outputs
                .iter()
                .position(|entry| match vendor {
                    Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
                    Vendor::Claude => entry["message"]["content"][0]["id"] == "write",
                    Vendor::Codex => entry["params"]["item"]["id"] == "write",
                })
                .unwrap();
            let thread = recording[end].msg["params"]["threadId"].clone();
            recording.splice(
                end..end,
                outputs[start..start + 2].iter().cloned().map(|mut msg| {
                    if vendor == Vendor::Claude && msg["type"] == "assistant" {
                        // Write reports an absolute native path, including backslashes on Windows.
                        msg["message"]["content"][0]["input"]["file_path"] = json!(plain.join("page.html"));
                    }
                    if vendor == Vendor::Codex {
                        msg["params"]["threadId"] = thread.clone();
                    }
                    support::Entry { input: false, msg }
                }),
            );
            let fake = support::fake(recording);
            let replay = fake.spawn.clone();
            let launched = Arc::new(Mutex::new(Vec::new()));
            let observed = launched.clone();
            let expected = plain.clone();
            let spawn = Arc::new(move |bin: &str, args: &[String], cwd: &Path| {
                if bin == vendor.name() {
                    // The shared app-server starts in the data folder; Codex receives its working folder in thread/start.
                    if vendor == Vendor::Claude {
                        assert_eq!(cwd, expected, "{vendor:?} {area:?}: actual CLI launch spelling");
                    }
                    observed.lock().unwrap().push(cwd.to_path_buf());
                    std::fs::write(expected.join("page.html"), "<p>Saved by this fixture turn</p>")?;
                }
                replay(bin, args, cwd)
            });
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let engine = Engine::new(
                EngineOptions {
                    data_dir: data.path().into(),
                    #[cfg(windows)]
                    chat_dir: data.chat_path().into(),
                    clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
                    spawn,
                    windows: false,
                    claude_env_billed: false,
                    max_warm: 0,
                    idle: Duration::from_secs(120),
                    runtime: tokio::runtime::Handle::current(),
                    ask: None,
                },
                Arc::new(move |event| {
                    let _ = tx.send(event);
                }),
            )
            .unwrap();
            let (conversation_id, project_id) = match area {
                Area::Chat => (Some(engine.chat_folder(None, Some(&plain)).unwrap().id), None),
                Area::Project => (None, Some(engine.add_project(&plain.to_string_lossy()).unwrap().id)),
            };
            let id = engine
                .send(SendRequest {
                    conversation_id,
                    project_id,
                    area,
                    text: "In one sentence: why is the sky blue?".into(),
                    images: vec![],
                    files: vec![],
                    choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
                })
                .unwrap();
            loop {
                let event = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.expect("the fixture turn finishes").unwrap();
                if let AppEvent::Message { message, .. } = event
                    && message.role == Role::Assistant
                    && message.status == Some(Status::Done)
                {
                    break;
                }
            }
            assert_eq!(launched.lock().unwrap().len(), 1, "{vendor:?} {area:?}: a real driver was dispatched");
            if vendor == Vendor::Codex {
                let sent = fake.wire.messages();
                let start = sent.iter().find(|msg| msg["method"] == "thread/start").expect("the driver starts a real thread");
                assert_eq!(
                    Path::new(start["params"]["cwd"].as_str().unwrap()),
                    plain,
                    "the real thread launch uses the checked plain folder"
                );
            }
            let saved = engine.open(&id).unwrap();
            assert!(saved.messages.iter().flat_map(|m| m.tools.iter().flatten()).any(|t| t.id == "write" && t.ok == Some(true)));
            let visual = engine.visual_answers(&id);
            assert!(
                visual
                    .iter()
                    .any(|a| a.name.as_deref() == Some("page.html") && a.versions[0].source == "<p>Saved by this fixture turn</p>"),
                "{vendor:?} {area:?}: the successful turn in a plain-form working folder must capture its written page"
            );
            assert!(fake.mismatches.lock().unwrap().is_empty());
            engine.shutdown();
        }
    }
}

fn fixture(vendor: Vendor, root: &Path) -> Message {
    let root = pin(root).unwrap();
    let source = match vendor {
        Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
        Vendor::Claude => include_str!("../../tests/fixtures/visual-files/claude.json"),
        Vendor::Codex => include_str!("../../tests/fixtures/visual-files/codex.json"),
    };
    let entries: Vec<Value> = serde_json::from_str(source).unwrap();
    let mut state = CodexTurnState::default();
    let mut message = Message::new(vendor.name().into(), Role::Assistant, String::new(), 1);
    for entry in entries {
        let events = match vendor {
            Vendor::Gemini => unreachable!("the tests run Claude and Codex"),
            Vendor::Claude => faber_engine::claude::parse_claude_message(&entry),
            Vendor::Codex => parse_codex_notification(&entry, &mut state),
        };
        for event in events {
            match event {
                EngineEvent::Tool { id, name, input, parent } => message
                    .tools
                    .get_or_insert_default()
                    .push(serde_json::from_value(json!({"id":id,"name":name,"input":input,"parent":parent})).unwrap()),
                EngineEvent::ToolResult { id, ok, declined, .. } => {
                    let tool = message.tools.as_mut().unwrap().iter_mut().find(|tool| tool.id == id).unwrap();
                    tool.ok = Some(ok);
                    tool.declined = declined;
                    capture(std::slice::from_mut(&mut message), &root, &id);
                }
                EngineEvent::Text(text) => message.text.push_str(&text),
                _ => {}
            }
        }
    }
    message
}

#[test]
fn both_cli_fixtures_capture_only_written_bounded_visual_files_and_closed_mermaid() {
    let dir = support::canonical_tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    for path in ["page.html", "read.html", "failed.html", "nested/page.htm"] {
        std::fs::write(root.join(path), "<title>Page</title><p>First</p>").unwrap();
    }
    std::fs::write(root.join("drawing.svg"), "<svg><title>Drawing</title></svg>").unwrap();
    std::fs::write(root.join("flow.mmd"), "flowchart LR\nA-->B").unwrap();
    std::fs::write(root.join("flow.mermaid"), "sequenceDiagram\nA->>B:Hello").unwrap();
    std::fs::write(root.join("large.htm"), "x".repeat(FILE_BYTES + 1)).unwrap();
    std::fs::write(dir.path().join("outside.html"), "outside").unwrap();
    std::fs::write(root.join("private.txt"), "unrelated").unwrap();
    for vendor in Vendor::RUNNABLE {
        let message = fixture(vendor, &root);
        assert_eq!(
            message.visual_files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            ["page.html", "drawing.svg", "flow.mmd", "flow.mermaid", "nested/page.htm"],
            "{vendor:?}"
        );
        let answers = detect(&[message]);
        assert_eq!(answers.len(), 6, "one closed reply diagram and five files");
        assert_eq!(answers[0].kind, VisualKind::Mermaid);
        assert_eq!(answers[3].kind, VisualKind::Mermaid);
    }
}

fn snapshot(message: &mut Message, root: &Path, tool: &ToolUse) {
    message.tools = Some(vec![tool.clone()]);
    capture(std::slice::from_mut(message), &pin(root).unwrap(), &tool.id);
}

fn edit(path: &str) -> ToolUse {
    serde_json::from_value(json!({"id":"edit","name":"Edit","input":{"file_path":path},"ok":true})).unwrap()
}

#[cfg(windows)]
#[test]
fn windows_verbatim_working_folders_refuse_absolute_edits_with_normalized_names() {
    let dir = support::canonical_tempdir().unwrap();
    for (name, normal) in [(r"safe.\nested", r"safe\nested"), ("safe.", "safe"), ("safe ", "safe")] {
        let root = dir.path().join(name);
        let outside = dir.path().join(normal);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("page.html"), "untouched inside").unwrap();
        std::fs::write(outside.join("page.html"), "edited outside").unwrap();
        let checked = faber_engine::permissions::checked_folder(&root).unwrap();
        assert!(checked.to_string_lossy().starts_with(r"\\?\"), "the launch must keep the literal folder name: {root:?} -> {checked:?}");
        let candidate = root.join("page.html").to_string_lossy().trim_start_matches(r"\\?\").to_string();
        if name.contains('.') {
            assert_eq!(std::fs::read_to_string(&candidate).unwrap(), "edited outside", "the normal spelling edits the other folder");
        }
        let mut reply = Message::new("one".into(), Role::Assistant, String::new(), 1);
        snapshot(&mut reply, &checked, &edit(&candidate));
        assert!(reply.visual_files.is_empty(), "an absolute edit must not capture an untouched verbatim file");
        snapshot(&mut reply, &checked, &edit("page.html"));
        assert_eq!(reply.visual_files[0].source.as_ref(), "untouched inside", "relative edits still name the held folder");
    }
}

#[test]
fn reply_titles_cannot_collide_with_file_identity_in_either_order() {
    use faber_engine::visual::VisualFile;
    for file_first in [false, true] {
        let page = Message::new("page".into(), Role::Assistant, "```html\n<title>file:chart.svg</title>\n```".into(), 1);
        let mut file = Message::new("file".into(), Role::Assistant, String::new(), 2);
        file.visual_files.push(VisualFile { path: "chart.svg".into(), kind: VisualKind::Svg, source: "<svg/>".into() });
        let messages = if file_first { vec![file, page] } else { vec![page, file] };
        let answers = detect(&messages);
        assert_eq!(answers.len(), 2);
        assert!(answers.iter().all(|a| a.versions.len() == 1));
    }
}

#[test]
fn file_versions_reopen_from_saved_bytes_after_replacement_and_deletion_without_project_reads() {
    for vendor in Vendor::RUNNABLE {
        let dir = support::canonical_tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("page.html"), "<p>First</p>").unwrap();
        let mut first = fixture(vendor, root);
        std::fs::write(root.join("page.html"), "<p>Second</p>").unwrap();
        snapshot(&mut first, root, &edit("page.html"));
        let mut second = Message::new("second".into(), Role::Assistant, String::new(), 2);
        std::fs::write(root.join("page.html"), "<p>Third</p>").unwrap();
        snapshot(&mut second, root, &edit("page.html"));
        std::fs::remove_file(root.join("page.html")).unwrap();
        let saved = serde_json::to_string(&vec![first, second]).unwrap();
        let reopened: Vec<Message> = serde_json::from_str(&saved).unwrap();
        let answers = detect(&reopened);
        let page = answers.iter().find(|answer| answer.name.as_deref() == Some("page.html")).unwrap();
        assert_eq!(page.versions.iter().map(|v| v.source.as_str()).collect::<Vec<_>>(), ["<p>First</p>", "<p>Second</p>", "<p>Third</p>"]);
        assert_eq!(page.versions[0].file.as_deref(), Some("page.html"));
    }
}

#[test]
fn visual_file_capture_has_turn_limits_and_refuses_links_and_unsuccessful_or_child_tools() {
    let dir = support::canonical_tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("page.html"), "x".repeat(FILE_BYTES)).unwrap();
    let mut message = Message::new("one".into(), Role::Assistant, String::new(), 1);
    for n in 0..TURN_FILES + 1 {
        std::fs::write(root.join("page.html"), format!("{n:02}{}", "x".repeat(FILE_BYTES - 2))).unwrap();
        snapshot(&mut message, root, &edit("page.html"));
    }
    assert_eq!(message.visual_files.iter().map(|f| f.source.len()).sum::<usize>(), TURN_BYTES);
    message.visual_files.clear();
    for n in 0..TURN_FILES + 1 {
        std::fs::write(root.join("page.html"), n.to_string()).unwrap();
        snapshot(&mut message, root, &edit("page.html"));
    }
    assert_eq!(message.visual_files.len(), TURN_FILES);
    message.visual_files.clear();
    for tool in [json!({"ok":false}), json!({"declined":true}), json!({"parent":"agent"})] {
        let mut value = serde_json::to_value(edit("page.html")).unwrap();
        value.as_object_mut().unwrap().extend(tool.as_object().unwrap().clone());
        snapshot(&mut message, root, &serde_json::from_value(value).unwrap());
    }
    let target = support::canonical_tempdir().unwrap();
    std::fs::write(target.path().join("outside.html"), "secret").unwrap();
    #[cfg(windows)]
    junction::create(target.path(), root.join("link")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(target.path(), root.join("link")).unwrap();
    snapshot(&mut message, root, &edit("link/outside.html"));
    #[cfg(windows)]
    junction::delete(root.join("link")).unwrap();
    assert!(message.visual_files.is_empty());
}

#[test]
fn editing_tool_paths_are_literal_not_percent_decoded_trimmed_links_or_line_references() {
    let dir = support::canonical_tempdir().unwrap();
    let root = dir.path();
    for (path, text) in
        [("report%20draft.html", "edited"), ("report draft.html", "unrelated"), (" page.html", "leading"), ("page.html", "other")]
    {
        std::fs::write(root.join(path), text).unwrap();
    }
    let mut message = Message::new("one".into(), Role::Assistant, String::new(), 1);
    snapshot(&mut message, root, &edit("report%20draft.html"));
    snapshot(&mut message, root, &edit(" page.html"));
    snapshot(&mut message, root, &edit("page.html#L1"));
    assert_eq!(
        message.visual_files.iter().map(|f| (f.path.as_str(), f.source.as_ref())).collect::<Vec<_>>(),
        [("report%20draft.html", "edited"), (" page.html", "leading")]
    );
}

#[cfg(unix)]
#[test]
fn a_turn_pinned_before_root_substitution_never_captures_the_outside_file() {
    let dir = support::canonical_tempdir().unwrap();
    let project = dir.path().join("project");
    let outside = dir.path().join("outside");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(project.join("page.html"), "edited").unwrap();
    std::fs::write(outside.join("page.html"), "unrelated secret").unwrap();
    let pinned = pin(&project).unwrap();
    std::fs::rename(&project, dir.path().join("renamed")).unwrap();
    std::os::unix::fs::symlink(&outside, &project).unwrap();
    let mut reply = Message::new("one".into(), Role::Assistant, String::new(), 1);
    reply.tools = Some(vec![edit("page.html")]);
    capture(std::slice::from_mut(&mut reply), &pinned, "edit");
    assert!(reply.visual_files.is_empty());
    assert!(pin(&project).is_none(), "a replaced canonical working path cannot be repinned through its link");
}

#[cfg(unix)]
#[test]
fn unix_literal_backslashes_cannot_select_a_different_nested_file() {
    let dir = support::canonical_tempdir().unwrap();
    std::fs::create_dir(dir.path().join("a")).unwrap();
    std::fs::write(dir.path().join("a\\b.html"), "edited").unwrap();
    std::fs::write(dir.path().join("a/b.html"), "unrelated").unwrap();
    let mut reply = Message::new("one".into(), Role::Assistant, String::new(), 1);
    snapshot(&mut reply, dir.path(), &edit("a\\b.html"));
    assert!(reply.visual_files.is_empty(), "an ambiguous Unix name is refused, never rewritten as another path");
}

#[cfg(unix)]
#[test]
fn unix_non_unicode_working_folder_cannot_alias_a_unicode_absolute_edit() {
    use std::os::unix::ffi::OsStringExt;
    let dir = support::canonical_tempdir().unwrap();
    let working = dir.path().join(std::ffi::OsString::from_vec(b"p\xff".to_vec()));
    let outside = dir.path().join("p\u{fffd}");
    // APFS refuses names that are not UTF-8 (EILSEQ), so such a folder cannot exist there.
    if std::fs::create_dir(&working).is_err() && cfg!(target_os = "macos") {
        return;
    }
    assert!(working.is_dir());
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(working.join("page.html"), "untouched inside").unwrap();
    std::fs::write(outside.join("page.html"), "edited outside").unwrap();
    let mut reply = Message::new("one".into(), Role::Assistant, String::new(), 1);
    snapshot(&mut reply, &working, &edit(outside.join("page.html").to_str().unwrap()));
    assert!(reply.visual_files.is_empty(), "lossy root spelling must not capture the untouched native file");
    snapshot(&mut reply, &working, &edit("page.html"));
    assert_eq!(reply.visual_files[0].source.as_ref(), "untouched inside", "relative edits still use the pinned native folder");
}

#[cfg(unix)]
#[test]
fn unix_working_folder_backslashes_cannot_alias_an_outside_absolute_edit() {
    let dir = support::canonical_tempdir().unwrap();
    let working = dir.path().join("a\\b");
    let outside = dir.path().join("a/b");
    std::fs::create_dir(&working).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(working.join("page.html"), "untouched inside").unwrap();
    std::fs::write(outside.join("page.html"), "edited outside").unwrap();
    let mut reply = Message::new("one".into(), Role::Assistant, String::new(), 1);
    snapshot(&mut reply, &working, &edit(outside.join("page.html").to_str().unwrap()));
    assert!(reply.visual_files.is_empty(), "root spelling must not turn the outside edit into an untouched inside file");
    snapshot(&mut reply, &working, &edit("page.html"));
    assert_eq!(reply.visual_files[0].source.as_ref(), "untouched inside", "the real working folder remains usable");
}

#[test]
fn conversation_cap_stops_new_captures_without_removing_earlier_versions() {
    let dir = support::canonical_tempdir().unwrap();
    std::fs::write(dir.path().join("page.html"), "x".repeat(FILE_BYTES)).unwrap();
    let root = pin(dir.path()).unwrap();
    let mut messages = vec![];
    for n in 0..CONVERSATION_BYTES / FILE_BYTES + 2 {
        let mut reply = Message::new(n.to_string(), Role::Assistant, String::new(), n as u64);
        let mut tool = edit("page.html");
        tool.id = n.to_string();
        reply.tools = Some(vec![tool]);
        messages.push(reply);
        capture(&mut messages, &root, &n.to_string());
    }
    assert_eq!(messages.iter().flat_map(|m| &m.visual_files).map(|f| f.source.len()).sum::<usize>(), CONVERSATION_BYTES);
    assert!(messages.last().unwrap().visual_files.is_empty());
    let all = detect(&messages);
    assert_eq!(all[0].versions.len(), CONVERSATION_BYTES / FILE_BYTES);
    let listed = faber_engine::visual::list(&messages, None);
    assert!(listed.iter().flat_map(|a| &a.versions).all(|v| v.source.is_empty()));
    let selected = faber_engine::visual::list(&messages, Some(("1", 0)));
    assert_eq!(selected.iter().flat_map(|a| &a.versions).filter(|v| !v.source.is_empty()).count(), 1);
    assert_eq!(selected[0].versions[1].source.len(), FILE_BYTES);
}
