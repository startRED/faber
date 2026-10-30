use crate::support;
use base64::Engine as _;
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Vendor};
use faber_engine::proc::Spawner;
use faber_engine::router::Choice;
use faber_engine::workspace::Attachment;
use faber_engine::{Engine, EngineOptions, SendRequest, WarmRequest};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn engine(root: &crate::support::DataFolder, vendor: Vendor, spawn: Spawner) -> Engine {
    Engine::new(
        EngineOptions {
            data_dir: root.path().into(),
            #[cfg(windows)]
            chat_dir: root.chat_path().into(),
            clis: BTreeMap::from([(vendor, Cli { command: vendor.name().into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    )
    .unwrap()
}

fn file(name: &str, bytes: &[u8]) -> Attachment {
    Attachment { name: name.into(), base64: base64::engine::general_purpose::STANDARD.encode(bytes) }
}

#[tokio::test]
async fn chat_groups_keep_bounded_copies_membership_and_plain_instructions() {
    let dir = support::DataFolder::new();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Travel").unwrap();
    assert_eq!(app.create_chat_group(" ").unwrap_err(), "error.groupName");
    let instructions = "é".repeat(8000);
    assert!(app.save_chat_group(&group.id, None, Some(&instructions)).is_ok());
    assert_eq!(app.save_chat_group(&group.id, None, Some(&format!("{instructions}x"))).unwrap_err(), "error.groupInstructions");
    let ten = (0..10).map(|n| file(&format!("{n}.txt"), &vec![b'a'; 500_000])).collect::<Vec<_>>();
    let group = app.add_chat_group_files(&group.id, &ten).unwrap();
    assert_eq!(group.files.iter().map(|f| f.size).sum::<usize>(), 5_000_000);
    assert!(serde_json::to_vec(&app.state().groups).unwrap().len() < 25_000, "the state retains metadata, never file bytes");
    for f in &group.files {
        assert_eq!(std::fs::metadata(dir.path().join("chat-groups").join(&group.id).join(&f.id)).unwrap().len(), 500_000);
    }
    let chat = app.new_group_chat(&group.id).unwrap();
    assert_eq!(app.state().groups[0].conversation_ids.as_slice(), std::slice::from_ref(&chat.id));
    app.move_chat_to_group(&chat.id, None).unwrap();
    assert!(app.state().groups[0].conversation_ids.is_empty());
    app.move_chat_to_group(&chat.id, Some(&group.id)).unwrap();
    let after = app.remove_chat_group_file(&group.id, &group.files[0].id).unwrap();
    assert_eq!(after.files.len(), 9);
    app.shutdown();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    assert_eq!(app.state().groups[0].instructions, instructions);
    assert_eq!(app.state().groups[0].conversation_ids.as_slice(), std::slice::from_ref(&chat.id));
    app.remove_chat_group(&group.id).unwrap();
    assert!(app.open(&chat.id).is_some(), "deleting a group keeps its chats");
    assert!(!dir.path().join("chat-groups").join(&group.id).exists());
    assert!(app.state().groups.is_empty());
    app.shutdown();
}

#[tokio::test]
async fn chat_groups_enforce_the_file_count_below_the_byte_limit() {
    let dir = support::DataFolder::new();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Small files").unwrap();
    let ten = (0..10).map(|n| file(&format!("{n}.txt"), b"x")).collect::<Vec<_>>();
    let group = app.add_chat_group_files(&group.id, &ten).unwrap();
    assert_eq!(group.files.iter().map(|f| f.size).sum::<usize>(), 10);
    assert_eq!(app.add_chat_group_files(&group.id, &[file("extra.txt", b"x")]).unwrap_err(), "error.groupFiles");
    assert_eq!(app.state().groups[0].files.len(), 10);
    app.shutdown();
}

#[tokio::test]
async fn move_chat_to_project_leaves_its_group_and_launches_without_group_context() {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        let dir = support::DataFolder::new();
        let folder = support::work_folder();
        let fake = support::fake(support::fixture(vendor.name(), "chat"));
        let app = engine(&dir, vendor, fake.spawn);
        let group = app.create_chat_group("Glossary").unwrap();
        app.save_chat_group(&group.id, None, Some("Use the group glossary exactly.")).unwrap();
        let group = app.add_chat_group_files(&group.id, &[file("glossary.txt", b"group knowledge")]).unwrap();
        let chat = app.new_group_chat(&group.id).unwrap();
        let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
        app.move_to_project(&chat.id, &project.id).unwrap();
        let store = faber_engine::store::Store::new(dir.path());
        let groups = store.chat_groups().unwrap();
        assert!(groups[0].conversation_ids.is_empty(), "moving to a project also durably leaves the Chat group");
        assert_eq!(groups[0].instructions, "Use the group glossary exactly.");
        let knowledge = dir.path().join("chat-groups").join(&group.id).join(&group.files[0].id);
        assert_eq!(std::fs::read(&knowledge).unwrap(), b"group knowledge");
        app.send(SendRequest {
            conversation_id: Some(chat.id.clone()),
            area: Area::Project,
            project_id: Some(project.id),
            text: "In one sentence: why is the sky blue?".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
        })
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !fake.wire.messages().iter().any(|m| m["type"] == "user" || m["method"] == "turn/start") {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the first project turn starts");
        let args = fake.args.lock().unwrap();
        assert!(!args.iter().flatten().any(|s| s.contains("Use the group glossary exactly.")));
        let wire = format!("{:?}", fake.wire.messages());
        assert!(!wire.contains("Use the group glossary exactly.") && !wire.contains("glossary.txt") && !wire.contains("group-"), "{wire}");
        assert!(fake.mismatches.lock().unwrap().is_empty());
        drop(args);
        app.shutdown();
    }
}

#[tokio::test]
async fn move_chat_to_project_group_write_failure_keeps_the_chat_and_membership() {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Work").unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
    let store = faber_engine::store::Store::new(dir.path());
    let before = store.chat_groups().unwrap();
    for blocker in [dir.path().join("chat-groups.json.tmp"), dir.path().join("conversations").join(format!("{}.json.tmp", chat.id))] {
        std::fs::create_dir_all(blocker.join("held")).unwrap();
        assert_eq!(app.move_to_project(&chat.id, &project.id).unwrap_err(), "error.store.write");
        assert_eq!(store.get(&chat.id).unwrap(), chat);
        assert_eq!(app.open(&chat.id).unwrap().area, Area::Chat);
        // Remove the deterministic disk fault before checking recovery of either failed write.
        std::fs::remove_dir_all(&blocker).unwrap();
        assert_eq!(store.chat_groups().unwrap(), before);
    }
    app.move_to_project(&chat.id, &project.id).unwrap();
    assert!(store.chat_groups().unwrap()[0].conversation_ids.is_empty());
    app.shutdown();
}

#[tokio::test]
async fn move_chat_to_project_group_commit_recovers_after_interruption() {
    let dir = support::DataFolder::new();
    let folder = support::work_folder();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Work").unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let project = app.add_project(&folder.path().to_string_lossy()).unwrap();
    let before = app.state().groups;
    app.shutdown();
    let journal = dir.path().join("chat-group-move.json");
    let groups_file = dir.path().join("chat-groups.json");
    let mut after = before.clone();
    after[0].conversation_ids.clear();
    for committed in [false, true] {
        let store = faber_engine::store::Store::new(dir.path());
        let mut record = chat.clone();
        if committed {
            record.area = Area::Project;
            record.project_id = Some(project.id.clone());
            record.project_start_pending = true;
        }
        store.save_checked(&record).unwrap();
        std::fs::write(&journal, json!({"conversationId":chat.id,"groups":before}).to_string()).unwrap();
        std::fs::write(&groups_file, serde_json::to_vec(&after).unwrap()).unwrap();
        drop(store);
        let store = faber_engine::store::Store::new(dir.path());
        if !committed {
            let blocker = dir.path().join("chat-groups.json.tmp");
            std::fs::create_dir_all(blocker.join("held")).unwrap();
            let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
            assert_eq!(app.state().groups, before, "a blocked rollback still publishes the journal's coherent membership");
            assert_eq!(app.open(&chat.id).unwrap().area, Area::Chat);
            app.rename(&chat.id, "Still in Work");
            assert_eq!(store.get(&chat.id).unwrap().title, "Still in Work", "ordinary Chat saves remain available during recovery");
            let mut moved = chat.clone();
            moved.area = Area::Project;
            assert_eq!(store.save_checked(&moved).unwrap_err(), faber_engine::store::Fault::Write);
            assert!(journal.is_file(), "area and membership mutations stay blocked until recovery can finish");
            let attachment = dir.scratch_path().join(&chat.id).join("attachments").join("notes.txt");
            std::fs::create_dir_all(attachment.parent().unwrap()).unwrap();
            std::fs::write(&attachment, b"keep these notes").unwrap();
            assert_eq!(app.remove_with(&chat.id, false).unwrap_err(), "error.chat.deleteFailed");
            assert!(store.get(&chat.id).is_some(), "a refused deletion retains its conversation record");
            assert_eq!(std::fs::read(&attachment).unwrap(), b"keep these notes", "a refused deletion retains its files too");
            app.shutdown();
            std::fs::remove_dir_all(blocker).unwrap();
        }
        assert_eq!(store.chat_groups().unwrap(), if committed { after.clone() } else { before.clone() });
        assert_eq!(store.get(&chat.id).unwrap().area, if committed { Area::Project } else { Area::Chat });
        assert!(!journal.exists(), "recovery finishes before any later membership write");
        assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&groups_file).unwrap()).unwrap(), json!(store.chat_groups().unwrap()));
    }
}

#[tokio::test]
async fn chat_groups_only_accept_shared_image_types_and_keep_svg_as_text() {
    let dir = support::DataFolder::new();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Pictures").unwrap();
    for extension in ["bmp", "avif"] {
        assert_eq!(
            app.add_chat_group_files(&group.id, &[file(&format!("picture.{extension}"), b"unsupported")]).unwrap_err(),
            "error.groupFileType"
        );
    }
    let group = app
        .add_chat_group_files(
            &group.id,
            &[
                file("vector.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
                file("picture.JPG", b"jpeg"),
                file("picture.png", b"png"),
                file("picture.gif", b"gif"),
                file("picture.webp", b"webp"),
            ],
        )
        .unwrap();
    assert_eq!(
        group.files.iter().map(|f| f.mime.as_str()).collect::<Vec<_>>(),
        ["text/plain", "image/jpeg", "image/png", "image/gif", "image/webp"]
    );
    app.shutdown();
}

#[tokio::test]
async fn chat_groups_reject_invalid_files_without_partial_storage() {
    let dir = support::DataFolder::new();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Work").unwrap();
    assert_eq!(
        app.add_chat_group_files(&group.id, &[file("a.txt", b"valid"), file("binary.bin", &[0xff])]).unwrap_err(),
        "error.groupFileType"
    );
    assert!(app.state().groups[0].files.is_empty());
    assert!(!dir.path().join("chat-groups").join(&group.id).exists());
    assert_eq!(app.add_chat_group_files(&group.id, &[file("big.txt", &vec![b'a'; 5_000_001])]).unwrap_err(), "error.groupFiles");
    app.shutdown();
}

#[tokio::test]
async fn chat_groups_failed_deletion_preserves_files_membership_and_instructions() {
    let dir = support::DataFolder::new();
    let app = engine(&dir, Vendor::Claude, support::fake(vec![]).spawn);
    let group = app.create_chat_group("Work").unwrap();
    app.save_chat_group(&group.id, None, Some("Keep the glossary")).unwrap();
    let group = app.add_chat_group_files(&group.id, &[file("glossary.txt", b"Keep these exact words")]).unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let before = app.state().groups;
    let path = dir.path().join("chat-groups").join(&group.id).join(&group.files[0].id);
    // An occupied atomic-write path deterministically fails metadata writes on every OS.
    let blocker = dir.path().join("chat-groups.json.tmp");
    std::fs::create_dir_all(blocker.join("held")).unwrap();
    assert_eq!(app.remove_chat_group_file(&group.id, &group.files[0].id).unwrap_err(), "error.store.write");
    assert_eq!(std::fs::read(&path).unwrap(), b"Keep these exact words");
    assert_eq!(app.state().groups, before);
    assert_eq!(app.remove_chat_group(&group.id).unwrap_err(), "error.store.write");
    assert_eq!(std::fs::read(&path).unwrap(), b"Keep these exact words");
    assert_eq!(app.state().groups, before);
    assert!(app.open(&chat.id).is_some());
    std::fs::remove_dir_all(blocker).unwrap();
    // The reader also recovers a crash before the metadata commit.
    let folder = path.parent().unwrap();
    std::fs::rename(folder, folder.with_extension("removing")).unwrap();
    assert_eq!(app.state().groups, before);
    assert_eq!(std::fs::read(&path).unwrap(), b"Keep these exact words");
    std::fs::rename(&path, path.with_extension("removing")).unwrap();
    assert_eq!(app.state().groups, before);
    assert_eq!(std::fs::read(&path).unwrap(), b"Keep these exact words");
    app.remove_chat_group(&group.id).unwrap();
    assert!(!folder.exists());
    app.shutdown();
}

#[tokio::test]
async fn chat_groups_edits_refresh_a_warm_chat_while_renaming_keeps_its_process() {
    let dir = support::DataFolder::new();
    let fake = support::fake(vec![]);
    let app = engine(&dir, Vendor::Claude, fake.spawn);
    let group = app.create_chat_group("Work").unwrap();
    app.save_chat_group(&group.id, None, Some("First group words")).unwrap();
    let chat = app.new_group_chat(&group.id).unwrap();
    let warm = || WarmRequest {
        conversation_id: Some(chat.id.clone()),
        area: Area::Chat,
        project_id: None,
        choice: Choice { vendor: "claude".into(), model: None, effort: None },
    };
    app.warm(warm());
    app.save_chat_group(&group.id, None, Some("New group words")).unwrap();
    app.warm(warm());
    assert_eq!(fake.args.lock().unwrap().len(), 2);
    app.save_chat_group(&group.id, Some("Renamed"), None).unwrap();
    app.warm(warm());
    assert_eq!(fake.args.lock().unwrap().len(), 2, "a name changes no model context");
    app.add_chat_group_files(&group.id, &[file("glossary.txt", b"A widget is a tool")]).unwrap();
    app.warm(warm());
    assert_eq!(fake.args.lock().unwrap().len(), 3, "the next launch gets added files too");
    app.move_chat_to_group(&chat.id, None).unwrap();
    app.warm(warm());
    let launches = fake.args.lock().unwrap();
    assert_eq!(launches.len(), 4);
    let words = |args: &[String]| args[args.iter().position(|a| a == "--append-system-prompt").unwrap() + 1].clone();
    assert!(words(&launches[0]).contains("First group words"));
    assert!(words(&launches[1]).contains("New group words"));
    assert!(!words(&launches[3]).contains("group words"));
    app.shutdown();
}

async fn launch_context(vendor: Vendor, grouped: bool, resumed: bool) {
    let dir = support::DataFolder::new();
    let (spawn, mut input, mut output) = support::controlled();
    let args: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = args.clone();
    let recorded: Spawner = Arc::new(move |bin, argv, cwd| {
        *seen.lock().unwrap() = argv.to_vec();
        spawn(bin, argv, cwd)
    });
    let app = engine(&dir, vendor, recorded.clone());
    let group = app.create_chat_group("Work").unwrap();
    app.save_chat_group(&group.id, None, Some("Old group words")).unwrap();
    let group = app
        .add_chat_group_files(
            &group.id,
            &[file("glossary.txt", b"A widget is a small tool."), file("diagram.png", b"image bytes"), file("vector.svg", b"<svg/>")],
        )
        .unwrap();
    let id = grouped.then(|| app.new_group_chat(&group.id).unwrap().id);
    app.save_chat_group(&group.id, None, Some("Use our glossary, exactly as written.")).unwrap();
    app.shutdown();
    let store = faber_engine::store::Store::new(dir.path());
    // Previously saved SVG metadata must not send an unsupported image on the next launch either.
    let mut saved = store.chat_groups().unwrap();
    saved[0].files[2].mime = "image/svg+xml".into();
    store.save_chat_groups(&saved).unwrap();
    if let Some(id) = &id
        && resumed
    {
        let mut chat = store.get(id).unwrap();
        chat.sessions.insert(vendor, "91dcb98f-7a91-4324-a10a-bccfe07ab7a4".into());
        store.save(&chat);
    }
    let app = engine(&dir, vendor, recorded);
    app.send(SendRequest {
        conversation_id: id.clone(),
        area: Area::Chat,
        project_id: None,
        text: "Explain widgets".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
    })
    .unwrap();
    let turn: Value;
    if vendor == Vendor::Codex {
        let init = support::expect_method(&mut input, "initialize").await;
        support::emit(&mut output, json!({"id":init["id"],"result":{}})).await;
        support::expect_method(&mut input, "initialized").await;
        let thread = support::expect_method(&mut input, if resumed { "thread/resume" } else { "thread/start" }).await;
        let words = thread["params"]["developerInstructions"].as_str().unwrap();
        assert!(words.contains("You are Faber"));
        assert_eq!(words.contains("Use our glossary, exactly as written."), grouped);
        assert!(!words.contains("Old group words"));
        support::emit(&mut output, json!({"id":thread["id"],"result":{"thread":{"id":"group-test"}}})).await;
        turn = support::expect_method(&mut input, "turn/start").await;
    } else {
        turn = support::read_message(&mut input).await;
        let args = args.lock().unwrap();
        let words = &args[args.iter().position(|a| a == "--append-system-prompt").unwrap() + 1];
        assert!(words.contains("You are Faber"));
        assert_eq!(words.contains("Use our glossary, exactly as written."), grouped);
        assert!(!words.contains("Old group words"));
        assert_eq!(args.iter().any(|a| a == "--resume"), resumed);
    }
    let text = turn.to_string();
    assert_eq!(text.contains("attachments/"), grouped, "{turn}");
    assert_eq!(text.contains("image/png") || text.contains("data:image/png"), grouped, "{turn}");
    assert_eq!(text.contains(&base64::engine::general_purpose::STANDARD.encode(b"image bytes")), grouped, "{turn}");
    assert!(!text.contains("image/svg+xml"), "SVG goes through the file path, never the image path: {turn}");
    assert_eq!(text.contains("vector.svg"), grouped, "{turn}");
    if grouped {
        // The original source is never needed again: launches read Faber's stored copy.
        assert!(dir.path().join("chat-groups").join(&group.id).join(&group.files[0].id).is_file());
        let copied = dir
            .scratch_path()
            .join(id.as_ref().unwrap())
            .join("attachments")
            .join(format!("group-{}", group.id))
            .join(format!("{}.txt", group.files[0].id));
        assert_eq!(std::fs::read_to_string(copied).unwrap(), "A widget is a small tool.");
        let svg = dir
            .scratch_path()
            .join(id.unwrap())
            .join("attachments")
            .join(format!("group-{}", group.id))
            .join(format!("{}.txt", group.files[2].id));
        assert_eq!(std::fs::read_to_string(svg).unwrap(), "<svg/>");
    }
    app.shutdown();
}

#[tokio::test]
async fn chat_groups_launch_claude_with_persona_instructions_and_attachments_only_inside_group() {
    launch_context(Vendor::Claude, true, true).await;
    launch_context(Vendor::Claude, false, false).await;
}

#[tokio::test]
async fn chat_groups_launch_codex_with_persona_instructions_and_attachments_only_inside_group() {
    launch_context(Vendor::Codex, true, true).await;
    launch_context(Vendor::Codex, false, false).await;
}

#[tokio::test]
async fn chat_groups_launch_fresh_grouped_chats_for_both_clis() {
    launch_context(Vendor::Claude, true, false).await;
    launch_context(Vendor::Codex, true, false).await;
}
