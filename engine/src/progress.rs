//! What a project's agent tells the person about its own work, through three of Faber's tools (task 017): `plan`, the
//! conversation's checklist; `status`, a few words of what it is doing now; and `wrap_up`, the closing box of a task, whose
//! `done` also ends Keep going. Each call only writes down what it says: the engine stores it on the conversation or the
//! reply, the screen draws it, and the call answers at once. Both CLIs call them like the other tools of Faber's server.

use crate::model::{Plan, PlanStep, StepState, WrapUp};
use serde_json::{Value, json};

/// The names both drivers give the calls, whatever the CLI calls them.
pub const PLAN_TOOL: &str = "Plan";
pub const STATUS_TOOL: &str = "Status";
pub const WRAP_UP_TOOL: &str = "WrapUp";
/// The tools as Codex knows them, and as Faber's MCP server lists them to Claude Code.
pub const PLAN: &str = "plan";
pub const STATUS: &str = "status";
pub const WRAP_UP: &str = "wrap_up";
/// The tools as Claude Code names them: `mcp__<server>__<tool>`.
pub const CLAUDE_PLAN: &str = "mcp__faber__plan";
pub const CLAUDE_STATUS: &str = "mcp__faber__status";
pub const CLAUDE_WRAP_UP: &str = "mcp__faber__wrap_up";

/// Steps one plan holds at most.
pub const MAX_STEPS: usize = 12;
/// The longest a step, a status, a recap, a question and a button are, in characters; longer ones are cut, never refused.
const STEP_CHARS: usize = 80;
pub const STATUS_CHARS: usize = 50;
const RECAP_CHARS: usize = 400;
const ASK_CHARS: usize = 120;
const ACTION_CHARS: usize = 40;

const PLAN_DESCRIPTION: &str = "Shows the person your plan for this request as a short checklist, kept with the conversation \
across turns. Send the whole list (at most 12 short imperative steps, each pending, in_progress or done, exactly one in_progress \
while you work) with the first tool call of a task that has more than one step, in the same message, never in a message of its \
own. Then send {\"advance\": true} together with the call that starts the next step: it finishes the current step and starts the \
next pending one. Send the whole list again when the plan changes. Never repeat the plan as a list in your text.";

const STATUS_DESCRIPTION: &str = "Shows the person, in 3 to 6 words, what you are doing now (at most 50 characters), in their \
language, with no tool, command or file names: \"Fixing the login page\", \"Checking the new layout\". Call it when substantial \
work starts and again only when the phase of the work changes, together with another tool call, never on a timer.";

const WRAP_UP_DESCRIPTION: &str = "Closes a task with a quiet box at the end of your reply. Call it once, as your last tool \
call, when the task the person asked for ends or stops. recap: 2 to 3 short sentences in the person's language and in everyday \
words, saying what is now true, then what is not done or not verified; no next-steps list. ask: at most one short question, \
only when the next move is the person's to make, with actions: one or two replies of 1 to 3 words, your recommendation first. \
done: true when the person's whole request is finished and verified. Then end your turn without repeating the recap in text.";

fn plan_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "steps": {
                "type": "array",
                "maxItems": MAX_STEPS,
                "description": "The whole plan, in order.",
                "items": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "A short imperative step." },
                        "state": { "type": "string", "enum": ["pending", "in_progress", "done"] }
                    },
                    "required": ["text", "state"],
                    "additionalProperties": false
                }
            },
            "advance": { "type": "boolean", "description": "Finish the current step and start the next pending one." }
        },
        "additionalProperties": false
    })
}

fn status_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "text": { "type": "string", "description": "3 to 6 words, at most 50 characters." } },
        "required": ["text"],
        "additionalProperties": false
    })
}

fn wrap_up_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "recap": { "type": "string", "description": "2 to 3 sentences: what is now true, then what is not done or not verified." },
            "ask": { "type": "string", "description": "One short question for the person, only when the next move is theirs." },
            "actions": {
                "type": "array",
                "maxItems": 2,
                "items": { "type": "string" },
                "description": "With ask: one or two replies of 1 to 3 words, the recommendation first."
            },
            "done": { "type": "boolean", "description": "The person's whole request is finished and verified." }
        },
        "required": ["recap"],
        "additionalProperties": false
    })
}

