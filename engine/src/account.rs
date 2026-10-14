//! Signing in to each CLI with its own official flow, and asking the CLI whether it is signed in.
//! Faber never reads the CLIs' credential files; it only runs their status command and keeps the verdict.
//!
//! Claude Code: `claude auth login`, `claude auth status` prints JSON with `loggedIn` and exits 0 when signed in, 1
//! when not (https://code.claude.com/docs/en/cli-reference).
//! Codex: `codex login`, `codex login status` prints the active mode and exits 0 when signed in
//! (https://developers.openai.com/codex/cli/reference/#codex-login).

use crate::cli::Cli;
use crate::model::Vendor;
use crate::proc::Spawner;
use crate::signals::{Signals, claude_plan};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountStatus {
    SignedIn,
    SignedOut,
    Unknown,
}

/// The CLI's own interactive sign-in, meant for a terminal the user sees.
pub fn login_args(vendor: Vendor) -> &'static [&'static str] {
    match vendor {
        Vendor::Claude => &["auth", "login"],
        Vendor::Codex => &["login"],
    }
}

pub fn status_args(vendor: Vendor) -> &'static [&'static str] {
    match vendor {
        Vendor::Claude => &["auth", "status"],
        Vendor::Codex => &["login", "status"],
    }
}

// Bound transient output, including diagnostics, before discarding it after the verdict.
const OUTPUT_LIMIT: u64 = 64 * 1024;

/// What a status command printed and how it ended.
struct Ran {
    out: String,
    err: String,
    code: Option<i32>,
}

