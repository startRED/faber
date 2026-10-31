//! What a project's agent tells the person about its own work, through three of Faber's tools (task 017): `plan`, the
//! conversation's checklist; `status`, a few words of what it is doing now; and `wrap_up`, the closing box of a task, whose
//! `done` also ends Keep going. Each call only writes down what it says: the engine stores it on the conversation or the
//! reply, the screen draws it, and the call answers at once. Both CLIs call them like the other tools of Faber's server.

use crate::model::{Estimate, Plan, PlanStep, StepState, WrapUp};
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
/// What a step's `id`, `files` and `waits` may hold. The agent reads these keys in the error of a refused `plan` call (they never
/// reach the screen, so the locales have no line for them), each followed by what to fix.
const ID_CHARS: usize = 24;
pub const MAX_FILES: usize = 20;
const FILE_CHARS: usize = 200;
pub const BAD_ID: &str = "error.plan.badId";
pub const DUPLICATE_ID: &str = "error.plan.duplicateId";
pub const BAD_FILE: &str = "error.plan.badFile";
pub const TOO_MANY_FILES: &str = "error.plan.tooManyFiles";
pub const UNKNOWN_WAIT: &str = "error.plan.unknownWait";
pub const SELF_WAIT: &str = "error.plan.selfWait";
pub const CYCLE: &str = "error.plan.cycle";
pub const NO_TEXT: &str = "error.plan.noText";

const PLAN_DESCRIPTION: &str = "Shows the person your plan for this request as a short checklist, kept with the conversation \
across turns. Send the whole list (at most 12 short imperative steps, each pending, in_progress or done, exactly one in_progress \
while you work) with the first tool call of a task that has more than one step, in the same message, never in a message of its \
own. Then send {\"advance\": true} together with the call that starts the next step: it finishes the current step and starts the \
next pending one. Send the whole list again when the plan changes. Never repeat the plan as a list in your text. For big \
work, also send estimate: what the whole job will cost in US dollars at API price, your honest guess; Faber then counts the \
spend and, when the job has cost half as much again, tells you to keep to the plan and use cheaper helpers.";

const STATUS_DESCRIPTION: &str = "Shows the person, in 3 to 6 words, what you are doing now (at most 50 characters), in their \
language, with no tool, command or file names: \"Fixing the login page\", \"Checking the new layout\". Call it when substantial \
work starts and again only when the phase of the work changes, together with another tool call, never on a timer.";

const WRAP_UP_DESCRIPTION: &str = "Closes a task with a quiet box at the end of your reply. Call it once, as your last tool \
call, when the task the person asked for ends or stops. recap: 2 to 3 short sentences in the person's language and in everyday \
words, saying what is now true, then what is not done or not verified; no next-steps list. ask: at most one short question, \
only when the next move is the person's to make, with actions: one or two replies of 1 to 3 words, your recommendation first. \
done: true when the person's whole request is finished and verified. When the task fixed a mistake, first look where the same \
kind of mistake could also be (other callers, sibling code, copies of the same text); if you find it, name the places in the \
recap and make fixing them your ask, and never fix them unasked. Then end your turn without repeating the recap in text.";

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
                        "state": { "type": "string", "enum": ["pending", "in_progress", "done"] },
                        "id": { "type": "string", "description": "Short id for waits; default s1, s2 by position." },
                        "files": { "type": "array", "maxItems": MAX_FILES, "items": { "type": "string" }, "description": "Paths or folders it will change, relative to the project." },
                        "waits": { "type": "array", "items": { "type": "string" }, "description": "Ids of steps to finish first." }
                    },
                    "required": ["text", "state"],
                    "additionalProperties": false
                }
            },
            "advance": { "type": "boolean", "description": "Finish the current step and start the next pending one." },
            "estimate": { "type": "number", "description": "For big work: the whole job's cost in US dollars at API price." }
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

/// The most an estimate may say, in US dollars: above it the number is a mistake, not a job.
const MAX_ESTIMATE_USD: f64 = 10_000.0;

/// What the model reads while economy mode is on (rule 7), added to the answer of every `plan` call.
pub const ECONOMY_LINE: &str =
    "The job has cost half as much again as your estimate: keep to the plan, use cheap helpers for support work, and drop extras.";

/// Whether a job has spent more than half again its estimate: `spent` is the conversation's spend now, in cents.
pub fn passed(e: &Estimate, spent: u64) -> bool {
    spent.saturating_sub(e.base) * 2 > u64::from(e.cents) * 3
}