/// The three tools as Codex `dynamicTools` entries.
pub fn codex_tools() -> [Value; 3] {
    [
        json!({ "type": "function", "name": PLAN, "description": PLAN_DESCRIPTION, "inputSchema": plan_schema() }),
        json!({ "type": "function", "name": STATUS, "description": STATUS_DESCRIPTION, "inputSchema": status_schema() }),
        json!({ "type": "function", "name": WRAP_UP, "description": WRAP_UP_DESCRIPTION, "inputSchema": wrap_up_schema() }),
    ]
}

/// The three tools as Faber's MCP server lists them to Claude Code.
pub fn mcp_tools() -> [Value; 3] {
    [
        json!({ "name": PLAN, "description": PLAN_DESCRIPTION, "inputSchema": plan_schema() }),
        json!({ "name": STATUS, "description": STATUS_DESCRIPTION, "inputSchema": status_schema() }),
        json!({ "name": WRAP_UP, "description": WRAP_UP_DESCRIPTION, "inputSchema": wrap_up_schema() }),
    ]
}

/// The name both drivers give a call of one of these tools, from the name Codex or the MCP server knows.
pub fn tool_of(name: &str) -> Option<&'static str> {
    match name {
        PLAN => Some(PLAN_TOOL),
        STATUS => Some(STATUS_TOOL),
        WRAP_UP => Some(WRAP_UP_TOOL),
        _ => None,
    }
}

/// The name both drivers give a call, from the name Claude Code knows.
pub fn claude_tool(name: &str) -> Option<&'static str> {
    match name {
        CLAUDE_PLAN => Some(PLAN_TOOL),
        CLAUDE_STATUS => Some(STATUS_TOOL),
        CLAUDE_WRAP_UP => Some(WRAP_UP_TOOL),
        _ => None,
    }
}

/// `text` on one line, trimmed, cut to `most` characters with an ellipsis.
fn line(text: &str, most: usize) -> String {
    let one = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match one.char_indices().nth(most) {
        Some((at, _)) => format!("{}…", one[..at].trim_end()),
        None => one,
    }
}

fn text_of(v: &Value, most: usize) -> Option<String> {
    v.as_str().map(|t| line(t, most)).filter(|t| !t.is_empty())
}

/// The plan after a `plan` call, from the one before: a new list replaces it, `advance` moves it one step on. At most one
/// step is in progress (the first one the list marks), and a list with no step clears the plan. Err says what to fix.
pub fn plan(before: Option<&Plan>, input: &Value, at: u64) -> Result<Option<Plan>, String> {
    let mut steps = match input.get("steps").and_then(Value::as_array) {
        Some(list) => {
            let mut active = false;
            let steps: Vec<PlanStep> = list
                .iter()
                .filter_map(|s| {
                    let text = text_of(&s["text"], STEP_CHARS)?;
                    let state = match s["state"].as_str() {
                        Some("done") => StepState::Done,
                        Some("in_progress") if !active => {
                            active = true;
                            StepState::Active
                        }
                        _ => StepState::Pending,
                    };
                    Some(PlanStep { text, state })
                })
                .take(MAX_STEPS)
                .collect();
            if steps.is_empty() {
                return Ok(None);
            }
            steps
        }
        None => before.map(|p| p.steps.clone()).ok_or("There is no plan yet: send the whole list in steps first.")?,
    };
    if input.get("advance").and_then(Value::as_bool) == Some(true) {
        if let Some(current) = steps.iter_mut().find(|s| s.state == StepState::Active) {
            current.state = StepState::Done;
        }
        if let Some(next) = steps.iter_mut().find(|s| s.state == StepState::Pending) {
            next.state = StepState::Active;
        }
    }
    Ok(Some(Plan { steps, at }))
}

/// What a `status` call shows.
pub fn status(input: &Value) -> Result<String, String> {
    text_of(&input["text"], STATUS_CHARS).ok_or_else(|| "Send the status as text: 3 to 6 words.".into())
}

