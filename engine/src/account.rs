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
    /// A replayed recording has no process and so no exit status: its text is all there is.
    replayed: bool,
}

/// Runs the status command without a shell, with its output bounded. None when the CLI fails to start, does not answer within
/// `timeout` (it is killed when the run ends) or prints more than the bound.
async fn run(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Ran> {
    run_args(cli, status_args(vendor).iter().map(|a| a.to_string()).collect(), spawn, cwd, timeout).await
}

/// Runs a status command or a reading as a background process: both are short and periodic, and neither may compete with a
/// game or the person’s own work (idle priority on Windows; see `proc::spawn_background`).
async fn run_args(cli: &Cli, args: Vec<String>, spawn: &Spawner, cwd: &Path, timeout: Duration) -> Option<Ran> {
    let args: Vec<String> = cli.prefix.iter().cloned().chain(args).collect();
    let p = crate::proc::spawn_background(spawn, &cli.command, &args, cwd).ok()?;
    drop(p.stdin); // nothing to type: a CLI waiting for input sees end of input instead
    let mut child = p.child;
    let run = async {
        let (Some(out), Some(err)) = tokio::join!(read_all(Some(p.stdout)), read_all(p.stderr)) else { return None };
        let code = match &mut child {
            Some(c) => c.wait().await.ok().and_then(|s| s.code()),
            None => None,
        };
        Some(Ran { out, err, code, replayed: child.is_none() })
    };
    tokio::time::timeout(timeout, run).await.ok().flatten()
}

/// Runs the status command. Its output is read into memory only to reach a verdict, then dropped; a CLI that fails to start or
/// does not answer within `timeout` is `Unknown`, and is killed when the check ends.
pub async fn check(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> AccountStatus {
    let Some(Ran { out, err, code, .. }) = run(vendor, cli, spawn, cwd, timeout).await else { return AccountStatus::Unknown };
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
    usage_from(&ran.out, if ran.replayed { Some(0) } else { ran.code }, now, offset)
}

/// What the output of `claude -p /usage` means. A reading needs a clean exit, no error flag and every metering field present
/// and well typed (`num_turns`, the token counts and the cost); a missing or mistyped field is no reading at all, never
/// zero. Any model work (a turn, tokens, cost) makes it `Spent`.
fn usage_from(out: &str, code: Option<i32>, now: f64, offset: i64) -> UsageRead {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(out) else { return UsageRead::Failed };
    // Positive evidence of model work comes first, so a damaged answer that still shows spending switches `/usage` off for good.
    let counts =
        ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"].map(|k| v["usage"][k].as_u64());
    let (turns, cost) = (v["num_turns"].as_u64(), v["total_cost_usd"].as_f64());
    if turns.is_some_and(|t| t > 0) || counts.iter().flatten().any(|c| *c > 0) || cost.is_some_and(|c| c != 0.0) {
        return UsageRead::Spent;
    }
    // Then every field must be there and well typed, and zero: a missing one is no reading, never zero.
    if turns.is_none() || cost.is_none() || counts.iter().any(Option::is_none) {
        return UsageRead::Failed;
    }
    if code != Some(0) || v["is_error"] != false {
        return UsageRead::Failed;
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

    const TEXT: &str = "Current session: 10% used · resets Oct 14, 11:29pm (X/Y)
Current week (all models): 36% used · resets Oct 20, 4:59am (X/Y)";
    const NOW: f64 = 1_791_620_000.0;

    fn clean() -> serde_json::Value {
        serde_json::json!({ "is_error": false, "num_turns": 0, "total_cost_usd": 0,
            "usage": { "input_tokens": 0, "output_tokens": 0, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0 }, "result": TEXT })
    }

    fn read(v: &serde_json::Value, code: Option<i32>) -> UsageRead {
        usage_from(&v.to_string(), code, NOW, 0)
    }

    #[test]
    fn a_clean_local_answer_is_a_reading() {
        assert!(matches!(read(&clean(), Some(0)), UsageRead::Windows(w) if w.len() == 2));
    }

    #[test]
    fn a_failed_answer_is_never_a_reading() {
        let mut errored = clean();
        errored["is_error"] = true.into();
        assert_eq!(read(&errored, Some(0)), UsageRead::Failed);
        assert_eq!(read(&clean(), Some(1)), UsageRead::Failed, "a non-zero exit");
        assert_eq!(read(&clean(), None), UsageRead::Failed, "killed, no exit code");
        let mut no_flag = clean();
        no_flag.as_object_mut().unwrap().remove("is_error");
        assert_eq!(read(&no_flag, Some(0)), UsageRead::Failed, "an answer that does not say it succeeded");
        assert_eq!(usage_from("not json", Some(0), NOW, 0), UsageRead::Failed);
    }

    #[test]
    fn a_missing_or_mistyped_metering_field_is_no_reading_never_zero() {
        for field in ["num_turns", "total_cost_usd"] {
            let mut v = clean();
            v.as_object_mut().unwrap().remove(field);
            assert_eq!(read(&v, Some(0)), UsageRead::Failed, "{field} missing");
        }
        for field in ["input_tokens", "output_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"] {
            let mut v = clean();
            v["usage"].as_object_mut().unwrap().remove(field);
            assert_eq!(read(&v, Some(0)), UsageRead::Failed, "{field} missing");
            v["usage"][field] = "0".into();
            assert_eq!(read(&v, Some(0)), UsageRead::Failed, "{field} a string");
        }
        let mut v = clean();
        v.as_object_mut().unwrap().remove("usage");
        assert_eq!(read(&v, Some(0)), UsageRead::Failed);
        let mut v = clean();
        v["total_cost_usd"] = "0".into();
        assert_eq!(read(&v, Some(0)), UsageRead::Failed);
    }

    #[test]
    fn spending_is_seen_before_missing_fields_so_a_damaged_answer_still_switches_usage_off() {
        let mut turns = clean();
        turns["num_turns"] = 1.into();
        turns.as_object_mut().unwrap().remove("usage");
        assert_eq!(read(&turns, Some(0)), UsageRead::Spent);
        let mut tokens = clean();
        tokens["usage"] = serde_json::json!({ "output_tokens": 9 });
        tokens.as_object_mut().unwrap().remove("num_turns");
        assert_eq!(read(&tokens, Some(1)), UsageRead::Spent);
        let mut cost = clean();
        cost["total_cost_usd"] = 0.02.into();
        cost.as_object_mut().unwrap().remove("num_turns");
        assert_eq!(read(&cost, None), UsageRead::Spent);
    }

    #[test]
    fn any_model_work_is_spent_even_when_the_exit_is_bad() {
        for (key, value) in [("num_turns", serde_json::json!(1)), ("total_cost_usd", serde_json::json!(0.01))] {
            let mut v = clean();
            v[key] = value;
            assert_eq!(read(&v, Some(0)), UsageRead::Spent, "{key}");
            assert_eq!(read(&v, Some(1)), UsageRead::Spent, "{key}, exit 1");
        }
        let mut v = clean();
        v["usage"]["output_tokens"] = 5.into();
        assert_eq!(read(&v, Some(0)), UsageRead::Spent);
    }
}
