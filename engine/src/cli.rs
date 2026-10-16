//! Finding the official CLIs on the user's computer.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// How to start a CLI: the program, plus arguments that go before Faber's own (a script for `node`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cli {
    pub command: String,
    pub prefix: Vec<String>,
}

impl Cli {
    /// A cheap stand-in for the installed version: the size and modified time of the program and of each script given to it.
    /// It changes when the CLI is updated or downgraded, and costs a few file reads of metadata, never a process.
    pub fn stamp(&self) -> String {
        std::iter::once(&self.command)
            .chain(&self.prefix)
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| format!("{}:{:?}", m.len(), m.modified().ok()))
            .collect::<Vec<_>>()
            .join(";")
    }
}

/// Finds a CLI on PATH without a shell. On Windows, npm installs `.cmd` shims, which cannot be started without a
/// shell, so Faber reads the shim and runs what it points to: a native .exe, or `node script.js`.
/// `exists` and `read` are parameters so tests can describe a computer without touching the disk.
pub fn resolve_cli(name: &str, path_var: &str, windows: bool, exists: &dyn Fn(&str) -> bool, read: &dyn Fn(&str) -> String) -> Option<Cli> {
    let (sep, delim) = if windows { ('\\', ';') } else { ('/', ':') };
    let join = |dir: &str, file: &str| format!("{}{sep}{file}", dir.trim_end_matches(sep));
    let dirs: Vec<&str> = path_var.split(delim).filter(|d| !d.is_empty()).collect();
    let exts: &[&str] = if windows { &[".exe", ".cmd"] } else { &[""] };
    for dir in &dirs {
        for ext in exts {
            let path = join(dir, &format!("{name}{ext}"));
            if !exists(&path) {
                continue;
            }
            if *ext != ".cmd" {
                return Some(Cli { command: path, prefix: vec![] });
            }
            let targets = shim_targets(&read(&path)).into_iter().map(|t| join(dir, &t)).collect::<Vec<_>>();
            let lower = |t: &String| t.to_ascii_lowercase();
            if let Some(exe) = targets.iter().rev().find(|t| lower(t).ends_with(".exe") && !lower(t).ends_with("node.exe"))
                && exists(exe)
            {
                return Some(Cli { command: exe.clone(), prefix: vec![] });
            }
            let script = targets.iter().rev().find(|t| [".js", ".cjs", ".mjs"].iter().any(|e| lower(t).ends_with(e)));
            let node = std::iter::once(dir).chain(dirs.iter()).map(|d| join(d, "node.exe")).find(|p| exists(p));
            if let (Some(script), Some(node)) = (script, node) {
                return Some(Cli { command: node, prefix: vec![script.clone()] });
            }
        }
    }
    None
}

/// The quoted paths after `%dp0%` or `%~dp0` in an npm shim, relative to the shim's folder.
fn shim_targets(shim: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lower = shim.to_ascii_lowercase();
    for marker in ["\"%~dp0", "\"%dp0%"] {
        let mut from = 0;
        while let Some(at) = lower[from..].find(marker) {
            let start = from + at + marker.len();
            let rest = &shim[start..];
            let rest = rest.strip_prefix('\\').unwrap_or(rest);
            if let Some(end) = rest.find('"') {
                let target = &rest[..end];
                if !target.is_empty() && !target.contains('%') {
                    found.push((start, target.to_string()));
                }
            }
            from = start;
        }
    }
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, t)| t).collect()
}

impl Cli {
    /// Finds a CLI on PATH, then in the folders the official installers use, which an app started from the desktop
    /// may not have on its PATH: the native installer's `~/.local/bin` (https://code.claude.com/docs/en/setup), npm's
    /// global folder on Windows, and Homebrew's on macOS.
    pub fn find_in(
        name: &str,
        path_var: &str,
        windows: bool,
        env: &dyn Fn(&str) -> Option<String>,
        exists: &dyn Fn(&str) -> bool,
        read: &dyn Fn(&str) -> String,
    ) -> Option<Cli> {
        let mut dirs = vec![path_var.to_string()];
        if windows {
            dirs.extend(env("USERPROFILE").map(|h| format!("{h}\\.local\\bin")));
            dirs.extend(env("APPDATA").map(|a| format!("{a}\\npm")));
        } else {
            dirs.extend(env("HOME").map(|h| format!("{h}/.local/bin")));
            dirs.extend(["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string()]);
        }
        resolve_cli(name, &dirs.join(if windows { ";" } else { ":" }), windows, exists, read)
    }

    /// Installs an official CLI on Windows (Claude Code native installer or Codex GitHub release archive).
    pub async fn install_windows(
        request: &serde_json::Value,
        fetch: &crate::install::Fetch<'_>,
        download: &crate::install::Download<'_>,
        register: &(dyn Fn(&Path) -> std::io::Result<()> + Send + Sync),
        spawn: crate::proc::Spawner,
        progress: &(dyn Fn(serde_json::Value) + Send + Sync),
    ) -> Result<serde_json::Value, serde_json::Value> {
        crate::install::install_windows(request, fetch, download, register, spawn, progress).await
    }
}

/// Looks the CLI up on this computer.
pub fn find_cli(name: &str) -> Option<Cli> {
    let path_var = std::env::var("PATH").or_else(|_| std::env::var("Path")).unwrap_or_default();
    Cli::find_in(name, &path_var, cfg!(windows), &|k| std::env::var(k).ok(), &|p| Path::new(p).is_file(), &|p| {
        std::fs::read_to_string(p).unwrap_or_default()
    })
}
