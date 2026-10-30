//! Setup guide backend: checking CLI status, installation, and login.

use crate::Faber;
use crate::terminal;
use faber_engine::cli::Cli;
use faber_engine::model::Vendor;
use faber_engine::{AccountStatus, AppEvent};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type Res<T> = Result<T, String>;

/// How long the sign-in check may take. The status commands run at idle priority, so a busy PC slows them: Codex's
/// took 5.2 s on the maintainer's PC (2026-10-29), and the old 5 s limit read a signed-in account as unsure.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub vendor: Vendor,
    pub installed: bool,
    pub account: AccountStatus,
    pub can_install: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgressEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub vendor: Vendor,
    pub step: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub fn parse_vendor(s: &str) -> Option<Vendor> {
    match s.trim().to_ascii_lowercase().as_str() {
        "claude" => Some(Vendor::Claude),
        "codex" | "chatgpt" => Some(Vendor::Codex),
        _ => None,
    }
}

/// Looks for installed CLIs, checking FABER_CLI_<NAME>, FABER_DATA_DIR, and PATH.
pub fn detect_clis() -> BTreeMap<Vendor, Cli> {
    detect_clis_with(&|k| std::env::var(k).ok())
}

pub fn detect_clis_with(env: &dyn Fn(&str) -> Option<String>) -> BTreeMap<Vendor, Cli> {
    Vendor::RUNNABLE
        .into_iter()
        .filter_map(|v| {
            let cli = match env(&format!("FABER_CLI_{}", v.name().to_uppercase())) {
                Some(json) => serde_json::from_str(&json).ok(),
                None => find_cli_candidate(v.name(), env),
            };
            cli.map(|c| (v, c))
        })
        .collect()
}