/// Runs the status command without a shell, with its output bounded. None when the CLI fails to start, does not answer within
/// `timeout` (it is killed when the run ends) or prints more than the bound.
async fn run(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Ran> {
    let args: Vec<String> = cli.prefix.iter().cloned().chain(status_args(vendor).iter().map(|a| a.to_string())).collect();
    let p = spawn(&cli.command, &args, cwd).ok()?;
    drop(p.stdin); // nothing to type: a CLI waiting for input sees end of input instead
    let mut child = p.child;
    let run = async {
        let (Some(out), Some(err)) = tokio::join!(read_all(Some(p.stdout)), read_all(p.stderr)) else { return None };
        let code = match &mut child {
            Some(c) => c.wait().await.ok().and_then(|s| s.code()),
            None => None,
        };
        Some(Ran { out, err, code })
    };
    tokio::time::timeout(timeout, run).await.ok().flatten()
}

/// Runs the status command. Its output is read into memory only to reach a verdict, then dropped; a CLI that fails to start or
/// does not answer within `timeout` is `Unknown`, and is killed when the check ends.
pub async fn check(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> AccountStatus {
    let Some(Ran { out, err, code }) = run(vendor, cli, spawn, cwd, timeout).await else { return AccountStatus::Unknown };
    if vendor == Vendor::Claude { verdict(vendor, &out, code) } else { verdict(vendor, &format!("{out}\n{err}"), code) }
}

/// What `claude auth status` says about the plan and how the login is billed; nothing else of its output is kept (the email
/// and organisation in it are dropped unread). None when the command failed, timed out or returned invalid status data.
pub async fn claude_account(cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Signals> {
    let ran = run(Vendor::Claude, cli, spawn, cwd, timeout).await?;
    serde_json::from_str::<serde_json::Value>(&ran.out).ok()?["loggedIn"].as_bool()?;
    Some(claude_plan(&ran.out))
}

fn utc_seconds(text: &str) -> Option<f64> {
    let text = text.strip_suffix('Z').or_else(|| text.strip_suffix("+00:00"))?;
    let (date, clock) = text.split_once('T')?;
    let date: Vec<u32> = date.split('-').map(str::parse).collect::<Result<_, _>>().ok()?;
    let clock: Vec<u32> = clock.split('.').next()?.split(':').map(str::parse).collect::<Result<_, _>>().ok()?;
    if date.len() != 3 || clock.len() != 3 {
        return None;
    }
    let (year, month, day) = (date[0], date[1], date[2]);
    let leap = |y: u32| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let months = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > months[(month - 1) as usize]
        || clock[0] > 23
        || clock[1] > 59
        || clock[2] > 59
    {
        return None;
    }
    let days: u64 = (1970..year).map(|y| if leap(y) { 366 } else { 365 }).sum::<u64>()
        + months[..(month - 1) as usize].iter().map(|d| u64::from(*d)).sum::<u64>()
        + u64::from(day - 1);
    Some((days * 86400 + u64::from(clock[0] * 3600 + clock[1] * 60 + clock[2])) as f64)
}

fn usage_events(report: &serde_json::Value, terminal: &serde_json::Value) -> Option<Vec<crate::events::EngineEvent>> {
    use crate::events::EngineEvent;
    // `/usage` labels its cached fallback; receipt time must not renew old server data.
    let cached = |text: &str| {
        let text = text.to_ascii_lowercase();
        text.contains("last-known usage") || text.contains("last known usage")
    };
    let content = &report["message"]["content"];
    if content.as_str().is_some_and(cached)
        || content.as_array().is_some_and(|items| items.iter().any(|item| item["text"].as_str().is_some_and(cached)))
    {
        return None;
    }
    if terminal["local_command"] != "usage"
        || terminal["total_cost_usd"].as_f64() != Some(0.0)
        || terminal["modelUsage"].as_object().is_none_or(|models| !models.is_empty())
    {
        return None;
    }
    let rates = &report["usage_report"]["rate_limits"];
    let rows = rates["limits"].as_array()?;
    let mut events = vec![];
    for row in rows {
        let name = match row["kind"].as_str()? {
            "weekly_all" => "seven_day",
            "session" => "five_hour",
            // Scoped display names are not canonical model identifiers.
            _ => continue,
        };
        let used = row["percent"].as_f64()? / 100.0;
        let resets_at = utc_seconds(row["resets_at"].as_str()?)?;
        if !(0.0..=1.0).contains(&used) {
            return None;
        }
        if events.iter().any(|event| matches!(event,EngineEvent::Quota{window,..} if window==name)) {
            return None;
        }
        events.push(EngineEvent::Quota { window: name.into(), used, resets_at });
    }
    if !events.iter().any(|event| matches!(event,EngineEvent::Quota{window,..} if window=="seven_day")) {
        return None;
    }
    events.push(EngineEvent::Signals(Signals { paid_fallback: rates["extra_usage"]["is_enabled"].as_bool(), ..Default::default() }));
    Some(events)
}

/// Stock `/usage` is a local command; its terminal receipt must prove that no model loop ran.
pub(crate) async fn claude_limits(cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Vec<crate::events::EngineEvent> {
    use crate::{
        claude::{ClaudeLaunch, project_claude_args},
        model::Area,
        project::Role,
    };
    use tokio::io::{AsyncBufReadExt, BufReader};
    let launch = ClaudeLaunch {
        area: Area::Project,
        model: Some("claude-haiku-5-5".into()),
        effort: Some("low".into()),
        resume: None,
        user_hooks: false,
        instructions: None,
        user_name: None,
    };
    let Ok(mut args) = project_claude_args(&launch, Role::Lead) else { return vec![] };
    args.retain(|arg| arg != "--disable-slash-commands");
    args.push("--no-session-persistence".into());
    let args: Vec<_> = cli.prefix.iter().cloned().chain(args).collect();
    let Ok(mut p) = spawn(&cli.command, &args, cwd) else { return vec![] };
    let _tail = crate::proc::stderr_tail(p.stderr.take());
    let mut lines = BufReader::new(p.stdout.take(OUTPUT_LIMIT)).lines();
    let ask = async {
        crate::proc::write_line(
            &mut *p.stdin,
            &serde_json::json!({"type":"user",
            "message":{"role":"user","content":"/usage"},"parent_tool_use_id":null,"session_id":""}),
        )
        .await
        .ok()?;
        let mut report = None;
        while let Ok(Some(line)) = lines.next_line().await {
            let msg: serde_json::Value = serde_json::from_str(&line).ok()?;
            if msg["type"] == "stream_event" || (msg["type"] == "assistant" && msg["usage_report"].is_null()) {
                return None;
            }
            if !msg["usage_report"].is_null() {
                report = Some(msg.clone());
            }
            if msg["type"] == "result" {
                return usage_events(report.as_ref()?, &msg);
            }
            if msg["type"] == "control_request" {
                return None;
            }
        }
        None
    };
    let result = tokio::time::timeout(timeout, ask).await.ok().flatten().unwrap_or_default();
    crate::proc::stop(&mut *p.stdin, &mut p.child).await;
    result
}

async fn read_all(stream: Option<Box<dyn AsyncRead + Send + Unpin>>) -> Option<String> {
    let mut buf = Vec::new();
    if let Some(s) = stream {
        s.take(OUTPUT_LIMIT + 1).read_to_end(&mut buf).await.ok()?;
    }
    if buf.len() as u64 > OUTPUT_LIMIT {
        return None;
    }
    String::from_utf8(buf).ok()
}

fn verdict(vendor: Vendor, output: &str, code: Option<i32>) -> AccountStatus {
    let said = match vendor {
        Vendor::Claude => serde_json::from_str::<serde_json::Value>(output).ok().and_then(|v| v["loggedIn"].as_bool()),
        // ponytail: Codex documents only its exit code; the wording is matched for replays, which have none.
        Vendor::Codex => output.lines().map(str::trim).find_map(|l| {
            let l = l.to_ascii_lowercase();
            if l.starts_with("not logged in") {
                Some(false)
            } else if l.starts_with("logged in") {
                Some(true)
            } else {
                None
            }
        }),
    };
    match (said, code) {
        (Some(true), _) | (None, Some(0)) => AccountStatus::SignedIn,
        (Some(false), _) => AccountStatus::SignedOut,
        (None, Some(1)) if vendor == Vendor::Claude => AccountStatus::SignedOut,
        _ => AccountStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_decide_when_the_output_says_nothing() {
        assert_eq!(verdict(Vendor::Codex, "", Some(0)), AccountStatus::SignedIn);
        assert_eq!(verdict(Vendor::Claude, "", Some(1)), AccountStatus::SignedOut);
        assert_eq!(verdict(Vendor::Codex, "", Some(1)), AccountStatus::Unknown);
        assert_eq!(verdict(Vendor::Claude, "{\"loggedIn\":false}", Some(0)), AccountStatus::SignedOut);
    }

    #[test]
    fn usage_command_requires_local_completion_and_zero_counters() {
        let report = serde_json::json!({"type":"assistant","usage_report":{
            "rate_limits":{"extra_usage":{"is_enabled":false},"limits":[
                {"kind":"weekly_all","percent":31,"resets_at":"2026-10-20T08:00:00.296594+00:00"},
                {"kind":"session","percent":22,"resets_at":"2026-10-14T21:30:00.296575+00:00"}
            ]}
        }});
        let terminal = serde_json::json!({"type":"result","local_command":"usage","total_cost_usd":0,"modelUsage":{}});
        let events = usage_events(&report, &terminal).unwrap();
        assert!(events.iter().any(|e| matches!(e, crate::events::EngineEvent::Quota {window,used,resets_at}
            if window == "seven_day" && *used == 0.31 && *resets_at == 1792483200.0)));
        assert!(events.iter().any(|e| matches!(e,crate::events::EngineEvent::Signals(s) if s.paid_fallback==Some(false))));
        for bad in [
            serde_json::json!({"type":"result","total_cost_usd":0}),
            serde_json::json!({"type":"result","local_command":"usage","total_cost_usd":0.01}),
            serde_json::json!({"type":"result","local_command":"usage","total_cost_usd":0,"modelUsage":{"m":{"outputTokens":1}}}),
        ] {
            assert!(usage_events(&report, &bad).is_none());
        }
        assert!(usage_events(&serde_json::json!({}), &terminal).is_none());
    }

    #[test]
    fn quota_reset_timestamps_are_checked_calendar_values() {
        assert_eq!(utc_seconds("1970-01-01T00:00:00Z"), Some(0.0));
        assert_eq!(utc_seconds("2026-10-20T08:00:00.296594+00:00"), Some(1792483200.0));
        for text in [
            "2026-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2026-01-01T00:00:60Z",
            "2026-01-01T00:00:00+03:00",
            "x",
        ] {
            assert_eq!(utc_seconds(text), None, "{text}");
        }
    }

    #[test]
    fn last_known_usage_cannot_be_stamped_as_a_fresh_zero_turn_reading() {
        let report = serde_json::json!({"usage_report":{"rate_limits":{"limits":[{
            "kind":"weekly_all","percent":31,"resets_at":"2026-10-20T08:00:00Z"
        }]}},"message":{"content":[{"type":"text","text":"Showing last-known usage from 20 minutes ago"}]}});
        let terminal = serde_json::json!({"local_command":"usage","total_cost_usd":0,"modelUsage":{}});
        assert!(usage_events(&report, &terminal).is_none());
    }
}
