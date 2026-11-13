//! One step up when the work shows it. A model that stops early looks like a turn that ended without checking itself, so
//! two signs in the turn's own tools make an offer. Code that changed with nothing checking it first offers the cheapest
//! step, the one the makers name first: the same model checks its own work. The next rung is offered when the last check
//! failed, or when the answer after that offer is still unchecked. A rung is one step at a time (more effort first below
//! high, then the cheaper sufficient improvement) and the person decides. Pure functions over a finished message; the
//! engine feeds them.

use crate::model::{Area, Message, Status, ToolUse, Vendor};
use crate::route::{PROFILES, Profile};
use crate::router::Pick;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The retry offered under a finished answer: why, and the rung to run it on. `reason` is a key for the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nudge {
    pub reason: String,
    pub vendor: Vendor,
    pub model: String,
    pub effort: String,
}

/// What a finished answer offers.
#[derive(Debug, Clone, PartialEq)]
pub enum Offer {
    /// Code changed and nothing checked it: the model that wrote it is asked to check it. Nothing runs again, so this
    /// is kept apart from the rung (`Message::self_check`), and a build that knows only rungs offers nothing.
    SelfCheck,
    /// Run the request again one rung up.
    Rung(Nudge),
}

/// What the conversation already offered, so the same step is not offered twice.
#[derive(Debug, Clone, Copy, Default)]
pub struct Before {
    /// The answer just before this one offered the self-check: one still unchecked offers the rung instead.
    pub self_check: bool,
    /// An earlier answer offered the stronger model Auto would use; that offer is made once.
    pub model_bar: bool,
}

const EFFORTS: [&str; 4] = ["low", "medium", "high", "xhigh"];

/// Below high, raise effort first. From high on, compare that row with the next stronger model's lowest effort that
/// improves at least one score without lowering either; offer the cheaper (faster on a tie). Only account-allowed rows qualify.
pub fn next_rung(now: &Pick, allowed: &[Profile]) -> Option<Pick> {
    let model = now.model.as_deref()?;
    let here = PROFILES.iter().find(|p| p.vendor == now.vendor && p.model == model && p.effort == now.effort)?;
    let up = EFFORTS.iter().position(|e| *e == here.effort)? + 1;
    let same = EFFORTS.get(up).and_then(|e| allowed.iter().find(|p| p.vendor == here.vendor && p.model == here.model && p.effort == *e));
    let stronger = allowed
        .iter()
        .filter(|p| {
            p.vendor == here.vendor
                && p.model != here.model
                && p.iq >= here.iq
                && p.agent >= here.agent
                && (p.iq > here.iq || p.agent > here.agent)
        })
        .min_by_key(|p| (p.iq, p.agent));
    let next = if up < 3 {
        same.or(stronger)
    } else {
        same.into_iter().chain(stronger).min_by(|a, b| a.cost.total_cmp(&b.cost).then(a.secs.total_cmp(&b.secs)))
    }?;
    Some(Pick {
        vendor: next.vendor,
        model: Some(next.model.into()),
        effort: next.effort.into(),
        reason: "reason.chosen".into(),
        pace: None,
    })
}

/// The next stronger model of the same AI, for a piece that keeps ending with nothing changed (D7 step A): the weakest
/// allowed model that beats the current one on one score and loses on neither, at the current effort when that row exists,
/// else at its lowest. With it, how many times the current row's price a turn costs there (the table's relative cost).
pub fn stronger_model(now: &Pick, allowed: &[Profile]) -> Option<(Pick, f32)> {
    let model = now.model.as_deref()?;
    let here = PROFILES.iter().find(|p| p.vendor == now.vendor && p.model == model && p.effort == now.effort)?;
    let better = |p: &&Profile| {
        p.vendor == here.vendor
            && p.model != here.model
            && p.iq >= here.iq
            && p.agent >= here.agent
            && (p.iq > here.iq || p.agent > here.agent)
    };
    let target = allowed.iter().filter(better).min_by_key(|p| (p.iq, p.agent))?;
    let rows = || allowed.iter().filter(|p| p.vendor == here.vendor && p.model == target.model).filter(better);
    let next = rows().find(|p| p.effort == here.effort).or_else(|| rows().min_by_key(|p| EFFORTS.iter().position(|e| *e == p.effort)))?;
    let pick = Pick {
        vendor: next.vendor,
        model: Some(next.model.into()),
        effort: next.effort.into(),
        reason: "reason.chosen".into(),
        pace: None,
    };
    Some((pick, next.cost / here.cost.max(f32::EPSILON)))
}

