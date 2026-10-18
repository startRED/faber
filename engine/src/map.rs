//! Faber's project map (R146): what a project has, read from its code by small extractors, each entry tied to the file
//! and line it came from. Nothing in it is written by an AI. A project's agent gets a short index through the `map` tool
//! and looks details up by kind and words, so it extends what exists instead of building a second one.
//!
//! The extractors go language by language: Rust (exports, Tauri commands, a `Settings` struct), TypeScript and
//! JavaScript (exports, screen files, browser storage keys, the English locale's text keys) and `package.json` scripts.

use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;

/// The name both drivers give the call, whatever the CLI calls it.
pub const TOOL: &str = "Map";
/// The tool as Codex knows it, and as Faber's MCP server lists it to Claude Code.
pub const NAME: &str = "map";
/// The tool as Claude Code names it: `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__faber__map";

/// Folders that hold no source of the project's own: dependencies, builds and caches.
const SKIP: [&str; 9] = ["node_modules", "target", "dist", "dist-test", "build", "out", "vendor", "coverage", "__pycache__"];
/// A larger tree is read up to here, so one call stays well under a second.
const MOST_FILES: usize = 20_000;
/// Entries of any kind one scan visits, read files or not.
const MOST_ENTRIES: usize = 200_000;
/// Folders below the project's own.
const MOST_DEPTH: usize = 32;
/// A file this big is generated or data, not source.
const MOST_BYTES: u64 = 1 << 20;
/// Text one scan holds at once.
const MOST_TOTAL: u64 = 64 << 20;
/// Entries one scan keeps: far more than an index needs, and a few tens of MB at most.
const MOST_ITEMS: usize = 100_000;
/// Entries one lookup lists; the rest are counted.
const SHOWN: usize = 60;

const DESCRIPTION: &str = "Lists what this project has, read from its code: the commands its screen may call, its settings, \
the texts of its screens, the keys it keeps in browser storage, its screen files, its package scripts and the names its modules \
export, each with its file and line. Call it with nothing for a short index, then with a kind, words to look for, or both, for \
the entries. Look here before you add a setting, command, text or screen, so you extend what exists instead of adding a second \
way to do the same thing.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string", "enum": KINDS.map(Kind::word), "description": "Only entries of this kind." },
            "query": { "type": "string", "description": "Words that each entry's name or file must contain, in any case." }
        },
        "additionalProperties": false
    })
}

