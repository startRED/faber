//! The verbs of `faber`: short names over the screen's own commands. `faber call` reaches every other command of the table.

use crate::args::Args;
use crate::connect;
use crate::out::{DONE, Out, REFUSED, Tone, UNREACHED, USAGE};
use crate::thread::{self, StepState};
use crate::words::{self, Words};
use faber_channel::Client;
use serde_json::{Value, json};
use std::io::Read;

/// Every verb, in the order the help lists them, with the key of its line in the help.
pub const VERBS: &[(&str, &str)] = &[
    ("status", "faber status"),
    ("list", "faber list [--project <id>] [--archived] [--limit <n>]"),
    ("show", "faber show <conversation>"),
    (
        "chat",
        "faber chat [text] [--id <conversation>] [--project <id>] [--vendor <claude|codex>] [--model <m>] [--effort <low|medium|high>] [--no-wait]",
    ),
    ("stop", "faber stop <conversation>"),
    ("rename", "faber rename <conversation> <title>"),
    ("archive", "faber archive <conversation>"),
    ("unarchive", "faber unarchive <conversation>"),
    ("pin", "faber pin <conversation>"),
    ("unpin", "faber unpin <conversation>"),
    ("remove", "faber remove <conversation>"),
    ("quota", "faber quota"),
    ("projects", "faber projects"),
    ("project", "faber project <add <folder>|remove <id>>"),
    ("settings", "faber settings [set <name> <value>]"),
    ("sign-in", "faber sign-in <claude|codex>"),
    ("install", "faber install <claude|codex>"),
    ("watch", "faber watch"),
    ("call", "faber call <command> [<arguments as JSON>|-]"),
    ("commands", "faber commands"),
];

pub fn help(out: &Out) {
    out.line(&out.paint(Tone::Strong, &out.t("cli.help.title")));
    out.line("");
    for (verb, usage) in VERBS {
        out.line(&format!("  {}", out.paint(Tone::Accent, usage)));
        out.line(&format!("      {}", out.t(&format!("cli.help.{verb}"))));
    }
    out.line("");
    out.line(&out.t("cli.help.flags"));
    out.line(&out.t("cli.help.screen"));
}

/// One connected run of a verb.
pub struct Session {
    pub client: Client,
    pub out: Out,
    pub args: Args,
    state: Option<Value>,
}

/// Why a verb stopped early, already told: the exit code to leave with.
pub type Stop = i32;

impl Session {
    pub async fn open(args: Args, out: Out, client_name: &str) -> Result<Session, (Out, Stop)> {
        match connect::connect(&args, client_name).await {
            Ok(client) => Ok(Session { client, out, args, state: None }),
            Err(why) => {
                let code = out.fail(why.key(), UNREACHED);
                Err((out, code))
            }
        }
    }

    /// One command; a refusal is told by its key and stops the verb.
    pub async fn ask(&self, cmd: &str, args: Value) -> Result<Value, Stop> {
        match self.client.call(cmd, args).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(key)) => Err(self.out.fail(&key, REFUSED)),
            Err(_) => Err(self.out.fail("cli.error.channel", UNREACHED)),
        }
    }

    /// The whole state, read once per run. It also settles the language: the one Settings chose wins over the system's.
    pub async fn state(&mut self) -> Result<&Value, Stop> {
        if self.state.is_none() {
            let state = self.ask("state", Value::Null).await?;
            let language = state["state"]["settings"]["language"].as_str().or(state["settings"]["language"].as_str());
            self.out.words = Words::new(&words::language(language));
            self.state = Some(state);
        }
        Ok(self.state.as_ref().expect("just read"))
    }
}

/// The app state inside `state`'s answer (the shell wraps it with its look, models and version).
fn inner(state: &Value) -> &Value {
    if state.get("state").is_some() { &state["state"] } else { state }
}

