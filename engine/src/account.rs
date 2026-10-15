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
    run_args(cli, status_args(vendor).iter().map(|a| a.to_string()).collect(), spawn, cwd, timeout).await
}

async fn run_args(cli: &Cli, args: Vec<String>, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Ran> {
    let args: Vec<String> = cli.prefix.iter().cloned().chain(args).collect();
    let p = spawn(&cli.command, &args, cwd).ok()?;
    #[cfg(windows)]
    if let Some(pid) = p.child.as_ref().and_then(|c| c.id()) {
        // A reading in the background never competes with a game or the person's work; if it cannot be lowered it still runs.
        let _ = faber_winproc::lower_priority(pid);
    }
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

/// What `claude -p /usage` gave.
#[derive(Debug, Clone, PartialEq)]
pub enum UsageRead {
    /// The vendor-wide windows: name, used (0 to 1), reset in Unix seconds.
    Windows(Vec<(&'static str, f64, f64)>),
    /// The CLI did not take `/usage` as a local command and asked a model: it is never used for readings again.
    Spent,
    Failed,
}

/// Reads Claude Code's quota windows through its own `/usage` command, which answers locally: no prompt is sent, no model
/// runs, and the result says so (`num_turns` 0 and no tokens). Faber's usual isolation applies (no hooks, settings files or
/// MCP servers), and no session is saved. A CLI whose result shows model work is reported as `Spent`.
pub async fn claude_usage(cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration, now: f64, offset: i64) -> UsageRead {
    let args = [
        "-p",
        "/usage",
        "--output-format",
        "json",
        "--safe-mode",
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--no-session-persistence",
    ];
    let Some(ran) = run_args(cli, args.map(String::from).to_vec(), spawn, cwd, timeout).await else { return UsageRead::Failed };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&ran.out) else { return UsageRead::Failed };
    let tokens: u64 = ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"]
        .iter()
        .map(|k| v["usage"][*k].as_u64().unwrap_or(0))
        .sum();
    if v["num_turns"].as_u64().unwrap_or(0) > 0 || tokens > 0 || v["total_cost_usd"].as_f64().unwrap_or(0.0) > 0.0 {
        return UsageRead::Spent;
    }
    let windows = crate::signals::claude_usage_windows(v["result"].as_str().unwrap_or_default(), now, offset);
    if windows.is_empty() { UsageRead::Failed } else { UsageRead::Windows(windows) }
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
}
