//! Done only with proof. A project agent that edited code and concludes done names, for each thing the person asked, the
//! receipt that shows it: a command it ran, `verify`, or `look` for a page it opened. Faber matches each against what the
//! agent's tools really ran after its last edit to source, with no process or model call, and reads the edits themselves
//! (`crate::guard`). Something unproven is refused at most [`MAX_ASKS`] times; after that the conclusion goes through
//! labelled, so the person sees "2 of 4 confirmed" instead of a plain Done, and no agent is ever trapped (a soft label, not a
//! block: chosen for the owner, "IA (Erick delegou)", 2026-11-13).

use crate::guard::{self, Fact};
use crate::model::{ConclusionSummary, Conversation, Message, Proof, Role, ToolUse};

/// How many times a request's done conclusion may be refused for missing proof before it is let through labelled.
pub const MAX_ASKS: usize = 2;
/// How a refusal begins, so the engine counts them in the stored conclusion errors.
pub const REFUSED: &str = "Faber did not accept done";
/// The shortest command a proof may name inside its own words.
const NAMED_CHARS: usize = 6;
/// The longest requirement text kept in a label.
const ASK_CHARS: usize = 160;

/// The replies of the request under way: after the person's last message (Faber's own wakes belong to the request).
pub fn request(c: &Conversation) -> &[Message] {
    let start = c.messages.iter().rposition(|m| m.role == Role::User && m.wake.is_none()).map_or(0, |i| i + 1);
    &c.messages[start..]
}

/// The person's words that started the request.
pub fn asked(c: &Conversation) -> &str {
    c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()).map_or("", |m| m.text.as_str())
}

/// How many of this request's done conclusions were refused for missing proof, the reply under way included: each
/// refusal is counted on its reply (`Message::proof_asks`), so retries inside one reply count too.
pub fn refusals(c: &Conversation) -> usize {
    request(c).iter().map(|m| usize::from(m.proof_asks)).sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Check,
    Run,
    Look,
}

struct Receipt {
    kind: Kind,
    /// The whole command line, then each command of a chain (`cd app && npm test` also gives `npm test`).
    commands: Vec<String>,
}

