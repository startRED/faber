//! What an edit step changed, read from its own input with no model call (R161 step 9): how many lines it added and
//! removed, and the first changed lines. The engine keeps this on the step so the screen never needs the whole input.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A line an edit step added or removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedLine {
    pub added: bool,
    pub text: String,
}

/// The counts cover every changed line; `lines` keeps only the first [`KEPT`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Change {
    pub added: usize,
    pub removed: usize,
    pub lines: Vec<ChangedLine>,
}

/// The screen shows four lines and counts the rest, so a few dozen is plenty.
pub const KEPT: usize = 40;
/// The screen clips a line to one row, so a kept line needs no more than this many characters.
const LINE_CHARS: usize = 200;
/// Past this many lines on a side, a replacement is compared only at its ends: the longest common run is quadratic.
const COMPARED: usize = 400;

fn lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let text = text.replace("\r\n", "\n");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    text.split('\n').map(str::to_owned).collect()
}

fn line(added: bool, text: &str) -> ChangedLine {
    ChangedLine { added, text: text.to_owned() }
}

/// One replacement as the lines it changed: a line both sides keep, in order, is not a change.
fn replaced(before: &str, after: &str) -> Vec<ChangedLine> {
    let a = lines(before);
    let b = lines(after);
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end = 0;
    while end < a.len() - start && end < b.len() - start && a[a.len() - 1 - end] == b[b.len() - 1 - end] {
        end += 1;
    }
    let x = &a[start..a.len() - end];
    let y = &b[start..b.len() - end];
    if x.len() > COMPARED || y.len() > COMPARED {
        return x.iter().map(|t| line(false, t)).chain(y.iter().map(|t| line(true, t))).collect();
    }
    // keep[i][j]: how many lines x[i..] and y[j..] keep in common.
    let mut keep = vec![vec![0usize; y.len() + 1]; x.len() + 1];
    for i in (0..x.len()).rev() {
        for j in (0..y.len()).rev() {
            keep[i][j] = if x[i] == y[j] { keep[i + 1][j + 1] + 1 } else { keep[i + 1][j].max(keep[i][j + 1]) };
        }
    }
    let (mut gone, mut came) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0, 0);
    while i < x.len() || j < y.len() {
        if i < x.len() && j < y.len() && x[i] == y[j] {
            i += 1;
            j += 1;
        } else if j < y.len() && (i == x.len() || keep[i][j + 1] >= keep[i + 1][j]) {
            came.push(line(true, &y[j]));
            j += 1;
        } else {
            gone.push(line(false, &x[i]));
            i += 1;
        }
    }
    // Removed lines read before the lines that replace them, as in a diff.
    gone.extend(came);
    gone
}

/// A unified diff's own lines. Its file headers (`---`, `+++`) come before the first hunk; inside a hunk every `+` or `-`
/// line is content, even one that starts with three of them.
fn diffed(diff: &str) -> Vec<ChangedLine> {
    let all = lines(diff);
    let mut in_hunk = !all.iter().any(|l| l.starts_with("@@"));
    let mut out = Vec::new();
    for l in &all {
        if l.starts_with("@@") {
            in_hunk = true;
        } else if in_hunk && (l.starts_with('+') || l.starts_with('-')) {
            out.push(line(l.starts_with('+'), &l[1..]));
        }
    }
    out
}

/// A map's values in the order the screen walks them: JavaScript puts keys that read as array indexes first, by number,
/// then the rest in their stored order (sorted, as the engine keeps JSON).
fn values(o: &serde_json::Map<String, Value>) -> Vec<&Value> {
    let index = |k: &str| k.parse::<u32>().ok().filter(|n| *n < u32::MAX && n.to_string() == k);
    let mut keyed: Vec<_> = o.iter().map(|(k, v)| (index(k).map_or(1, |_| 0), index(k).unwrap_or(0), v)).collect();
    keyed.sort_by_key(|(group, n, _)| (*group, *n));
    keyed.into_iter().map(|(_, _, v)| v).collect()
}

/// One of Codex's file changes: a list item with its `diff`, or a value keyed by its path that holds a unified diff or,
/// for a new file, its content, directly or under its kind (`{ update: { unified_diff } }`, `{ add: { content } }`).
fn codex_change(change: &Value) -> Vec<ChangedLine> {
    let inner: Vec<&Value> = match change {
        Value::Object(o) => {
            let diff = o.get("diff").filter(|d| !d.is_null()).or_else(|| o.get("unified_diff"));
            if let Some(diff) = diff.and_then(Value::as_str) {
                return diffed(diff);
            }
            if let Some(content) = o.get("content").and_then(Value::as_str) {
                return lines(content).iter().map(|t| line(true, t)).collect();
            }
            values(o)
        }
        Value::Array(a) => a.iter().collect(),
        _ => return Vec::new(),
    };
    inner.into_iter().filter(|v| v.is_object()).flat_map(codex_change).collect()
}

