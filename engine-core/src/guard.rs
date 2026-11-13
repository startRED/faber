//! What Faber notices in a request's own edits, read from the edit steps' inputs and the commands the agent ran, with no
//! process or model call: a test turned off or weakened, a test file deleted, a stub left in code, files changed outside the
//! plan. These are facts shown with a done conclusion (`crate::proof`), never a block: a person may have asked for any of them.

use serde::{Deserialize, Serialize};

use crate::model::{Plan, ToolUse};

/// One thing Faber noticed. `file` is the path as the agent's tool named it, shortened to the project for the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Fact {
    /// A test was turned off (a skip or ignore mark) or the others were turned off by focusing one (`only`).
    Skipped { file: String },
    /// A test file lost assertions: more removed than added.
    Weakened { file: String, removed: u32 },
    /// A test file was deleted.
    Deleted { file: String },
    /// Code that still says it is not written (`todo!()`, `NotImplementedError`, "not implemented").
    Stub { file: String },
    /// Files changed that no plan step named and the request did not mention.
    Outside { files: Vec<String>, count: u32 },
}

impl Fact {
    /// The file the fact is about, for "the agent already explained it" (`crate::proof`).
    pub fn files(&self) -> Vec<&str> {
        match self {
            Fact::Skipped { file } | Fact::Weakened { file, .. } | Fact::Deleted { file } | Fact::Stub { file } => vec![file],
            Fact::Outside { files, .. } => files.iter().map(String::as_str).collect(),
        }
    }

    /// The fact in the words the agent reads with a refused conclusion.
    pub fn told(&self) -> String {
        match self {
            Fact::Skipped { file } => format!("a test was turned off in {file}"),
            Fact::Weakened { file, removed } => format!("{file} lost {removed} assertions"),
            Fact::Deleted { file } => format!("the test file {file} was deleted"),
            Fact::Stub { file } => format!("{file} still has code marked not implemented"),
            Fact::Outside { files, count } => format!("{count} changed files are outside the plan and the request: {}", files.join(", ")),
        }
    }
}

/// At most this many files are named in one `Outside` fact, and this many facts are kept.
const NAMED: usize = 5;
const KEPT: usize = 8;

/// A test file by the usual conventions of JavaScript, Python, Go, Rust, Java and Ruby.
pub fn is_test(path: &str) -> bool {
    let path = path.replace('\\', "/").to_lowercase();
    let name = path.rsplit('/').next().unwrap_or(&path);
    let stem = name.split('.').next().unwrap_or(name);
    ["/test/", "/tests/", "/__tests__/", "/spec/", "/e2e/"].iter().any(|d| format!("/{path}").contains(d))
        || [".test.", ".spec.", "_test.", "_tests.", "_spec."].iter().any(|m| name.contains(m))
        || stem.starts_with("test_")
        || stem == "tests"
        || stem.ends_with("test") && name.ends_with(".java")
}

/// A mark that turns a test off, or turns the others off by running one alone.
fn skips(line: &str) -> bool {
    let l = line.trim().to_lowercase();
    const MARKS: [&str; 15] = [
        "#[ignore",
        "it.skip(",
        "test.skip(",
        "describe.skip(",
        "context.skip(",
        "it.only(",
        "test.only(",
        "describe.only(",
        "@pytest.mark.skip",
        "@unittest.skip",
        "pytest.skip(",
        "@disabled",
        "@ignore",
        "t.skip(",
        "this.skip()",
    ];
    MARKS.iter().any(|m| l.starts_with(m) || l.contains(&format!(" {m}")) || l.contains(&format!("({m}")))
        || ["xit(", "xdescribe(", "xtest("].iter().any(|m| l.starts_with(m))
}

fn asserts(line: &str) -> bool {
    let l = line.to_lowercase();
    ["assert", "expect(", ".should", "t.error", "t.fatal", "require.", "tobe(", "toequal("].iter().any(|m| l.contains(m))
}

