//! Claude Code's sessions of one folder (parity F5). Claude Code has no command that lists them, so, with the owner's
//! leave, Faber reads its history files of that folder only: `<config>/projects/<folder key>/<session id>.jsonl`, never
//! another file of the config folder (sign-ins, settings, other folders' history). Of each file it reads the start, enough
//! to name the session, and keeps only sessions started in a terminal.
use crate::model::{CliSession, Vendor};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// How much of a session file is read to name it: its first prompt and title sit at its start.
const HEAD_BYTES: u64 = 256 * 1024;
/// The longest title kept: more than a row shows.
const TITLE_CHARS: usize = 200;
/// How much of a long file's end is read again: a later rename or model change sits there.
const TAIL_BYTES: u64 = 64 * 1024;
/// Claude Code cuts a longer folder key to this many characters and adds a hash of the whole path.
const KEY_CHARS: usize = 200;
/// The most files opened for one list: a folder of thousands of Faber's own runs costs this many, not all of them.
const FILES_READ: usize = 200;

/// Claude Code's config folder: `CLAUDE_CONFIG_DIR` when set, else `.claude` in the home folder.
pub fn config_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    // A relative value would be read against Faber's own folder, not the terminal's: it names nothing here.
    if let Some(dir) = env("CLAUDE_CONFIG_DIR").map(|d| d.trim().to_string()).filter(|d| !d.is_empty()) {
        return Path::new(&dir).is_absolute().then(|| PathBuf::from(dir));
    }
    env(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(|home| PathBuf::from(home).join(".claude"))
}

/// The name Claude Code gives a folder's history: every character but a letter or a digit becomes `-`
/// (`E:\work\my app` is `E--work-my-app`).
pub fn folder_key(folder: &Path) -> String {
    folder.to_string_lossy().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
}

/// The sessions started in a terminal in `folder`, newest first, at most `limit`.
pub fn list_claude_sessions(config: &Path, folder: &Path, limit: usize) -> Vec<CliSession> {
    let mut files: Vec<(PathBuf, String, u64)> = history_dirs(config, folder)
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter_map(|e| {
            let path = e.path();
            let id = path.file_stem()?.to_str()?.to_string();
            // Session ids are UUIDs; anything else in the folder is not a session (and an id is passed to `--resume`).
            if path.extension().is_none_or(|x| x != "jsonl") || !uuid(&id) || !e.file_type().ok()?.is_file() {
                return None;
            }
            let at = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some((path, id, at))
        })
        .collect();
    // Newest first, and only as many heads read as it takes to fill the list: a folder of thousands of sessions costs
    // a few files, not all of them.
    files.sort_by_key(|f| std::cmp::Reverse(f.2));
    files
        .into_iter()
        .take(FILES_READ)
        .filter_map(|(path, id, at)| {
            let head = read_head(&path)?;
            (head.terminal && head.cwd.as_deref().is_some_and(|cwd| same_folder(cwd, folder))).then(|| CliSession {
                vendor: Vendor::Claude,
                id,
                title: head.title.unwrap_or_default(),
                model: head.model,
                updated_at: at,
            })
        })
        .take(limit)
        .collect()
}

/// The folders that may hold `folder`'s history, each a real folder right inside `<config>/projects`: never one a link
/// sends elsewhere. A long key is cut and given a hash Faber does not recompute, so every folder with the cut key is a
/// candidate; the `cwd` each session records tells them apart.
fn history_dirs(config: &Path, folder: &Path) -> Vec<PathBuf> {
    let projects = config.join("projects");
    let Ok(root) = projects.canonicalize() else { return vec![] };
    let key = folder_key(folder);
    let candidates: Vec<PathBuf> = if key.chars().count() <= KEY_CHARS {
        vec![projects.join(&key)]
    } else {
        let cut: String = key.chars().take(KEY_CHARS).collect();
        std::fs::read_dir(&projects)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with(&cut) && n.len() > cut.len()))
            .map(|e| e.path())
            .collect()
    };
    candidates
        .into_iter()
        .filter(|dir| {
            std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir())
                && dir.canonicalize().is_ok_and(|real| real.parent() == Some(root.as_path()))
        })
        .collect()
}