fn find_cli_candidate(name: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<Cli> {
    if let Some(dir) = env("FABER_DATA_DIR") {
        let base = PathBuf::from(dir);
        let candidates =
            [base.join("bin").join(format!("{name}.exe")), base.join(format!("{name}.exe")), base.join("bin").join(name), base.join(name)];
        for candidate in candidates {
            if candidate.is_file() {
                return Some(Cli { command: candidate.to_string_lossy().into_owned(), prefix: vec![] });
            }
        }
    }
    let path_var = env("PATH").or_else(|| env("Path")).unwrap_or_default();
    Cli::find_in(name, &path_var, cfg!(windows), env, &|p| Path::new(p).is_file(), &|p| std::fs::read_to_string(p).unwrap_or_default())
}

fn user_home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

#[tauri::command]
pub async fn setup_check(app: AppHandle, faber: State<'_, Faber>, vendor: String) -> Res<CheckResult> {
    let Some(v) = parse_vendor(&vendor) else {
        return Err("error.setup.check".into());
    };
    let clis = detect_clis();
    let installed = clis.contains_key(&v);
    faber.engine.replace_clis(clis.clone());
    let account = if installed { faber.engine.account_status(v, SIGN_IN_TIMEOUT).await } else { AccountStatus::Unknown };

    if account == AccountStatus::SignedIn {
        let updated = detect_clis();
        faber.engine.replace_clis(updated.clone());
        if faber.engine.ledger().of(v).billing == faber_engine::budget::Billing::Unknown {
            // Short on purpose: the screen waits on this answer to show Connected, and a plan missed here is read again later.
            faber.engine.check_plans(Duration::from_secs(5)).await;
        }
        // The guide and the tutorial check again on every focus and step; the models are asked for only while Faber has none.
        if let Some(codex) = updated.get(&Vendor::Codex).filter(|_| crate::lock(&faber.models).is_empty()) {
            crate::load_models(&app, codex.clone(), faber.data_dir.clone());
        }
        let _ = app.emit("faber:event", AppEvent::List);
    }

    let can_install = cfg!(windows);
    Ok(CheckResult { vendor: v, installed, account, can_install })
}

#[tauri::command]
pub async fn setup_sign_in(vendor: String) -> Res<()> {
    let Some(v) = parse_vendor(&vendor) else {
        return Err("error.setup.signIn".into());
    };
    let clis = detect_clis();
    let Some(cli) = clis.get(&v) else {
        return Err("error.setup.signIn".into());
    };
    let mut args = cli.prefix.clone();
    args.extend(faber_engine::account::login_args(v).iter().map(|s| s.to_string()));
    let cmd = faber_engine::TerminalCommand { command: cli.command.clone(), args, cwd: user_home_dir() };
    if !terminal::is_safe(&cmd) {
        return Err("error.setup.signIn".into());
    }
    terminal::open(&cmd).map_err(|_| "error.setup.signIn".into())
}

#[tauri::command]
pub async fn setup_install(app: AppHandle, faber: State<'_, Faber>, vendor: String) -> Res<()> {
    // An update never ends the process in the middle of an install (update.rs).
    let _running = crate::update::SetupRunning::begin()?;
    let Some(v) = parse_vendor(&vendor) else {
        let _ = app.emit(
            "faber:event",
            SetupProgressEvent {
                event_type: "setup".into(),
                vendor: Vendor::Claude,
                step: "failed".into(),
                error: Some("error.setup.unsupported".into()),
                detail: Some("Unknown vendor".into()),
            },
        );
        return Err("error.setup.unsupported".into());
    };

    let emit_progress = |step: &str, error: Option<&str>, detail: Option<&str>| {
        let _ = app.emit(
            "faber:event",
            SetupProgressEvent {
                event_type: "setup".into(),
                vendor: v,
                step: step.into(),
                error: error.map(String::from),
                detail: detail.map(String::from),
            },
        );
    };

    if !cfg!(windows) {
        emit_progress("failed", Some("error.setup.unsupported"), Some("Only Windows is supported"));
        return Err("error.setup.unsupported".into());
    }

    // FABER_SETUP_SOURCE (tests): every download comes from that loopback server, the program lands in FABER_DATA_DIR and
    // the user's PATH is left alone. The engine refuses any other address, so tests run the same install code as people.
    let source = std::env::var("FABER_SETUP_SOURCE").ok().map(|s| s.trim_end_matches('/').to_string());
    let destination = match source {
        Some(_) => faber.data_dir.join("bin"),
        None => user_home_dir().join(".local").join("bin"),
    };
    let mut request = serde_json::json!({
        "vendor": v.name(),
        "windows": true,
        "arch": std::env::consts::ARCH,
        "data_dir": faber.data_dir,
        "destination": destination,
    });
    if let Some(base) = &source {
        request["sources"] = serde_json::json!({
            "loopback": base,
            "claude_base": base,
            "codex_release": format!("{base}/releases/latest"),
        });
    }

    let fetch = |url: &str| {
        let (url, loopback) = (url.to_string(), source.clone());
        run_on_thread(async move { faber_engine::install::default_fetch(&url, loopback.as_deref()).await })
    };
    let download = |url: &str, dest: &Path| {
        let (url, dest, loopback) = (url.to_string(), dest.to_path_buf(), source.clone());
        run_on_thread(async move { faber_engine::install::default_download(&url, &dest, loopback.as_deref()).await })
    };
    let register = |dir: &Path| if source.is_some() { Ok(()) } else { faber_engine::install::default_register_path(dir) };
    let progress = |val: serde_json::Value| {
        let stage = val["stage"].as_str().unwrap_or("failed");
        emit_progress(stage, val["key"].as_str(), val["detail"].as_str());
    };

    faber_engine::cli::Cli::install_windows(&request, &fetch, &download, &register, faber_engine::proc::real_spawner(), &progress)
        .await
        .map_err(|err| err["key"].as_str().unwrap_or("error.setup.install").to_string())?;

    // After install succeeds: redetect, replace in engine, reload Codex models, emit list
    let updated = detect_clis();
    faber.engine.replace_clis(updated.clone());
    if let Some(codex) = updated.get(&Vendor::Codex) {
        crate::load_models(&app, codex.clone(), faber.data_dir.clone());
    }
    let _ = app.emit("faber:event", AppEvent::List);

    Ok(())
}

/// Runs a network job to the end on its own thread, because the installer calls it from a synchronous callback.
fn run_on_thread<T: Send + 'static>(job: impl std::future::Future<Output = std::io::Result<T>> + Send + 'static) -> std::io::Result<T> {
    std::thread::spawn(move || tauri::async_runtime::block_on(job))
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::other("network thread panicked")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sign_in_check_outlasts_a_slow_status_command() {
        // Codex's status took 5.2 s at idle priority on a real PC; the check must not give up before a busy one answers.
        assert!(SIGN_IN_TIMEOUT >= Duration::from_secs(15));
    }

    #[test]
    fn c3_1_login_uses_official_args_in_user_dir_and_refuses_shell_characters() {
        let claude_args = faber_engine::account::login_args(Vendor::Claude);
        assert_eq!(claude_args, &["auth", "login"]);

        let codex_args = faber_engine::account::login_args(Vendor::Codex);
        assert_eq!(codex_args, &["login"]);

        let safe_cmd = faber_engine::TerminalCommand {
            command: "claude.exe".into(),
            args: vec!["auth".into(), "login".into()],
            cwd: PathBuf::from("C:\\Users\\Test"),
        };
        assert!(terminal::is_safe(&safe_cmd));

        let unsafe_cmd = faber_engine::TerminalCommand {
            command: "claude.exe".into(),
            args: vec!["auth\" & calc".into()],
            cwd: PathBuf::from("C:\\Users\\Test"),
        };
        assert!(!terminal::is_safe(&unsafe_cmd));
    }

    #[test]
    fn c3_2_cli_detection_handles_json_empty_and_absent() {
        let fake_env = |k: &str| match k {
            "FABER_CLI_CLAUDE" => Some("{\"command\":\"test-claude.exe\",\"prefix\":[\"run\"]}".to_string()),
            "FABER_CLI_CODEX" => Some("".to_string()),
            _ => None,
        };
        let detected = detect_clis_with(&fake_env);
        assert_eq!(detected.get(&Vendor::Claude), Some(&Cli { command: "test-claude.exe".into(), prefix: vec!["run".into()] }));
        assert_eq!(detected.get(&Vendor::Codex), None);
    }

    #[test]
    fn c3_4_can_install_verdict() {
        let res = CheckResult { vendor: Vendor::Claude, installed: false, account: AccountStatus::Unknown, can_install: cfg!(windows) };
        assert_eq!(res.can_install, cfg!(windows));
    }

    #[test]
    fn c3_5_progress_event_serialization_matches_schema() {
        let event = SetupProgressEvent {
            event_type: "setup".into(),
            vendor: Vendor::Claude,
            step: "failed".into(),
            error: Some("error.setup.download".into()),
            detail: Some("Connection refused".into()),
        };
        let json_str = serde_json::to_string(&event).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(val["type"], "setup");
        assert_eq!(val["vendor"], "claude");
        assert_eq!(val["step"], "failed");
        assert_eq!(val["error"], "error.setup.download");
        assert_eq!(val["detail"], "Connection refused");
    }
}
