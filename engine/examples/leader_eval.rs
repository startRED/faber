//! Manual measurement of when a project's agent splits a request into conversations of its own (R160, R161: it decides alone
//! when to lead). Never part of `cargo t`: it runs the real CLIs with the person's own sign-in and spends a little quota.
//!
//!     cargo run -p faber-engine --example leader_eval [-- claude|codex]
//!
//! Each of twelve hand-labelled requests (six big, six small) goes to a fresh project conversation, through the real engine,
//! so the agent's instructions and Faber's tools are exactly the product's, in a throwaway git repository under the system's
//! temporary folder. A run stops once the reply ends, three conversations were started, or `RUN_LIMIT` passed, and every
//! conversation is then stopped: the decision is measured, not the work. It prints a verdict per request and a summary per vendor. Run it before and after a
//! change to the agent's instructions or tools.
use faber_engine::cli::find_cli;
use faber_engine::model::{Area, Message, Permissions, Role, Settings, Status, Vendor};
use faber_engine::proc;
use faber_engine::router::Choice;
use faber_engine::sibling;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Long enough for an agent to read the request and the files and decide; a direct answer to a big request goes on past it.
const RUN_LIMIT: Duration = Duration::from_secs(150);

/// (big, request), worded the way a person writes them; the fixture repository below has the files they name.
const REQUESTS: [(bool, &str); 12] = [
    (true, "Add dark mode to every screen, with a switch in Settings, tests and docs."),
    (true, "Migrate the three services in services/ from callbacks to async/await, with tests for each service."),
    (true, "Add Spanish and French to every screen, with a language picker in Settings, tests and docs."),
    (true, "Add input validation with clear error messages to the form of every screen, with tests and a page documenting the rules."),
    (true, "Give the project a full test suite: unit tests for every module in src/ and services/, a script that runs them, and docs."),
    (true, "Build an admin area: a screen listing users, editing their roles and an audit log of changes, with tests and docs."),
    (false, "Rename the function formatDate in src/app.js to formatDay and update its callers."),
    (false, "Why does the test in test/app.test.js fail?"),
    (false, "Fix the typo \"Seetings\" in src/screens/settings.js."),
    (false, "What does services/billing/index.js do? Two sentences."),
    (false, "Change the default page size in src/settings.js from 10 to 20."),
    (false, "Add a one-line comment above formatDate in src/app.js saying what its argument is."),
];

/// A small app with screens, services and a failing test: enough for each request to be about something real.
const FILES: [(&str, &str); 10] = [
    ("package.json", "{ \"name\": \"tiny-app\", \"scripts\": { \"test\": \"node test/app.test.js\" } }\n"),
    ("src/app.js", "function formatDate(ms) {\n  return new Date(ms).toISOString().slice(0, 10);\n}\nmodule.exports = { formatDate };\n"),
    ("src/settings.js", "module.exports = { pageSize: 10, theme: 'light' };\n"),
    (
        "src/screens/home.js",
        "const { formatDate } = require('../app');\nmodule.exports = () => `<h1>Home</h1><form><input name=q></form><p>${formatDate(Date.now())}</p>`;\n",
    ),
    ("src/screens/profile.js", "module.exports = (user) => `<h1>Profile</h1><form><input name=email value=\"${user.email}\"></form>`;\n"),
    ("src/screens/settings.js", "module.exports = () => `<h1>Seetings</h1><form><select name=theme></select></form>`;\n"),
    ("services/auth/index.js", "exports.login = (user, cb) => setTimeout(() => cb(null, { user, token: 'x' }), 10);\n"),
    (
        "services/billing/index.js",
        "exports.charge = (user, cents, cb) => setTimeout(() => cb(cents > 0 ? null : new Error('no amount'), { user, cents }), 10);\n",
    ),
    ("services/search/index.js", "exports.find = (q, cb) => setTimeout(() => cb(null, [q]), 10);\n"),
    (
        "test/app.test.js",
        "const assert = require('assert');\nconst { formatDate } = require('../src/app');\nassert.strictEqual(formatDate(0), '1970-01-02');\n",
    ),
];

fn hidden(bin: &str) -> Command {
    let command = Command::new(bin);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(0x0800_0000);
        command
    };
    command
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    let out = hidden("git").args(args).current_dir(root).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1").output();
    match out {
        Ok(o) if o.status.success() => Ok(()),
        _ => Err(format!("throwaway git failed: {args:?}")),
    }
}

/// What the agent decided in one reply: the conversations it started and the longest plan it sent.
#[derive(Debug, Default, PartialEq)]
struct Tally {
    starts: usize,
    plan: usize,
}

/// Counts the conversation tool's starts among a reply's steps, read the way the tool reads them (a `title` and `brief` with no
/// `action` is a start too); `plan` is the most steps its plan held.
fn tally<'a>(steps: impl IntoIterator<Item = (&'a str, &'a Value)>, plan: usize) -> Tally {
    let starts = steps
        .into_iter()
        .filter(|(name, input)| *name == sibling::TOOL && matches!(sibling::request(input), Ok(sibling::Request::Start { .. })))
        .count();
    Tally { starts, plan }
}

