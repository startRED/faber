//! Look around after a fix. A turn that went from red to green fixed something: a check failed, the work changed, and the
//! last check passed. The same kind of mistake often lives in other places, so such a reply is marked for the offer to
//! look for it there; the person decides, and nothing runs until they do. The mirror of the step up (`nudge`): the same
//! rules for a check and an edit, and the two never hold together. Pure functions over a finished message; the engine
//! feeds them.

use crate::model::{Message, Status, ToolUse};
use crate::nudge::{command, edited, is_check, is_source};
use serde::{Deserialize, Serialize};

/// The offer under a finished answer. `reason` is a key for the screen: `look.fixed` (red to green) or `look.sameText`
/// (text a change removed is still in other files).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub reason: String,
    /// Where the text a change removed still is, as `path:line` from the conversation's folder (at most `MOST_PLACES`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<String>,
}

/// Places one offer names at most.
pub const MOST_PLACES: usize = 8;
/// A removed text found in more places than this is a common line (an import, a closing brace with a comment), not a
/// mistake that spread: it is dropped.
const COMMON: usize = 8;
/// Removed texts looked for at most, and the most lines one change may remove and still count: a fix is small, a rewrite
/// is not a mistake to look for elsewhere.
const MOST_NEEDLES: usize = 5;
const MOST_REMOVED: usize = 10;
/// A removed text shorter than this, once trimmed, matches too much to mean anything.
const SHORTEST: usize = 12;

/// The bounds of the read that looks for removed text: files read, names looked at (folders included, so a tree of
/// empty folders ends too), bytes read, the largest file read, and time.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub files: usize,
    pub entries: usize,
    pub bytes: u64,
    pub file_bytes: u64,
    pub time: std::time::Duration,
}

impl Limits {
    pub const AFTER_TURN: Limits =
        Limits { files: 2_000, entries: 20_000, bytes: 8 << 20, file_bytes: 512 << 10, time: std::time::Duration::from_millis(300) };
}

/// Folders never read: version control, dependencies and build output, where a match says nothing about the work.
const SKIP: [&str; 9] = ["node_modules", "target", "dist", "build", "out", "vendor", "__pycache__", "venv", "coverage"];

/// What a finished turn offers, if anything: only a turn that ended done, where some check failed, the work changed, and
/// the same command passed after that change; every command that failed in the turn ended green, the last check passed,
/// no code changed after it, and its closing box asks nothing (one question at a time). A command is its whole line, so a
/// failure rerun narrower (`cargo test` then `cargo test --lib`) stays red and nothing is offered: quiet beats wrong.
pub fn of(m: &Message) -> Option<Look> {
    if m.status != Some(Status::Done) || m.wrap_up.as_ref().is_some_and(|w| w.ask.is_some()) {
        return None;
    }
    let tools = m.tools.as_deref().unwrap_or_default();
    // A declined check never ran, so it neither failed nor passed.
    let runs: Vec<(usize, &str, Option<bool>)> = tools
        .iter()
        .enumerate()
        .filter(|(_, t)| is_check(t) && !t.declined)
        .filter_map(|(i, t)| Some((i, command(t)?.trim(), t.ok)))
        .collect();
    let &(last, _, ok) = runs.last()?;
    let ends_green = |cmd: &str| runs.iter().rev().find(|r| r.1 == cmd).is_some_and(|r| r.2 == Some(true));
    if ok != Some(true) || runs.iter().any(|r| r.2 == Some(false) && !ends_green(r.1)) {
        return None;
    }
    // A refused edit (ok = false) changed nothing.
    let changed = |t: &ToolUse| t.ok != Some(false) && !edited(t).is_empty();
    let fixed = runs.iter().filter(|r| r.2 == Some(false)).any(|&(failed, cmd, _)| {
        tools
            .iter()
            .enumerate()
            .skip(failed + 1)
            .find(|(_, t)| changed(t))
            .is_some_and(|(change, _)| runs.iter().any(|r| r.0 > change && r.1 == cmd && r.2 == Some(true)))
    });
    let unchecked = tools[last + 1..].iter().any(|t| changed(t) && edited(t).into_iter().any(is_source));
    (fixed && !unchecked).then(|| Look { reason: "look.fixed".into(), places: vec![] })
}

