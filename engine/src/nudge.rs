//! One step up when the work shows it. A model that stops early looks like a turn that ended without checking itself, so
//! two signs in the turn's own tools offer the next rung: the last check failed, or code changed and nothing checked it.
//! The offer is one step at a time (more effort first, a stronger model only when the model is already at its top) and
//! the person decides. Pure functions over a finished message; the engine feeds them.

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

const EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// The next rung above a pick: the same model one effort up when the table has that row, else the next stronger model
/// of the same vendor at its default (medium) effort, else None. Only profiles in `allowed` (what the account can run) are
/// offered. A pick the table does not know has no rung.
pub fn next_rung(now: &Pick, allowed: &[Profile]) -> Option<Pick> {
    let model = now.model.as_deref()?;
    let here = PROFILES.iter().find(|p| p.vendor == now.vendor && p.model == model && p.effort == now.effort)?;
    let up = EFFORTS.iter().position(|e| *e == here.effort)? + 1;
    let same = EFFORTS.get(up).and_then(|e| allowed.iter().find(|p| p.vendor == here.vendor && p.model == here.model && p.effort == *e));
    let next = same.or_else(|| {
        allowed
            .iter()
            .filter(|p| p.vendor == here.vendor && p.effort == "medium" && (p.iq, p.agent) > (here.iq, here.agent))
            .min_by_key(|p| (p.iq, p.agent))
    })?;
    Some(Pick {
        vendor: next.vendor,
        model: Some(next.model.into()),
        effort: next.effort.into(),
        reason: "reason.chosen".into(),
        pace: None,
    })
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
/// quotes. None when the line uses syntax this does not read: substitution, subshells, heredocs, an open quote.
fn segments(line: &str) -> Option<Vec<Vec<String>>> {
    let (mut all, mut words, mut word, mut started) = (vec![], vec![], String::new(), false);
    let mut quote: Option<char> = None;
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
            (None, '<') if next == Some('<') => return None,
            (None, '&' | '|' | ';' | '\n') => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
                all.push(std::mem::take(&mut words));
                if matches!(c, '&' | '|') && next == Some(c) {
                    chars.next();
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
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(word);
    }
    all.push(words);
    Some(all)
}

/// A program name without its folder or extension: `C:\bin\Cargo.EXE` is `cargo`.
fn program(word: &str) -> String {
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
fn command(tool: &ToolUse) -> Option<&str> {
    match tool.name.as_str() {
        "Bash" | "PowerShell" => tool.input["command"].as_str(),
        "commandExecution" => tool.input.as_str().or(tool.input["command"].as_str()),
        _ => None,
    }
}

fn is_check(tool: &ToolUse) -> bool {
    command(tool).is_some_and(|c| is_check_line(c, 0))
}

/// The paths a file-editing tool touched: Claude's `file_path`, Codex's `changes` (a list of `{path}`, or keyed by path).
fn edited(tool: &ToolUse) -> Vec<&str> {
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

fn is_source(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, ext)| SOURCE.iter().any(|s| ext.eq_ignore_ascii_case(s)))
}

/// What a finished turn offers, if anything. Only a turn that ended done, on a model the table knows, with a rung above that
/// `allowed` (the profiles the account can run) holds.
pub fn of(m: &Message, area: Area, allowed: &[Profile]) -> Option<Nudge> {
    if m.status != Some(Status::Done) {
        return None;
    }
    let now = Pick { vendor: m.vendor?, model: m.model.clone(), effort: m.effort.clone()?, reason: String::new(), pace: None };
    let next = next_rung(&now, allowed)?;
    let tools = m.tools.as_deref().unwrap_or_default();
    // A declined check never ran, so it neither failed nor verified anything.
    let last_check = tools.iter().rposition(|t| is_check(t) && !t.declined);
    let reason = if last_check.is_some_and(|i| tools[i].ok == Some(false)) {
        "nudge.failedCheck"
    } else if area == Area::Project {
        // A refused edit (ok = false) changed nothing; only the last edit to source decides.
        let last_edit = tools.iter().rposition(|t| t.ok != Some(false) && edited(t).into_iter().any(is_source));
        match last_edit {
            Some(i) if last_check.is_none_or(|c| c < i) => "nudge.noCheck",
            _ => return None,
        }
    } else {
        return None;
    };
    Some(Nudge { reason: reason.into(), vendor: next.vendor, model: next.model?, effort: next.effort })
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
    }

    #[test]
    fn a_model_at_its_top_effort_moves_to_the_next_model_at_medium() {
        assert_eq!(rung(Vendor::Claude, "claude-sonnet-5-5", "high"), to("claude-opus-5-5", "medium"));
        assert_eq!(rung(Vendor::Claude, "claude-haiku-5-5", "medium"), to("claude-sonnet-5-5", "medium"));
        assert_eq!(rung(Vendor::Codex, "gpt-6-luna", "medium"), to("gpt-6.1-sol", "medium"));
    }

    #[test]
    fn the_strongest_pick_of_a_vendor_and_a_pick_the_table_does_not_know_have_no_rung() {
        assert_eq!(rung(Vendor::Claude, "claude-opus-5-5", "high"), None);
        assert_eq!(rung(Vendor::Codex, "gpt-6.1-sol", "high"), None);
        assert_eq!(rung(Vendor::Claude, "claude-haiku-5-5", "high"), None);
        assert_eq!(rung(Vendor::Claude, "sonnet", "medium"), None);
        assert_eq!(next_rung(&Pick { model: None, ..pick(Vendor::Codex, "x", "medium") }, &PROFILES), None);
    }

    fn tool(name: &str, input: Value, ok: Option<bool>) -> ToolUse {
        ToolUse { id: format!("t{name}{input}"), name: name.into(), input, ok, output: None, declined: false }
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

    fn why(m: &Message, area: Area) -> Option<String> {
        of(m, area, &PROFILES).map(|n| n.reason)
    }

    #[test]
    fn a_last_check_that_failed_on_a_turn_that_ended_done_offers_the_next_rung() {
        let m = turn(vec![edit("src/a.rs"), bash("cd engine && cargo test --lib", false)]);
        let n = of(&m, Area::Project, &PROFILES).unwrap();
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
        let n = of(&m, Area::Project, &PROFILES).unwrap();
        assert_eq!((n.reason.as_str(), n.model.as_str(), n.effort.as_str()), ("nudge.failedCheck", "gpt-6.1-sol", "high"));
    }

    #[test]
    fn code_edited_and_never_checked_offers_the_next_rung_in_a_project_only() {
        let m = turn(vec![edit("src/a.rs"), bash("git diff", true)]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some("nudge.noCheck"));
        assert_eq!(why(&m, Area::Chat), None, "chat never asks for a check");
    }

    #[test]
    fn a_check_after_the_last_code_edit_clears_it_and_one_before_it_does_not() {
        let m = turn(vec![edit("src/a.rs"), bash("cargo build", true)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![bash("cargo build", true), edit("src/a.rs")]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some("nudge.noCheck"));
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
        let n = of(&m, Area::Project, &PROFILES).unwrap();
        assert_eq!((n.reason.as_str(), n.effort.as_str()), ("nudge.noCheck", "medium"));
        let notes = tool("fileChange", json!([{ "path": "docs/a.md", "kind": "add" }]), Some(true));
        m.tools = Some(vec![notes]);
        assert_eq!(of(&m, Area::Project, &PROFILES), None);
    }

    #[test]
    fn a_stopped_turn_and_a_pick_with_no_rung_offer_nothing() {
        let mut m = turn(vec![bash("cargo test", false)]);
        m.status = Some(Status::Stopped);
        assert_eq!(of(&m, Area::Project, &PROFILES), None);
        let mut m = turn(vec![bash("cargo test", false)]);
        m.model = Some("claude-opus-5-5".into());
        m.effort = Some("high".into());
        assert_eq!(of(&m, Area::Project, &PROFILES), None);
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
    fn a_search_that_finds_nothing_is_not_a_failed_check_and_an_echo_is_not_a_run_check() {
        let m = turn(vec![bash(r#"rg "cargo test" README.md"#, false)]);
        assert_eq!(why(&m, Area::Project), None);
        let m = turn(vec![edit("src/a.rs"), bash(r#"echo "cargo test""#, true)]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some("nudge.noCheck"));
    }

    #[test]
    fn a_declined_check_never_ran_so_it_is_neither_a_failure_nor_a_verification() {
        let declined = || ToolUse { declined: true, ..bash("cargo test", false) };
        assert_eq!(why(&turn(vec![declined()]), Area::Project), None);
        assert_eq!(why(&turn(vec![edit("src/a.rs"), declined()]), Area::Project).as_deref(), Some("nudge.noCheck"));
        // A check that ran and failed before the declined one still counts.
        let m = turn(vec![bash("cargo test", false), declined()]);
        assert_eq!(why(&m, Area::Project).as_deref(), Some("nudge.failedCheck"));
    }

    #[test]
    fn a_rung_the_account_cannot_run_is_not_offered() {
        // Codex lists Luna but not Sol: Luna has nothing above it that this account can run.
        let luna = candidates(&[Vendor::Codex], &["gpt-6-luna".to_string()]);
        assert_eq!(next_rung(&pick(Vendor::Codex, "gpt-6-luna", "medium"), &luna), None);
        let mut m = turn(vec![bash("cargo test", false)]);
        m.vendor = Some(Vendor::Codex);
        m.model = Some("gpt-6-luna".into());
        m.effort = Some("medium".into());
        assert_eq!(of(&m, Area::Project, &luna), None);
        assert!(of(&m, Area::Project, &PROFILES).is_some(), "the full table still offers Sol");
        // Without a connected Claude CLI, Haiku has no Sonnet to move to; effort steps stay on the same model.
        let sol = candidates(&[Vendor::Codex], &["gpt-6.1-sol".to_string()]);
        assert_eq!(next_rung(&pick(Vendor::Codex, "gpt-6.1-sol", "low"), &sol).map(|p| p.effort), Some("medium".into()));
        assert_eq!(next_rung(&pick(Vendor::Claude, "claude-haiku-5-5", "medium"), &sol), None);
        // A stronger model skips a missing one and lands on the next the account has.
        let no_sonnet: Vec<Profile> = PROFILES.into_iter().filter(|p| p.model != "claude-sonnet-5-5").collect();
        assert_eq!(rung_in(&no_sonnet, Vendor::Claude, "claude-haiku-5-5", "medium"), to("claude-opus-5-5", "medium"));
    }
}