/// The lines an edit step changed: Claude Code's Edit, MultiEdit and Write, and Codex's file changes. Anything else, or
/// an edit that changed nothing, has none.
pub fn of(name: &str, input: &Value) -> Option<Change> {
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_owned();
    let changed: Vec<ChangedLine> = match (name, input) {
        ("Edit", Value::Object(_)) => replaced(&text(input, "old_string"), &text(input, "new_string")),
        ("MultiEdit", Value::Object(o)) => match o.get("edits") {
            Some(Value::Array(edits)) => edits.iter().flat_map(|e| replaced(&text(e, "old_string"), &text(e, "new_string"))).collect(),
            _ => Vec::new(),
        },
        ("Write", Value::Object(_)) => lines(&text(input, "content")).iter().map(|t| line(true, t)).collect(),
        ("fileChange", Value::Array(a)) => a.iter().flat_map(codex_change).collect(),
        ("fileChange", Value::Object(o)) => values(o).into_iter().flat_map(codex_change).collect(),
        _ => Vec::new(),
    };
    if changed.is_empty() {
        return None;
    }
    let added = changed.iter().filter(|l| l.added).count();
    let removed = changed.len() - added;
    let lines = changed
        .into_iter()
        .take(KEPT)
        .map(|mut l| {
            if let Some((at, _)) = l.text.char_indices().nth(LINE_CHARS) {
                l.text.truncate(at);
            }
            l
        })
        .collect();
    Some(Change { added, removed, lines })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shown(c: &Change) -> Vec<(bool, &str)> {
        c.lines.iter().map(|l| (l.added, l.text.as_str())).collect()
    }

    // The same cases as the screen check "worklog: an edit step counts the lines it added and removed".
    #[test]
    fn an_edit_step_counts_the_lines_it_added_and_removed() {
        let edit =
            of("Edit", &json!({ "file_path": "a.rs", "old_string": "fn a() {\n  1\n}", "new_string": "fn a() {\n  2\n  3\n}" })).unwrap();
        assert_eq!((edit.added, edit.removed), (2, 1));
        assert_eq!(shown(&edit), [(false, "  1"), (true, "  2"), (true, "  3")]);
        let write = of("Write", &json!({ "file_path": "b.md", "content": "# B\n\nText\n" })).unwrap();
        assert_eq!(write.added, 3, "a written file is all added");
        let multi =
            of("MultiEdit", &json!({ "edits": [{ "old_string": "a", "new_string": "b" }, { "old_string": "c\nd", "new_string": "c" }] }))
                .unwrap();
        assert_eq!(multi.removed, 2);
        let codex =
            of("fileChange", &json!([{ "path": "n.txt", "diff": "--- a/n.txt\n+++ b/n.txt\n@@ -1 +1 @@\n-baseline\n+escaped" }])).unwrap();
        assert_eq!(shown(&codex), [(false, "baseline"), (true, "escaped")]);
        let keyed = of(
            "fileChange",
            &json!({ "n.txt": { "update": { "unified_diff": "@@ -1 +1 @@\n-a\n+b\n+c" } }, "new.txt": { "add": { "content": "x" } } }),
        )
        .unwrap();
        assert_eq!(keyed.added, 3, "Codex keyed by path");
        let numbered = of("fileChange", &json!({ "10": { "content": "ten" }, "b": { "content": "b" }, "2": { "content": "two" } }));
        let texts: Vec<_> = numbered.as_ref().unwrap().lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["two", "ten", "b"], "file names that read as numbers come first, as on the screen");
        let inside = of("Edit", &json!({ "old_string": "old A\nkeep\nold B", "new_string": "new A\nkeep\nnew B" })).unwrap();
        let texts: Vec<_> = inside.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["old A", "old B", "new A", "new B"], "a line kept inside is not a change");
        let signs = of("fileChange", &json!([{ "path": "x", "diff": "--- a/x\n+++ b/x\n@@ -1 +1 @@\n--- old\n+++ new" }]));
        assert_eq!(signs.unwrap().added, 1, "inside a hunk, three signs are content");
        assert_eq!(of("Edit", &json!({ "old_string": "same", "new_string": "same" })), None, "an edit that changed nothing");
        assert_eq!(of("Read", &json!({ "file_path": "a.rs" })), None);
    }

    #[test]
    fn a_big_edit_keeps_its_counts_and_only_the_first_short_lines() {
        let body: Vec<String> = (0..500).map(|i| format!("{i} {}", "x".repeat(300))).collect();
        let change = of("Write", &json!({ "content": body.join("\n") })).unwrap();
        assert_eq!((change.added, change.removed, change.lines.len()), (500, 0, KEPT));
        assert!(change.lines.iter().all(|l| l.text.chars().count() == LINE_CHARS));
        let before = body.join(
            "
",
        );
        let mut middle = body.clone();
        middle[250] = "changed".into();
        let one = of(
            "Edit",
            &json!({ "old_string": before, "new_string": middle.join("
") }),
        )
        .unwrap();
        assert_eq!((one.added, one.removed), (1, 1), "the common ends are trimmed first");
        // Past the compared size, the rest is all removed then all added, as on the screen.
        let wide = of(
            "Edit",
            &json!({ "old_string": format!("a
{before}"), "new_string": format!("b
{before}
c") }),
        );
        let wide = wide.unwrap();
        assert_eq!((wide.added, wide.removed), (502, 501));
    }
}