/// The tool as a Codex `dynamicTools` entry.
pub fn codex_tool() -> Value {
    json!({ "type": "function", "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// The tool as Faber's MCP server lists it to Claude Code.
pub fn mcp_tool() -> Value {
    json!({ "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Command,
    Setting,
    Text,
    Stored,
    Screen,
    Script,
    Export,
}

const KINDS: [Kind; 7] = [Kind::Command, Kind::Setting, Kind::Text, Kind::Stored, Kind::Screen, Kind::Script, Kind::Export];

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Command => "command",
            Kind::Setting => "setting",
            Kind::Text => "text",
            Kind::Stored => "stored",
            Kind::Screen => "screen",
            Kind::Script => "script",
            Kind::Export => "export",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Kind::Command => "commands the screen may call (Tauri)",
            Kind::Setting => "fields of the stored settings",
            Kind::Text => "text keys of the English locale",
            Kind::Stored => "keys kept in browser storage",
            Kind::Screen => "screen files",
            Kind::Script => "package scripts",
            Kind::Export => "names modules export",
        }
    }
}

/// One thing the project has, where it is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    /// Relative to the project's folder, with `/`.
    pub file: String,
    /// 1-based.
    pub line: usize,
}

/// Everything the extractors find in `root`, by kind, then file, then line. A storage key written in several places is
/// listed once, where it first appears.
pub fn scan(root: &Path) -> Vec<Item> {
    let mut files = Vec::new();
    walk(root, root, 0, &mut Budget::default(), &mut files);
    let prefixes: BTreeSet<String> = files
        .iter()
        .filter(|(rel, _)| script(rel) && !test_file(rel))
        .flat_map(|(_, text)| storage_calls(text))
        .filter_map(|(key, _)| prefix(&key))
        .collect();
    let mut items = Vec::new();
    for (rel, text) in &files {
        let mut push = |kind, name: String, line| {
            if items.len() < MOST_ITEMS {
                items.push(Item { kind, name, file: rel.clone(), line });
            }
        };
        if rel.ends_with(".rs") {
            if !test_file(rel) {
                rust(text, &mut push);
            }
        } else if script(rel) {
            scripts(rel, text, &prefixes, &mut push);
        } else if rel == "package.json" || rel.ends_with("/package.json") {
            package(text, &mut push);
        }
        if locale(rel) {
            for (name, line) in text_keys(text) {
                push(Kind::Text, name, line);
            }
        }
    }
    items.sort_by(|a, b| (a.kind, &a.file, a.line).cmp(&(b.kind, &b.file, b.line)));
    let mut seen = BTreeSet::new();
    items.retain(|i| i.kind != Kind::Stored || seen.insert(i.name.clone()));
    items
}

/// What one scan may still visit and read.
#[derive(Default)]
struct Budget {
    entries: usize,
    bytes: u64,
}

/// The project's files the extractors read, with their text: no hidden folder, no dependency or build folder, no link,
/// only plain files, within the budget's limits however many entries a folder holds.
// ponytail: a file swapped for a link between the type check and the read is read through it; the agent that asks is in
// full access and can read that file itself, so it learns nothing it could not, and the read is still capped.
fn walk(root: &Path, dir: &Path, depth: usize, budget: &mut Budget, out: &mut Vec<(String, String)>) {
    if depth > MOST_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().take(MOST_ENTRIES.saturating_sub(budget.entries)).collect();
    budget.entries += entries.len();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MOST_FILES || budget.bytes >= MOST_TOTAL {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue };
        if name.starts_with('.') || kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            if !SKIP.contains(&name.as_str()) {
                walk(root, &path, depth + 1, budget, out);
            }
            continue;
        }
        let read = name.ends_with(".rs") || name == "package.json" || script(&name) || locale_file(&name);
        if !kind.is_file() || !read {
            continue;
        }
        let Some(text) = read_capped(&path) else { continue };
        budget.bytes += text.len() as u64;
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        out.push((rel, text.replace("\r\n", "\n")));
    }
}

/// A plain text file of at most `MOST_BYTES`, however much it grows while it is read. It is opened without following a
/// link and, on Unix, without waiting (a pipe put in its place cannot hold the scan), then read only if it is a file.
fn read_capped(path: &Path) -> Option<String> {
    use std::io::Read;
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: a link is opened as itself, and is not a file.
        std::fs::OpenOptions::new().read(true).custom_flags(0x0020_0000).open(path).ok()?
    };
    #[cfg(unix)]
    let file = {
        use rustix::fs::{Mode, OFlags, open};
        std::fs::File::from(open(path, OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC, Mode::empty()).ok()?)
    };
    #[cfg(not(any(windows, unix)))]
    let file = std::fs::File::open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MOST_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MOST_BYTES {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn script(rel: &str) -> bool {
    [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".vue", ".svelte"].iter().any(|e| rel.ends_with(e)) && !rel.ends_with(".d.ts")
}

/// A check, a test or a story: what it names is not something the project has.
fn test_file(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    rel.starts_with("tests/") || rel.contains("/tests/") || [".test.", ".spec.", ".check.", ".stories."].iter().any(|m| name.contains(m))
}

fn locale_file(name: &str) -> bool {
    ["en.ts", "en.js", "en.json", "en-US.json", "en-us.json"].contains(&name)
}

/// The English locale, in a `locales`, `i18n`, `lang` or `translations` folder.
fn locale(rel: &str) -> bool {
    let mut parts = rel.rsplit('/');
    let name = parts.next().unwrap_or_default();
    locale_file(name) && parts.next().is_some_and(|dir| ["locales", "i18n", "lang", "translations"].contains(&dir))
}

/// The name an item starts with: letters, digits and `_`.
fn ident(text: &str) -> &str {
    let end = text.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(text.len());
    &text[..end]
}

fn camel(snake: &str) -> String {
    let mut out = String::new();
    for (i, part) in snake.split('_').enumerate() {
        let mut chars = part.chars();
        match chars.next() {
            Some(first) if i > 0 => out.extend(first.to_uppercase().chain(chars)),
            Some(first) => out.extend(std::iter::once(first).chain(chars)),
            None => {}
        }
    }
    out
}

fn rust(text: &str, push: &mut impl FnMut(Kind, String, usize)) {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        // Only items at the top of a file: an indented `pub fn` is a method.
        if let Some(rest) = line.strip_prefix("pub ") {
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            for keyword in ["fn ", "struct ", "enum ", "trait ", "const ", "type "] {
                if let Some(name) = rest.strip_prefix(keyword).map(ident).filter(|n| !n.is_empty()) {
                    push(Kind::Export, name.to_string(), i + 1);
                }
            }
        }
        let trimmed = line.trim();
        if trimmed.starts_with("pub struct Settings ") || trimmed.starts_with("struct Settings ") {
            settings(&lines, i, push);
        }
        if let Some(at) = line.find("generate_handler![") {
            commands(&lines, i, at + "generate_handler![".len(), push);
        }
    }
}

