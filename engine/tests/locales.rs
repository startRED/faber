//! Every text in `ui/locales/en.ts` is shown somewhere: the screen names it, the engine or the shell returns it as a key, or
//! the screen builds it from a fixed start (`look.${id}.name`, `format!("reason.pace.{level}")`). A text nothing shows still
//! has to be translated and reviewed in every language, so a key that loses its last use goes in the same commit.

use std::path::{Path, PathBuf};

fn repo(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path)
}

/// The text of every file under `dir` with one of `exts`, leaving out the folder `skip` (none when empty: every path ends
/// with the empty one).
fn sources(dir: &str, exts: &[&str], skip: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![repo(dir)];
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(&folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if skip.is_empty() || !path.ends_with(skip) {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|x| exts.iter().any(|e| x == *e)) {
                found.push(std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    found
}

/// The keys of the English texts: each entry starts a line, indented by two spaces and quoted either way.
fn keys(en: &str) -> Vec<String> {
    en.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("  ")?;
            let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
            let (key, after) = rest[1..].split_once(quote)?;
            after.starts_with(':').then(|| key.to_string())
        })
        .collect()
}

/// The fixed starts of keys built at run time: a template literal or a format string that holds a dot before its first hole.
fn built_starts(code: &[String]) -> Vec<String> {
    let mut starts = Vec::new();
    for text in code {
        for (quote, hole) in [('`', "${"), ('"', "{")] {
            for piece in text.split(quote).skip(1) {
                if let Some((start, _)) = piece.split_once(hole)
                    && start.contains('.')
                    && start.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
                {
                    starts.push(start.to_string());
                }
            }
        }
    }
    starts
}

#[test]
fn every_english_text_is_shown_somewhere() {
    let en = std::fs::read_to_string(repo("ui/locales/en.ts")).unwrap();
    let keys = keys(&en);
    assert!(keys.len() > 500, "the keys of ui/locales/en.ts were not found");
    let mut code = sources("ui", &["ts", "tsx"], "locales");
    code.extend(sources("engine/src", &["rs"], ""));
    code.extend(sources("src-tauri/src", &["rs"], ""));
    let starts = built_starts(&code);
    // "What's new" reads every `notes.<id>.<kind>` from the English file itself (`ui/notes.ts`): its keys are the list. In a
    // branch the id is still the placeholder `next` or `next-2` (R164) until `scripts/numbers.mjs` gives it its number.
    let listed = |key: &str| {
        key.strip_prefix("notes.").and_then(|rest| rest.split_once('.')).is_some_and(|(id, _)| {
            id.parse::<u32>().is_ok() || id == "next" || id.strip_prefix("next-").is_some_and(|n| n.parse::<u32>().is_ok())
        })
    };
    let unused: Vec<&String> = keys
        .iter()
        .filter(|key| {
            let quoted = [format!("'{key}'"), format!("\"{key}\""), format!("`{key}`")];
            !listed(key)
                && !code.iter().any(|text| quoted.iter().any(|q| text.contains(q.as_str())))
                && !starts.iter().any(|s| key.starts_with(s.as_str()))
        })
        .collect();
    assert!(unused.is_empty(), "texts in ui/locales/en.ts that nothing shows: {unused:?}");
}

#[test]
fn the_check_finds_keys_quoted_either_way_and_starts_built_at_run_time() {
    let en = "export const en = {\n  'a.one': 'A',\n  \"b.two\": \"B\",\n  'c': { one: 'x', other: 'y' },\n    'nested': 'no',\n};";
    assert_eq!(keys(en), ["a.one", "b.two", "c"]);
    let code = ["t(`look.${id}.name`)".to_string(), "format!(\"reason.pace.{level}\")".to_string(), "\"{n} running\"".to_string()];
    assert_eq!(built_starts(&code), ["look.", "reason.pace."]);
}