/// The job's estimate after a `plan` call, from the one the conversation has. The first number starts the count from `spent`
/// cents; a different one later only changes what the job is measured against, and the count stays from the first, so
/// re-estimating cannot dodge the warning. Once economy mode is on, a number changes nothing. Err says what to fix.
pub fn estimate(had: Option<Estimate>, input: &Value, spent: u64) -> Result<Option<Estimate>, String> {
    let Some(v) = input.get("estimate").filter(|v| !v.is_null()) else { return Ok(had) };
    match v.as_f64() {
        Some(usd) if usd > 0.0 && usd <= MAX_ESTIMATE_USD => {
            let cents = (usd * 100.0).round().max(1.0) as u32;
            Ok(Some(match had {
                Some(e) if e.economy => e,
                Some(e) => Estimate { cents, ..e },
                None => Estimate { cents, base: spent, economy: false },
            }))
        }
        _ => Err(format!("Send estimate as a number of US dollars above 0 and at most {MAX_ESTIMATE_USD}.")),
    }
}

/// The plan after a `plan` call, from the one before: a new list replaces it, `advance` moves it one step on. At most one
/// step is in progress (the first one the list marks), and a list with no step clears the plan. Err says what to fix: a
/// list whose ids, files or waits are wrong is refused whole with an `error.plan.*` key, never trimmed to something else.
pub fn plan(before: Option<&Plan>, input: &Value, at: u64) -> Result<Option<Plan>, String> {
    let mut steps = match input.get("steps").and_then(Value::as_array) {
        Some(list) => {
            let mut active = false;
            let mut steps: Vec<PlanStep> = Vec::new();
            for s in list {
                let Some(text) = text_of(&s["text"], STEP_CHARS) else {
                    // A blank step is skipped, but one that carries an id, files or waits would drop them unseen.
                    if ["id", "files", "waits"].iter().any(|k| !s[*k].is_null()) {
                        return Err(format!("{NO_TEXT}: a step with an id, files or waits needs its text"));
                    }
                    continue;
                };
                if steps.len() == MAX_STEPS {
                    break;
                }
                let state = match s["state"].as_str() {
                    Some("done") => StepState::Done,
                    Some("in_progress") if !active => {
                        active = true;
                        StepState::Active
                    }
                    _ => StepState::Pending,
                };
                // No id sent: `s<position>`, so a rewrite of the same list keeps every id and waits on the position stay valid.
                let id = match &s["id"] {
                    Value::Null => format!("s{}", steps.len() + 1),
                    Value::String(id) if !id.is_empty() && id.chars().count() <= ID_CHARS && !id.contains(char::is_whitespace) => {
                        id.clone()
                    }
                    other => return Err(format!("{BAD_ID}: {}", line(&other.to_string(), ID_CHARS))),
                };
                let files = names(&s["files"], BAD_FILE)?;
                if files.len() > MAX_FILES {
                    return Err(format!("{TOO_MANY_FILES}: {id} has {}, at most {MAX_FILES}", files.len()));
                }
                let files = files
                    .into_iter()
                    .map(|f| path_of(f).ok_or_else(|| format!("{BAD_FILE}: {}", line(f, FILE_CHARS))))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut waits: Vec<String> = Vec::new();
                for w in names(&s["waits"], UNKNOWN_WAIT)? {
                    if !waits.iter().any(|seen| seen == w) {
                        waits.push(w.to_string());
                    }
                }
                steps.push(PlanStep { id, text, state, files, waits });
            }
            if steps.is_empty() {
                return Ok(None);
            }
            check_waits(&steps)?;
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
    Ok(Some(Plan { steps, at, estimate: None }))
}

/// The strings of an optional list; anything that is not a string becomes "" (so the checks after refuse it), anything that
/// is not a list is refused with `key`.
fn names<'a>(v: &'a Value, key: &str) -> Result<Vec<&'a str>, String> {
    match v {
        Value::Null => Ok(vec![]),
        Value::Array(items) => Ok(items.iter().map(|i| i.as_str().unwrap_or("")).collect()),
        _ => Err(key.to_string()),
    }
}

/// A file or folder as the plan keeps it: relative to the project, written with `/`, without a leading `./` or a trailing
/// `/`. None when it is empty, too long, absolute (a root, a drive or a UNC share) or leaves the project with `..`.
fn path_of(raw: &str) -> Option<String> {
    let path = raw.trim().replace('\\', "/");
    let path = path.trim_start_matches("./").trim_end_matches('/');
    let outside = path.is_empty()
        || path == "."
        || path.starts_with('/')
        || path.as_bytes().get(1) == Some(&b':')
        || path.contains('\0')
        || path.split('/').any(|part| part == "..")
        || path.chars().count() > FILE_CHARS;
    (!outside).then(|| path.to_string())
}