// ponytail: command-line phrases read in the program position of each command segment; a check hidden in a script
// (`make ci`) is not seen, and so is anything this small reader does not understand (substitution, subshells,
// heredocs, an open quote). Better than a nudge nobody can trust, and a list the router could own later.
const CHECKS: [&[&str]; 14] = [
    &["cargo", "test"],
    &["cargo", "t"],
    &["cargo", "build"],
    &["cargo", "check"],
    &["cargo", "clippy"],
    &["go", "test"],
    &["go", "build"],
    &["dotnet", "test"],
    &["pytest"],
    &["tsc"],
    &["mvn"],
    &["mvnw"],
    &["gradle"],
    &["gradlew"],
];

/// Scripts of the JavaScript package managers that verify the work.
const SCRIPTS: [&str; 4] = ["test", "lint", "typecheck", "build"];

/// Code the person's own tests or build can judge; notes, docs and configuration do not count.
const SOURCE: [&str; 17] = ["rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "cs", "c", "h", "cpp", "hpp", "swift"];

/// Splits a command line into the words of each command, breaking at `&&`, `||`, `;`, `|`, `&` and newlines outside
/// quotes. A heredoc's body is text, not commands, and is skipped. None when the line uses syntax this does not read:
/// substitution, subshells, here-strings, a heredoc never closed, an open quote.
pub fn segments(line: &str) -> Option<Vec<Vec<String>>> {
    read_segments(line, false)
}

/// Commands whose successful shell result proves every segment succeeded: a single call or a chain joined by `&&`.
/// Alternatives, pipelines, background calls and unconditional separators cannot prove an individual commit succeeded.
pub fn successful_segments(line: &str) -> Option<Vec<Vec<String>>> {
    read_segments(line.trim(), true)
}

fn read_segments(line: &str, require_success: bool) -> Option<Vec<Vec<String>>> {
    let (mut all, mut words, mut word, mut started) = (vec![], vec![], String::new(), false);
    let mut quote: Option<char> = None;
    // The heredocs opened on the current line: their end word, and whether `<<-` lets it be indented with tabs.
    let mut bodies: Vec<(String, bool)> = vec![];
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        let next = chars.peek().copied();
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some('"'), '\\') if matches!(next, Some('"' | '\\')) => word.extend(chars.next()),
            (Some('"'), '`') => return None,
            (Some('"'), '$') if next == Some('(') => return None,
            (Some(_), _) => word.push(c),
            (None, '\'' | '"') => (quote, started) = (Some(c), true),
            (None, '`' | '(' | ')') => return None,
            (None, '$') if next == Some('(') => return None,
            (None, '<') if next == Some('<') => {
                chars.next();
                if chars.peek() == Some(&'<') {
                    return None;
                }
                let tabs = chars.next_if_eq(&'-').is_some();
                while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
                let mut tag = String::new();
                if let Some(q) = chars.next_if(|c| *c == '\'' || *c == '"') {
                    loop {
                        match chars.next() {
                            Some(c) if c == q => break,
                            Some('\n') | None => return None,
                            Some(c) => tag.push(c),
                        }
                    }
                } else {
                    while let Some(c) = chars.next_if(|c| !c.is_whitespace() && !"&|;<>()'\"`$".contains(*c)) {
                        tag.push(c);
                    }
                }
                // A tag glued to more quoted or bare text (`<<'E'OF`) is one word to the shell: too rare to read.
                if tag.is_empty() || chars.peek().is_some_and(|c| !c.is_whitespace() && !"&|;<>()".contains(*c)) {
                    return None;
                }
                bodies.push((tag, tabs));
            }
            (None, '#') if require_success => return None,
            (None, '&' | '|' | ';' | '\n') => {
                if require_success && !(c == '&' && next == Some('&')) {
                    return None;
                }
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
                all.push(std::mem::take(&mut words));
                if matches!(c, '&' | '|') && next == Some(c) {
                    chars.next();
                }
                // The lines after a heredoc's line are its body, up to its end word.
                let opened = if c == '\n' { std::mem::take(&mut bodies) } else { vec![] };
                for (tag, tabs) in opened {
                    loop {
                        let mut body = String::new();
                        let ended = loop {
                            match chars.next() {
                                Some('\n') => break false,
                                Some(c) => body.push(c),
                                None => break true,
                            }
                        };
                        let body = body.trim_end_matches('\r');
                        if (if tabs { body.trim_start_matches('\t') } else { body }) == tag {
                            break;
                        }
                        if ended {
                            return None;
                        }
                    }
                }
            }
            (None, _) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, _) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() || !bodies.is_empty() {
        return None;
    }
    if started {
        words.push(word);
    }
    all.push(words);
    Some(all)
}

/// A program name without its folder or extension: `C:\bin\Cargo.EXE` is `cargo`.
pub fn program(word: &str) -> String {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word).to_lowercase();
    ["exe", "cmd", "bat", "ps1"].iter().find_map(|e| name.strip_suffix(&format!(".{e}"))).unwrap_or(&name).to_string()
}