/// The closing box a `wrap_up` call draws. A question needs at least one action, and actions need a question.
pub fn wrap_up(input: &Value) -> Result<WrapUp, String> {
    let recap = input["recap"].as_str().map(|r| r.trim()).filter(|r| !r.is_empty()).ok_or("Send a recap of 2 to 3 sentences.")?;
    let recap = match recap.char_indices().nth(RECAP_CHARS) {
        Some((at, _)) => format!("{}…", recap[..at].trim_end()),
        None => recap.to_string(),
    };
    let ask = text_of(&input["ask"], ASK_CHARS);
    let actions: Vec<String> = match &ask {
        Some(_) => input["actions"].as_array().into_iter().flatten().filter_map(|a| text_of(a, ACTION_CHARS)).take(2).collect(),
        None => vec![],
    };
    let ask = ask.filter(|_| !actions.is_empty());
    Ok(WrapUp { recap, ask, actions, done: input["done"].as_bool() == Some(true), answered: None })
}

/// The first sentence of a recap, for a running row, a done card and the tray: up to its first full stop, question or
/// exclamation mark followed by a space, or the whole recap.
pub fn first_sentence(recap: &str) -> String {
    let text = recap.trim();
    let end = text
        .char_indices()
        .find(|(i, c)| matches!(c, '.' | '!' | '?' | '。') && text[i + c.len_utf8()..].starts_with(char::is_whitespace))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(text.len());
    line(&text[..end], 160)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states(p: &Plan) -> Vec<(&str, StepState)> {
        p.steps.iter().map(|s| (s.text.as_str(), s.state)).collect()
    }

    #[test]
    fn a_plan_is_replaced_whole_moves_on_with_advance_and_keeps_one_step_in_progress() {
        let list = json!({ "steps": [
            { "text": "Read the login page", "state": "in_progress" },
            { "text": "Fix the redirect", "state": "in_progress" },
            { "text": "  Check   it\nworks ", "state": "pending" }
        ]});
        let p = plan(None, &list, 1).unwrap().unwrap();
        assert_eq!(
            states(&p),
            [("Read the login page", StepState::Active), ("Fix the redirect", StepState::Pending), ("Check it works", StepState::Pending)],
            "one step in progress, the first; words on one line"
        );
        let p = plan(Some(&p), &json!({ "advance": true }), 2).unwrap().unwrap();
        assert_eq!(states(&p)[..2], [("Read the login page", StepState::Done), ("Fix the redirect", StepState::Active)]);
        let p =
            plan(Some(&p), &json!({ "advance": true }), 3).and_then(|p| plan(p.as_ref(), &json!({ "advance": true }), 4)).unwrap().unwrap();
        assert!(p.steps.iter().all(|s| s.state == StepState::Done), "advancing past the end finishes it: {p:?}");
        assert!(plan(None, &json!({ "advance": true }), 5).is_err(), "nothing to advance yet");
        assert_eq!(plan(Some(&p), &json!({ "steps": [] }), 6).unwrap(), None, "an empty list clears it");
        let long = json!({ "steps": (0..20).map(|i| json!({ "text": format!("Step {i}"), "state": "pending" })).collect::<Vec<_>>() });
        assert_eq!(plan(None, &long, 7).unwrap().unwrap().steps.len(), MAX_STEPS);
    }

    #[test]
    fn a_status_is_one_short_line_and_a_wrap_up_question_comes_with_its_buttons() {
        assert_eq!(status(&json!({ "text": " Fixing the\nlogin page " })).unwrap(), "Fixing the login page");
        assert!(
            status(&json!({ "text": "Checking every screen of the app in all five languages at once" })).unwrap().chars().count()
                <= STATUS_CHARS + 1
        );
        assert!(status(&json!({})).is_err());
        let w = wrap_up(&json!({ "recap": "The page works. Not seen on a phone yet.", "ask": "Ship it?", "actions": ["Yes", "Not yet", "Later"], "done": true })).unwrap();
        assert_eq!((w.ask.as_deref(), w.actions.len(), w.done), (Some("Ship it?"), 2, true));
        let w = wrap_up(&json!({ "recap": "Done.", "ask": "Ship it?" })).unwrap();
        assert_eq!((w.ask, w.actions.len(), w.done), (None, 0, false), "a question with no button is no question");
        assert!(wrap_up(&json!({ "recap": " " })).is_err());
    }

    #[test]
    fn the_first_sentence_of_a_recap_ends_at_its_first_stop() {
        assert_eq!(first_sentence("The page works again. It was not seen on a phone."), "The page works again.");
        assert_eq!(first_sentence("Version 1.2 is out"), "Version 1.2 is out");
        assert_eq!(first_sentence("Pronto! Falta ver no celular."), "Pronto!");
    }
}