pub async fn run(args: Args, out: Out) -> i32 {
    let verb = args.word(0).unwrap_or_default().to_string();
    if !VERBS.iter().any(|(v, _)| *v == verb) {
        eprintln!("{}", out.paint(Tone::Bad, &out.words.fill("cli.error.verb", &[("verb", &verb)])));
        return USAGE;
    }
    let mut session = match Session::open(args, out, "faber").await {
        Ok(session) => session,
        Err((_, code)) => return code,
    };
    let done = match verb.as_str() {
        "status" => status(&mut session).await,
        "list" => list(&mut session).await,
        "show" => show(&mut session).await,
        "chat" => crate::chat::run(&mut session).await,
        "stop" => simple(&mut session, "interrupt", |id, _| json!({ "id": id })).await,
        "rename" => simple(&mut session, "rename", |id, rest| json!({ "id": id, "title": rest })).await,
        "archive" => simple(&mut session, "archive", |id, _| json!({ "id": id, "on": true })).await,
        "unarchive" => simple(&mut session, "archive", |id, _| json!({ "id": id, "on": false })).await,
        "pin" => simple(&mut session, "pin", |id, _| json!({ "id": id, "on": true })).await,
        "unpin" => simple(&mut session, "pin", |id, _| json!({ "id": id, "on": false })).await,
        "remove" => simple(&mut session, "remove", |id, _| json!({ "id": id, "discard": false })).await,
        "quota" => quota(&mut session).await,
        "projects" => projects(&mut session).await,
        "project" => project(&mut session).await,
        "settings" => settings(&mut session).await,
        "sign-in" => vendor_step(&mut session, "setup_sign_in", "cli.signIn.opened").await,
        "install" => vendor_step(&mut session, "setup_install", "cli.install.started").await,
        "watch" => watch(&mut session).await,
        "call" => call(&mut session).await,
        "commands" => commands(&mut session).await,
        _ => unreachable!("every verb is matched"),
    };
    done.unwrap_or_else(|code| code)
}

fn usage(session: &Session, verb: &str) -> Stop {
    let line = VERBS.iter().find(|(v, _)| *v == verb).map(|(_, u)| *u).unwrap_or("faber help");
    eprintln!("{}", session.out.paint(Tone::Bad, &session.out.words.fill("cli.error.usage", &[("usage", line)])));
    USAGE
}

/// A verb that takes a conversation and sends one command about it.
async fn simple(session: &mut Session, cmd: &str, args: impl Fn(&str, String) -> Value) -> Result<i32, Stop> {
    let verb = session.args.word(0).unwrap_or_default().to_string();
    let Some(id) = session.args.word(1).map(str::to_string) else { return Err(usage(session, &verb)) };
    let rest = session.args.rest(2);
    if verb == "rename" && rest.trim().is_empty() {
        return Err(usage(session, &verb));
    }
    session.state().await?;
    session.ask(cmd, args(&id, rest)).await?;
    if session.out.json {
        session.out.json(&json!({ "ok": true }));
    } else {
        session.out.line(&session.out.t("cli.done"));
    }
    Ok(DONE)
}

fn vendor_name(out: &Out, vendor: &str) -> String {
    out.t(&format!("cli.vendor.{vendor}"))
}

/// The account's state as one word: `signedIn`, `signedOut` or `unknown`, however the engine spells it.
fn account_word(account: &Value) -> &'static str {
    match account.as_str().unwrap_or("").to_ascii_lowercase().replace('_', "").as_str() {
        "signedin" => "signedIn",
        "signedout" => "signedOut",
        _ => "unknown",
    }
}