fn stub(line: &str) -> bool {
    let l = line.to_lowercase();
    ["todo!(", "unimplemented!(", "notimplementederror", "not implemented", "notimplementedexception"].iter().any(|m| l.contains(m))
}

/// The edit tools' changed lines per file, in order: Claude Code's Edit, MultiEdit and Write, Codex's file changes.
fn changed_lines(tool: &ToolUse) -> Vec<(String, Vec<crate::change::ChangedLine>)> {
    let paths = crate::nudge::edited(tool);
    if paths.len() == 1 {
        return crate::change::of(&tool.name, &tool.input).map(|c| vec![(paths[0].to_string(), c.lines)]).unwrap_or_default();
    }
    // Codex: one change per path, each read on its own.
    let one =
        |path: &str, v: &serde_json::Value| crate::change::of("fileChange", &serde_json::json!([v])).map(|c| (path.to_string(), c.lines));
    match &tool.input {
        serde_json::Value::Array(changes) => changes.iter().filter_map(|c| one(c["path"].as_str()?, c)).collect(),
        serde_json::Value::Object(changes) => changes.iter().filter_map(|(path, v)| one(path, v)).collect(),
        _ => vec![],
    }
}

/// Test paths a command deletes: `rm`, `del`, `git rm`, `Remove-Item`, `unlink`.
pub(crate) fn deleted_by_command(tool: &ToolUse) -> Vec<String> {
    let Some(line) = crate::nudge::command(tool) else { return vec![] };
    let Some(all) = crate::nudge::segments(line) else { return vec![] };
    let mut out = vec![];
    for words in all {
        let Some((first, rest)) = words.split_first() else { continue };
        let prog = crate::nudge::program(first);
        let args: &[String] = match prog.as_str() {
            "rm" | "del" | "erase" | "unlink" | "remove-item" | "ri" => rest,
            "git" if rest.first().is_some_and(|w| w == "rm") => &rest[1..],
            _ => continue,
        };
        out.extend(args.iter().filter(|a| !a.starts_with('-') && is_test(a)).cloned());
    }
    out
}

/// Codex's deletions: a change whose kind is delete.
fn deleted_by_change(tool: &ToolUse) -> Vec<String> {
    if tool.name != "fileChange" {
        return vec![];
    }
    let gone = |v: &serde_json::Value| {
        v.get("delete").is_some() || [&v["type"], &v["kind"], &v["kind"]["type"]].iter().any(|k| k.as_str() == Some("delete"))
    };
    let paths: Vec<String> = match &tool.input {
        serde_json::Value::Array(changes) => {
            changes.iter().filter(|c| gone(c)).filter_map(|c| c["path"].as_str().map(str::to_string)).collect()
        }
        serde_json::Value::Object(changes) => changes.iter().filter(|(_, v)| gone(v)).map(|(p, _)| p.clone()).collect(),
        _ => vec![],
    };
    paths.into_iter().filter(|p| is_test(p)).collect()
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches("./").to_lowercase()
}

/// Whether a changed path is one a plan step named: the same file, or inside a named folder.
fn named(changed: &str, listed: &str) -> bool {
    let (c, l) = (norm(changed), norm(listed));
    !l.is_empty() && (c == l || c.ends_with(&format!("/{l}")) || (l.ends_with('/') && (c.starts_with(&l) || c.contains(&format!("/{l}")))))
}

/// The path as the person knows it: the part after the working folder, else as written.
fn short(path: &str, root: &str) -> String {
    let (p, r) = (path.replace('\\', "/"), root.replace('\\', "/"));
    let r = r.trim_end_matches('/');
    p.strip_prefix(r).map(|rest| rest.trim_start_matches('/').to_string()).filter(|s| !s.is_empty()).unwrap_or(p)
}