/// The 8-4-4-4-12 hex form of a session id.
fn uuid(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    parts.len() == 5 && parts.iter().zip([8, 4, 4, 4, 12]).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

struct Head {
    terminal: bool,
    cwd: Option<String>,
    title: Option<String>,
    model: Option<String>,
}

fn read_head(path: &Path) -> Option<Head> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut head = Head { terminal: false, cwd: None, title: None, model: None };
    let (mut entry, mut custom, mut prompt) = (None::<String>, None::<String>, None::<String>);
    for line in BufReader::new((&mut file).take(HEAD_BYTES)).lines() {
        let Ok(line) = line else { break };
        let Ok(record) = serde_json::from_str::<Value>(&line) else { continue };
        if entry.is_none() {
            entry = record["entrypoint"].as_str().map(String::from);
        }
        if head.cwd.is_none() {
            head.cwd = record["cwd"].as_str().map(String::from);
        }
        match record["type"].as_str() {
            Some("custom-title") => custom = record["customTitle"].as_str().map(one_line).filter(|t| !t.is_empty()),
            Some("user") if prompt.is_none() && record["isMeta"] != true => prompt = first_text(&record["message"]["content"]),
            // Claude Code names an error reply's model "<synthetic>": no model at all.
            Some("assistant") => {
                head.model = record["message"]["model"].as_str().filter(|m| !m.starts_with('<')).map(String::from).or(head.model)
            }
            _ => {}
        }
    }
    // A long session's latest rename and model are at its end: read that too, past its first, cut line.
    if len > HEAD_BYTES {
        use std::io::{Seek, SeekFrom};
        // One byte early, so the cut line skipped below ends at the newline before a record that starts right at the cut.
        file.seek(SeekFrom::Start(len.saturating_sub(TAIL_BYTES).max(HEAD_BYTES) - 1)).ok()?;
        for line in BufReader::new(file.take(TAIL_BYTES + 1)).lines().skip(1) {
            let Ok(line) = line else { break };
            let Ok(record) = serde_json::from_str::<Value>(&line) else { continue };
            match record["type"].as_str() {
                Some("custom-title") => custom = record["customTitle"].as_str().map(one_line).filter(|t| !t.is_empty()).or(custom),
                Some("assistant") => {
                    head.model = record["message"]["model"].as_str().filter(|m| !m.starts_with('<')).map(String::from).or(head.model)
                }
                _ => {}
            }
        }
    }
    // "cli" is Claude Code started in a terminal; Faber's own runs and the desktop apps name themselves otherwise.
    head.terminal = entry.as_deref() == Some("cli");
    head.title = custom.or(prompt);
    Some(head)
}

/// The person's words in a user record: a plain string, or the first text block. Command echoes and notes Claude Code
/// adds in tags are not words the person wrote.
fn first_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => Some(s.as_str()),
        Value::Array(blocks) => blocks.iter().find(|b| b["type"] == "text").and_then(|b| b["text"].as_str()),
        _ => None,
    }?;
    let text = one_line(text);
    (!text.is_empty() && !text.starts_with('<')).then_some(text)
}

/// On one line, and no longer than a row shows: a prompt can be a whole pasted log.
fn one_line(text: &str) -> String {
    text.split_whitespace().take(60).collect::<Vec<_>>().join(" ").chars().take(TITLE_CHARS).collect()
}

fn same_folder(a: &str, b: &Path) -> bool {
    let norm = |s: &str| {
        let s = s.trim_end_matches(['/', '\\']).replace('\\', "/");
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    norm(a) == norm(&b.to_string_lossy())
}
