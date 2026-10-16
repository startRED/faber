//! The product earns its own role receipts. A role gate opens only on a private, fresh receipt (`qualification`); this module
//! produces that receipt from live probes of the installed CLIs, hidden and contained, and never from anything a caller says.
//!
//! The probes (`codex`, `claude`) are the same observations the manual qualification run makes. Each one runs in its own
//! child process (this program started with `--faber-qualify`), inside a job object, with a scrubbed environment, a hard
//! deadline and a folder of its own; a model turn costs a few cents at most and most probes need none. `judge` reads the
//! rows the probes left and writes a receipt only when every check passed. A failed probe means no receipt, and the
//! reason is returned for the screen.

#[cfg(windows)]
pub mod claude;
#[cfg(windows)]
pub mod codex;
pub mod judge;

use crate::model::Vendor;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Every probe run, in order: the receipt name of its row and the mode the child runs. Codex first, then Claude.
pub const STEPS: [(Vendor, &str, &str); 7] = [
    (Vendor::Codex, "codex-loadout", "--live-loadout"),
    (Vendor::Codex, "codex-native", "--live-native"),
    (Vendor::Codex, "codex-stop", "--live-codex-stop"),
    (Vendor::Claude, "claude-loadout", "--live-claude-loadout"),
    (Vendor::Claude, "claude-native", "--live-claude-native"),
    (Vendor::Claude, "claude-stop", "--live-claude-stop"),
    (Vendor::Claude, "claude-quota", "--live-claude-quota"),
];

/// Seconds each probe took on the first real run (2026-10-15, 204 s in all), for the time left the screen shows.
const STEP_SECONDS: [u64; 7] = [15, 30, 45, 20, 20, 20, 65];
/// One probe's hard deadline; the child and everything it started end when it passes.
const PROBE_DEADLINE: Duration = Duration::from_secs(150);

/// Both vendors a project mission uses have a receipt that opens their gate now.
pub fn ready() -> Result<(), &'static str> {
    for vendor in [Vendor::Claude, Vendor::Codex] {
        super::policy::admit(vendor).map_err(|blocker| blocker.key())?;
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn random_marker() -> String {
    use std::hash::BuildHasher;
    // Independent randomly seeded hashers keep the sentinel absent from the prompt and executable bytes.
    let first = std::collections::hash_map::RandomState::new().hash_one("outside-marker");
    let second = std::collections::hash_map::RandomState::new().hash_one("outside-marker");
    format!("T13_MARKER_{first:016x}{second:016x}")
}

#[cfg(windows)]
fn fingerprint(path: &Path) -> io::Result<String> {
    super::qualification::fingerprint(path)
}

/// A folder under a CLI's credential directory is never on the child's path.
fn protected(path: &Path) -> bool {
    path.components().any(|part| {
        part.as_os_str()
            .to_str()
            .is_some_and(|name| [".claude", ".codex", ".gemini"].iter().any(|blocked| name.eq_ignore_ascii_case(blocked)))
    })
}

/// Directory and runtime names only: no API keys, tokens, loader options or provider overrides.
fn child_environment(temp: &Path, probe: &str, workspace: &Path) -> io::Result<std::collections::BTreeMap<String, String>> {
    let mut env = std::collections::BTreeMap::new();
    for name in
        ["PATH", "SystemRoot", "WINDIR", "USERPROFILE", "APPDATA", "LOCALAPPDATA", "TEMP", "TMP", "ProgramFiles", "ProgramFiles(x86)"]
    {
        if let Ok(value) = std::env::var(name) {
            env.insert(name.to_string(), value);
        }
    }
    env.insert("TEMP".into(), temp.to_string_lossy().into_owned());
    env.insert("TMP".into(), temp.to_string_lossy().into_owned());
    env.insert("FABER_QUALIFICATION_PROBE".into(), probe.into());
    env.insert("FABER_QUALIFICATION_WORKSPACE".into(), workspace.to_string_lossy().into_owned());
    if let Some(path) = env.get("PATH") {
        let permitted: Vec<_> = std::env::split_paths(path).filter(|p| !protected(p)).collect();
        env.insert("PATH".into(), std::env::join_paths(permitted).map_err(io::Error::other)?.to_string_lossy().into_owned());
    }
    Ok(env)
}

/// Runs one probe in a contained child and waits for it. `Err` is a locale key (`reason.qualify.*`): the screen words it, and
/// nothing a probe printed is passed on.
async fn run_probe(exe: &Path, probe: &str, dir: &Path, workspace: &Path) -> Result<(), String> {
    let data = |_: io::Error| "reason.qualify.data".to_string();
    std::fs::create_dir_all(dir.join("temp")).map_err(data)?;
    let env = child_environment(&dir.join("temp"), probe, workspace).map_err(data)?;
    let mut command = tokio::process::Command::new(exe);
    command.arg("--faber-qualify").arg(probe).arg(dir).env_clear().envs(env).current_dir(dir);
    let child = crate::proc::spawn_command(command).map_err(|_| "reason.qualify.start".to_string())?;
    match tokio::time::timeout(PROBE_DEADLINE, crate::proc::command_output(child)).await {
        Ok((true, _)) => Ok(()),
        Ok((false, _)) => Err("reason.qualify.probe".into()),
        Err(_) => Err("reason.qualify.timeout".into()),
    }
}

/// Runs every probe and issues the receipts the rows earn. `exe` is this program; `progress(step, total, vendor)` is told
/// before each probe. `Ok` means both vendors now have a receipt; `Err` is the reason no mission may start.
pub async fn earn(data: &Path, exe: &Path, progress: &(dyn Fn(usize, usize, Vendor) + Send + Sync)) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("reason.qualify.os".into());
    }
    let run: PathBuf = data.join("qualification-run");
    // A run is never resumed or replayed: the folder is Faber's own and starts empty.
    let _ = std::fs::remove_dir_all(&run);
    std::fs::create_dir_all(&run).map_err(|_| "reason.qualify.data".to_string())?;
    for (index, (vendor, name, probe)) in STEPS.iter().enumerate() {
        progress(index + 1, STEPS.len(), *vendor);
        run_probe(exe, probe, &run.join(name), &run.join("work").join(name)).await?;
    }
    let report = judge::issue_all(&run, data).map_err(|_| "reason.qualify.proof".to_string())?;
    let _ = std::fs::remove_dir_all(&run);
    if report.values().any(|checks| checks.values().any(|ok| !ok)) {
        return Err("reason.qualify.proof".into());
    }
    ready().map_err(|key| key.to_string())
}

