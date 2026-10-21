//! Look around after a fix. A turn that went from red to green fixed something: a check failed, the work changed, and the
//! last check passed. The same kind of mistake often lives in other places, so such a reply is marked for the offer to
//! look for it there; the person decides, and nothing runs until they do. The mirror of the step up (`nudge`): the same
//! rules for a check and an edit, and the two never hold together. Pure functions over a finished message; the engine
//! feeds them.

use crate::model::{Message, Status, ToolUse};
use crate::nudge::{command, edited, is_check, is_source};
use serde::{Deserialize, Serialize};

/// The offer under a finished answer. `reason` is a key for the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Look {
    pub reason: String,
}

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
    (fixed && !unchecked).then(|| Look { reason: "look.fixed".into() })
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
            check: false,
            started: None,
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
        });
        assert_eq!(of(&m), None, "one question at a time");
        m.wrap_up = Some(WrapUp { recap: "Fixed.".into(), ask: None, actions: vec![], done: true, answered: None });
        assert_eq!(of(&m).map(|l| l.reason).as_deref(), Some("look.fixed"), "a box with no question leaves room for the offer");
    }
}
