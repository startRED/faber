//! What a subagent did, as facts (R161 step 4): the files it changed, the checks it ran and how they ended, and the files
//! it changed that its brief never named. The main agent grading its own helper is not trusted, so the row shows these
//! instead of a verdict. Pure: counted from the steps of the reply when the subagent's call ends.

use crate::model::ToolUse;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Facts {
    /// Each file it changed, once, in the order it first changed it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edited: Vec<String>,
    /// The checks it ran (tests, lints, builds: `nudge::is_check`), and how many of them failed.
    #[serde(default)]
    pub checks: u32,
    #[serde(default)]
    pub failed: u32,
    /// The changed files whose name the brief never mentions. Empty when the CLI gives no brief (Codex names its subagents
    /// only), since nothing can be compared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unnamed: Vec<String>,
}

/// The brief the agent wrote for the subagent: Claude Code's `prompt`, a helper's `brief`.
fn brief(call: &ToolUse) -> &str {
    call.input["prompt"].as_str().or(call.input["brief"].as_str()).unwrap_or_default()
}

/// Whether the brief names a file, as a whole word in any case and either slash: its file name, or the end of its path
/// (`src/a.rs` names `C:\repo\src\a.rs`). `data.rs` and `a.rs.bak` do not name `a.rs`.
fn names(brief: &str, path: &str) -> bool {
    let path = path.to_lowercase().replace('\\', "/");
    let file = path.rsplit('/').next().unwrap_or(&path).to_string();
    let brief = brief.to_lowercase().replace('\\', "/");
    let part = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | '.');
    // A full stop after a name is not part of it; the dot that starts a dotfile is.
    brief
        .split(|c: char| !part(c))
        .map(|word| word.trim_matches('/').trim_end_matches('.'))
        .any(|word| !word.is_empty() && (word == file || (word.contains('/') && (path == word || path.ends_with(&format!("/{word}"))))))
}

/// The facts of a subagent's call from the reply's steps, or None for a step that does not start a subagent.
pub fn of(tools: &[ToolUse], call: &ToolUse) -> Option<Facts> {
    if call.parent.is_some() || !matches!(call.name.as_str(), "Agent" | "Task" | "Helper") {
        return None;
    }
    let mut facts = Facts::default();
    for tool in tools.iter().filter(|t| crate::stuck::root(tools, t) == Some(call.id.as_str())) {
        if tool.declined {
            continue;
        }
        // Only a change that ended well changed something: a failed one did not, and one still open never said.
        for path in crate::nudge::edited(tool).into_iter().filter(|_| tool.ok == Some(true)) {
            if !facts.edited.iter().any(|p| p == path) {
                facts.edited.push(path.to_string());
            }
        }
        if crate::nudge::is_check(tool) && tool.ok.is_some() {
            facts.checks += 1;
            facts.failed += u32::from(tool.ok == Some(false));
        }
    }
    let brief = brief(call);
    if !brief.trim().is_empty() {
        facts.unnamed = facts.edited.iter().filter(|p| !names(brief, p)).cloned().collect();
    }
    Some(facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn tool(id: &str, name: &str, input: Value, parent: Option<&str>, ok: Option<bool>) -> ToolUse {
        ToolUse {
            id: id.into(),
            name: name.into(),
            input,
            ok,
            output: None,
            declined: false,
            parent: parent.map(String::from),
            sign: None,
            facts: None,
            told: vec![],
            check: false,
            started: None,
            ended: None,
        }
    }

    #[test]
    fn a_subagent_counts_its_changed_files_its_checks_and_the_files_its_brief_never_named() {
        let call = tool("a", "Agent", json!({ "prompt": "Fix the parser in src/parser.rs and run the tests." }), None, Some(true));
        let tools = vec![
            call.clone(),
            tool("e1", "Edit", json!({ "file_path": "src/parser.rs" }), Some("a"), Some(true)),
            tool("e2", "Edit", json!({ "file_path": "src/Parser.rs" }), Some("a"), Some(true)),
            tool("e3", "Write", json!({ "file_path": "C:\\repo\\build.rs" }), Some("a"), Some(true)),
            tool("e1b", "Edit", json!({ "file_path": "src/parser.rs" }), Some("a"), Some(true)),
            tool("t1", "Bash", json!({ "command": "cargo test" }), Some("a"), Some(false)),
            tool("t2", "Bash", json!({ "command": "cargo test" }), Some("a"), Some(true)),
            tool("l", "Bash", json!({ "command": "ls" }), Some("a"), Some(true)),
            tool("own", "Edit", json!({ "file_path": "README.md" }), None, Some(true)),
        ];
        let facts = of(&tools, &call).unwrap();
        assert_eq!(facts.edited, ["src/parser.rs", "src/Parser.rs", "C:\\repo\\build.rs"]);
        assert_eq!((facts.checks, facts.failed), (2, 1), "ls is no check, and the agent's own edit is not the subagent's");
        assert_eq!(facts.unnamed, ["C:\\repo\\build.rs"], "a file named in the brief in another case counts as named");
    }

    #[test]
    fn a_name_counts_only_as_a_whole_word_and_only_a_change_that_ended_well_counts() {
        assert!(names("Fix src/a.rs.", "src/a.rs"));
        assert!(names("look at a.rs, then", "C:\\repo\\src\\A.rs"));
        assert!(names("in (src/a.rs)", "C:/repo/src/a.rs"));
        assert!(!names("Fix data.rs", "src/a.rs"), "data.rs does not name a.rs");
        assert!(!names("see a.rs.bak", "a.rs"));
        assert!(!names("see my-a.rs", "a.rs"));
        assert!(names("Edit .gitignore.", ".gitignore"), "a dotfile keeps its dot");
        assert!(!names("see lib/a.rs", "src/a.rs.x"));
        let call = tool("a", "Agent", json!({ "prompt": "x" }), None, Some(true));
        let open = tool("o", "Edit", json!({ "file_path": "open.rs" }), Some("a"), None);
        let failed = tool("f", "Edit", json!({ "file_path": "failed.rs" }), Some("a"), Some(false));
        let mut declined = tool("d", "Edit", json!({ "file_path": "declined.rs" }), Some("a"), Some(false));
        declined.declined = true;
        let facts = of(&[call.clone(), open, failed, declined], &call).unwrap();
        assert!(facts.edited.is_empty(), "{facts:?}");
    }

    #[test]
    fn a_codex_subagent_with_no_brief_names_nothing_and_a_plain_step_has_no_facts() {
        let call = tool("c", "Agent", json!({ "description": "read docs" }), None, Some(true));
        let tools = vec![call.clone(), tool("f", "fileChange", json!([{ "path": "docs/a.md" }]), Some("c"), Some(true))];
        let facts = of(&tools, &call).unwrap();
        assert_eq!(facts.edited, ["docs/a.md"]);
        assert!(facts.unnamed.is_empty());
        assert_eq!(of(&tools, &tools[1]), None);
    }
}