fn normal(text: &str) -> String {
    text.to_lowercase().replace(['`', '"', '\''], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_look(tool: &ToolUse) -> bool {
    let name = tool.name.to_lowercase();
    tool.name == crate::browser::TOOL || name.contains("screenshot") || name.contains("browser")
}

/// The receipt of one call, when it ended well: Faber's verify that ran checks, a command that exited cleanly (a check
/// command counts as a check), or a page or screen opened.
fn receipt(tool: &ToolUse) -> Option<Receipt> {
    if tool.declined || tool.ok != Some(true) {
        return None;
    }
    if crate::nudge::judged(tool) == Some(true) {
        return Some(Receipt { kind: Kind::Check, commands: vec!["verify".into()] });
    }
    if let Some(command) = crate::nudge::command(tool) {
        // A clean exit says nothing about a command before `||` (`npm test || true`): only what follows the last one counts.
        let (masked, trusted) = command.rsplit_once("||").map_or((false, command), |(_, rest)| (true, rest));
        let kind = if crate::nudge::is_check(tool) && !masked { Kind::Check } else { Kind::Run };
        let mut commands = if masked { vec![] } else { vec![normal(command)] };
        // Each command of a chain, without its redirections (`npm test > out.txt 2>&1` reads as `npm test`).
        let plain = |words: Vec<String>| words.into_iter().take_while(|w| !w.contains(['>', '<'])).collect::<Vec<_>>().join(" ");
        commands.extend(crate::nudge::segments(trusted).into_iter().flatten().map(|words| normal(&plain(words))));
        return Some(Receipt { kind, commands });
    }
    is_look(tool).then(|| Receipt { kind: Kind::Look, commands: vec![] })
}

/// Whether a check ran and failed: a check command that exited badly, or a verify whose checks failed.
fn failed_check(tool: &ToolUse) -> Option<bool> {
    if tool.declined {
        return None;
    }
    crate::nudge::judged(tool).map(|ok| !ok).or_else(|| crate::nudge::is_check(tool).then(|| tool.ok == Some(false)))
}

/// Whether a proof the agent named matches a receipt: `verify` or `check` any passing check, `look` any page opened, and a
/// command the agent ran: the proof names the command (`npm test (node test.js) exit 0`; at least [`NAMED_CHARS`], so a
/// bare `npm` proves no `npm test`) or the command starts with the proof. A command that only carries another as text
/// (`echo npm test`) proves nothing.
fn matches(proof: &str, receipts: &[Receipt]) -> bool {
    let p = normal(proof);
    let p = p.strip_prefix("run:").or_else(|| p.strip_prefix("ran ")).or_else(|| p.strip_prefix("run ")).unwrap_or(&p).trim();
    if p.len() < 3 {
        return false;
    }
    let first = p.split([' ', ':', ',']).next().unwrap_or("");
    match first {
        "verify" | "check" | "checks" => receipts.iter().any(|r| r.kind == Kind::Check),
        "look" | "looked" | "screenshot" => receipts.iter().any(|r| r.kind == Kind::Look),
        _ => receipts
            .iter()
            .filter(|r| r.kind != Kind::Look)
            .flat_map(|r| &r.commands)
            .any(|c| (c.len() >= NAMED_CHARS && p.contains(c.as_str())) || (c.len() >= 3 && c.starts_with(p))),
    }
}

/// The lines of a shell call that are commands, without the bodies of its heredocs (`<<'EOF'` … `EOF`).
fn command_lines(command: &str) -> Vec<&str> {
    let (mut out, mut end) = (vec![], None::<String>);
    for line in command.lines() {
        if let Some(e) = &end {
            if line.trim() == e {
                end = None;
            }
            continue;
        }
        out.push(line);
        end = line.split_once("<<").map(|(_, rest)| {
            rest.trim_start_matches('-').trim().trim_matches(['\'', '"']).split_whitespace().next().unwrap_or("").to_string()
        });
        end = end.filter(|e| !e.is_empty());
    }
    out
}

/// Source files a shell command wrote in the project: the target of `>` or `>>` (`cat > form.js <<'EOF'`), of `tee`,
/// `sed -i`, `cp` or `mv`. Agents edit this way too, and an edit Faber does not see would skip the proof. A scratch file
/// outside the project (`/tmp/x.js`) is not an edit.
fn written(tool: &ToolUse, root: &str) -> Vec<String> {
    let Some(command) = crate::nudge::command(tool) else { return vec![] };
    let lines = command_lines(command).join(" ; ");
    let words: Vec<&str> = lines.split_whitespace().collect();
    let clean = |w: &str| w.trim_matches(['"', '\'', ';', ')', '(']).to_string();
    let mut out = vec![];
    for (i, w) in words.iter().enumerate() {
        let target = match *w {
            ">" | ">>" => words.get(i + 1).map(|t| clean(t)),
            _ if w.starts_with('>') && !w.starts_with(">&") => Some(clean(w.trim_start_matches('>'))),
            _ => None,
        };
        out.extend(target);
    }
    for run in words.split(|w| matches!(*w, "&&" | ";" | "|" | "||")) {
        let args = || run[1..].iter().filter(|w| !w.starts_with('-'));
        match run.first().map(|w| crate::nudge::program(w)).as_deref() {
            Some("tee") => out.extend(args().map(|w| clean(w))),
            Some("sed") if run.iter().any(|w| w.starts_with("-i")) => out.extend(run.last().map(|w| clean(w))),
            Some("cp" | "mv" | "copy" | "move") => out.extend(args().next_back().map(|w| clean(w))),
            _ => {}
        }
    }
    let root = root.replace('\\', "/").trim_end_matches('/').to_lowercase();
    let inside = |p: &String| {
        let p = p.replace('\\', "/").to_lowercase();
        let absolute = p.starts_with('/') || p.get(1..2) == Some(":");
        !absolute || (!root.is_empty() && p.starts_with(&format!("{root}/")))
    };
    out.retain(|p| crate::nudge::is_source(p) && inside(p));
    out
}

/// A call that changed source: an edit step that did not fail, or a shell call that writes a source file, failed or not
/// (a shell line can write its file and then fail on a later command).
fn edits_source(tool: &ToolUse, root: &str) -> bool {
    !tool.declined
        && ((tool.ok != Some(false) && crate::nudge::edited(tool).into_iter().any(crate::nudge::is_source))
            || !written(tool, root).is_empty()
            || !guard::deleted_by_command(tool).is_empty())
}

/// What a shell call that edits ran after its last write (`cp new.js form.js && npm test` gives `npm test`), as a call
/// of its own: a check before the write (`npm test && cp broken.js a.js`) proves nothing about the written file.
fn after_write(tool: &ToolUse, root: &str) -> Option<ToolUse> {
    let command = command_lines(crate::nudge::command(tool)?).join("\n");
    let end = written(tool, root).iter().filter_map(|t| command.rfind(t.as_str()).map(|i| i + t.len())).max()?;
    let skip = |c: char| c.is_whitespace() || matches!(c, '&' | ';' | '|' | '\'' | '"');
    let mut rest = command[end..].trim_start_matches(skip);
    // A heredoc marker (`<<'EOF'`) is the write itself; its body is already gone (`command_lines`).
    if let Some(marker) = rest.strip_prefix("<<") {
        let marker = marker.trim_start_matches(['-', ' ']);
        rest = marker.find(|c: char| c.is_whitespace() || c == ';' || c == '&').map_or("", |i| &marker[i..]).trim_start_matches(skip);
    }
    (!rest.trim().is_empty()).then(|| {
        let mut own = tool.clone();
        own.input = serde_json::json!({ "command": rest });
        own
    })
}

/// What Faber reads behind a done conclusion, and what it would tell the agent (English, empty when all is proven).
pub struct Assessment {
    pub proof: Proof,
    pub requirements: Vec<bool>,
    pub problems: Vec<String>,
}

/// Weigh a done conclusion against the request's own calls. None when the request edited no source file: an answer or a
/// note needs no receipt.
pub fn assess(c: &Conversation, summary: &ConclusionSummary, root: &str) -> Option<Assessment> {
    let tools: Vec<&ToolUse> = request(c).iter().filter(|m| m.role == Role::Assistant).flat_map(|m| m.tools.iter().flatten()).collect();
    let last_edit = tools.iter().rposition(|t| edits_source(t, root))?;
    // A shell call that writes and then tests (`cp new.js form.js && npm test`) proves its own edit.
    let own = after_write(tools[last_edit], root);
    let after: Vec<&ToolUse> = own.iter().chain(tools[last_edit + 1..].iter().copied()).collect();
    let receipts: Vec<Receipt> = after.iter().filter_map(|t| receipt(t)).collect();
    let failed = after.iter().rev().find_map(|t| failed_check(t)).unwrap_or(false);
    let confirmed: Vec<bool> = summary.requirements.iter().map(|r| matches(&r.proof, &receipts)).collect();
    let mut missing: Vec<String> = summary
        .requirements
        .iter()
        .zip(&confirmed)
        .filter(|(_, ok)| !**ok)
        .map(|(r, _)| r.asked.chars().take(ASK_CHARS).collect())
        .collect();
    let plan = crate::conversation_state::fresh_plan(c);
    let facts = guard::facts(&tools, asked(c), plan, root);
    let said =
        normal(&[&summary.did, &summary.result, &summary.not_done, &summary.assumed, &summary.verified].map(|s| s.as_str()).join(" "));
    let explained = |f: &Fact| f.files().iter().all(|file| said.contains(&normal(file.rsplit('/').next().unwrap_or(file))));
    let mut problems = vec![];
    if failed {
        problems.push("the last check after your last edit failed".to_string());
    }
    if summary.requirements.is_empty() {
        problems.push("requirements is empty: list each thing the person asked, with the command, verify or look that proves it".into());
    }
    if receipts.is_empty() {
        problems.push("nothing ran after your last edit: no check, command or look".into());
    }
    for (r, ok) in summary.requirements.iter().zip(&confirmed) {
        if !ok {
            problems.push(match r.proof.trim() {
                "" => format!("\"{}\" names no proof", r.asked),
                proof => format!("\"{}\": \"{proof}\" matches nothing your tools ran after the last edit", r.asked),
            });
        }
    }
    if !summary.could_not_reach.is_empty() {
        missing.push(summary.could_not_reach.chars().take(ASK_CHARS).collect());
        problems.push(format!("you could not reach {}, so the request is not done", summary.could_not_reach));
    }
    problems.extend(facts.iter().filter(|f| !explained(f)).map(|f| format!("{}; restore it, or say why in not_done", f.told())));
    let total = summary.requirements.len() as u32;
    let proof = Proof {
        confirmed: confirmed.iter().filter(|ok| **ok).count() as u32,
        total,
        missing,
        unchecked: receipts.is_empty(),
        failed,
        facts,
        labelled: false,
    };
    Some(Assessment { proof, requirements: confirmed, problems })
}

/// What the agent reads when its done conclusion is refused.
pub fn refusal(problems: &[String]) -> String {
    format!(
        "{REFUSED}: {}. Run what proves each requirement after your last edit and name it in proof (the command, verify, or look), fix what is missing, or conclude needs_you and say what is not verified. Asked at most {MAX_ASKS} times; after that the conclusion shows as not confirmed",
        problems.join("; ")
    )
}

/// Whether the screen has something to say under the conclusion.
pub fn worth_showing(p: &Proof) -> bool {
    p.total > 0 || p.labelled || !p.facts.is_empty() || p.failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Requirement;
    use serde_json::json;

    fn tool(name: &str, input: serde_json::Value, ok: bool) -> ToolUse {
        serde_json::from_value(json!({"id": "t", "name": name, "input": input, "ok": ok})).unwrap()
    }
    fn edit(path: &str) -> ToolUse {
        tool("Edit", json!({"file_path": path, "old_string": "a", "new_string": "b"}), true)
    }
    fn bash(command: &str, ok: bool) -> ToolUse {
        tool("Bash", json!({"command": command}), ok)
    }
    fn conversation(tools: Vec<ToolUse>) -> Conversation {
        let mut c: Conversation = serde_json::from_value(json!({
            "id": "c", "title": "", "area": "project", "createdAt": 0, "updatedAt": 0,
            "messages": [
                {"id": "u", "role": "user", "text": "Add a blue Save button and keep the phone layout", "at": 1},
                {"id": "r", "role": "assistant", "text": "", "at": 2}
            ]
        }))
        .unwrap();
        c.messages[1].tools = Some(tools);
        c
    }
    fn summary(requirements: &[(&str, &str)]) -> ConclusionSummary {
        ConclusionSummary {
            did: "Added the button.".into(),
            result: "It is blue.".into(),
            verified: "Tests.".into(),
            requirements: requirements
                .iter()
                .map(|(a, p)| Requirement { asked: (*a).into(), proof: (*p).into(), confirmed: false })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn each_requirement_needs_a_receipt_after_the_last_edit() {
        let c = conversation(vec![
            bash("npm test", true),
            edit("/w/src/Save.tsx"),
            bash("npm test", true),
            bash("npx playwright screenshot --viewport-size=390,844 page.html", true),
        ]);
        let a = assess(
            &c,
            &summary(&[
                ("blue Save button", "npm test"),
                ("keep the phone layout", "npx playwright screenshot --viewport-size=390,844 page.html"),
                ("works offline", ""),
            ]),
            "/w",
        )
        .unwrap();
        assert_eq!(a.requirements, [true, true, false]);
        assert_eq!((a.proof.confirmed, a.proof.total), (2, 3));
        assert_eq!(a.proof.missing, ["works offline"]);
        assert_eq!(a.problems, ["\"works offline\" names no proof"]);
    }

    #[test]
    fn a_receipt_before_the_last_edit_or_a_failed_check_proves_nothing() {
        let c = conversation(vec![bash("npm test", true), edit("/w/src/Save.tsx")]);
        let a = assess(&c, &summary(&[("blue Save button", "npm test passed")]), "/w").unwrap();
        assert_eq!(a.requirements, [false]);
        assert!(a.proof.unchecked);
        let c = conversation(vec![edit("/w/src/Save.tsx"), bash("npm test", false)]);
        let a = assess(&c, &summary(&[]), "/w").unwrap();
        assert!(a.proof.failed);
        assert_eq!(
            a.problems,
            [
                "the last check after your last edit failed",
                "requirements is empty: list each thing the person asked, with the command, verify or look that proves it",
                "nothing ran after your last edit: no check, command or look"
            ]
        );
    }

    #[test]
    fn verify_and_look_are_named_by_kind() {
        let verify = ToolUse { output: Some(format!("{}checks", crate::nudge::RAN)), ..tool(crate::nudge::TOOL, json!({}), true) };
        let c = conversation(vec![edit("/w/src/a.rs"), verify, tool(crate::browser::TOOL, json!({"url": "http://localhost"}), true)]);
        let a = assess(&c, &summary(&[("saves", "verify fast"), ("shows", "look at the page on a phone")]), "/w").unwrap();
        assert_eq!(a.requirements, [true, true]);
        assert!(a.problems.is_empty());
    }

    #[test]
    fn no_source_edit_needs_no_proof_and_a_done_after_edits_names_its_requirements() {
        assert!(assess(&conversation(vec![edit("/w/README.md")]), &summary(&[]), "/w").is_none());
        let c = conversation(vec![edit("/w/src/a.rs"), bash("cargo test", true)]);
        let a = assess(&c, &summary(&[]), "/w").unwrap();
        assert_eq!(a.problems.len(), 1, "something ran, but nothing says what it proves: {:?}", a.problems);
        let a = assess(&c, &summary(&[("fix a", "cargo test")]), "/w").unwrap();
        assert!(a.problems.is_empty() && worth_showing(&a.proof));
    }

    #[test]
    fn could_not_reach_and_unexplained_facts_are_problems() {
        let skip =
            tool("Edit", json!({"file_path": "/w/src/pay.test.ts", "old_string": "it('pays'", "new_string": "it.skip('pays'"}), true);
        let c = conversation(vec![skip, bash("npm test", true)]);
        let mut s = summary(&[("pays", "npm test")]);
        s.could_not_reach = "the payments key".into();
        let a = assess(&c, &s, "/w").unwrap();
        assert_eq!(a.proof.missing, ["the payments key"]);
        assert_eq!(a.problems.len(), 2, "{:?}", a.problems);
        assert!(a.problems[1].starts_with("a test was turned off in src/pay.test.ts"));
        s.could_not_reach.clear();
        s.not_done = "pay.test.ts is skipped until the sandbox is back".into();
        assert!(assess(&c, &s, "/w").unwrap().problems.is_empty(), "a fact the conclusion explains is shown, not refused");
    }

    #[test]
    fn refusals_count_only_this_request() {
        let mut c = conversation(vec![]);
        c.messages[1].proof_asks = 1;
        let wake: Message =
            serde_json::from_value(json!({"id": "w", "role": "user", "text": "", "at": 3, "wake": {"why": "wake.conclusion"}})).unwrap();
        let reply: Message = serde_json::from_value(json!({"id": "r2", "role": "assistant", "text": "", "at": 4})).unwrap();
        c.messages.extend([wake, reply]);
        assert_eq!(refusals(&c), 1);
        c.messages.last_mut().unwrap().proof_asks = 1;
        assert_eq!(refusals(&c), 2, "a retry within the reply under way counts too");
        let user: Message = serde_json::from_value(json!({"id": "u2", "role": "user", "text": "next", "at": 5})).unwrap();
        c.messages.push(user);
        assert_eq!(refusals(&c), 0);
    }

    /// Seen in the bench: an agent wrote the file through the shell and named the test it ran inside a `cd` chain.
    #[test]
    fn shell_writes_count_as_edits_and_chained_commands_are_receipts() {
        let write = bash(
            "cd \"E:/w\" && cat > form.js <<'EOF'
function render() {}
EOF",
            true,
        );
        let c = conversation(vec![write.clone()]);
        let a = assess(&c, &summary(&[]), "/w").expect("a heredoc into form.js is an edit");
        assert!(a.problems.iter().any(|p| p.starts_with("nothing ran")), "{:?}", a.problems);
        let c = conversation(vec![write, bash("cd \"E:/w\" && node test.js; echo exit $?", true)]);
        let a = assess(&c, &summary(&[("blue button", "node test.js (exit 0) after the edit")]), "/w").unwrap();
        assert_eq!(a.requirements, vec![true], "{:?}", a.problems);
        assert_eq!(written(&bash("echo hi > notes.txt && sed -i s/a/b/ src/a.rs", true), "/w"), vec!["src/a.rs".to_string()]);
        assert!(written(&bash("npm test 2>&1 | tee out.log", true), "/w").is_empty());
        let multi = bash(
            "cd /w
cat > csv.js <<'EOF'
const x = a > b.js
EOF
node test.js",
            true,
        );
        assert_eq!(written(&multi, "/w"), vec!["csv.js".to_string()], "a later line writes; a heredoc body is not a command");
        let failed_write = bash(
            "cat > cart.js <<'EOF'
x
EOF
false",
            false,
        );
        let c = conversation(vec![failed_write, bash("cd /w && npm test > /w/out.txt 2>&1; echo done", true)]);
        let a = assess(&c, &summary(&[("discount", "npm test (exit 0) after the edit")]), "/w")
            .expect("a failed shell call may still have written");
        assert_eq!(a.requirements, vec![true], "a redirected command still proves itself: {:?}", a.problems);
        assert!(written(&bash("git show HEAD:csv.js > /tmp/old.js", true), "/w").is_empty(), "a scratch copy is not an edit");
        assert_eq!(written(&bash("cp /tmp/new.js /w/form.js", true), "/w"), vec!["/w/form.js".to_string()]);
        let c = conversation(vec![bash("cp /tmp/new.js form.js && npm test", true)]);
        let a = assess(&c, &summary(&[("blue", "npm test")]), "/w").unwrap();
        assert_eq!(a.requirements, vec![true], "the call that wrote and tested proves its own edit");
    }

    /// From the other AI's review: a proof that only names a command, masks its failure, or ran before the last write
    /// proves nothing; a done with no requirements after edits is asked for them.
    #[test]
    fn a_proof_cannot_be_named_masked_or_run_before_the_write() {
        let proven = |calls: Vec<ToolUse>| {
            let c = conversation(calls);
            assess(&c, &summary(&[("blue", "npm test")]), "/w").unwrap().requirements == vec![true]
        };
        assert!(!proven(vec![edit("/w/a.js"), bash("echo npm test", true)]), "echo only names the test");
        assert!(!proven(vec![edit("/w/a.js"), bash("npm", true)]), "a bare npm is no npm test");
        let named = conversation(vec![edit("/w/a.js"), bash("cd /w && node test.js; echo exit=$?", true)]);
        assert_eq!(
            assess(&named, &summary(&[("blue", "npm test (node test.js) exit 0, after edits")]), "/w").unwrap().requirements,
            [true]
        );
        assert!(!proven(vec![edit("/w/a.js"), bash("npm test || true", true)]), "|| true hides a failed test");
        assert!(proven(vec![edit("/w/a.js"), bash("npm test || true; npm test", true)]), "what follows the last || counts");
        assert!(!proven(vec![bash("npm test && cp /tmp/broken.js a.js", true)]), "the test ran before the write");
        assert!(proven(vec![bash("cp /tmp/new.js a.js && npm test", true)]));
        let c = conversation(vec![edit("/w/a.js"), bash("npm test", true)]);
        let a = assess(&c, &summary(&[]), "/w").unwrap();
        assert!(a.problems.iter().any(|p| p.starts_with("requirements is empty")), "{:?}", a.problems);
        let c = conversation(vec![bash("rm src/pay.test.ts", true)]);
        assert!(assess(&c, &summary(&[]), "/w").is_some_and(|a| !a.proof.facts.is_empty()), "deleting a test is weighed too");
    }
}