/// How a run ended: the reply ended well, it failed, or the run reached `RUN_LIMIT` or three starts first.
#[derive(Debug, Clone, Copy, PartialEq)]
enum End {
    Done,
    Failed,
    Cut,
}

/// The verdict on one request: a big one should start at least two conversations or plan at least three steps, a small one
/// start none and finish. A small request that failed or did not finish proves nothing.
fn verdict(big: bool, t: &Tally, end: End) -> &'static str {
    match (big, t.starts >= 2, t.plan >= 3, t.starts == 0, end) {
        (true, true, ..) => "split",
        (true, false, true, ..) => "planned only",
        (true, false, false, ..) => "did it alone",
        (false, _, _, false, _) => "split a small one",
        (false, _, _, true, End::Done) => "direct",
        (false, _, _, true, End::Failed) => "failed",
        (false, _, _, true, End::Cut) => "unfinished",
    }
}

struct Run {
    tally: Tally,
    end: End,
    tokens: u64,
    cost: f64,
    /// Replies whose usage never arrived after the stop: their tokens and dollars are missing, never counted as zero.
    unpriced: usize,
    seconds: u64,
    error: Option<String>,
    /// The start of the reply's words, so a verdict that disagrees with its label can be read.
    said: String,
}

/// One request in a throwaway folder; the folder goes whatever happens.
async fn run(vendor: Vendor, cli: faber_engine::cli::Cli, request: &str) -> Result<Run, String> {
    let world = std::env::temp_dir().join(format!("faber-leader-eval-{}", uuid::Uuid::new_v4().simple()));
    let result = measure(&world, vendor, cli, request).await;
    for attempt in 0..60 {
        match std::fs::remove_dir_all(&world) {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
            Err(e) if attempt == 59 => eprintln!("could not remove {}: {e}", world.display()),
            Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
    result
}

async fn measure(world: &Path, vendor: Vendor, cli: faber_engine::cli::Cli, request: &str) -> Result<Run, String> {
    let repo = world.join("repo");
    for (path, text) in FILES {
        let file = repo.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(file, text).map_err(|e| e.to_string())?;
    }
    git(&repo, &["init", "-q", "-b", "main"])?;
    git(&repo, &["add", "--", "."])?;
    git(&repo, &["-c", "user.name=Faber Eval", "-c", "user.email=eval@example.invalid", "commit", "-q", "-m", "first"])?;
    // The longest plan each conversation sent, from the engine's own events, so a plan replaced between two polls still counts.
    let plans: Arc<Mutex<BTreeMap<String, usize>>> = Arc::default();
    let seen = plans.clone();
    let engine = Engine::new(
        EngineOptions {
            data_dir: world.join("state"),
            #[cfg(windows)]
            chat_dir: world.join("chat-work"),
            clis: BTreeMap::from([(vendor, cli)]),
            spawn: proc::real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(10),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |e| {
            if let AppEvent::Plan { conversation_id, plan: Some(plan) } = e {
                let mut seen = seen.lock().unwrap_or_else(|e| e.into_inner());
                let most = seen.entry(conversation_id).or_default();
                *most = (*most).max(plan.steps.len());
            }
        }),
    )
    .map_err(|e| e.to_string())?;
    let result = drive(&engine, &repo, &plans, vendor, request).await;
    engine.shutdown();
    result
}

async fn drive(engine: &Engine, repo: &Path, plans: &Mutex<BTreeMap<String, usize>>, vendor: Vendor, request: &str) -> Result<Run, String> {
    engine.save_settings(Settings { permissions: Some(Permissions::FullAccess), ..Default::default() })?;
    let project = engine.add_project(&repo.to_string_lossy())?;
    // A fresh data folder does not know the plan yet; the installed app reads it at start the same way.
    engine.check_plans(Duration::from_secs(30)).await;
    let started = Instant::now();
    let parent = engine.send(SendRequest {
        conversation_id: None,
        area: Area::Project,
        project_id: Some(project.id),
        text: request.into(),
        images: vec![],
        files: vec![],
        // The vendor's own default model and effort, as a person who picked only the AI gets.
        choice: Choice { vendor: vendor.name().into(), model: None, effort: None },
    })?;
    let reply = |id: &str| engine.open(id).and_then(|c| c.messages.last().cloned()).filter(|m| m.role == Role::Assistant);
    let plan = || plans.lock().unwrap_or_else(|e| e.into_inner()).get(&parent).copied().unwrap_or(0);
    let count = |m: &Message| tally(m.tools.iter().flatten().map(|t| (t.name.as_str(), &t.input)), plan());
    let streaming = |m: &Message| m.status == Some(Status::Streaming);
    let mut last = None;
    while started.elapsed() < RUN_LIMIT {
        tokio::time::sleep(Duration::from_secs(1)).await;
        last = reply(&parent);
        if last.as_ref().is_some_and(|m| !streaming(m) || count(m).starts >= 3) {
            break;
        }
    }
    let seconds = started.elapsed().as_secs();
    let end = match &last {
        Some(m) if m.status == Some(Status::Done) => End::Done,
        Some(m) if !streaming(m) => End::Failed,
        _ => End::Cut,
    };
    // Every conversation it started is stopped with the rest: only the decision is measured. A stopped reply's usage comes with
    // its CLI's last word, so the totals wait for every reply to settle, up to a bound.
    let conversations = engine.state().conversations;
    conversations.iter().for_each(|c| engine.interrupt(&c.id));
    let settled = Instant::now();
    let replies = loop {
        let replies: Vec<Message> = conversations.iter().filter_map(|c| reply(&c.id)).collect();
        if replies.iter().all(|m| !streaming(m)) || settled.elapsed() > Duration::from_secs(30) {
            break replies;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    let unpriced = replies.iter().filter(|m| m.cost_usd.is_none() || m.output_tokens.is_none()).count();
    let tokens = replies.iter().map(|m| m.input_tokens.unwrap_or(0) + m.output_tokens.unwrap_or(0)).sum();
    let cost = replies.iter().filter_map(|m| m.cost_usd).sum();
    let error = last.as_ref().and_then(|m| m.error.clone());
    let said =
        last.as_ref().map(|m| m.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(90).collect()).unwrap_or_default();
    let tally = last.as_ref().map(count).unwrap_or(Tally { starts: 0, plan: plan() });
    Ok(Run { tally, end, tokens, cost, unpriced, seconds, error, said })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let only = std::env::args().nth(1);
    let vendors: Vec<(Vendor, faber_engine::cli::Cli)> = [(Vendor::Claude, "claude"), (Vendor::Codex, "codex")]
        .into_iter()
        .filter(|(v, _)| only.as_deref().is_none_or(|o| o == v.name()))
        .filter_map(|(v, bin)| find_cli(bin).map(|cli| (v, cli)))
        .collect();
    if vendors.is_empty() {
        return Err("no CLI to run: install Claude Code or Codex, or name one that is installed".into());
    }
    for (vendor, cli) in vendors {
        println!(
            "\n## {}\n\n| label | request | starts | plan steps | ended | verdict | seconds | tokens | US$ | it said |\n|---|---|---|---|---|---|---|---|---|---|",
            vendor.name()
        );
        let (mut split, mut planned, mut direct, mut tokens, mut cost, mut unpriced) = (0, 0, 0, 0, 0.0, 0);
        for (big, request) in REQUESTS {
            let label = if big { "big" } else { "small" };
            let r = match run(vendor, cli.clone(), request).await {
                Ok(r) => r,
                Err(e) => {
                    println!("| {label} | {request} | | | could not run: {e} | | | | | |");
                    continue;
                }
            };
            let v = verdict(big, &r.tally, r.end);
            split += usize::from(big && r.tally.starts >= 2);
            planned += usize::from(big && r.tally.plan >= 3);
            direct += usize::from(v == "direct");
            (tokens, cost, unpriced) = (tokens + r.tokens, cost + r.cost, unpriced + r.unpriced);
            let why = r.error.map(|e| format!(" ({e})")).unwrap_or_default();
            println!(
                "| {label} | {request} | {} | {} | {:?}{why} | {v} | {} | {} | {:.2} | {} |",
                r.tally.starts,
                r.tally.plan,
                r.end,
                r.seconds,
                r.tokens,
                r.cost,
                r.said.replace('|', "/")
            );
        }
        let missing = if unpriced > 0 { format!(" ({unpriced} replies had no usage yet, so these are low)") } else { String::new() };
        println!(
            "\n{}: big requests that started 2+ conversations {split}/6, big requests with a plan of 3+ steps {planned}/6, small \
             requests answered without starting any {direct}/6 (must stay 6/6); {tokens} tokens, US$ {cost:.2} at API price{missing}",
            vendor.name()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_conversation_tool_s_starts_count_and_the_verdict_follows_the_label() {
        let start = json!({"action": "start", "title": "UI", "brief": "Build the screens."});
        let bare = json!({"title": "Docs", "brief": "Write the docs."});
        let (read, other) = (json!({"action": "read", "id": "x"}), json!({"title": "Docs", "brief": "Write the docs."}));
        let steps = [("Conversation", &start), ("Conversation", &read), ("Read", &other), ("Conversation", &bare), ("Helper", &other)];
        let t = tally(steps, 4);
        assert_eq!(t, Tally { starts: 2, plan: 4 }, "a start with no action counts; a read, another tool and a helper do not");
        assert_eq!(verdict(true, &t, End::Cut), "split");
        assert_eq!(verdict(false, &t, End::Done), "split a small one");
        assert_eq!(verdict(true, &Tally { starts: 1, plan: 3 }, End::Done), "planned only");
        assert_eq!(verdict(true, &Tally::default(), End::Done), "did it alone");
        assert_eq!(verdict(false, &Tally::default(), End::Done), "direct");
        assert_eq!(verdict(false, &Tally::default(), End::Failed), "failed", "a failed small request proves nothing");
        assert_eq!(verdict(false, &Tally::default(), End::Cut), "unfinished");
    }
}