/// The child entry point: runs one probe to completion. Exit code 0 means the probe's row was observed and written.
pub fn child(probe: &str, data: &Path) -> i32 {
    #[cfg(windows)]
    {
        let result = tokio::runtime::Builder::new_current_thread().enable_all().build().and_then(|runtime| {
            runtime.block_on(async { if probe.starts_with("--live-claude") { claude::run(data).await } else { codex::run(data).await } })
        });
        if let Err(error) = result {
            eprintln!("Qualification failed ({error}); inspect the private receipt for requested model turns.");
            return 1;
        }
        0
    }
    #[cfg(not(windows))]
    {
        let _ = (probe, data);
        eprintln!("Qualification failed (requires the Windows hidden desktop); inspect the private receipt for requested model turns.");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outside_marker_content_changes_without_disclosing_it_in_a_prompt() {
        let first = random_marker();
        let second = random_marker();
        assert_ne!(first, second);
        assert_eq!(first.len(), 43);
        assert!(first.starts_with("T13_MARKER_"));
    }

    #[test]
    fn the_probe_environment_is_scrubbed() {
        let temp = std::env::temp_dir();
        let env = child_environment(&temp, "--live-native", &temp.join("w")).unwrap();
        assert_eq!(env["FABER_QUALIFICATION_PROBE"], "--live-native");
        assert!(env.keys().all(|k| !k.to_lowercase().contains("key") && !k.to_lowercase().contains("token")));
        assert!(protected(Path::new("C:/fixture/.codex/packages/bin")));
        assert!(!protected(Path::new("C:/fixture/installed/bin")));
    }

    #[test]
    fn every_receipt_row_the_judge_reads_has_a_probe() {
        let names: Vec<&str> = STEPS.iter().map(|s| s.1).collect();
        for needed in ["codex-loadout", "codex-native", "codex-stop", "claude-loadout", "claude-native", "claude-stop", "claude-quota"] {
            assert!(names.contains(&needed), "{needed}");
        }
    }
}

/// What the screen is told about the check of the agents: `checking`, `ready` or `failed`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsView {
    pub state: &'static str,
    pub step: usize,
    pub total: usize,
    pub vendor: Option<Vendor>,
    /// A rough time left, from the number of probes that remain.
    pub estimate_seconds: u64,
    /// The locale key (`reason.qualify.*`) of why the check failed.
    pub detail: Option<String>,
}

impl AgentsView {
    pub fn checking(step: usize, vendor: Vendor) -> Self {
        let left: u64 = STEP_SECONDS.iter().skip(step.saturating_sub(1)).sum();
        Self { state: "checking", step, total: STEPS.len(), vendor: Some(vendor), estimate_seconds: left, detail: None }
    }
    pub fn ready() -> Self {
        Self { state: "ready", step: STEPS.len(), total: STEPS.len(), vendor: None, estimate_seconds: 0, detail: None }
    }
    pub fn failed(detail: String) -> Self {
        Self { state: "failed", step: 0, total: STEPS.len(), vendor: None, estimate_seconds: 0, detail: Some(detail) }
    }
}