/// True when one command (already split into words) runs a verification: the program position holds the check, after
/// leading `NAME=value` words and the usual launchers. Shell launchers (`bash -lc "..."`, `powershell -Command "..."`)
/// hold a whole command line in one word, which is read again.
fn runs_check(words: &[String], depth: u8) -> bool {
    let words: Vec<String> = words
        .iter()
        .skip_while(|w| w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_')))
        .map(|w| w.to_lowercase())
        .collect();
    let Some((first, rest)) = words.split_first() else { return false };
    let prog = program(first);
    let after = |skip: usize| runs_check(rest.get(skip..).unwrap_or_default(), depth);
    match prog.as_str() {
        "time" | "exec" => after(0),
        "npx" | "pnpx" | "bunx" => after(rest.iter().take_while(|w| w.starts_with('-')).count()),
        "uv" => rest.first().is_some_and(|w| w == "run") && after(1),
        "python" | "python3" | "py" => rest.first().is_some_and(|w| w == "-m") && after(1),
        "npm" | "pnpm" | "yarn" | "bun" => {
            let script = usize::from(rest.first().is_some_and(|w| w == "run"));
            rest.get(script).is_some_and(|w| SCRIPTS.contains(&w.as_str())) || (prog != "npm" && after(0))
        }
        "bash" | "sh" | "zsh" | "dash" | "pwsh" | "powershell" | "cmd" if depth < 2 => {
            let flag = rest.iter().position(|w| matches!(w.as_str(), "-c" | "-lc" | "-ic" | "-command" | "/c"));
            flag.and_then(|i| rest.get(i + 1)).is_some_and(|line| is_check_line(line, depth + 1))
        }
        _ => {
            CHECKS.iter().any(|phrase| std::iter::once(&prog).chain(rest).map(String::as_str).take(phrase.len()).eq(phrase.iter().copied()))
        }
    }
}

/// True when some command of the line runs a verification (see `runs_check`).
fn is_check_line(line: &str, depth: u8) -> bool {
    segments(line).is_some_and(|all| all.iter().any(|words| runs_check(words, depth)))
}

/// The command line a shell tool ran: Claude's `Bash` and `PowerShell` carry `{command}`, Codex's `commandExecution` the
/// bare string.
pub fn command(tool: &ToolUse) -> Option<&str> {
    match tool.name.as_str() {
        "Bash" | "PowerShell" => tool.input["command"].as_str(),
        "commandExecution" => tool.input.as_str().or(tool.input["command"].as_str()),
        _ => None,
    }
}

pub fn is_check(tool: &ToolUse) -> bool {
    command(tool).is_some_and(|c| is_check_line(c, 0))
}

/// The paths a file-editing tool touched: Claude's `file_path`, Codex's `changes` (a list of `{path}`, or keyed by path).
pub fn edited(tool: &ToolUse) -> Vec<&str> {
    match tool.name.as_str() {
        "Edit" | "Write" | "MultiEdit" => tool.input["file_path"].as_str().into_iter().collect(),
        "fileChange" => match &tool.input {
            Value::Array(changes) => changes.iter().filter_map(|c| c["path"].as_str()).collect(),
            Value::Object(changes) => changes.keys().map(String::as_str).collect(),
            _ => vec![],
        },
        _ => vec![],
    }
}

pub fn is_source(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| SOURCE.iter().any(|s| ext.eq_ignore_ascii_case(s)))
}

/// Whether a call checked the work, and how it ended: a command this reader knows, or Faber's own verify when it ran its
/// checks. A declined check never ran, so it neither failed nor verified anything.
fn checked(tool: &ToolUse) -> Option<bool> {
    if is_check(tool) { (!tool.declined).then_some(tool.ok != Some(false)) } else { judged(tool) }
}

/// What a finished turn offers, if anything, and only when it ended done. A rung needs a model the table knows with a
/// rung above that `allowed` (the profiles the account can run) holds; the self-check needs neither.
pub fn of(m: &Message, area: Area, allowed: &[Profile], before: Before) -> Option<Offer> {
    if m.status != Some(Status::Done) {
        return None;
    }
    let tools = m.tools.as_deref().unwrap_or_default();
    let last_check = tools.iter().enumerate().rev().find_map(|(i, t)| checked(t).map(|passed| (i, passed)));
    let reason = if last_check.is_some_and(|(_, passed)| !passed) {
        "nudge.failedCheck"
    } else if !before.model_bar && m.moved_from.is_none() && m.acts.iter().any(|a| a.params.get("modelBar").is_some_and(|v| v == "unmet")) {
        "nudge.modelBar"
    } else if area == Area::Project {
        // A refused edit (ok = false) changed nothing; only the last edit to source decides.
        let last_edit = tools.iter().rposition(|t| t.ok != Some(false) && edited(t).into_iter().any(is_source));
        match last_edit {
            Some(i) if last_check.is_none_or(|(c, _)| c < i) && !before.self_check => return Some(Offer::SelfCheck),
            Some(i) if last_check.is_none_or(|(c, _)| c < i) => "nudge.noCheck",
            _ => return None,
        }
    } else {
        return None;
    };
    let now = Pick { vendor: m.vendor?, model: m.model.clone(), effort: m.effort.clone()?, reason: String::new(), pace: None };
    let next = next_rung(&now, allowed)?;
    Some(Offer::Rung(Nudge { reason: reason.into(), vendor: next.vendor, model: next.model?, effort: next.effort }))
}