/// The texts the reply's applied changes removed: the lines of Claude's `Edit` and `MultiEdit` `old_string` and of Codex's
/// `fileChange` diffs (`-` lines) that the same change did not keep, trimmed, at least `SHORTEST` characters with a letter
/// or digit, from changes that removed at most `MOST_REMOVED` lines. A line inside a longer `old_string` counts whole, so a
/// fragment the agent changed inside a line is found where the same fragment is a line's part elsewhere.
pub fn removed(m: &Message) -> Vec<String> {
    let applied: Vec<&ToolUse> = m.tools.as_deref().unwrap_or_default().iter().filter(|t| t.ok == Some(true) && !t.declined).collect();
    // A text the same reply wrote anywhere was moved, not removed: finding it again says nothing.
    let added: Vec<String> = applied.iter().flat_map(|t| pairs(t)).map(|(_, new)| new).collect();
    let mut found: Vec<String> = vec![];
    for tool in &applied {
        for (old, new) in pairs(tool) {
            let kept: Vec<&str> = new.lines().map(str::trim).collect();
            let gone: Vec<&str> = old.lines().map(str::trim).filter(|l| !l.is_empty() && !kept.contains(l)).collect();
            if gone.len() > MOST_REMOVED {
                continue;
            }
            for line in gone {
                let moved = added.iter().any(|a| a.contains(line));
                if line.chars().count() >= SHORTEST && line.chars().any(char::is_alphanumeric) && !moved && !found.iter().any(|f| f == line)
                {
                    found.push(line.to_string());
                }
            }
        }
    }
    found.truncate(MOST_NEEDLES);
    found
}

/// The (removed, kept) text of each change a tool made.
fn pairs(tool: &ToolUse) -> Vec<(String, String)> {
    let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_string();
    match tool.name.as_str() {
        "Edit" => vec![(text(&tool.input["old_string"]), text(&tool.input["new_string"]))],
        "Write" => vec![(String::new(), text(&tool.input["content"]))],
        "MultiEdit" => {
            tool.input["edits"].as_array().into_iter().flatten().map(|e| (text(&e["old_string"]), text(&e["new_string"]))).collect()
        }
        "fileChange" => {
            let changes: Vec<&serde_json::Value> = match &tool.input {
                serde_json::Value::Array(all) => all.iter().collect(),
                serde_json::Value::Object(by_path) => by_path.values().collect(),
                _ => vec![],
            };
            changes
                .into_iter()
                .filter_map(|c| c["diff"].as_str())
                .map(|diff| {
                    let side = |sign: char, other: &str| {
                        diff.lines()
                            .filter(|l| l.starts_with(sign) && !l.starts_with(other))
                            .map(|l| &l[1..])
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    (side('-', "---"), side('+', "+++"))
                })
                .collect()
        }
        _ => vec![],
    }
}

/// Where the removed texts still are under `root`, as `path:line` with `/` between folders, sorted by path: at most
/// `MOST_PLACES`, a text found in more than `COMMON` places dropped. Hidden folders and those in `SKIP` are never read, nor
/// a file over `file_bytes`, one that is not a plain file or not UTF-8 text, or one past the bounds; a read that hits a
/// bound keeps what it found.
pub fn search(root: &std::path::Path, needles: &[String], limits: Limits) -> Vec<String> {
    if needles.is_empty() {
        return vec![];
    }
    let start = std::time::Instant::now();
    let (mut files, mut seen, mut bytes) = (0usize, 0usize, 0u64);
    let mut hits: Vec<Vec<String>> = vec![vec![]; needles.len()];
    let mut dirs = vec![root.to_path_buf()];
    'walk: while let Some(dir) = dirs.pop() {
        if seen >= limits.entries || start.elapsed() >= limits.time {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        // A folder is listed no further than the names left, so one huge folder cannot fill memory before a bound.
        let mut entries: Vec<_> = entries.flatten().take(limits.entries - seen).collect();
        seen += entries.len();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if start.elapsed() >= limits.time {
                break 'walk;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            // A link is never followed: it may lead out of the folder, or round in a circle.
            let Ok(kind) = entry.file_type() else { continue };
            if name.starts_with('.') || kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    dirs.push(entry.path());
                }
                continue;
            }
            // Only a plain file: opening a pipe, socket or device can wait for ever.
            if !kind.is_file() {
                continue;
            }
            if files >= limits.files || bytes >= limits.bytes {
                break 'walk;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            if size > limits.file_bytes {
                continue;
            }
            files += 1;
            bytes += size;
            let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
            if text.contains('\0') {
                continue;
            }
            let path = entry.path();
            let shown = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            for (at, line) in text.lines().enumerate() {
                for (i, needle) in needles.iter().enumerate() {
                    if hits[i].len() <= COMMON && line.contains(needle.as_str()) {
                        hits[i].push(format!("{shown}:{}", at + 1));
                    }
                }
            }
        }
    }
    let mut places: Vec<String> = hits.into_iter().filter(|h| h.len() <= COMMON).flatten().collect();
    places.sort_by(|a, b| natural(a).cmp(&natural(b)));
    places.dedup();
    places.truncate(MOST_PLACES);
    places
}

