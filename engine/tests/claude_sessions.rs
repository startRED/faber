//! Parity F5: Claude Code's sessions of a folder, read from its history files of that folder only.
use faber_engine::claude_sessions::{config_dir, folder_key, list_claude_sessions};
use faber_engine::model::Vendor;
use serde_json::json;
use std::path::Path;

fn session(dir: &Path, id: &str, records: &[serde_json::Value]) {
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(dir.join(format!("{id}.jsonl")), text).unwrap();
}

fn user(entry: &str, cwd: &str, content: serde_json::Value) -> serde_json::Value {
    json!({ "type": "user", "entrypoint": entry, "cwd": cwd, "message": { "role": "user", "content": content } })
}

#[test]
fn the_folder_key_is_the_one_claude_code_uses() {
    assert_eq!(folder_key(Path::new(r"E:\Documentos\ProjetosIA\faber")), "E--Documentos-ProjetosIA-faber");
    assert_eq!(folder_key(Path::new(r"E:\Adobe\Adobe Photoshop 2026")), "E--Adobe-Adobe-Photoshop-2026");
    assert_eq!(folder_key(Path::new("/home/me/my.app")), "-home-me-my-app");
    let elsewhere = std::env::temp_dir().join("elsewhere");
    let moved = elsewhere.to_string_lossy().to_string();
    let env = |k: &str| Some(if k == "CLAUDE_CONFIG_DIR" { format!(" {moved} ") } else { "/home/me".to_string() });
    assert_eq!(config_dir(&env), Some(elsewhere));
    // A relative value would be read against Faber's folder, not the terminal's.
    let relative = |k: &str| Some(if k == "CLAUDE_CONFIG_DIR" { "claude-config" } else { "/home/me" }.to_string());
    assert_eq!(config_dir(&relative), None);
    let home = |k: &str| (k != "CLAUDE_CONFIG_DIR").then(|| "/home/me".to_string());
    assert_eq!(config_dir(&home), Some(Path::new("/home/me").join(".claude")));
}

#[test]
fn only_the_terminal_sessions_of_the_folder_are_listed_with_their_title_or_first_words() {
    let config = tempfile::tempdir().unwrap();
    let folder = Path::new("/work/app");
    let cwd = "/work/app";
    let dir = config.path().join("projects").join(folder_key(folder));
    std::fs::create_dir_all(&dir).unwrap();
    // A terminal session renamed by the person: its title wins.
    session(
        &dir,
        "aaaaaaaa-0000-4000-8000-000000000001",
        &[
            json!({ "type": "queue-operation", "sessionId": "x" }),
            user("cli", cwd, json!("fix the login form")),
            json!({ "type": "assistant", "entrypoint": "cli", "cwd": cwd, "message": { "model": "claude-sonnet-5-5", "content": [] } }),
            // An error reply's stand-in model is no model.
            json!({ "type": "assistant", "entrypoint": "cli", "cwd": cwd, "message": { "model": "<synthetic>", "content": [] } }),
            json!({ "type": "custom-title", "customTitle": "Login  form" }),
        ],
    );
    // Notes Claude Code adds and command echoes are not the person's words; the first real prompt names it.
    session(
        &dir,
        "aaaaaaaa-0000-4000-8000-000000000002",
        &[
            json!({ "type": "user", "entrypoint": "cli", "cwd": cwd, "isMeta": true, "message": { "content": "Caveat: ..." } }),
            user("cli", cwd, json!([{ "type": "text", "text": "<command-name>/clear</command-name>" }])),
            user("cli", cwd, json!([{ "type": "text", "text": "why does\n the build fail" }])),
        ],
    );
    // Faber's own runs and the desktop app are not terminal sessions; neither is a file that is not a session.
    session(&dir, "aaaaaaaa-0000-4000-8000-000000000003", &[user("sdk-cli", cwd, json!("from Faber"))]);
    session(&dir, "aaaaaaaa-0000-4000-8000-000000000004", &[user("claude-desktop", cwd, json!("from the app"))]);
    std::fs::write(dir.join("notes.jsonl"), user("cli", cwd, json!("not a session")).to_string()).unwrap();
    // A name that is not a whole session id never reaches `--resume`.
    std::fs::write(dir.join("-c.jsonl"), user("cli", cwd, json!("a flag")).to_string()).unwrap();
    // Another folder's history is never read.
    let other = config.path().join("projects").join(folder_key(Path::new("/work/other")));
    std::fs::create_dir_all(&other).unwrap();
    session(&other, "bbbbbbbb-0000-4000-8000-000000000001", &[user("cli", "/work/other", json!("elsewhere"))]);

    let list = list_claude_sessions(config.path(), folder, 30);
    let mut got: Vec<(&str, &str, Option<&str>)> = list.iter().map(|s| (s.id.as_str(), s.title.as_str(), s.model.as_deref())).collect();
    got.sort();
    assert_eq!(
        got,
        [
            ("aaaaaaaa-0000-4000-8000-000000000001", "Login form", Some("claude-sonnet-5-5")),
            ("aaaaaaaa-0000-4000-8000-000000000002", "why does the build fail", None),
        ]
    );
    assert!(list.iter().all(|s| s.vendor == Vendor::Claude && s.updated_at > 0));
    assert_eq!(list_claude_sessions(config.path(), folder, 1).len(), 1, "the limit holds");
    assert!(list_claude_sessions(config.path(), Path::new("/work/none"), 30).is_empty(), "a folder with no history has none");
}