/// Refuses ids used twice, a step waiting on itself or on an id the plan does not have, and waits that go round in a circle.
fn check_waits(steps: &[PlanStep]) -> Result<(), String> {
    for (i, s) in steps.iter().enumerate() {
        if steps[..i].iter().any(|before| before.id == s.id) {
            return Err(format!("{DUPLICATE_ID}: {}", s.id));
        }
    }
    for s in steps {
        for w in &s.waits {
            if *w == s.id {
                return Err(format!("{SELF_WAIT}: {}", s.id));
            }
            if !steps.iter().any(|other| other.id == *w) {
                return Err(format!("{UNKNOWN_WAIT}: {} waits on {w}", s.id));
            }
        }
    }
    // Start with the steps that wait on nothing and keep adding the ones whose waits are all in: what is left never starts.
    let (mut left, mut started): (Vec<&PlanStep>, Vec<&str>) = (steps.iter().collect(), vec![]);
    loop {
        let (go, stay): (Vec<_>, Vec<_>) = left.into_iter().partition(|s| s.waits.iter().all(|w| started.contains(&w.as_str())));
        if go.is_empty() {
            return match stay.is_empty() {
                true => Ok(()),
                false => Err(format!("{CYCLE}: {}", stay.iter().map(|s| s.id.as_str()).collect::<Vec<_>>().join(", "))),
            };
        }
        started.extend(go.iter().map(|s| s.id.as_str()));
        left = stay;
    }
}