async fn status(session: &mut Session) -> Result<i32, Stop> {
    let state = inner(session.state().await?).clone();
    let mut accounts = Vec::new();
    for vendor in ["claude", "codex"] {
        let check = session.client.call("setup_check", json!({ "vendor": vendor })).await;
        accounts.push(match check {
            Ok(Ok(check)) => check,
            Ok(Err(key)) => json!({ "vendor": vendor, "error": key }),
            Err(_) => return Err(session.out.fail("cli.error.channel", UNREACHED)),
        });
    }
    let update = session.client.call("update_status", Value::Null).await.ok().and_then(Result::ok);
    let running = state["running"]["items"].as_array().cloned().unwrap_or_default();
    let out = &session.out;
    if out.json {
        out.json(&json!({
            "version": session.client.offer["version"],
            "protocol": session.client.offer["protocol"],
            "accounts": accounts,
            "quotas": state["quotas"],
            "usage": state["usage"],
            "running": running,
            "conversations": state["conversations"].as_array().map_or(0, Vec::len),
            "projects": state["projects"].as_array().map_or(0, Vec::len),
            "update": update,
        }));
        return Ok(DONE);
    }
    let version = session.client.offer["version"].as_str().unwrap_or("?");
    out.line(&format!("{} {}", out.paint(Tone::Strong, "Faber"), out.paint(Tone::Quiet, version)));
    out.line("");
    for account in &accounts {
        let vendor = account["vendor"].as_str().unwrap_or("?");
        let name = format!("{:<14}", vendor_name(out, vendor));
        let (word, tone, hint) = if let Some(key) = account["error"].as_str() {
            (out.t(key), Tone::Bad, None)
        } else if account["installed"].as_bool() != Some(true) {
            (out.t("cli.account.notInstalled"), Tone::Warn, Some(format!("faber install {vendor}")))
        } else {
            match account_word(&account["account"]) {
                "signedIn" => (out.t("cli.account.signedIn"), Tone::Good, None),
                "signedOut" => (out.t("cli.account.signedOut"), Tone::Warn, Some(format!("faber sign-in {vendor}"))),
                _ => (out.t("cli.account.unknown"), Tone::Quiet, None),
            }
        };
        let hint = hint.map(|h| format!("   {}", out.paint(Tone::Quiet, &out.words.fill("cli.account.hint", &[("command", &h)]))));
        out.line(&format!("  {} {}{}", out.paint(Tone::Strong, &name), out.paint(tone, &word), hint.unwrap_or_default()));
    }
    out.line("");
    quota_lines(out, &state);
    let needs = running.iter().filter(|r| r["state"].as_str() == Some("needsYou")).count();
    let mut line = out.words.count("cli.running", running.len());
    if needs > 0 {
        line = format!("{line} · {}", out.paint(Tone::Warn, &out.words.count("cli.needsYou", needs)));
    }
    out.line(&format!("  {line}"));
    if let Some(stage) = update.as_ref().and_then(|u| u["phase"].as_str()).filter(|p| *p != "idle" && *p != "none") {
        out.line(&format!("  {}", out.paint(Tone::Accent, &out.words.fill("cli.update", &[("phase", stage)]))));
    }
    Ok(DONE)
}