#[test]
fn a_long_session_shows_its_latest_name_and_a_pasted_log_is_cut() {
    let config = tempfile::tempdir().unwrap();
    let folder = Path::new("/work/app");
    let dir = config.path().join("projects").join(folder_key(folder));
    std::fs::create_dir_all(&dir).unwrap();
    let filler = json!({ "type": "progress", "data": "x".repeat(1000) });
    let mut records = vec![user("cli", "/work/app", json!(format!("read this {}", "log ".repeat(3000))))];
    records.extend(std::iter::repeat_n(filler, 400));
    records.push(json!({ "type": "custom-title", "customTitle": "Renamed late" }));
    records.push(json!({ "type": "assistant", "message": { "model": "claude-opus-5-5", "content": [] } }));
    session(&dir, "aaaaaaaa-0000-4000-8000-000000000001", &records);
    session(&dir, "aaaaaaaa-0000-4000-8000-000000000002", &records[..1]);
    let list = list_claude_sessions(config.path(), folder, 30);
    let title = |end: char| list.iter().find(|s| s.id.ends_with(end)).unwrap();
    assert_eq!((title('1').title.as_str(), title('1').model.as_deref()), ("Renamed late", Some("claude-opus-5-5")));
    assert!(title('2').title.starts_with("read this log") && title('2').title.chars().count() <= 200);
}

#[test]
fn a_long_folder_finds_its_cut_key_and_checks_each_session_folder() {
    let config = tempfile::tempdir().unwrap();
    let folder = format!("/work/{}", "deep/".repeat(50));
    let folder = Path::new(folder.trim_end_matches('/'));
    let cut: String = folder_key(folder).chars().take(200).collect();
    let dir = config.path().join("projects").join(format!("{cut}-1a2b3c"));
    std::fs::create_dir_all(&dir).unwrap();
    session(&dir, "aaaaaaaa-0000-4000-8000-000000000001", &[user("cli", &folder.to_string_lossy(), json!("deep work"))]);
    // Another long folder sharing the cut key is told apart by its own cwd.
    let twin = config.path().join("projects").join(format!("{cut}-9f8e7d"));
    std::fs::create_dir_all(&twin).unwrap();
    session(&twin, "aaaaaaaa-0000-4000-8000-000000000002", &[user("cli", "/work/twin", json!("twin"))]);
    let list = list_claude_sessions(config.path(), folder, 30);
    assert_eq!(list.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["deep work"]);
}

#[cfg(unix)]
#[test]
fn a_history_folder_that_links_elsewhere_is_not_read() {
    let config = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    session(outside.path(), "aaaaaaaa-0000-4000-8000-000000000001", &[user("cli", "/work/app", json!("outside"))]);
    let projects = config.path().join("projects");
    std::fs::create_dir_all(&projects).unwrap();
    std::os::unix::fs::symlink(outside.path(), projects.join(folder_key(Path::new("/work/app")))).unwrap();
    assert!(list_claude_sessions(config.path(), Path::new("/work/app"), 30).is_empty());
}

#[test]
fn a_rename_that_starts_right_where_the_end_is_read_still_counts() {
    let config = tempfile::tempdir().unwrap();
    let folder = Path::new("/work/app");
    let dir = config.path().join("projects").join(folder_key(folder));
    std::fs::create_dir_all(&dir).unwrap();
    let mut text = format!(
        "{}
",
        user("cli", "/work/app", json!("first words"))
    );
    let filler = json!({ "type": "progress", "data": "x".repeat(1000) });
    for _ in 0..300 {
        text.push_str(&format!(
            "{filler}
"
        ));
    }
    // The rename and the last line fill the last 64 KB exactly, so the rename starts at the cut.
    let title = format!(
        "{}
",
        json!({ "type": "custom-title", "customTitle": "Right at the cut" })
    );
    let bare = format!(
        "{}
",
        json!({ "type": "progress", "data": "" })
    );
    let last = format!(
        "{}
",
        json!({ "type": "progress", "data": "y".repeat(64 * 1024 - title.len() - bare.len()) })
    );
    text.push_str(&title);
    text.push_str(&last);
    std::fs::write(dir.join("aaaaaaaa-0000-4000-8000-000000000001.jsonl"), text).unwrap();
    assert_eq!(list_claude_sessions(config.path(), folder, 30)[0].title, "Right at the cut");
}
