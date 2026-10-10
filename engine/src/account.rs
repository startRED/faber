//! Signing in to each CLI with its own official flow, and asking the CLI whether it is signed in.
//! Faber never reads the CLIs' credential files; it only runs their status command and keeps the verdict.
//!
//! Claude Code: `claude auth login`, `claude auth status` prints JSON with `loggedIn` and exits 0 when signed in, 1
//! when not (https://code.claude.com/docs/en/cli-reference).
//! Codex: `codex login`, `codex login status` prints the active mode and exits 0 when signed in
//! (https://learn.chatgpt.com/docs/developer-commands?surface=cli).

use crate::cli::Cli;
use crate::model::Vendor;
use crate::proc::Spawner;
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

// A status line is short; anything longer is not a status and is never kept.
const OUTPUT_LIMIT: u64 = 64 * 1024;

/// Runs the status command without a shell. Its output is read into memory only to reach a verdict, then dropped;
/// a CLI that fails to start or does not answer within `timeout` is `Unknown`, and is killed when the check ends.
pub async fn check(vendor: Vendor, cli: &Cli, spawn: &Spawner, cwd: &Path, timeout: Duration) -> AccountStatus {
    let args: Vec<String> = cli.prefix.iter().cloned().chain(status_args(vendor).iter().map(|a| a.to_string())).collect();
    let Ok(p) = spawn(&cli.command, &args, cwd) else { return AccountStatus::Unknown };
    drop(p.stdin); // nothing to type: a CLI waiting for input sees end of input instead
    let mut child = p.child;
    let run = async {
        let (out, err) = tokio::join!(read_all(Some(p.stdout)), read_all(p.stderr));
        let code = match &mut child {
            Some(c) => c.wait().await.ok().and_then(|s| s.code()),
            None => None,
        };
        verdict(vendor, &format!("{out}\n{err}"), code)
    };
    tokio::time::timeout(timeout, run).await.unwrap_or(AccountStatus::Unknown)
}

async fn read_all(stream: Option<Box<dyn AsyncRead + Send + Unpin>>) -> String {
    let mut buf = Vec::new();
    if let Some(s) = stream {
        let _ = s.take(OUTPUT_LIMIT).read_to_end(&mut buf).await;
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn verdict(vendor: Vendor, output: &str, code: Option<i32>) -> AccountStatus {
    let said = match vendor {
        Vendor::Claude => output.lines().find_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok()?["loggedIn"].as_bool()),
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
