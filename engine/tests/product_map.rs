//! `docs/product-map.md` is what agents read before they build, so it may not miss anything a person can set, open or
//! reach: every setting, every screen file, every shell command and every key the screen keeps in the browser. A new one
//! fails here until the map says what it is, where it lives and where a person changes it.

use faber_engine::model::{Experience, Permissions, Settings, Setup};
use std::path::{Path, PathBuf};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

/// A checkout with CRLF line endings reads the same as one with LF.
fn read(path: &str) -> String {
    std::fs::read_to_string(repo(path)).unwrap().replace("\r\n", "\n")
}

/// Settings fields as stored in `settings.json`. No `..Default::default()`: a new field stops this file from compiling
/// until it is listed here, and then the map has to name it.
fn settings_keys() -> Vec<String> {
    let every = Settings {
        appearance: Some(Default::default()),
        user_hooks: Some(true),
        permissions: Some(Permissions::Ask),
        notifications: Some(true),
        notification_sound: Some(true),
        language: Some(String::new()),
        setup: Some(Setup::Done),
        experience: Some(Experience::Beginner),
        tutorial_done: Some(true),
        user_name: Some(String::new()),
        keep_awake: Some(true),
        autostart: Some(true),
        web_search: Some(Default::default()),
        reasoning_summary: Some(Default::default()),
        verbosity: Some(Default::default()),
        grants_epoch: Some(String::new()),
        previews: Some(true),
    };
    serde_json::to_value(every).unwrap().as_object().unwrap().keys().cloned().collect()
}

/// The commands the screen may call, from `generate_handler!` in the shell; test-only commands are left out.
fn commands() -> Vec<String> {
    let main = read("src-tauri/src/main.rs");
    let list = main.split("generate_handler![").nth(1).expect("generate_handler! in src-tauri/src/main.rs");
    let mut names = Vec::new();
    let mut test_only = false;
    for line in list.lines().map(str::trim) {
        if line.starts_with(']') {
            break;
        }
        if line.starts_with("#[") {
            test_only = line.contains(r#"feature = "test-hooks""#) && !line.contains("not(");
            continue;
        }
        let name = line.trim_end_matches(',').rsplit("::").next().unwrap_or_default();
        // A form this reader does not know fails here instead of slipping a command past the map.
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
            "cannot read `{line}` in generate_handler!: keep one command per line and each attribute on one line"
        );
        if !name.is_empty() && !std::mem::take(&mut test_only) {
            names.push(name.to_string());
        }
    }
    names
}

/// Every `ui/*.tsx` file, and every `faber.*` browser storage key (or key prefix) written in `ui/`.
// ponytail: reads `ui/` alone, which holds every screen file today (only `locales/` sits below it); scan subfolders once
// screen code moves into one.
fn screen_files_and_keys() -> (Vec<String>, Vec<String>) {
    let (mut files, mut keys) = (Vec::new(), Vec::new());
    for entry in std::fs::read_dir(repo("ui")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        if name.ends_with(".tsx") {
            files.push(format!("ui/{name}"));
        }
        if name.ends_with(".ts") || name.ends_with(".tsx") {
            let text = read(&format!("ui/{name}"));
            for (at, _) in text.match_indices("faber.") {
                if at > 0 && matches!(text.as_bytes()[at - 1], b'\'' | b'"' | b'`') {
                    let key: String = text[at..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-').collect();
                    keys.push(key);
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    (files, keys)
}

#[derive(Clone, Copy, PartialEq)]
enum Cells {
    First,
    Rows,
    Anywhere,
}

/// The backticked names in one `## ` section of the map: in the first cell of its table rows, in all their cells, or
/// anywhere in it.
fn listed(map: &str, section: &str, cells: Cells) -> Vec<String> {
    let body = map.split(&format!("\n## {section}\n")).nth(1).unwrap_or_default();
    let body = body.split("\n## ").next().unwrap_or_default();
    let mut names = Vec::new();
    for line in body.lines().filter(|line| cells == Cells::Anywhere || line.starts_with("| ")) {
        let text = if cells == Cells::First { line.split('|').nth(1).unwrap_or_default() } else { line };
        names.extend(text.split('`').skip(1).step_by(2).map(str::to_string));
    }
    names
}

#[test]
fn the_product_map_names_every_setting_screen_command_and_stored_key() {
    let map = read("docs/product-map.md");
    let (files, keys) = screen_files_and_keys();
    let commands = commands();
    assert!(commands.len() > 10 && commands.contains(&"setup_check".to_string()), "command list not found: {commands:?}");
    let settings = settings_keys();
    let mut problems = Vec::new();
    if keys.iter().any(|key| key == "faber.") {
        problems.push(
            "a browser storage key is built from `faber.` alone: write its whole prefix, such as 'faber.place.', so the map can name it"
                .to_string(),
        );
    }
    // Settings and commands sit in tables by their exact names, so a row for one that is gone is caught too.
    for (names, section, cells) in [
        (&settings, "Every setting", Cells::First),
        (&files, "Screens and surfaces", Cells::Anywhere),
        (&commands, "Shell commands", Cells::Rows),
        (&keys, "Other stored state", Cells::Anywhere),
    ] {
        let entries = listed(&map, section, cells);
        // A prefix written in the code (`faber.choice.`) is listed with its variable part (`faber.choice.<conversationId>`).
        let found = |name: &str| entries.iter().any(|entry| entry == name || (name.ends_with('.') && entry.starts_with(name)));
        for name in names.iter().filter(|name| !found(name)) {
            problems.push(format!("`{name}` is new: before adding it under \"{section}\", check the map for something that already does the same; if it is really new, say what it is, where it lives and where a person changes it"));
        }
        // A row for something that is gone tells the next agent it still exists.
        for gone in entries.iter().filter(|entry| cells != Cells::Anywhere && !names.contains(entry)) {
            problems.push(format!("`{gone}` is listed under \"{section}\" but no longer exists: remove it"));
        }
    }
    assert!(problems.is_empty(), "docs/product-map.md is out of date:\n{}", problems.join("\n"));
}

/// The project map an agent asks for (`faber_engine::map`) finds in Faber's own code what this file checks the product
/// map against: the same commands, settings, screen files and browser storage keys.
#[test]
fn the_project_map_read_from_faber_s_code_finds_what_the_product_map_is_checked_against() {
    use faber_engine::map::{Kind, scan};
    use std::collections::BTreeSet;
    let items = scan(&repo(""));
    let of = |kind: Kind, keep: &dyn Fn(&str) -> bool| -> BTreeSet<String> {
        items.iter().filter(|i| i.kind == kind && keep(&i.file)).map(|i| i.name.clone()).collect()
    };
    let set = |names: Vec<String>| names.into_iter().collect::<BTreeSet<String>>();
    let (files, keys) = screen_files_and_keys();
    assert_eq!(of(Kind::Command, &|f| f == "src-tauri/src/main.rs"), set(commands()));
    assert_eq!(of(Kind::Setting, &|f| f.starts_with("engine/")), set(settings_keys()));
    assert_eq!(of(Kind::Screen, &|f| f.starts_with("ui/") && !f[3..].contains('/')), set(files));
    assert_eq!(of(Kind::Stored, &|f| f.starts_with("ui/")), set(keys));
    assert!(of(Kind::Text, &|f| f == "ui/locales/en.ts").contains("composer.send"));
}