/// One line per quota window: who, which window, a bar, how much is used and when it resets.
fn quota_lines(out: &Out, state: &Value) {
    let quotas = state["quotas"].as_array().cloned().unwrap_or_default();
    if quotas.is_empty() {
        out.line(&format!("  {}", out.paint(Tone::Quiet, &out.t("cli.quota.none"))));
        return;
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    for q in quotas {
        let vendor = q["vendor"].as_str().unwrap_or("?");
        let window = q["window"].as_str().unwrap_or("?");
        let used = q["used"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
        let filled = (used * 20.0).round() as usize;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(20 - filled));
        let tone = if used >= 0.9 {
            Tone::Bad
        } else if used >= 0.7 {
            Tone::Warn
        } else {
            Tone::Good
        };
        let window_name = out.t(&format!("quota.{window}"));
        let window_name = if window_name.starts_with("quota.") { window.to_string() } else { window_name };
        let resets = q["resetsAt"]
            .as_f64()
            .filter(|at| *at > now)
            .map(|at| out.words.fill("cli.quota.resets", &[("when", &duration(out, (at - now) as u64))]));
        out.line(&format!(
            "  {:<14} {:<8} {} {:>4}  {}",
            vendor_name(out, vendor),
            window_name,
            out.paint(tone, &bar),
            format!("{}%", (used * 100.0).round()),
            out.paint(Tone::Quiet, &resets.unwrap_or_default())
        ));
    }
}

/// "3 min", "5 h", "2 d", in the person's language.
pub fn duration(out: &Out, seconds: u64) -> String {
    let (n, key) = match seconds {
        s if s < 3600 => ((s / 60).max(1), "cli.time.minutes"),
        s if s < 86_400 => (s / 3600, "cli.time.hours"),
        s => (s / 86_400, "cli.time.days"),
    };
    out.words.fill(key, &[("n", &n.to_string())])
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

async fn list(session: &mut Session) -> Result<i32, Stop> {
    let state = inner(session.state().await?).clone();
    let archived = session.args.on("archived");
    let project = session.args.value("project").map(str::to_string);
    let limit: usize = session.args.value("limit").and_then(|l| l.parse().ok()).unwrap_or(20);
    let mut rows: Vec<Value> = state["conversations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c["archived"].as_bool().unwrap_or(false) == archived)
        .filter(|c| project.as_deref().is_none_or(|p| c["projectId"].as_str() == Some(p)))
        .collect();
    rows.sort_by_key(|c| (c["pinnedAt"].is_null(), std::cmp::Reverse(c["updatedAt"].as_u64().unwrap_or(0))));
    rows.truncate(limit);
    let out = &session.out;
    if out.json {
        out.json(&Value::Array(rows));
        return Ok(DONE);
    }
    if rows.is_empty() {
        out.line(&out.paint(Tone::Quiet, &out.t("cli.list.empty")));
        return Ok(DONE);
    }
    let projects: Vec<Value> = state["projects"].as_array().cloned().unwrap_or_default();
    let now = now_ms();
    for c in &rows {
        let title = c["title"].as_str().filter(|t| !t.is_empty()).map(str::to_string).unwrap_or_else(|| out.t("cli.list.untitled"));
        let title: String = title.chars().take(60).collect();
        let place = match c["projectId"].as_str() {
            Some(p) => projects.iter().find(|x| x["id"].as_str() == Some(p)).and_then(|x| x["name"].as_str()).unwrap_or(p).to_string(),
            None => out.t("cli.list.chat"),
        };
        let age = duration(out, now.saturating_sub(c["updatedAt"].as_u64().unwrap_or(now)) / 1000);
        let pin = if c["pinnedAt"].is_null() { " " } else { "•" };
        out.line(&format!(
            "{pin} {}  {}  {}",
            out.paint(Tone::Strong, &format!("{title:<60}")),
            out.paint(Tone::Quiet, &format!("{place:<18} {age:>6}")),
            out.paint(Tone::Accent, c["id"].as_str().unwrap_or(""))
        ));
    }
    out.line(&out.paint(Tone::Quiet, &out.words.count("cli.list.count", rows.len())));
    Ok(DONE)
}

async fn show(session: &mut Session) -> Result<i32, Stop> {
    let Some(id) = session.args.word(1).map(str::to_string) else { return Err(usage(session, "show")) };
    session.state().await?;
    let conversation = session.ask("open", json!({ "id": id })).await?;
    if conversation.is_null() {
        return Err(session.out.fail("cli.error.noConversation", REFUSED));
    }
    if session.out.json {
        session.out.json(&conversation);
        return Ok(DONE);
    }
    let out = &session.out;
    out.line(&out.paint(Tone::Strong, conversation["title"].as_str().unwrap_or("")));
    for message in conversation["messages"].as_array().into_iter().flatten() {
        out.line("");
        print_message(out, message);
    }
    Ok(DONE)
}

/// A message as a person reads it at a terminal: the person's words marked, the steps one line each, then the answer.
pub fn print_message(out: &Out, message: &Value) {
    let text = message["text"].as_str().unwrap_or("");
    if message["role"].as_str() == Some("user") {
        for (i, line) in text.lines().enumerate() {
            out.line(&out.paint(Tone::Accent, &format!("{} {line}", if i == 0 { "›" } else { " " })));
        }
        return;
    }
    for tool in message["tools"].as_array().into_iter().flatten() {
        out.line(&step(out, tool));
    }
    print_text(out, text);
    if let Some(meta) = meta_line(out, message) {
        out.line(&out.paint(Tone::Quiet, &meta));
    }
}

pub fn step(out: &Out, tool: &Value) -> String {
    let (mark, tone) = match thread::step_state(tool) {
        StepState::Done => ("✓", Tone::Good),
        StepState::Failed => ("✗", Tone::Bad),
        StepState::Declined => ("–", Tone::Warn),
        StepState::Running => ("…", Tone::Accent),
    };
    format!("  {} {}", out.paint(tone, mark), out.paint(Tone::Quiet, &thread::step_line(tool)))
}

/// Markdown read plainly: headings strong, code quiet, the rest as written.
pub fn print_text(out: &Out, text: &str) {
    let mut code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            out.line(&out.paint(Tone::Quiet, &format!("    {line}")));
        } else if let Some(heading) = line.strip_prefix('#') {
            out.line(&out.paint(Tone::Strong, heading.trim_start_matches('#').trim()));
        } else {
            out.line(line);
        }
    }
}

/// Who answered, how long it took and what it cost, on one quiet line.
pub fn meta_line(out: &Out, message: &Value) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(vendor) = message["vendor"].as_str() {
        parts.push(vendor_name(out, vendor));
    }
    if let Some(model) = message["model"].as_str() {
        parts.push(model.to_string());
    }
    if let Some(took) = message["took"].as_u64() {
        parts.push(format!("{:.1} s", took as f64 / 1000.0));
    }
    let tokens = message["inputTokens"].as_u64().unwrap_or(0) + message["outputTokens"].as_u64().unwrap_or(0);
    if tokens > 0 {
        parts.push(out.words.fill("cli.tokens", &[("n", &tokens.to_string())]));
    }
    if let Some(error) = message["error"].as_str() {
        parts.push(out.t(error));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

async fn quota(session: &mut Session) -> Result<i32, Stop> {
    let state = inner(session.state().await?).clone();
    if session.out.json {
        session.out.json(&json!({ "quotas": state["quotas"], "usage": state["usage"] }));
    } else {
        quota_lines(&session.out, &state);
    }
    Ok(DONE)
}

async fn projects(session: &mut Session) -> Result<i32, Stop> {
    let state = inner(session.state().await?).clone();
    let projects = state["projects"].as_array().cloned().unwrap_or_default();
    let out = &session.out;
    if out.json {
        out.json(&Value::Array(projects));
        return Ok(DONE);
    }
    if projects.is_empty() {
        out.line(&out.paint(Tone::Quiet, &out.t("cli.projects.empty")));
    }
    for p in &projects {
        out.line(&format!(
            "  {}  {}  {}",
            out.paint(Tone::Strong, &format!("{:<24}", p["name"].as_str().unwrap_or(""))),
            out.paint(Tone::Quiet, p["path"].as_str().unwrap_or("")),
            out.paint(Tone::Accent, p["id"].as_str().unwrap_or(""))
        ));
    }
    Ok(DONE)
}

async fn project(session: &mut Session) -> Result<i32, Stop> {
    session.state().await?;
    let (action, target) = (session.args.word(1).map(str::to_string), session.args.word(2).map(str::to_string));
    let answer = match (action.as_deref(), target) {
        (Some("add"), Some(folder)) => {
            // The engine judges the folder by its resolved path; it gets the whole path, never one relative to this terminal.
            let folder = std::path::absolute(&folder).unwrap_or_else(|_| folder.into());
            session.ask("open_folders", json!({ "paths": [folder.to_string_lossy()] })).await?
        }
        (Some("remove"), Some(id)) => session.ask("remove_project", json!({ "id": id })).await?,
        _ => return Err(usage(session, "project")),
    };
    if session.out.json {
        session.out.json(&answer);
    } else {
        session.out.line(&session.out.t("cli.done"));
    }
    Ok(DONE)
}

async fn settings(session: &mut Session) -> Result<i32, Stop> {
    let state = inner(session.state().await?).clone();
    match (session.args.word(1), session.args.word(2)) {
        (None, _) => {
            if session.out.json {
                session.out.json(&state["settings"]);
            } else {
                for (name, value) in state["settings"].as_object().into_iter().flatten() {
                    session.out.line(&format!("  {}  {}", session.out.paint(Tone::Strong, &format!("{name:<28}")), value));
                }
            }
            Ok(DONE)
        }
        (Some("set"), Some(name)) => {
            let raw = session.args.rest(3);
            if raw.is_empty() {
                return Err(usage(session, "settings"));
            }
            // A value that reads as JSON is JSON (`true`, `3`, `"x"`, `{...}`); anything else is a text.
            let value = serde_json::from_str::<Value>(&raw).unwrap_or(Value::String(raw));
            let saved = session.ask("save_settings", json!({ "patch": { name: value } })).await?;
            if session.out.json {
                session.out.json(&saved);
            } else {
                session.out.line(&session.out.t("cli.done"));
            }
            Ok(DONE)
        }
        _ => Err(usage(session, "settings")),
    }
}

async fn vendor_step(session: &mut Session, cmd: &str, done_key: &str) -> Result<i32, Stop> {
    let verb = session.args.word(0).unwrap_or_default().to_string();
    let Some(vendor) = session.args.word(1).filter(|v| ["claude", "codex"].contains(v)).map(str::to_string) else {
        return Err(usage(session, &verb));
    };
    session.state().await?;
    session.ask(cmd, json!({ "vendor": vendor })).await?;
    if session.out.json {
        session.out.json(&json!({ "ok": true }));
    } else {
        session.out.line(&session.out.words.fill(done_key, &[("name", &vendor_name(&session.out, &vendor))]));
    }
    Ok(DONE)
}

async fn watch(session: &mut Session) -> Result<i32, Stop> {
    session.state().await?;
    let Ok(mut events) = session.client.subscribe().await else { return Err(session.out.fail("cli.error.channel", UNREACHED)) };
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { return Err(session.out.fail("cli.error.gone", UNREACHED)) };
                if session.out.json {
                    session.out.line(&event.to_string());
                } else {
                    let kind = event["type"].as_str().unwrap_or("?");
                    let about = event["conversationId"].as_str().unwrap_or("");
                    session.out.line(&format!("{} {}", session.out.paint(Tone::Accent, &format!("{kind:<12}")), session.out.paint(Tone::Quiet, about)));
                }
            }
            _ = tokio::signal::ctrl_c() => return Ok(DONE),
        }
    }
}