/// The name both drivers give the call, whatever the CLI calls it, so the screen and the engine know one name.
pub const TOOL: &str = "Verify";

/// How the text of a verify that ran its checks begins (`verify::Outcome::text`), which is how a saved call is told from one
/// that ran nothing.
pub const RAN: &str = "Faber ran the ";

/// What a finished verify call says about the work, read from the call an answer keeps: whether its checks passed, or
/// None when none ran (no checks declared, nothing changed, refused, stopped) or the call was declined or never ended.
pub fn judged(tool: &crate::model::ToolUse) -> Option<bool> {
    let ran = tool.name == TOOL && !tool.declined && tool.output.as_deref().is_some_and(|text| text.starts_with(RAN));
    tool.ok.filter(|_| ran)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Role;
    use crate::route::candidates;
    use serde_json::json;

    fn pick(vendor: Vendor, model: &str, effort: &str) -> Pick {
        Pick { vendor, model: Some(model.into()), effort: effort.into(), reason: String::new(), pace: None }
    }

    fn rung(vendor: Vendor, model: &str, effort: &str) -> Option<(String, String)> {
        next_rung(&pick(vendor, model, effort), &PROFILES).map(|p| (p.model.unwrap(), p.effort))
    }

    fn rung_in(allowed: &[Profile], vendor: Vendor, model: &str, effort: &str) -> Option<(String, String)> {
        next_rung(&pick(vendor, model, effort), allowed).map(|p| (p.model.unwrap(), p.effort))
    }

    fn to(model: &str, effort: &str) -> Option<(String, String)> {
        Some((model.into(), effort.into()))
    }

    #[test]
    fn more_effort_comes_before_a_stronger_model() {
        assert_eq!(rung(Vendor::Claude, "claude-sonnet-5-5", "medium"), to("claude-sonnet-5-5", "high"));
        assert_eq!(rung(Vendor::Claude, "claude-opus-5-5", "medium"), to("claude-opus-5-5", "high"));
        assert_eq!(rung(Vendor::Codex, "gpt-6.1-sol", "low"), to("gpt-6.1-sol", "medium"));
        assert_eq!(rung(Vendor::Codex, "gpt-6.1-terra", "medium"), to("gpt-6.1-terra", "high"));
    }

    #[test]
    fn high_effort_compares_the_next_effort_with_the_next_models_sufficient_start() {
        for (vendor, model, effort, next_model, next_effort) in [
            (Vendor::Claude, "claude-sonnet-5-5", "high", "claude-opus-5-5", "medium"),
            (Vendor::Claude, "claude-opus-5-5", "high", "claude-fable-5-2", "medium"),
            (Vendor::Claude, "claude-opus-5-5", "xhigh", "claude-fable-5-2", "high"),
            (Vendor::Claude, "claude-fable-5-2", "medium", "claude-fable-5-2", "high"),
            (Vendor::Codex, "gpt-6.1-terra", "high", "gpt-6.1-sol", "medium"),
            (Vendor::Codex, "gpt-6.1-sol", "high", "gpt-6.1-sol", "xhigh"),
            (Vendor::Claude, "claude-haiku-5-5", "high", "claude-haiku-5-5", "xhigh"),
        ] {
            assert_eq!(rung(vendor, model, effort), to(next_model, next_effort), "{model} {effort}");
        }
    }

    #[test]
    fn a_model_at_its_top_effort_moves_to_the_next_models_lowest_sufficient_effort() {
        assert_eq!(rung(Vendor::Claude, "claude-sonnet-5-5", "xhigh"), to("claude-opus-5-5", "high"));
        assert_eq!(rung(Vendor::Claude, "claude-haiku-5-5", "xhigh"), to("claude-sonnet-5-5", "high"));
        assert_eq!(rung(Vendor::Codex, "gpt-6-luna", "medium"), to("gpt-6-luna", "high"));
        assert_eq!(rung(Vendor::Codex, "gpt-6-luna", "high"), to("gpt-6.1-terra", "low"));
        assert_eq!(rung(Vendor::Codex, "gpt-6.1-terra", "xhigh"), to("gpt-6.1-sol", "high"));
    }

    #[test]
    fn high_rungs_compare_price_then_time_and_keep_working_when_only_one_option_is_allowed() {
        let now = pick(Vendor::Claude, "claude-sonnet-5-5", "high");
        let same = *PROFILES.iter().find(|p| p.model == "claude-sonnet-5-5" && p.effort == "xhigh").unwrap();
        let next = *PROFILES.iter().find(|p| p.model == "claude-opus-5-5" && p.effort == "medium").unwrap();
        assert_eq!(next_rung(&now, &[same]).unwrap().model.as_deref(), Some(same.model));
        assert_eq!(next_rung(&now, &[next]).unwrap().model.as_deref(), Some(next.model));
        assert!(next_rung(&now, &[]).is_none());
        let tied = Profile { cost: same.cost, ..next };
        assert_eq!(next_rung(&now, &[same, tied]).unwrap().model.as_deref(), Some(tied.model));
        let expensive = Profile { cost: same.cost + 0.01, ..next };
        assert_eq!(next_rung(&now, &[same, expensive]).unwrap().model.as_deref(), Some(same.model));
        let weaker = Profile { agent: 32, ..next };
        assert!(next_rung(&now, &[weaker]).is_none(), "a model change cannot lower either score");
    }

    #[test]
    fn the_strongest_pick_of_a_vendor_and_a_pick_the_table_does_not_know_have_no_rung() {
        assert_eq!(rung(Vendor::Claude, "claude-fable-5-2", "high"), None);
        assert_eq!(rung(Vendor::Codex, "gpt-6.1-sol", "xhigh"), None);
        assert_eq!(rung(Vendor::Claude, "claude-haiku-5-5", "max"), None);
        assert_eq!(rung(Vendor::Claude, "sonnet", "medium"), None);
        assert_eq!(next_rung(&Pick { model: None, ..pick(Vendor::Codex, "x", "medium") }, &PROFILES), None);
    }

    fn tool(name: &str, input: Value, ok: Option<bool>) -> ToolUse {
        ToolUse {
            id: format!("t{name}{input}"),
            name: name.into(),
            input,
            ok,
            output: None,
            declined: false,
            interrupted: false,
            parent: None,
            sign: None,
            facts: None,
            told: vec![],
            team: None,
            runs_on: None,
            words: vec![],
            words_cut: false,
            words_chars: None,
            at: None,
            transcript: None,
            detail: None,
            change: None,
            check: false,
            started: None,
            ended: None,
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
        m.vendor = Some(Vendor::Claude);
        m.model = Some("claude-sonnet-5-5".into());
        m.effort = Some("medium".into());
        m.status = Some(Status::Done);
        m.tools = Some(tools);
        m
    }

    /// How the tests name the offer that is not a rung.
    const SELF_CHECK: &str = "selfCheck";

    /// The offer under `m`; `asked` says the answer before it already offered the self-check.
    fn offer(m: &Message, area: Area, allowed: &[Profile], asked: bool) -> Option<Offer> {
        of(m, area, allowed, Before { self_check: asked, model_bar: false })
    }

    /// The rung offered, for a test that expects one.
    fn rung_under(m: &Message, area: Area, allowed: &[Profile], asked: bool) -> Nudge {
        match offer(m, area, allowed, asked) {
            Some(Offer::Rung(n)) => n,
            other => panic!("a rung was expected, not {other:?}"),
        }
    }

    fn named(offer: Option<Offer>) -> Option<String> {
        offer.map(|o| match o {
            Offer::SelfCheck => SELF_CHECK.to_string(),
            Offer::Rung(n) => n.reason,
        })
    }

    fn why(m: &Message, area: Area) -> Option<String> {
        named(offer(m, area, &PROFILES, false))
    }

    /// The offer on an answer that follows one which already offered the self-check.
    fn why_after_the_ask(m: &Message, area: Area) -> Option<String> {
        named(offer(m, area, &PROFILES, true))
    }

    /// Faber's own verify as an answer keeps it: its result and the text the agent read.
    fn verify(ok: bool, text: &str) -> ToolUse {
        ToolUse { output: Some(text.into()), ..tool(TOOL, json!({ "level": "fast" }), Some(ok)) }
    }

    #[test]
    fn a_model_bar_offer_does_not_follow_a_quota_handover() {
        let mut m = turn(vec![]);
        m.effort = Some("high".into());
        let mut act = crate::act::FaberAct::route(&pick(Vendor::Claude, "claude-sonnet-5-5", "high"), crate::act::Actor::Faber);
        act.params.insert("modelBar".into(), "unmet".into());
        m.acts.push(act);
        assert_eq!(why(&m, Area::Chat).as_deref(), Some("nudge.modelBar"));
        m.moved_from = Some(Vendor::Codex);
        assert_eq!(why(&m, Area::Chat), None);
    }

    #[test]
    fn a_last_check_that_failed_on_a_turn_that_ended_done_offers_the_next_rung() {
        let m = turn(vec![edit("src/a.rs"), bash("cd engine && cargo test --lib", false)]);
        let n = rung_under(&m, Area::Project, &PROFILES, false);
        assert_eq!(
            (n.reason.as_str(), n.vendor, n.model.as_str(), n.effort.as_str()),
            ("nudge.failedCheck", Vendor::Claude, "claude-sonnet-5-5", "high")
        );
        assert_eq!(why(&m, Area::Chat).as_deref(), Some("nudge.failedCheck"));
    }

    #[test]
    fn a_passing_rerun_clears_the_failed_check() {
        let m = turn(vec![edit("src/a.rs"), bash("npm run typecheck", false), edit("src/a.rs"), bash("npm run typecheck", true)]);
        assert_eq!(why(&m, Area::Project), None);
    }

    #[test]
    fn only_the_last_check_counts_and_other_commands_are_not_checks() {
        let m = turn(vec![bash("cargo t", false), bash("cargo clippy -p faber", true)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![bash("cargo test", true), bash("git status", false)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![bash("cargo tree", false), bash("ls tsc-notes", false)]);
        assert_eq!(why(&m, Area::Project), None);
    }

    #[test]
    fn codex_commands_are_read_from_the_bare_string() {
        let mut m = turn(vec![tool("commandExecution", json!("pytest -q tests"), Some(false))]);
        m.vendor = Some(Vendor::Codex);
        m.model = Some("gpt-6.1-sol".into());
        m.effort = Some("medium".into());
        let n = rung_under(&m, Area::Project, &PROFILES, false);
        assert_eq!((n.reason.as_str(), n.model.as_str(), n.effort.as_str()), ("nudge.failedCheck", "gpt-6.1-sol", "high"));
    }

    #[test]
    fn code_edited_and_never_checked_asks_the_same_model_to_check_before_any_stronger_rung_in_a_project_only() {
        let m = turn(vec![edit("src/a.rs"), bash("git diff", true)]);
        assert_eq!(offer(&m, Area::Project, &PROFILES, false), Some(Offer::SelfCheck), "the first offer runs nothing again");
        let still = rung_under(&m, Area::Project, &PROFILES, true);
        assert_eq!(
            (still.reason.as_str(), still.model.as_str(), still.effort.as_str()),
            ("nudge.noCheck", "claude-sonnet-5-5", "high"),
            "an answer still unchecked after the ask offers one step up"
        );
        assert_eq!(why(&m, Area::Chat), None, "chat never asks for a check");
        assert_eq!(why_after_the_ask(&m, Area::Chat), None);
    }

    #[test]
    fn the_self_check_needs_no_rung_above_and_no_model_the_table_knows() {
        let mut top = turn(vec![edit("src/a.rs")]);
        (top.model, top.effort) = (Some("claude-fable-5-2".into()), Some("high".into()));
        assert_eq!(next_rung(&pick(Vendor::Claude, "claude-fable-5-2", "high"), &PROFILES), None);
        assert_eq!(why(&top, Area::Project).as_deref(), Some(SELF_CHECK));
        assert_eq!(why_after_the_ask(&top, Area::Project), None, "nothing stronger exists to offer");
        let mut unknown = turn(vec![edit("src/a.rs")]);
        (unknown.vendor, unknown.model, unknown.effort) = (None, None, None);
        assert_eq!(offer(&unknown, Area::Project, &PROFILES, false), Some(Offer::SelfCheck));
        assert_eq!(why_after_the_ask(&unknown, Area::Project), None);
    }

    #[test]
    fn a_failed_check_and_a_model_below_the_bar_still_offer_the_rung_at_once() {
        let failed = turn(vec![edit("src/a.rs"), bash("cargo test", false)]);
        assert_eq!(why(&failed, Area::Project).as_deref(), Some("nudge.failedCheck"), "the model already checked itself");
        assert_eq!(why_after_the_ask(&failed, Area::Project).as_deref(), Some("nudge.failedCheck"));
        let mut below = turn(vec![edit("src/a.rs")]);
        let mut act = crate::act::FaberAct::route(&pick(Vendor::Claude, "claude-sonnet-5-5", "medium"), crate::act::Actor::Faber);
        act.params.insert("modelBar".into(), "unmet".into());
        below.acts.push(act);
        assert_eq!(why(&below, Area::Project).as_deref(), Some("nudge.modelBar"));
        // The stronger model is offered once: after that, the unchecked code still gets its own offer.
        let once = Before { self_check: false, model_bar: true };
        assert_eq!(of(&below, Area::Project, &PROFILES, once), Some(Offer::SelfCheck));
        assert_eq!(of(&below, Area::Chat, &PROFILES, once), None);
        below.tools = Some(vec![]);
        assert_eq!(of(&below, Area::Project, &PROFILES, once), None);
    }

    #[test]
    fn fabers_own_verify_counts_as_a_check_only_when_it_ran_its_checks() {
        let passed = "Faber ran the fast checks of this folder in 4 s: 2 passed.\n";
        let failed = "Faber ran the fast checks of this folder in 4 s: 1 failed, 1 passed.\n\nFAILED npm test (3 s): 1 test failed\n";
        assert_eq!(why(&turn(vec![edit("src/a.rs"), verify(true, passed)]), Area::Project), None);
        assert_eq!(why(&turn(vec![verify(true, passed), edit("src/a.rs")]), Area::Project).as_deref(), Some(SELF_CHECK));
        assert_eq!(why(&turn(vec![edit("src/a.rs"), verify(false, failed)]), Area::Project).as_deref(), Some("nudge.failedCheck"));
        assert_eq!(why(&turn(vec![verify(false, failed), edit("src/a.rs"), verify(true, passed)]), Area::Project), None);
        // A verify that ran nothing proves nothing: no checks declared, nothing changed, refused, stopped.
        for (ok, text) in [
            (false, "This project declares no checks. Faber reads the commands under a \"Proof commands\" heading."),
            (true, "No file changed in this folder since its base, so the fast checks had nothing to check."),
            (false, "No check ran: Faber's checks run only in full access."),
            (false, "These checks were stopped: a newer verify of this folder started, or Faber is closing."),
        ] {
            let m = turn(vec![edit("src/a.rs"), verify(ok, text)]);
            assert_eq!(why(&m, Area::Project).as_deref(), Some(SELF_CHECK), "{text}");
            assert_eq!(why(&turn(vec![verify(ok, text)]), Area::Project), None, "{text}");
        }
        let declined = ToolUse { declined: true, ..verify(false, failed) };
        assert_eq!(why(&turn(vec![declined]), Area::Project), None);
        let unfinished = ToolUse { ok: None, output: None, ..verify(true, passed) };
        assert_eq!(why(&turn(vec![edit("src/a.rs"), unfinished]), Area::Project).as_deref(), Some(SELF_CHECK));
        // Another tool that happens to print the same words is not Faber's verify.
        let other = ToolUse { output: Some(passed.into()), ..tool("Read", json!({ "file_path": "log.txt" }), Some(true)) };
        assert_eq!(why(&turn(vec![edit("src/a.rs"), other]), Area::Project).as_deref(), Some(SELF_CHECK));
    }

    #[test]
    fn a_check_after_the_last_code_edit_clears_it_and_one_before_it_does_not() {
        let m = turn(vec![edit("src/a.rs"), bash("cargo build", true)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![bash("cargo build", true), edit("src/a.rs")]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some(SELF_CHECK));
        assert_eq!(why_after_the_ask(&m, Area::Project).as_deref(), Some("nudge.noCheck"));
        // Notes written after the check are not code.
        let m = turn(vec![edit("src/a.rs"), bash("cargo build", true), edit("docs/notes.md")]);
        assert_eq!(why(&m, Area::Project), None);
    }

    #[test]
    fn docs_config_and_refused_edits_do_not_ask_for_a_check() {
        for path in ["README.md", "notes.txt", "package.json", "config.toml", "Makefile"] {
            assert_eq!(why(&turn(vec![edit(path)]), Area::Project), None, "{path}");
        }
        let refused = tool("Edit", json!({ "file_path": "src/a.rs" }), Some(false));
        assert_eq!(why(&turn(vec![refused]), Area::Project), None);
    }

    #[test]
    fn codex_file_changes_count_as_edits() {
        let change = tool("fileChange", json!([{ "path": "src/main.py", "kind": "update" }]), Some(true));
        let mut m = turn(vec![change]);
        m.vendor = Some(Vendor::Codex);
        m.model = Some("gpt-6.1-sol".into());
        m.effort = Some("low".into());
        assert_eq!(offer(&m, Area::Project, &PROFILES, false), Some(Offer::SelfCheck));
        let n = rung_under(&m, Area::Project, &PROFILES, true);
        assert_eq!((n.reason.as_str(), n.effort.as_str()), ("nudge.noCheck", "medium"));
        let notes = tool("fileChange", json!([{ "path": "docs/a.md", "kind": "add" }]), Some(true));
        m.tools = Some(vec![notes]);
        assert_eq!(offer(&m, Area::Project, &PROFILES, false), None);
    }

    #[test]
    fn a_stopped_turn_and_a_pick_with_no_rung_offer_nothing() {
        let mut m = turn(vec![bash("cargo test", false)]);
        m.status = Some(Status::Stopped);
        assert_eq!(offer(&m, Area::Project, &PROFILES, false), None);
        let mut m = turn(vec![bash("cargo test", false)]);
        m.model = Some("claude-fable-5-2".into());
        m.effort = Some("high".into());
        assert_eq!(offer(&m, Area::Project, &PROFILES, false), None);
    }

    #[test]
    fn a_check_is_the_program_of_a_command_never_a_quoted_word_or_an_argument() {
        for line in [
            r#"echo "cargo test""#,
            r#"rg "cargo test" README.md"#,
            "grep -rn 'npm run lint' docs",
            "cat tsc.log",
            "git commit -m 'cargo build passes'",
            "cargo tree && ls pytest-notes",
            r#"bash -c "echo 'cargo test'""#,
            "echo $(cargo test)",
            "echo `cargo test`",
            "cat <<EOF\ncargo test\nEOF",
            "echo 'cargo test",
            "(cargo test)",
        ] {
            assert!(!is_check_line(line, 0), "{line}");
        }
    }

    #[test]
    fn a_check_is_found_in_any_command_of_the_line_behind_env_assignments_and_launchers() {
        for line in [
            "cargo test --lib",
            "cd engine && cargo clippy -- -D warnings",
            "false || cargo build; echo done",
            "cargo t 2>&1 | tail -5",
            "RUST_BACKTRACE=1 CI=true cargo test",
            "npx tsc --noEmit",
            "pnpm run lint",
            "yarn test",
            "npm run typecheck -- --pretty",
            "uv run pytest -q",
            "python -m pytest tests",
            r"C:\tools\Cargo.EXE check",
            "./gradlew test",
            r#"bash -lc "cd app && cargo test""#,
            r#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -Command "npm run build""#,
        ] {
            assert!(is_check_line(line, 0), "{line}");
        }
    }

    #[test]
    fn a_heredoc_body_is_text_and_the_commands_around_it_are_read() {
        assert!(is_check_line("cat > notes.md <<'EOF'\nIt's done, don't (worry).\nEOF\ncargo test", 0));
        assert!(is_check_line("cat <<-EOF > a.txt\n\tline\n\tEOF\ncargo build", 0), "<<- ends at a tab-indented word");
        assert!(is_check_line("cat > b.md <<\"END\"\r\nx\r\nEND\r\nnpm run lint", 0));
        assert!(is_check_line("cat <<EOF>out.md\nx\nEOF\ncargo test", 0), "a redirection right after the tag");
        assert_eq!(segments("cat <<A <<B\na\nA\nb\nB\nls").map(|s| s.len()), Some(2), "two bodies on one line");
        let glued = ["cat <<'E'OF\nE\ngh pr create\nEOF", "cat <<E'OF'\nx\nEOF"];
        for line in ["cat <<EOF\ncargo test", "cat <<EOF", "cat <<'EOF\nx", "cat <<<word", "cat << ;"].into_iter().chain(glued) {
            assert_eq!(segments(line), None, "{line}");
        }
    }

    #[test]
    fn a_search_that_finds_nothing_is_not_a_failed_check_and_an_echo_is_not_a_run_check() {
        let m = turn(vec![bash(r#"rg "cargo test" README.md"#, false)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![edit("src/a.rs"), bash(r#"echo "cargo test""#, true)]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some(SELF_CHECK));
    }

    #[test]
    fn a_declined_check_never_ran_so_it_is_neither_a_failure_nor_a_verification() {
        let declined = || ToolUse { declined: true, ..bash("cargo test", false) };
        assert_eq!(why(&turn(vec![declined()]), Area::Project), None);
        assert_eq!(why(&turn(vec![edit("src/a.rs"), declined()]), Area::Project).as_deref(), Some(SELF_CHECK));
        // A check that ran and failed before the declined one still counts.
        let m = turn(vec![bash("cargo test", false), declined()]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some("nudge.failedCheck"));
    }

    #[test]
    fn a_rung_the_account_cannot_run_is_not_offered() {
        // Codex lists Luna only: medium can rise to high, then no stronger allowed row remains.
        let luna = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
        assert_eq!(next_rung(&pick(Vendor::Codex, "gpt-6-luna", "medium"), &luna).unwrap().effort, "high");
        assert_eq!(next_rung(&pick(Vendor::Codex, "gpt-6-luna", "high"), &luna), None);
        let mut m = turn(vec![bash("cargo test", false)]);
        m.vendor = Some(Vendor::Codex);
        m.model = Some("gpt-6-luna".into());
        m.effort = Some("high".into());
        assert_eq!(offer(&m, Area::Project, &luna, false), None);
        assert!(offer(&m, Area::Project, &PROFILES, false).is_some(), "the full table still offers Terra");
        // Without a connected Claude CLI, Haiku has no Sonnet to move to; effort steps stay on the same model.
        let sol = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string()]);
        assert_eq!(next_rung(&pick(Vendor::Codex, "gpt-6.1-sol", "low"), &sol).map(|p| p.effort), Some("medium".into()));
        assert_eq!(next_rung(&pick(Vendor::Claude, "claude-haiku-5-5", "medium"), &sol), None);
        // A stronger model skips a missing one and lands on the next the account has.
        let no_sonnet: Vec<Profile> = PROFILES.into_iter().filter(|p| p.model != "claude-sonnet-5-5").collect();
        assert_eq!(rung_in(&no_sonnet, Vendor::Claude, "claude-haiku-5-5", "xhigh"), to("claude-opus-5-5", "medium"));
    }
}