/// What Faber noticed in these calls (a request's tools, in order). `request` is the person's message, `root` the working
/// folder; `plan` names the files each step expects to change.
pub fn facts(tools: &[&ToolUse], request: &str, plan: Option<&Plan>, root: &str) -> Vec<Fact> {
    let mut out: Vec<Fact> = vec![];
    let push = |fact: Fact, out: &mut Vec<Fact>| {
        if !out.contains(&fact) {
            out.push(fact);
        }
    };
    let mut weakened: Vec<(String, i64)> = vec![];
    // Markers count net of later edits: a skip or stub added then removed again before the end is no fact.
    let mut marks: Vec<(String, bool, i64)> = vec![];
    let mut mark = |file: &str, skip: bool, n: i64| match marks.iter_mut().find(|(f, k, _)| f == file && *k == skip) {
        Some((_, _, m)) => *m += n,
        None => marks.push((file.to_string(), skip, n)),
    };
    let mut changed: Vec<String> = vec![];
    for tool in tools.iter().filter(|t| !t.declined && t.ok != Some(false)) {
        for path in crate::nudge::edited(tool) {
            if !changed.iter().any(|c| c == path) {
                changed.push(path.to_string());
            }
        }
        for (path, lines) in changed_lines(tool) {
            let file = short(&path, root);
            let test = is_test(&path);
            let net = |is: fn(&str) -> bool| lines.iter().filter(|l| is(&l.text)).map(|l| if l.added { 1 } else { -1 }).sum::<i64>();
            mark(&file, true, net(skips));
            if test {
                let net = lines.iter().filter(|l| asserts(&l.text)).map(|l| if l.added { -1 } else { 1 }).sum::<i64>();
                match weakened.iter_mut().find(|(f, _)| *f == file) {
                    Some((_, n)) => *n += net,
                    None => weakened.push((file.clone(), net)),
                }
            } else if crate::nudge::is_source(&path) {
                mark(&file, false, net(stub));
            }
        }
        for path in deleted_by_command(tool).into_iter().chain(deleted_by_change(tool)) {
            push(Fact::Deleted { file: short(&path, root) }, &mut out);
        }
    }
    for (file, skip, _) in marks.into_iter().filter(|(_, _, n)| *n > 0) {
        push(if skip { Fact::Skipped { file } } else { Fact::Stub { file } }, &mut out);
    }
    for (file, n) in weakened.into_iter().filter(|(_, n)| *n > 0) {
        push(Fact::Weakened { file, removed: n as u32 }, &mut out);
    }
    let listed: Vec<&str> = plan.iter().flat_map(|p| &p.steps).flat_map(|s| &s.files).map(String::as_str).collect();
    if !listed.is_empty() {
        let request = norm(request);
        let outside: Vec<String> = changed
            .iter()
            .map(|p| short(p, root))
            .filter(|p| !norm(p).starts_with(".faber/"))
            .filter(|p| !listed.iter().any(|l| named(p, l)))
            .filter(|p| !request.contains(&norm(p)) && !request.contains(&norm(p.rsplit('/').next().unwrap_or(p))))
            .collect();
        if !outside.is_empty() {
            push(Fact::Outside { count: outside.len() as u32, files: outside.into_iter().take(NAMED).collect() }, &mut out);
        }
    }
    out.truncate(KEPT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tool(name: &str, input: serde_json::Value) -> ToolUse {
        serde_json::from_value(json!({"id": "t", "name": name, "input": input, "ok": true})).unwrap()
    }
    fn edit(path: &str, old: &str, new: &str) -> ToolUse {
        tool("Edit", json!({"file_path": path, "old_string": old, "new_string": new}))
    }

    #[test]
    fn a_skipped_or_weakened_test_is_noticed() {
        let skip = edit("/w/src/login.test.ts", "it('logs in', () => {", "it.skip('logs in', () => {");
        let weak = edit("/w/tests/test_cart.py", "    assert total == 10\n    assert tax == 1\n", "    pass\n");
        let ignore = edit("/w/src/store.rs", "    #[test]\n    fn saves() {", "    #[test]\n    #[ignore]\n    fn saves() {");
        let found = facts(&[&skip, &weak, &ignore], "fix the login", None, "/w");
        assert_eq!(
            found,
            [
                Fact::Skipped { file: "src/login.test.ts".into() },
                Fact::Skipped { file: "src/store.rs".into() },
                Fact::Weakened { file: "tests/test_cart.py".into(), removed: 2 },
            ]
        );
    }

    #[test]
    fn adding_tests_or_changing_an_expected_value_is_not_noticed() {
        let added = edit("/w/src/a.test.js", "", "test('x', () => { expect(a()).toBe(1) })");
        let changed = edit("/w/src/a.test.js", "expect(a()).toBe(1)", "expect(a()).toBe(2)");
        let todo = edit("/w/src/a.js", "", "// TODO: speed this up later");
        assert!(facts(&[&added, &changed, &todo], "x", None, "/w").is_empty());
    }

    #[test]
    fn deleted_tests_and_stubs_are_noticed() {
        let rm = tool("Bash", json!({"command": "rm -f tests/test_api.py && git rm src/x.spec.ts"}));
        let stubbed = tool("Write", json!({"file_path": "/w/src/pay.py", "content": "def pay():\n    raise NotImplementedError\n"}));
        let codex = tool("fileChange", json!({"src/old_test.go": {"delete": {}}}));
        let found = facts(&[&rm, &stubbed, &codex], "x", None, "/w");
        assert!(found.contains(&Fact::Deleted { file: "tests/test_api.py".into() }), "{found:?}");
        assert!(found.contains(&Fact::Deleted { file: "src/x.spec.ts".into() }));
        assert!(found.contains(&Fact::Deleted { file: "src/old_test.go".into() }));
        assert!(found.contains(&Fact::Stub { file: "src/pay.py".into() }));
    }

    #[test]
    fn files_outside_the_plan_and_the_request_are_noticed() {
        let plan: Plan = serde_json::from_value(
            json!({"steps": [{"id": "a", "text": "x", "state": "active", "files": ["src/invoice.js", "docs/"]}], "at": 0}),
        )
        .unwrap();
        let tools = [
            edit("/w/src/invoice.js", "a", "b"),
            edit("/w/docs/a.md", "a", "b"),
            edit("/w/src/util.js", "a", "b"),
            edit("/w/README.md", "a", "b"),
            edit("/w/.faber/decisions.md", "a", "b"),
        ];
        let refs: Vec<&ToolUse> = tools.iter().collect();
        assert_eq!(
            facts(&refs, "Fix the date on the invoice and README.md", Some(&plan), "/w"),
            [Fact::Outside { files: vec!["src/util.js".into()], count: 1 }]
        );
        assert!(facts(&refs, "x", None, "/w").is_empty(), "with no plan there is nothing to compare");
    }

    #[test]
    fn test_paths() {
        for p in ["src/a.test.ts", "tests/x.rs", "pkg/a_test.go", "test_a.py", "src/__tests__/b.js", "src/FooTest.java", "e2e/x.ts"] {
            assert!(is_test(p), "{p}");
        }
        for p in ["src/attest.rs", "src/latest.js", "src/contest.py", "README.md"] {
            assert!(!is_test(p), "{p}");
        }
    }

    /// Seen in review: a stub written first and filled in later is no fact at the end.
    #[test]
    fn a_marker_removed_by_a_later_edit_is_not_reported() {
        let first = edit("/w/src/pay.rs", "fn pay() {}", "fn pay() { todo!() }");
        let done = edit("/w/src/pay.rs", "fn pay() { todo!() }", "fn pay() { charge() }");
        assert_eq!(facts(&[&first], "", None, "/w"), vec![Fact::Stub { file: "src/pay.rs".into() }]);
        assert!(facts(&[&first, &done], "", None, "/w").is_empty());
        let skip = edit("/w/src/a.test.ts", "it('a'", "it.skip('a'");
        let back = edit("/w/src/a.test.ts", "it.skip('a'", "it('a'");
        assert!(facts(&[&skip, &back], "", None, "/w").is_empty());
    }
}