/// The fields of a `Settings` struct, as they are stored: camelCase when serde renames them so.
fn settings(lines: &[&str], at: usize, push: &mut impl FnMut(Kind, String, usize)) {
    let camel_case = lines[at.saturating_sub(4)..at].iter().any(|l| l.contains("rename_all = \"camelCase\""));
    // What the field's own `#[serde(...)]` says: the name it is stored under, or that it is never stored.
    let (mut renamed, mut skipped) = (None::<String>, false);
    for (i, line) in lines.iter().enumerate().skip(at + 1) {
        let trimmed = line.trim();
        if trimmed.starts_with('}') {
            return;
        }
        if let Some(args) = trimmed.strip_prefix("#[serde(") {
            for arg in args.trim_end_matches(")]").split(',').map(str::trim) {
                skipped |= arg == "skip" || arg == "skip_serializing";
                if let Some(value) = arg.strip_prefix("rename").map(str::trim_start).and_then(|r| r.strip_prefix('=')) {
                    renamed = Some(value.trim().trim_matches('"').to_string());
                }
            }
            continue;
        }
        let field = trimmed.strip_prefix("pub(crate) ").or_else(|| trimmed.strip_prefix("pub ")).unwrap_or(trimmed);
        let name = ident(field);
        if !name.is_empty() && field[name.len()..].trim_start().starts_with(':') {
            let stored = renamed.take().unwrap_or_else(|| if camel_case { camel(name) } else { name.to_string() });
            if !std::mem::take(&mut skipped) {
                push(Kind::Setting, stored, i + 1);
            }
        }
    }
}

/// The commands in `generate_handler![...]`, from `start` on line `at`. One behind a test-only `#[cfg(...)]` is left out.
fn commands(lines: &[&str], at: usize, start: usize, push: &mut impl FnMut(Kind, String, usize)) {
    let mut test_only = false;
    for (i, line) in lines.iter().enumerate().skip(at) {
        let line = if i == at { &line[start..] } else { line };
        // An attribute holds a `]` of its own, so it is read before the list's end is looked for.
        if line.trim().starts_with("#[") {
            test_only = line.contains("test") && !line.contains("not(");
            continue;
        }
        let (line, last) = match line.find(']') {
            Some(end) => (&line[..end], true),
            None => (line, false),
        };
        for piece in line.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let name = piece.rsplit("::").next().unwrap_or(piece);
            if !std::mem::take(&mut test_only) && !name.is_empty() && ident(name) == name {
                push(Kind::Command, name.to_string(), i + 1);
            }
        }
        if last {
            return;
        }
    }
}