/// A `path:line` sorted by path, then by line as a number.
fn natural(place: &str) -> (&str, usize) {
    place.rsplit_once(':').map_or((place, 0), |(p, l)| (p, l.parse().unwrap_or(0)))
}

/// The offer once the places are known: the red-to-green offer names them, and a reply with none of its own offers
/// `look.sameText` when it ended done with no closing question and no step up (one offer, one question at a time).
pub fn with_places(m: &Message, places: Vec<String>) -> Option<Look> {
    if let Some(look) = &m.look {
        return Some(Look { places, ..look.clone() });
    }
    let quiet = m.status == Some(Status::Done) && m.nudge.is_none() && !m.wrap_up.as_ref().is_some_and(|w| w.ask.is_some());
    (quiet && !places.is_empty()).then(|| Look { reason: "look.sameText".into(), places })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Role, WrapUp};
    use serde_json::{Value, json};

    fn tool(name: &str, input: Value, ok: Option<bool>) -> ToolUse {
        ToolUse {
            id: format!("t{name}{input}"),
            name: name.into(),
            input,
            ok,
            output: None,
            declined: false,
            parent: None,
            sign: None,
            facts: None,
            told: vec![],
            words: vec![],
            words_cut: false,
            words_chars: None,
            check: false,
            started: None,
            ended: None,
        }
    }

    fn bash(command: &str, ok: bool) -> ToolUse {
        tool("Bash", json!({ "command": command }), Some(ok))
    }

    fn edit(path: &str) -> ToolUse {
        tool("Edit", json!({ "file_path": path }), Some(true))
    }

    fn turn(tools: Vec<ToolUse>) -> Message {
        let mut m = Message::new("a".into(), Role::Assistant, "done".into(), 0);
        m.status = Some(Status::Done);
        m.tools = Some(tools);
        m
    }

    fn why(tools: Vec<ToolUse>) -> Option<String> {
        of(&turn(tools)).map(|l| l.reason)
    }

    #[test]
    fn a_turn_that_went_from_red_to_green_offers_to_look_around() {
        assert_eq!(why(vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true)]).as_deref(), Some("look.fixed"));
        // Codex: a bare command line and a file change.
        let change = tool("fileChange", json!([{ "path": "app/main.py", "kind": "update" }]), Some(true));
        let codex = vec![
            tool("commandExecution", json!("pytest -q"), Some(false)),
            change,
            tool("commandExecution", json!("pytest -q"), Some(true)),
        ];
        assert_eq!(why(codex).as_deref(), Some("look.fixed"));
        // A configuration or test-data change that made the check pass is a fix too: the check judged it.
        assert_eq!(why(vec![bash("npm test", false), edit("package.json"), bash("npm test", true)]).as_deref(), Some("look.fixed"));
    }

    #[test]
    fn green_with_no_red_before_it_or_no_change_between_is_no_fix() {
        assert_eq!(why(vec![edit("src/a.rs"), bash("cargo test", true)]), None, "nothing failed");
        assert_eq!(why(vec![bash("cargo test", false), bash("cargo test", true)]), None, "a flaky rerun changed nothing");
        assert_eq!(
            why(vec![edit("src/a.rs"), bash("cargo test", false), bash("cargo test", true)]),
            None,
            "the change came before the failure"
        );
        assert_eq!(why(vec![]), None);
    }

    #[test]
    fn a_red_last_check_is_the_step_up_s_and_never_a_look_around() {
        assert_eq!(why(vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", false)]), None);
        assert_eq!(why(vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true), bash("cargo clippy", false)]), None);
    }

    #[test]
    fn every_command_that_failed_must_end_green_and_one_must_turn_green_after_a_change() {
        // A later failure that passes on a rerun keeps the fix seen before it.
        let tools = vec![
            bash("cargo test", false),
            edit("src/a.rs"),
            bash("cargo test", true),
            bash("cargo clippy", false),
            bash("cargo clippy", true),
        ];
        assert_eq!(why(tools).as_deref(), Some("look.fixed"));
        // A test still red is no fix, even when another check passed last.
        assert_eq!(why(vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", false), bash("cargo clippy", true)]), None);
        // A failure rerun narrower stays red: nothing is offered rather than a guess.
        assert_eq!(why(vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test --lib", true)]), None);
        // Green after a change, but the command that went green never failed: only a flaky one passed again.
        assert_eq!(why(vec![bash("npm test", false), bash("npm test", true), edit("src/a.ts"), bash("npm run lint", true)]), None);
    }

    #[test]
    fn code_changed_after_the_last_check_is_unchecked_so_nothing_is_offered() {
        let tools = vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true), edit("src/b.rs")];
        assert_eq!(why(tools), None);
        // Notes written after the check are not code, as for the step up.
        let tools = vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true), edit("docs/notes.md")];
        assert_eq!(why(tools).as_deref(), Some("look.fixed"));
    }

    #[test]
    fn refused_edits_declined_checks_and_commands_that_are_not_checks_do_not_count() {
        let refused = tool("Edit", json!({ "file_path": "src/a.rs" }), Some(false));
        assert_eq!(why(vec![bash("cargo test", false), refused, bash("cargo test", true)]), None);
        let declined = ToolUse { declined: true, ..bash("cargo test", false) };
        assert_eq!(why(vec![declined, edit("src/a.rs"), bash("cargo test", true)]), None, "a declined check never ran");
        assert_eq!(
            why(vec![bash("rg todo src", false), edit("src/a.rs"), bash("cargo test", true)]),
            None,
            "a search that found nothing failed nothing"
        );
        // A check declined after the green one leaves the green one last.
        let declined = ToolUse { declined: true, ..bash("cargo test", false) };
        let tools = vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true), declined];
        assert_eq!(why(tools).as_deref(), Some("look.fixed"));
    }

    #[test]
    fn a_stopped_turn_and_a_closing_box_that_already_asks_offer_nothing() {
        let fixed = || vec![bash("cargo test", false), edit("src/a.rs"), bash("cargo test", true)];
        let mut m = turn(fixed());
        m.status = Some(Status::Stopped);
        assert_eq!(of(&m), None);
        let mut m = turn(fixed());
        m.wrap_up = Some(WrapUp {
            recap: "Fixed. The same check is missing in 2 more places.".into(),
            ask: Some("Fix them too?".into()),
            actions: vec!["Yes".into(), "Not now".into()],
            done: false,
            answered: None,
            ..Default::default()
        });
        assert_eq!(of(&m), None, "one question at a time");
        m.wrap_up = Some(WrapUp { recap: "Fixed.".into(), ask: None, actions: vec![], done: true, answered: None, ..Default::default() });
        assert_eq!(of(&m).map(|l| l.reason).as_deref(), Some("look.fixed"), "a box with no question leaves room for the offer");
    }

    fn edit_of(path: &str, old: &str, new: &str) -> ToolUse {
        tool("Edit", json!({ "file_path": path, "old_string": old, "new_string": new }), Some(true))
    }

    #[test]
    fn the_text_a_change_removed_is_its_lines_the_change_did_not_keep() {
        let m = turn(vec![edit_of("src/a.rs", "let d = parse(s).unwrap();\nlet n = 1;", "let d = parse(s)?;\nlet n = 1;")]);
        assert_eq!(removed(&m), vec!["let d = parse(s).unwrap();"], "a kept line and a short one are not looked for");
        // Codex: the `-` lines of a diff, never its `---` header.
        let diff = "--- a/app.py\n+++ b/app.py\n-total = price * qty  # no tax\n+total = price * qty * (1 + tax)";
        let codex = turn(vec![tool("fileChange", json!([{ "path": "app.py", "kind": { "type": "update" }, "diff": diff }]), Some(true))]);
        assert_eq!(removed(&codex), vec!["total = price * qty  # no tax"]);
        let multi = tool(
            "MultiEdit",
            json!({ "file_path": "a.md", "edits": [{ "old_string": "The colour of the sky", "new_string": "The color of the sky" }] }),
            Some(true),
        );
        assert_eq!(removed(&turn(vec![multi])), vec!["The colour of the sky"]);
    }

    #[test]
    fn refused_edits_moved_text_rewrites_and_punctuation_are_not_looked_for() {
        let refused = ToolUse { ok: Some(false), ..edit_of("a.rs", "let d = parse(s).unwrap();", "") };
        assert!(removed(&turn(vec![refused])).is_empty());
        let moved = vec![
            edit_of("a.rs", "fn helper(x: u32) -> u32 { x }", ""),
            tool("Write", json!({ "file_path": "b.rs", "content": "fn helper(x: u32) -> u32 { x }\n" }), Some(true)),
        ];
        assert!(removed(&turn(moved)).is_empty(), "text moved to another file was not removed");
        let rewrite: String = (0..11).map(|i| format!("let value_number_{i} = compute({i});\n")).collect();
        assert!(removed(&turn(vec![edit_of("a.rs", &rewrite, "")])).is_empty(), "a rewrite is not a mistake to look for");
        assert!(removed(&turn(vec![edit_of("a.rs", "}}}}}}}}}}}}}}}}", "")])).is_empty());
    }

    #[test]
    fn the_search_finds_the_same_text_in_other_files_and_skips_what_says_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let write = |path: &str, text: &str| {
            let file = dir.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        let bad = "let d = parse(s).unwrap();";
        write("src/a.rs", "fn a() {\n    let d = parse(s)?;\n}\n");
        write("src/b.rs", &format!("fn b() {{\n    {bad}\n}}\n"));
        write("src/deep/c.rs", &format!("// x\n\n    {bad} // again\n"));
        write("node_modules/x/index.js", bad);
        write(".git/HEAD", bad);
        write("target/debug/out.txt", bad);
        write("blob.bin", &format!("\0{bad}"));
        let places = search(dir.path(), &[bad.to_string()], Limits::AFTER_TURN);
        assert_eq!(places, vec!["src/b.rs:2", "src/deep/c.rs:3"]);
        // A text in more places than a mistake spreads to is a common line and is dropped.
        for i in 0..9 {
            write(&format!("common/{i}.rs"), "use std::collections::HashMap;\n");
        }
        assert!(search(dir.path(), &["use std::collections::HashMap;".to_string()], Limits::AFTER_TURN).is_empty());
        // The bounds keep what was found before them.
        let one = Limits { files: 1, ..Limits::AFTER_TURN };
        assert!(search(dir.path(), &[bad.to_string()], one).len() <= 1);
        assert!(search(dir.path(), &[], Limits::AFTER_TURN).is_empty());
    }

    /// Folders count against the bounds too: a tree of empty folders ends at the names limit, and out of time at once.
    #[test]
    fn the_search_stops_in_a_tree_of_empty_folders() {
        let dir = tempfile::tempdir().unwrap();
        let bad = "let d = parse(s).unwrap();";
        for i in 0..30 {
            std::fs::create_dir_all(dir.path().join(format!("a{i:02}/empty"))).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("z")).unwrap();
        std::fs::write(dir.path().join("z/hit.rs"), bad).unwrap();
        let needles = [bad.to_string()];
        assert_eq!(search(dir.path(), &needles, Limits::AFTER_TURN), vec!["z/hit.rs:1"]);
        assert!(search(dir.path(), &needles, Limits { entries: 10, ..Limits::AFTER_TURN }).is_empty());
        assert!(search(dir.path(), &needles, Limits { time: std::time::Duration::ZERO, ..Limits::AFTER_TURN }).is_empty());
    }

    /// A pipe nobody writes to would block the read for ever: only plain files are opened.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_search_never_opens_a_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let bad = "let d = parse(s).unwrap();";
        rustix::fs::mkfifoat(rustix::fs::CWD, dir.path().join("pipe"), rustix::fs::Mode::RWXU).unwrap();
        std::fs::write(dir.path().join("b.rs"), bad).unwrap();
        assert_eq!(search(dir.path(), &[bad.to_string()], Limits::AFTER_TURN), vec!["b.rs:1"]);
    }

    #[test]
    fn places_join_the_red_to_green_offer_or_make_one_of_their_own_only_when_nothing_else_is_offered() {
        let places = vec!["src/b.rs:2".to_string()];
        let mut m = turn(vec![]);
        m.look = Some(Look { reason: "look.fixed".into(), places: vec![] });
        assert_eq!(with_places(&m, places.clone()), Some(Look { reason: "look.fixed".into(), places: places.clone() }));
        m.look = None;
        assert_eq!(with_places(&m, places.clone()).map(|l| l.reason).as_deref(), Some("look.sameText"));
        assert_eq!(with_places(&m, vec![]), None);
        m.nudge = Some(crate::nudge::Nudge {
            reason: "nudge.noCheck".into(),
            vendor: crate::model::Vendor::Claude,
            model: "m".into(),
            effort: "high".into(),
        });
        assert_eq!(with_places(&m, places.clone()), None, "never beside a step up");
        m.nudge = None;
        m.wrap_up = Some(WrapUp {
            recap: "Done.".into(),
            ask: Some("Ship it?".into()),
            actions: vec!["Yes".into()],
            done: false,
            answered: None,
            ..Default::default()
        });
        assert_eq!(with_places(&m, places.clone()), None, "one question at a time");
        m.wrap_up = None;
        m.status = Some(Status::Stopped);
        assert_eq!(with_places(&m, places), None);
    }

    /// The cost of the read after a turn, on Faber's own folder: `cargo test -p faber-engine --lib look::tests::measure -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_the_search_on_this_repository() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let needles = vec!["let d = parse(s).unwrap();".to_string(), "pub fn removed(m: &Message) -> Vec<String> {".to_string()];
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let places = search(root, &needles, Limits::AFTER_TURN);
            println!("look search: {:?}, {} places", start.elapsed(), places.len());
        }
    }
}