async fn call(session: &mut Session) -> Result<i32, Stop> {
    let Some(cmd) = session.args.word(1).map(str::to_string) else { return Err(usage(session, "call")) };
    let raw = match session.args.word(2) {
        Some("-") => {
            let mut text = String::new();
            let _ = std::io::stdin().read_to_string(&mut text);
            text
        }
        Some(_) => session.args.rest(2),
        None => String::new(),
    };
    let args = if raw.trim().is_empty() {
        Value::Null
    } else {
        match serde_json::from_str(&raw) {
            Ok(args) => args,
            Err(_) => return Err(session.out.fail("cli.error.json", USAGE)),
        }
    };
    let answer = session.ask(&cmd, args).await?;
    session.out.json(&answer);
    Ok(DONE)
}

async fn commands(session: &mut Session) -> Result<i32, Stop> {
    let table = session.client.offer["commands"].clone();
    if session.out.json {
        session.out.json(&table);
        return Ok(DONE);
    }
    for c in table.as_array().into_iter().flatten() {
        let args: Vec<String> = c["args"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| format!("{}: {}", a[0].as_str().unwrap_or(""), a[1].as_str().unwrap_or("")))
            .collect();
        session.out.line(&format!(
            "  {} {} {}",
            session.out.paint(Tone::Strong, &format!("{:<28}", c["name"].as_str().unwrap_or(""))),
            session.out.paint(Tone::Quiet, &format!("{:<10}", c["kind"].as_str().unwrap_or(""))),
            args.join(", ")
        ));
    }
    Ok(DONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_verb_has_its_help_line_in_every_language() {
        for (verb, _) in VERBS {
            let key = format!("cli.help.{verb}");
            assert_ne!(Words::new("en").get(&key), key, "{key}");
        }
    }

    #[test]
    fn the_account_word_reads_every_spelling_of_the_engine() {
        assert_eq!(account_word(&json!("signedIn")), "signedIn");
        assert_eq!(account_word(&json!("signed_out")), "signedOut");
        assert_eq!(account_word(&json!("whatever")), "unknown");
    }
}