fn scripts(rel: &str, text: &str, prefixes: &BTreeSet<String>, push: &mut impl FnMut(Kind, String, usize)) {
    if test_file(rel) {
        return;
    }
    if [".tsx", ".jsx", ".vue", ".svelte"].iter().any(|e| rel.ends_with(e)) {
        push(Kind::Screen, rel.to_string(), 1);
    }
    for (i, line) in text.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("export ") {
            let rest = rest.strip_prefix("default ").unwrap_or(rest);
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            for keyword in
                ["function* ", "function ", "const ", "let ", "var ", "class ", "abstract class ", "interface ", "type ", "enum "]
            {
                if let Some(name) = rest.strip_prefix(keyword).map(ident).filter(|n| !n.is_empty()) {
                    push(Kind::Export, name.to_string(), i + 1);
                    break;
                }
            }
        }
    }
    // A key used straight in a storage call, and every literal that starts like the keys the project builds.
    for (key, line) in storage_calls(text) {
        push(Kind::Stored, key, line);
    }
    for (i, line) in text.lines().enumerate() {
        for prefix in prefixes {
            for (at, _) in line.match_indices(prefix.as_str()) {
                if at > 0 && matches!(line.as_bytes()[at - 1], b'\'' | b'"' | b'`') {
                    let key: String = line[at..].chars().take_while(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
                    // Only a whole literal, or the literal start of a template: never the cut front of a longer key.
                    if matches!(line[at + key.len()..].chars().next(), Some('\'' | '"' | '`' | '$')) {
                        push(Kind::Stored, key, i + 1);
                    }
                }
            }
        }
    }
}

/// The literal first argument of each `localStorage` or `sessionStorage` call, and its line. A key built at run time
/// keeps only its literal start (`'faber.choice.' + id` gives `faber.choice.`).
fn storage_calls(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        for call in ["Storage.getItem(", "Storage.setItem(", "Storage.removeItem("] {
            for (at, _) in line.match_indices(call) {
                let rest = line[at + call.len()..].trim_start();
                let Some(quote) = rest.chars().next().filter(|c| matches!(c, '\'' | '"' | '`')) else { continue };
                let key: String = rest[1..].chars().take_while(|&c| c != quote && c != '$').collect();
                if !key.is_empty() {
                    out.push((key, i + 1));
                }
            }
        }
    }
    out
}

/// The part of a key before and with its first dot: the project's own key prefix (`faber.`).
fn prefix(key: &str) -> Option<String> {
    key.find('.').filter(|&at| at > 0).map(|at| key[..=at].to_string())
}

/// The quoted keys of a flat locale (`'composer.send': 'Send',` or `"composer.send": "Send"`).
// ponytail: flat keys only, which is how Faber and most Vite apps write them; a nested JSON locale lists its top keys.
fn text_keys(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(quote) = trimmed.chars().next().filter(|c| matches!(c, '\'' | '"')) else { continue };
        let Some(end) = trimmed[1..].find(quote) else { continue };
        if trimmed[1 + end + 1..].trim_start().starts_with(':') {
            out.push((trimmed[1..1 + end].to_string(), i + 1));
        }
    }
    out
}

fn package(text: &str, push: &mut impl FnMut(Kind, String, usize)) {
    let Ok(json) = serde_json::from_str::<Value>(text) else { return };
    let Some(scripts) = json["scripts"].as_object() else { return };
    // Each script's line is looked for inside the `scripts` object, so a key of the same name above it is not taken.
    let start = text.lines().position(|l| l.contains("\"scripts\"")).unwrap_or(0);
    for name in scripts.keys() {
        let key = format!("\"{name}\"");
        let at = text.lines().skip(start).position(|l| l.trim_start().strip_prefix(&key).is_some_and(|r| r.trim_start().starts_with(':')));
        push(Kind::Script, format!("npm run {name}"), at.map_or(start + 1, |i| start + i + 1));
    }
}