/// The steps that can start now: pending, with every step they wait on done. A plan stored before waits existed has none, so
/// all its pending steps are ready, as before.
pub fn ready(plan: &Plan) -> Vec<&PlanStep> {
    let done = |id: &String| plan.steps.iter().any(|s| s.state == StepState::Done && s.id == *id);
    plan.steps.iter().filter(|s| s.state == StepState::Pending && s.waits.iter().all(done)).collect()
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
    fn the_first_estimate_starts_the_count_a_new_number_keeps_it_and_economy_mode_holds_its_estimate() {
        let with = |usd: Value| json!({ "estimate": usd });
        let first = estimate(None, &with(json!(1.0)), 0).unwrap();
        assert_eq!(first, Some(Estimate { cents: 100, base: 0, economy: false }), "counted from what was spent when it came");
        assert_eq!(estimate(first, &json!({}), 300).unwrap(), first, "a call with no number keeps it");
        assert_eq!(estimate(first, &with(json!(null)), 300).unwrap(), first, "a null is no number");
        assert_eq!(estimate(first, &with(json!(1.0)), 450).unwrap(), first, "the same number again changes nothing");
        // The estimate is for the whole job: another number moves the target, never the start of the count.
        let revised = estimate(first, &with(json!(0.8)), 140).unwrap();
        assert_eq!(revised, Some(Estimate { cents: 80, base: 0, economy: false }));
        assert!(passed(&revised.unwrap(), 140), "1.40 spent past a revised 0.80");
        assert!(!passed(&Estimate { base: 140, ..revised.unwrap() }, 140), "a reset base would have dodged the warning");
        let on = Some(Estimate { economy: true, ..first.unwrap() });
        assert_eq!(estimate(on, &with(json!(2.5)), 900).unwrap(), on, "in economy mode a number changes nothing");
        for bad in [json!(0), json!(-3), json!("5"), json!(1e9)] {
            assert!(estimate(None, &with(bad.clone()), 0).is_err(), "{bad}");
        }
        assert!(PLAN_DESCRIPTION.contains("estimate"), "the lead is told to give one for big work");
    }

    #[test]
    fn a_plan_never_carries_the_estimate_it_belongs_to_the_job_not_the_list() {
        let list =
            json!({ "steps": [{ "text": "Build it", "state": "in_progress" }, { "text": "Check it", "state": "done" }], "estimate": 1.0 });
        assert_eq!(plan(None, &list, 1).unwrap().unwrap().estimate, None);
        assert_eq!(plan(None, &json!({ "steps": [], "estimate": 1.0 }), 2).unwrap(), None, "an empty list clears the plan alone");
    }

    #[test]
    fn a_job_has_passed_its_estimate_only_above_one_and_a_half_times_it() {
        let e = Estimate { cents: 100, base: 40, economy: false };
        assert!(!passed(&e, 40) && !passed(&e, 189), "under");
        assert!(!passed(&e, 190), "exactly one and a half times: no");
        assert!(passed(&e, 191), "just above");
        assert!(!passed(&e, 10), "spend below the base never counts");
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
    fn the_closing_box_asks_about_the_same_mistake_elsewhere_only_after_a_fix() {
        // Look around (R369): scoped to the close of a fix, never a rule for every turn, and never a fix unasked.
        for words in ["When the task fixed a mistake", "same kind of mistake", "make fixing them your ask", "never fix them unasked"] {
            assert!(WRAP_UP_DESCRIPTION.contains(words), "{words}");
        }
        let tuning = crate::model::Tuning::default();
        let persona = crate::persona::persona(&crate::persona::PersonaInput { name: None, tuning: &tuning, research: None });
        assert!(!persona.contains("same kind of mistake"), "the persona weighs on every turn of both areas");
    }

    fn refused(input: Value) -> String {
        plan(None, &input, 1).unwrap_err()
    }

    #[test]
    fn a_step_names_its_files_and_the_steps_it_waits_on_and_the_plan_round_trips() {
        let list = json!({ "steps": [
            { "text": "Build the model", "state": "in_progress", "id": "model", "files": ["src/model.rs", "./src\\db/", "src/model.rs"] },
            { "text": "Build the screen", "state": "pending", "files": ["ui"], "waits": ["model", "model"] },
            { "text": "Check it", "state": "pending", "waits": ["model", "s2"] }
        ]});
        let p = plan(None, &list, 1).unwrap().unwrap();
        let ids: Vec<_> = p.steps.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["model", "s2", "s3"], "an id left out is its position");
        assert_eq!(p.steps[0].files, ["src/model.rs", "src/db", "src/model.rs"], "paths use / with no ./ or trailing /");
        assert_eq!(
            (p.steps[1].waits.as_slice(), p.steps[2].waits.as_slice()),
            (&["model".to_string()][..], &["model".into(), "s2".into()][..])
        );
        let again: Plan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(again, p, "stored and loaded back");
        // A rewrite of the same list keeps every id, and an advance only moves the states.
        assert_eq!(plan(Some(&p), &list, 2).unwrap().unwrap().steps, p.steps);
        let moved = plan(Some(&p), &json!({ "advance": true }), 3).unwrap().unwrap();
        assert_eq!((moved.steps[1].id.as_str(), moved.steps[1].waits.len()), ("s2", 1));
    }

    #[test]
    fn a_plan_stored_before_ids_files_and_waits_loads_and_every_pending_step_is_ready() {
        let old =
            r#"{"steps":[{"text":"Read","state":"done"},{"text":"Fix","state":"pending"},{"text":"Check","state":"pending"}],"at":5}"#;
        let p: Plan = serde_json::from_str(old).unwrap();
        assert!(p.steps.iter().all(|s| s.id.is_empty() && s.files.is_empty() && s.waits.is_empty()));
        assert_eq!(ready(&p).len(), 2);
        let stored = serde_json::to_string(&p).unwrap();
        assert!(!stored.contains("\"id\"") && !stored.contains("files") && !stored.contains("waits"), "nothing new is written for them");
        let advanced = plan(Some(&p), &json!({ "advance": true }), 6).unwrap().unwrap();
        assert_eq!(advanced.steps[1].state, StepState::Active);
    }

    #[test]
    fn an_unknown_wait_a_wait_on_itself_a_cycle_and_a_repeated_id_are_refused_with_their_keys() {
        let wait = |waits: Value| {
            refused(json!({ "steps": [
                { "text": "A", "state": "pending" },
                { "text": "B", "state": "pending", "waits": waits }
            ]}))
        };
        assert!(wait(json!(["s9"])).starts_with(UNKNOWN_WAIT), "an id the plan does not have");
        assert!(wait(json!([7])).starts_with(UNKNOWN_WAIT), "a wait that is no id");
        assert!(wait(json!("s1")).starts_with(UNKNOWN_WAIT), "waits that is no list");
        assert!(wait(json!(["s2"])).starts_with(SELF_WAIT));
        let round = refused(json!({ "steps": [
            { "text": "A", "state": "pending", "waits": ["s3"] },
            { "text": "B", "state": "pending", "waits": ["s1"] },
            { "text": "C", "state": "pending", "waits": ["s1"] },
            { "text": "D", "state": "pending" }
        ]}));
        assert!(round.starts_with(CYCLE) && round.ends_with("s1, s2, s3"), "{round}: the steps that never start, not the ones that do");
        let twice = refused(json!({ "steps": [{ "text": "A", "state": "pending", "id": "s2" }, { "text": "B", "state": "pending" }] }));
        assert!(twice.starts_with(DUPLICATE_ID), "{twice}");
        assert!(refused(json!({ "steps": [{ "text": "A", "state": "pending", "id": "two words" }] })).starts_with(BAD_ID));
        assert!(refused(json!({ "steps": [{ "text": "A", "state": "pending", "id": "x".repeat(25) }] })).starts_with(BAD_ID));
        for id in [json!(" "), json!(""), json!(3)] {
            assert!(
                refused(json!({ "steps": [{ "text": "A", "state": "pending", "id": id }] })).starts_with(BAD_ID),
                "{id} is sent, so never replaced"
            );
        }
        // A blank step that carries a wait would otherwise vanish with it, and an all-blank list would clear the plan.
        let blank = refused(json!({ "steps": [{ "text": "", "state": "pending", "id": "a", "waits": ["missing"] }] }));
        assert!(blank.starts_with(NO_TEXT), "{blank}");
        // A refused list changes nothing: the plan before stays, since the engine writes it only on Ok.
        let before = plan(None, &json!({ "steps": [{ "text": "A", "state": "pending" }] }), 1).unwrap();
        assert!(plan(before.as_ref(), &json!({ "steps": [{ "text": "A", "state": "pending", "waits": ["A"] }] }), 2).is_err());
    }

    #[test]
    fn a_file_stays_inside_the_project_and_a_step_names_at_most_twenty() {
        let files = |files: Value| refused(json!({ "steps": [{ "text": "A", "state": "pending", "files": files }] }));
        for bad in
            ["/etc/passwd", "\\\\server\\share\\a", "C:\\work\\a.rs", "c:a.rs", "../x", "src/../../x", "a\\..\\..\\x", "", "./", " ", "."]
        {
            assert!(files(json!([bad])).starts_with(BAD_FILE), "{bad:?}");
        }
        assert!(files(json!(["a".repeat(201)])).starts_with(BAD_FILE), "too long is refused, never cut");
        assert!(files(json!([3])).starts_with(BAD_FILE) && files(json!("a.rs")).starts_with(BAD_FILE));
        let ok = json!({ "steps": [{ "text": "A", "state": "pending", "files": ["a".repeat(200), "..hidden/x", "a..b"] }] });
        assert_eq!(plan(None, &ok, 1).unwrap().unwrap().steps[0].files.len(), 3);
        let many = |n: usize| json!((0..n).map(|i| format!("f{i}.rs")).collect::<Vec<_>>());
        assert!(files(many(21)).starts_with(TOO_MANY_FILES));
        assert!(plan(None, &json!({ "steps": [{ "text": "A", "state": "pending", "files": many(20) }] }), 1).is_ok());
    }

    #[test]
    fn ready_follows_the_waits_of_the_pending_steps() {
        let list = json!({ "steps": [
            { "text": "A", "state": "in_progress", "id": "a" },
            { "text": "B", "state": "pending", "id": "b" },
            { "text": "C", "state": "pending", "id": "c", "waits": ["a"] },
            { "text": "D", "state": "pending", "id": "d", "waits": ["b", "c"] }
        ]});
        let ids = |p: &Plan| ready(p).iter().map(|s| s.id.clone()).collect::<Vec<_>>();
        let mut p = plan(None, &list, 1).unwrap().unwrap();
        assert_eq!(ids(&p), ["b"], "C waits on a step in progress, D on two that are not done");
        p.steps[0].state = StepState::Done;
        assert_eq!(ids(&p), ["b", "c"], "a started or finished step is not ready to start");
        p.steps[1].state = StepState::Done;
        p.steps[2].state = StepState::Done;
        assert_eq!(ids(&p), ["d"]);
        p.steps[3].state = StepState::Done;
        assert!(ready(&p).is_empty());
    }

    #[test]
    fn the_plan_tool_offers_the_new_fields_and_the_lead_is_asked_for_them() {
        let props = &plan_schema()["properties"]["steps"]["items"]["properties"];
        assert!(props["id"].is_object() && props["files"]["maxItems"] == MAX_FILES && props["waits"]["type"] == "array");
        assert!(crate::persona::LEADS.contains("(files)") && crate::persona::LEADS.contains("(waits)"));
    }

    #[test]
    fn the_first_sentence_of_a_recap_ends_at_its_first_stop() {
        assert_eq!(first_sentence("The page works again. It was not seen on a phone."), "The page works again.");
        assert_eq!(first_sentence("Version 1.2 is out"), "Version 1.2 is out");
        assert_eq!(first_sentence("Pronto! Falta ver no celular."), "Pronto!");
    }
}