/// What the agent reads for one call: the index with no arguments, or the entries a kind and words pick.
pub fn answer(items: &[Item], input: &Value) -> (bool, String) {
    let kind = match input["kind"].as_str().filter(|k| !k.is_empty()) {
        Some(word) => match KINDS.into_iter().find(|k| k.word() == word) {
            Some(kind) => Some(kind),
            None => {
                let words: Vec<&str> = KINDS.iter().map(|k| k.word()).collect();
                return (false, format!("There is no kind \"{word}\". The kinds are: {}.", words.join(", ")));
            }
        },
        None => None,
    };
    let words: Vec<String> = input["query"].as_str().unwrap_or_default().split_whitespace().map(str::to_lowercase).collect();
    if kind.is_none() && words.is_empty() {
        return (true, index(items));
    }
    let found: Vec<&Item> = items
        .iter()
        .filter(|i| kind.is_none_or(|k| i.kind == k))
        .filter(|i| {
            let hay = format!("{} {}", i.name, i.file).to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    if found.is_empty() {
        return (true, "Nothing in the map matches. Call it with nothing to see what it holds.".into());
    }
    let mut out: Vec<String> = found.iter().take(SHOWN).map(|i| format!("{} {} — {}:{}", i.kind.word(), i.name, i.file, i.line)).collect();
    if found.len() > SHOWN {
        out.push(format!("({} more: add words to narrow it)", found.len() - SHOWN));
    }
    (true, out.join("\n"))
}

/// Each kind with its count and the files that hold the most of it.
fn index(items: &[Item]) -> String {
    let mut out = vec![
        "The project map, read from the code of this folder. Call it again with a kind, words, or both, for \
        the entries with their files and lines."
            .to_string(),
    ];
    for kind in KINDS {
        let of: Vec<&Item> = items.iter().filter(|i| i.kind == kind).collect();
        if of.is_empty() {
            continue;
        }
        let mut files: Vec<(&str, usize)> = Vec::new();
        for item in &of {
            match files.iter_mut().find(|(f, _)| *f == item.file) {
                Some((_, n)) => *n += 1,
                None => files.push((&item.file, 1)),
            }
        }
        files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let mut top: Vec<&str> = files.iter().take(3).map(|(f, _)| *f).collect();
        let more = files.len().saturating_sub(3);
        let more = if more > 0 { format!(" and {more} more files") } else { String::new() };
        if kind == Kind::Screen {
            top.clear();
        }
        let place = if top.is_empty() { String::new() } else { format!(", in {}{more}", top.join(", ")) };
        out.push(format!("- {} ({}): {}{place}", kind.word(), of.len(), kind.about()));
    }
    if out.len() == 1 {
        return "Faber found nothing it can read in this folder: it reads Rust, TypeScript, JavaScript and package.json.".into();
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let path = dir.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    fn names(items: &[Item], kind: Kind) -> Vec<String> {
        items.iter().filter(|i| i.kind == kind).map(|i| i.name.clone()).collect()
    }

    #[test]
    fn rust_gives_its_exports_its_commands_but_test_only_ones_and_its_stored_settings_names() {
        let main = "pub fn run() {}\npub async fn serve() {}\nimpl A {\n    pub fn method() {}\n}\nfn main() {\n    tauri::generate_handler![\n        a::open,\n        #[cfg(feature = \"test-hooks\")]\n        probe,\n        #[cfg(not(feature = \"test-hooks\"))]\n        close, quit\n    ];\n}\n";
        let model = "#[derive(Serialize)]\n#[serde(rename_all = \"camelCase\")]\npub struct Settings {\n    /// A comment.\n    #[serde(default)]\n    pub user_hooks: Option<bool>,\n    pub keep_awake: bool,\n    #[serde(rename = \"theme\")]\n    pub appearance: String,\n    #[serde(skip)]\n    pub cache: u8,\n}\n";
        let dir = tree(&[
            ("src/main.rs", main),
            ("src/model.rs", model),
            ("target/debug/gen.rs", "pub fn generated() {}"),
            ("tests/example.rs", "pub fn test_helper() {}\n"),
        ]);
        let items = scan(dir.path());
        assert_eq!(names(&items, Kind::Command), ["open", "close", "quit"]);
        assert_eq!(names(&items, Kind::Setting), ["userHooks", "keepAwake", "theme"], "serde's own rename and skip are kept");
        assert_eq!(names(&items, Kind::Export), ["run", "serve", "Settings"]);
        let open = items.iter().find(|i| i.name == "open").unwrap();
        assert_eq!((open.file.as_str(), open.line), ("src/main.rs", 8));
        let one_line = scan(tree(&[("lib.rs", "x(generate_handler![a, b::c]);\n")]).path());
        assert_eq!(names(&one_line, Kind::Command), ["a", "c"]);
    }

    #[test]
    fn scripts_give_screens_exports_storage_keys_and_the_english_locale_s_texts() {
        let app = "import x from 'y';\nexport function App() {}\nexport default async function main() {}\nexport const KEY = 'app.layout';\nlocalStorage.removeItem('app.choice.' + id);\nlocalStorage.getItem(`app.kept.${id}`);\nsessionStorage.setItem(\"plain\", '1');\nlocalStorage.getItem('app.user:theme');\nconst a = 'app.layout';\nconst b = 'apps.other';\n";
        let en = "export const en = {\n  'composer.send': 'Send',\n  \"notes.1.new\": 'It: new',\n} as const;\n";
        let dir = tree(&[
            ("ui/App.tsx", app),
            ("ui/locales/en.ts", en),
            ("ui/locales/pt-BR.ts", "export const pt = {\n  'composer.send': 'Enviar',\n};\n"),
            ("tests/ui/a.check.ts", "localStorage.setItem('app.only.in.a.check', '1');\n"),
            ("ui/Panel.test.tsx", "export function T() {}\n"),
            ("node_modules/x/index.js", "export function dep() {}\n"),
            ("types.d.ts", "export type T = 1;\n"),
        ]);
        let items = scan(dir.path());
        assert_eq!(names(&items, Kind::Screen), ["ui/App.tsx"]);
        assert_eq!(names(&items, Kind::Stored), ["app.layout", "app.choice.", "app.kept.", "plain", "app.user:theme"]);
        assert_eq!(names(&items, Kind::Text), ["composer.send", "notes.1.new"]);
        assert_eq!(names(&items, Kind::Export), ["App", "main", "KEY", "en", "pt"], "a test file exports nothing the project has");
        let layout = items.iter().find(|i| i.name == "app.layout").unwrap();
        assert_eq!(layout.line, 4, "a key written in several places is listed once, where it first appears");
    }

    #[test]
    fn package_json_gives_its_scripts_and_a_hidden_folder_is_never_read() {
        let package = "{\n  \"name\": \"x\",\n  \"lint\": \"metadata\",\n  \"scripts\": {\n    \"build\": \"vite build\",\n    \"lint\" : \"oxlint\"\n  }\n}\n";
        let dir = tree(&[("package.json", package), (".git/hooks/a.js", "export function hook() {}\n"), ("web/package.json", "not json")]);
        let items = scan(dir.path());
        assert_eq!(names(&items, Kind::Script), ["npm run build", "npm run lint"]);
        assert_eq!(items.iter().find(|i| i.name == "npm run lint").unwrap().line, 6, "inside scripts, with a space before the colon");
        assert!(names(&items, Kind::Export).is_empty());
        let deep = format!("{}/a.ts", vec!["d"; MOST_DEPTH + 2].join("/"));
        let items = scan(
            tree(&[
                (
                    &deep,
                    "export function lost() {}
",
                ),
                (
                    "x/a.ts",
                    "export function kept() {}
",
                ),
            ])
            .path(),
        );
        assert_eq!(names(&items, Kind::Export), ["kept"], "a folder deeper than the limit is not read");
    }

    #[test]
    fn the_index_counts_each_kind_and_a_lookup_lists_entries_with_their_place() {
        let dir = tree(&[
            ("src/main.rs", "fn main() {\n    generate_handler![open_file, close_file];\n}\npub fn helper() {}\n"),
            ("ui/locales/en.ts", "export const en = {\n  'file.open': 'Open',\n};\n"),
        ]);
        let items = scan(dir.path());
        let (ok, index) = answer(&items, &json!({}));
        assert!(ok);
        assert!(index.contains("- command (2): commands the screen may call (Tauri), in src/main.rs"), "{index}");
        assert!(index.contains("- text (1)") && index.contains("- export (2)"), "{index}");
        assert!(!index.contains("setting"), "a kind with nothing is left out: {index}");
        let (_, found) = answer(&items, &json!({"kind": "command", "query": "OPEN"}));
        assert_eq!(found, "command open_file — src/main.rs:2");
        let (_, both) = answer(&items, &json!({"query": "open"}));
        assert_eq!(both.lines().count(), 2, "{both}");
        let (ok, wrong) = answer(&items, &json!({"kind": "route"}));
        assert!(!ok && wrong.contains("command, setting"), "{wrong}");
        let (_, none) = answer(&items, &json!({"query": "zzz"}));
        assert!(none.starts_with("Nothing in the map matches"));
        let (_, empty) = answer(&[], &json!({}));
        assert!(empty.starts_with("Faber found nothing"));
    }
}
