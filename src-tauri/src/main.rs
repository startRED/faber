//! The desktop shell: one window, a tray icon, and the commands the screen may call. The engine does the work.

#![cfg_attr(windows, windows_subsystem = "windows")]

// The test hooks (`report_checks`, the checks page) never ship: a release build cannot be made with them.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

mod folder;
mod look;
mod project_commands;
mod setup;
mod terminal;
mod tray;
mod update;

#[cfg(windows)]
use awake::WindowsPower;
use awake::{CountedAwake, NoopPower, PowerKeepAwake};
use base64::Engine as _;
#[cfg(not(windows))]
use faber::autostart::NoopAutostart;
#[cfg(windows)]
use faber::autostart::WindowsAutostart;
use faber::autostart::{self, AutostartKeeper, FakeAutostart, creates_window};
use faber::awake;
use faber::local_update::{self, Development};
use faber_engine::cli::Cli;
use faber_engine::codex::{ModelInfo, list_codex_models, read_codex_limits};
use faber_engine::model::{Conversation, Project, Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::project::canary;
use faber_engine::project::coordinator::{self, Coordinator};
use faber_engine::project::git::{Broker, Process, enroll};
use faber_engine::project::mission;
use faber_engine::project::run::Runner;
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{DecisionView, EnrollView, MissionView, TaskView};
use faber_engine::project::{Effect, GitMode, Grant, Mission, RunMode};
use faber_engine::{AppEvent, AppState, BranchRequest, Engine, EngineOptions, SendRequest, WarmRequest};
use look::{Failure, Look, ThemeFile};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::Update;

pub(crate) struct Faber {
    pub(crate) engine: Engine,
    /// ChatGPT models this account can use; empty until the Codex CLI answers.
    pub(crate) models: Mutex<Vec<ModelInfo>>,
    /// A newer version, already downloaded; it installs at the first idle moment (`update.rs`) or when Faber quits.
    pub(crate) update: Mutex<Option<(Update, Vec<u8>)>>,
    /// The install at open, as the screen shows it: looking, downloading (how far), installing (`update.rs`).
    pub(crate) stage: Mutex<update::Status>,
    /// The window was shown after its first paint (or by the failsafe); false from its creation until then. A window not
    /// shown yet counts as seen, so nothing closes it under the person (`update.rs`).
    pub(crate) revealed: std::sync::atomic::AtomicBool,
    /// The person has typed, pasted or dropped something since Faber opened: a draft may exist only on the screen, so the
    /// install at open waits for an idle moment (`update.rs`).
    pub(crate) touched: std::sync::atomic::AtomicBool,
    /// Some field on the screen holds something typed or attached and not sent: no unattended install closes the window then.
    pub(crate) draft_dirty: std::sync::atomic::AtomicBool,
    pub(crate) data_dir: PathBuf,
    pub(crate) hidden: bool,
    /// Serialize journal snapshots and mutations, so an answer cannot overwrite a newer view.
    missions: Arc<tauri::async_runtime::Mutex<()>>,
    local: Option<Development>,
    pub(crate) awake: Arc<Mutex<CountedAwake>>,
    /// One check of the agents at a time; a second start waits for it and finds the receipts it earned.
    qualifying: tauri::async_runtime::Mutex<()>,
    _mission_events: faber_engine::project::store::MissionSubscription,
    pub(crate) autostart: Mutex<Arc<dyn AutostartKeeper>>,
}

type Res<T> = Result<T, String>;

#[derive(Serialize)]
struct FullState {
    #[serde(flatten)]
    state: AppState,
    /// The look of the saved appearance; the screen applies it before it renders.
    look: Look,
    models: Vec<ModelInfo>,
    version: String,
}

/// What `save_settings` answers: the settings as stored, and the look computed from the stored appearance.
#[derive(Serialize)]
struct SavedSettings {
    settings: Settings,
    look: Look,
}

const IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn main_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}

// The screen calls the engine through this fixed list of commands; nothing else crosses over.

#[tauri::command]
async fn state(app: AppHandle, faber: State<'_, Faber>) -> Res<FullState> {
    let mut state = faber.engine.state();
    state.settings = std::mem::take(&mut state.settings).shown();
    state.settings.autostart = Some(autostart::shown(lock(&faber.autostart).as_ref(), state.settings.autostart()));
    state.settings.appearance = look::usable(state.settings.appearance.take());
    let look = look::stored(state.settings.appearance.as_ref());
    Ok(FullState { state, look, models: lock(&faber.models).clone(), version: app.package_info().version.to_string() })
}

#[tauri::command]
async fn open(faber: State<'_, Faber>, id: String) -> Res<Option<Conversation>> {
    Ok(faber.engine.open(&id))
}

#[tauri::command]
async fn send(faber: State<'_, Faber>, req: SendRequest) -> Res<String> {
    faber.engine.send(req)
}

#[tauri::command]
async fn branch(faber: State<'_, Faber>, req: BranchRequest) -> Res<String> {
    faber.engine.branch(req)
}

#[tauri::command]
fn set_clock(faber: State<'_, Faber>, utc_offset: i64) {
    faber.engine.set_utc_offset(utc_offset);
}

/// The window was shown, hidden or focused: the quota is read more often while it is open, and at once on focus.
#[tauri::command]
fn quota_attention(faber: State<'_, Faber>, visible: bool, focused: bool) {
    faber.engine.set_attention(visible, focused);
}

#[tauri::command]
fn answer_plan(faber: State<'_, Faber>, vendor: Vendor, plan: String) {
    faber.engine.answer_plan(vendor, &plan);
}
#[tauri::command]
fn approve_credits(faber: State<'_, Faber>, id: String, turn_id: String, vendor: Vendor) -> Res<()> {
    faber.engine.approve_credits(&id, &turn_id, vendor)
}
#[tauri::command]
fn move_turn(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.move_turn(&id, &turn_id)
}
#[tauri::command]
fn wait_turn(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.wait_turn(&id, &turn_id)
}

#[tauri::command]
async fn warm(faber: State<'_, Faber>, req: WarmRequest) -> Res<()> {
    faber.engine.warm(req);
    Ok(())
}

#[tauri::command]
async fn route_draft(faber: State<'_, Faber>, req: WarmRequest, text: String) -> Res<()> {
    faber.engine.route_draft(req, text);
    Ok(())
}

#[tauri::command]
async fn interrupt(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.interrupt(&id);
    Ok(())
}

#[tauri::command]
async fn answer(faber: State<'_, Faber>, id: String, turn_id: String, request_id: String, allow: bool, always: bool) -> Res<()> {
    faber.engine.answer(&id, &turn_id, &request_id, allow, always).await
}

/// The person's choices for a question the agent asked: each question's text to the label picked.
#[tauri::command]
async fn answer_question(faber: State<'_, Faber>, id: String, turn_id: String, request_id: String, answers: serde_json::Value) -> Res<()> {
    faber.engine.answer_question(&id, &turn_id, &request_id, answers).await
}

#[tauri::command]
async fn rename(faber: State<'_, Faber>, id: String, title: String) -> Res<()> {
    faber.engine.rename(&id, &title);
    Ok(())
}

#[tauri::command]
async fn remove(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.remove(&id);
    Ok(())
}

#[tauri::command]
async fn remove_queued(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.remove_queued(&id, &turn_id);
    Ok(())
}

#[tauri::command]
async fn send_queued(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.send_queued(&id, &turn_id)
}

#[tauri::command]
async fn remove_project(faber: State<'_, Faber>, id: String) -> Res<()> {
    // One turn at a time with the mission commands, so a mission cannot start between the check and the removal.
    let _turn = faber.missions.lock().await;
    faber.engine.remove_project(&id)
}

/// The look of an appearance that is not saved. Failing pairs come back inside the look.
#[tauri::command]
async fn preview_look(appearance: Map<String, Value>) -> Result<Look, Failure> {
    look::preview(&appearance)
}

/// A theme file comes from other people, so an appearance is checked here before anything is saved.
#[tauri::command]
async fn save_settings(faber: State<'_, Faber>, mut patch: Settings) -> Result<SavedSettings, Failure> {
    if let Some(map) = &patch.appearance {
        patch.appearance = Some(look::validated(map)?.0.to_map());
    }
    let autostart = lock(&faber.autostart);
    let mut settings = autostart::save(autostart.as_ref(), patch.autostart, || faber.engine.save_settings(patch))?.shown();
    // The answer replaces the settings on screen, so the switch shows what Windows will do, as `state` does.
    settings.autostart = Some(autostart::shown(autostart.as_ref(), settings.autostart()));
    lock(&faber.awake).set_enabled(settings.keep_awake());
    settings.appearance = look::usable(settings.appearance.take());
    let look = look::stored(settings.appearance.as_ref());
    Ok(SavedSettings { settings, look })
}

#[tauri::command]
async fn save_theme(app: AppHandle, appearance: Map<String, Value>, name: String) -> Result<bool, Failure> {
    let json = look::write_theme_file(&appearance, &name)?;
    // The dialog suggests the theme's own name, which arrives in the person's language, keeping only plain characters.
    let stem: String = name.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).collect();
    let mut dialog = app.dialog().file().add_filter("JSON", &["json"]).set_file_name(format!("{}{}", stem.trim(), look::theme_extension()));
    if let Some(w) = main_window(&app) {
        dialog = dialog.set_parent(&w);
    }
    let Some(target) = dialog.blocking_save_file() else { return Ok(false) };
    look::export_theme(&target.into_path().map_err(|_| Failure::key("error.theme.write"))?, &json)?;
    Ok(true)
}

#[tauri::command]
async fn open_theme(app: AppHandle) -> Result<Option<ThemeFile>, Failure> {
    let mut dialog = app.dialog().file().add_filter("JSON", &["json"]);
    if let Some(w) = main_window(&app) {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_file() else { return Ok(None) };
    look::import_theme(&picked.into_path().map_err(|_| Failure::key("error.theme.read"))?).map(Some)
}

#[tauri::command]
async fn add_project(app: AppHandle, faber: State<'_, Faber>) -> Res<Option<Project>> {
    let mut dialog = app.dialog().file();
    if let Some(w) = main_window(&app) {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_folder() else { return Ok(None) };
    let path = picked.into_path().map_err(|_| "error.folder.notLocal")?;
    faber.engine.open_folder(&path.to_string_lossy()).map(Some)
}

/// The recent folders and saved environments, judged now: a folder that is gone or refused carries a reason key.
#[tauri::command]
async fn recents(faber: State<'_, Faber>) -> Res<faber_engine::recents::RecentsView> {
    Ok(faber.engine.recents())
}

#[tauri::command]
async fn open_folders(faber: State<'_, Faber>, paths: Vec<String>) -> Res<Vec<Project>> {
    faber.engine.open_folders(&paths)
}

#[tauri::command]
async fn forget_recent(faber: State<'_, Faber>, path: String) -> Res<()> {
    faber.engine.forget_recent(&path)
}

#[tauri::command]
async fn save_environment(faber: State<'_, Faber>, name: String, folders: Vec<String>) -> Res<()> {
    faber.engine.save_environment(&name, &folders)
}

#[tauri::command]
async fn forget_environment(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.forget_environment(&id)
}

#[tauri::command]
async fn start_environment(faber: State<'_, Faber>, id: String) -> Res<Vec<Project>> {
    faber.engine.start_environment(&id)
}

#[tauri::command]
async fn chat_folder(
    app: AppHandle,
    faber: State<'_, Faber>,
    id: Option<String>,
    scratch: bool,
    path: Option<String>,
) -> Res<Option<Conversation>> {
    if scratch {
        return faber.engine.chat_folder(id.as_deref(), None).map(Some);
    }
    // A folder taken from the recents needs no dialog; the engine checks it like any chosen folder.
    if let Some(path) = path {
        return faber.engine.chat_folder(id.as_deref(), Some(Path::new(&path))).map(Some);
    }
    // A hidden test build must never open a visible native dialog.
    if faber.hidden {
        return Err("error.chatFolder".into());
    }
    let mut dialog = app.dialog().file();
    if let Some(w) = main_window(&app) {
        dialog = dialog.set_parent(&w);
    }
    let Some(picked) = dialog.blocking_pick_folder() else { return Ok(None) };
    let path = picked.into_path().map_err(|_| "error.chatFolder")?;
    faber.engine.chat_folder(id.as_deref(), Some(&path)).map(Some)
}

/// Copies a file from a chat workspace to wherever the user picks.
#[tauri::command]
async fn save_file(app: AppHandle, faber: State<'_, Faber>, id: String, path: String) -> Res<bool> {
    let Some(source) = faber.engine.workspace_file(&id, &path) else { return Ok(false) };
    let mut dialog = app.dialog().file().set_file_name(source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    if let Some(w) = main_window(&app) {
        dialog = dialog.set_parent(&w);
    }
    let Some(target) = dialog.blocking_save_file() else { return Ok(false) };
    std::fs::copy(&source, target.into_path().map_err(|_| "error.file.save")?).map_err(|_| "error.file.save")?;
    Ok(true)
}

#[tauri::command]
async fn show_file(app: AppHandle, faber: State<'_, Faber>, id: String, path: String) -> Res<()> {
    let Some(source) = faber.engine.workspace_file(&id, &path) else { return Ok(()) };
    app.opener().reveal_item_in_dir(source).map_err(|_| "error.file.show".into())
}

/// Shows a project's folder in the file manager. The saved path is checked first (folder.rs), and the folder is only
/// revealed, never opened: if something else took its place, the file manager selects it and starts nothing.
#[tauri::command]
async fn show_project(app: AppHandle, faber: State<'_, Faber>, id: String) -> Res<()> {
    let project = faber.engine.project(&id)?;
    let folder = folder::local_folder(&project.path)?;
    app.opener().reveal_item_in_dir(folder).map_err(|_| "error.file.show".into())
}

/// An image from a chat workspace as a data URL, so the page never reads the disk itself.
#[tauri::command]
async fn read_image(faber: State<'_, Faber>, id: String, path: String) -> Res<Option<String>> {
    let Some(source) = faber.engine.workspace_file(&id, &path) else { return Ok(None) };
    let ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let Some((_, kind)) = IMAGE_TYPES.iter().find(|(e, _)| Some(*e) == ext.as_deref()) else { return Ok(None) };
    // A picture that cannot be read only has no preview.
    if !std::fs::metadata(&source).is_ok_and(|m| m.len() <= 8_000_000) {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(&source) else { return Ok(None) };
    Ok(Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

#[tauri::command]
async fn open_terminal(faber: State<'_, Faber>, id: String) -> Res<bool> {
    let Some(cmd) = faber.engine.terminal(&id) else { return Ok(false) };
    terminal::open(&cmd).map_err(|_| "error.terminal.open")?;
    Ok(true)
}

/// Links in answers open in the user's browser, never inside Faber.
#[tauri::command]
async fn open_link(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("error.link.web".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|_| "error.link.open".into())
}

/// The words the shell shows by itself (tray menu, tooltip, notification), sent whenever the language changes.
#[tauri::command]
async fn set_shell_texts(app: AppHandle, texts: tray::ShellTexts) -> Res<()> {
    tray::set_texts(&app, texts)
}

fn build_mission_view(faber: &Faber, mission: &Mission, journal: &Journal, grant: &Grant) -> Res<MissionView> {
    let plan = journal.plan().map_err(|e| e.key().to_string())?;
    if let Some(plan) = &plan {
        plan.validate().map_err(|e| e.key().to_string())?;
    }
    let decisions = journal.decisions().map_err(|e| e.key().to_string())?;
    let artifacts = journal.artifacts().map_err(|e| e.key().to_string())?;
    let decision_views = decisions
        .into_iter()
        .filter(|d| d.answer.is_none())
        .map(|d| DecisionView {
            id: d.id,
            question: d.question,
            kind: d.kind,
            options: d.options,
            default: d.default,
            task: Some(d.asked_by),
        })
        .collect();
    let task_views = if let Some(plan) = &plan {
        plan.tasks
            .iter()
            .map(|t| TaskView {
                id: t.id.clone(),
                title: t.title.clone(),
                state: t.state,
                reason: t.reason.clone(),
                builder: None,
                reviewer: None,
                detail: Some(faber_engine::project::wire::TaskDetail {
                    scope: t.writes.clone(),
                    proofs: t.proofs.clone(),
                    artifacts: artifacts.iter().filter(|a| a.task == t.id).cloned().collect(),
                }),
            })
            .collect()
    } else {
        vec![]
    };
    let (cost, pace) = faber.engine.mission_readings(&mission.id)?;
    Ok(MissionView {
        mission_id: mission.id.clone(),
        revision: project_commands::revision(mission),
        state: mission.state,
        mode: mission.mode,
        git_mode: grant.mode,
        goal: mission.goal.clone(),
        plan: None,
        tasks: task_views,
        cost,
        pace,
        decisions: decision_views,
        build: None,
        reason: mission.reason.clone(),
        progress: faber_engine::project::mission::progress(journal, mission),
        report: faber_engine::project::mission::report_view(journal, mission),
        needs: faber_engine::project::mission::needs(mission),
    })
}

fn find_mission_and_journal(faber: &Faber, mission_id: &str) -> Res<(Project, Journal, Mission)> {
    project_commands::id(mission_id)?;
    let projects = faber.engine.projects_checked()?;
    for p in projects {
        let record = Path::new(&p.path).join(".faber/missions").join(mission_id);
        let checkpoint = faber.data_dir.join("missions").join(mission_id).join("run.json");
        if !record.join("mission.json").is_file() && !(record.is_dir() && checkpoint.is_file()) {
            continue;
        }
        let journal = Journal::open(Path::new(&p.path), &faber.data_dir, mission_id).map_err(|e| e.key().to_string())?;
        let mission = Runner::open(&journal).map_err(str::to_string)?.mission;
        if mission.id != mission_id || mission.project_id != p.id || p.repo.as_ref().is_none_or(|r| r.common_dir != mission.repo) {
            return Err("error.journal.mismatch".into());
        }
        return Ok((p, journal, mission));
    }
    Err("error.journal.mismatch".into())
}

fn restore_mission_notices(app: &AppHandle) -> Res<()> {
    let faber = app.state::<Faber>();
    let _turn = faber.missions.blocking_lock();
    for project in faber.engine.projects_checked()? {
        let entries = match std::fs::read_dir(Path::new(&project.path).join(".faber/missions")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => continue,
        };
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let id = entry.file_name().to_string_lossy().into_owned();
            if !faber_engine::project::valid_id(&id) {
                continue;
            }
            let (owner, journal, _) = match find_mission_and_journal(&faber, &id) {
                Ok(found) => found,
                Err(_) => continue,
            };
            let mission = match awake::restore_mission(&faber.awake, &journal) {
                Ok(runner) => runner.mission,
                Err(error) => {
                    eprintln!("faber: mission recovery failed: {error}");
                    continue;
                }
            };
            let Some(grant) = owner.grant else { continue };
            let Ok(view) = build_mission_view(&faber, &mission, &journal, &grant) else { continue };
            tray::on_mission(app, &view);
            sync_mission_awake(&faber, &mission.id, mission.state);
        }
    }
    Ok(())
}

fn sync_mission_awake(faber: &Faber, mission_id: &str, state: faber_engine::project::MissionState) {
    let mut awake = lock(&faber.awake);
    awake.set_mission_awake(
        mission_id,
        matches!(state, faber_engine::project::MissionState::Running | faber_engine::project::MissionState::Planning),
    );
}

#[tauri::command]
async fn project_enroll(faber: State<'_, Faber>, project_id: String) -> Res<EnrollView> {
    project_commands::id(&project_id)?;
    let project = faber.engine.project(&project_id)?;
    let folder = PathBuf::from(&project.path);
    let mut proc = Process::find().map_err(|e| e.key().to_string())?;
    let repo_info = enroll(&mut proc, &folder, "HEAD").map_err(|e| e.key().to_string())?;
    if project.repo.as_ref().is_some_and(|r| r.common_dir != repo_info.identity.common_dir) {
        return Err("reason.grant.otherRepo".into());
    }
    let mut updated = project.clone();
    updated.repo = Some(repo_info.identity.clone());
    faber.engine.update_project(&project, &updated)?;
    let revision = project.grant.as_ref().map_or(Some(1), |g| g.revision.checked_add(1)).ok_or("reason.grant.stale")?;
    let mode = project.grant.as_ref().map_or(GitMode::Seguro, |g| g.mode);
    let self_update = project.grant.as_ref().is_some_and(|g| g.self_update);
    let remote = project.grant.as_ref().and_then(|g| g.remote.clone());
    let remote_account = project.grant.as_ref().and_then(|g| g.remote_account.clone());
    Ok(EnrollView {
        project_id,
        repo: project.path.clone(),
        base: repo_info.base,
        author: repo_info.author,
        mode,
        revision,
        self_update,
        remote,
        remote_account,
    })
}

#[tauri::command]
async fn project_grant(faber: State<'_, Faber>, project_id: String, mode: GitMode, self_update: bool, revision: u64) -> Res<Grant> {
    project_commands::id(&project_id)?;
    let mut project = faber.engine.project(&project_id)?;
    let before = project.clone();
    let folder = PathBuf::from(&project.path);
    let mut proc = Process::find().map_err(|e| e.key().to_string())?;
    let repo_info = enroll(&mut proc, &folder, "HEAD").map_err(|e| e.key().to_string())?;
    let common_dir = repo_info.identity.common_dir.clone();
    if project.repo.as_ref().is_some_and(|r| r.common_dir != repo_info.identity.common_dir) {
        return Err("reason.grant.otherRepo".into());
    }
    project.repo = Some(repo_info.identity);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut grant = Grant::new(&common_dir, mode, revision, now);
    grant.author = repo_info.author;
    grant.self_update = self_update;
    project.accept_grant(grant).map_err(str::to_string)?;
    faber.engine.update_project(&before, &project)?;
    project.grant.ok_or_else(|| "reason.grant.none".into())
}

#[tauri::command]
async fn project_revoke(faber: State<'_, Faber>, project_id: String) -> Res<()> {
    project_commands::id(&project_id)?;
    let mut project = faber.engine.project(&project_id)?;
    let before = project.clone();
    if let Some(grant) = &mut project.grant {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        grant.revoked_at = Some(now);
        faber.engine.update_project(&before, &project)?;
    }
    Ok(())
}

/// Tell the screen and the tray where a mission is now. Called after every step the coordinator finishes.
async fn publish(app: &AppHandle, mission_id: &str) {
    let faber = app.state::<Faber>();
    let _turn = faber.missions.lock().await;
    let Ok((project, journal, mission)) = find_mission_and_journal(&faber, mission_id) else { return };
    sync_mission_awake(&faber, &mission.id, mission.state);
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    if let Ok(view) = build_mission_view(&faber, &mission, &journal, &grant) {
        let _ = app.emit("mission", &view);
        tray::on_mission(app, &view);
    }
}

/// End a mission that could not even begin the way the coordinator would: a wait for the owner or a block, with the key.
fn halt(journal: &Journal, key: &str) {
    let Ok(mut runner) = Runner::load(journal) else { return };
    runner.mission.reason = Some(key.to_string());
    if matches!(coordinator::classify(key), coordinator::Stop::Block(_)) {
        runner.mission.state = faber_engine::project::MissionState::Blocked;
        let _ = runner.save(journal);
    } else {
        let _ = runner.pause(journal);
    }
}

/// A mission does not start or resume once an update install is decided (`update.rs`): the installer ends this process, and
/// a worker begun after the decision would be killed. The owner starts it again after Faber reopens.
fn refuse_while_installing() -> Res<()> {
    if update::installing() { Err("error.update.closing".into()) } else { Ok(()) }
}

/// Run a mission through the coordinator in the background until it lands or stops. Every step is saved to the journal,
/// so a stop, a crash or a restart resumes from the facts (R135). One drive per mission at a time.
fn drive_mission(app: &AppHandle, mission_id: String, commands: Vec<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let faber = app.state::<Faber>();
        // While it is held, the engine counts as working, so an update never ends the drive midway (C004), and its project
        // cannot be removed. A mission that cannot be found has nothing to drive.
        let project = {
            let _turn = faber.missions.lock().await;
            find_mission_and_journal(&faber, &mission_id).map(|(project, _, _)| project.id)
        };
        let Some(drive) = project.ok().and_then(|project| faber.engine.drive(&mission_id, &project)) else { return };
        // An update install decided after the line above finds this mission driven and waits; one decided before it is seen here.
        if update::installing() {
            return;
        }
        if let Err(key) = run_mission(&app, &faber, &mission_id, commands).await {
            let _turn = faber.missions.lock().await;
            if let Ok((_, journal, _)) = find_mission_and_journal(&faber, &mission_id) {
                halt(&journal, &key);
            }
        }
        drop(drive);
        publish(&app, &mission_id).await;
    });
}

async fn run_mission(app: &AppHandle, faber: &Faber, mission_id: &str, commands: Vec<String>) -> Result<(), String> {
    let (project, journal, mission) = {
        let _turn = faber.missions.lock().await;
        find_mission_and_journal(faber, mission_id)?
    };
    let engine = faber.engine.clone();
    // The ceiling and the fresh account readings it is admitted on (R131). A refusal here is a saved wait, not a failure.
    if engine.mission_readings(mission_id)?.0.cap_micros.is_none() {
        canary::read_accounts(&engine).await;
        canary::caps(&engine, mission_id, mission::MISSION_CAP_MICROS)?;
    }
    let pins = tauri::async_runtime::spawn_blocking(mission::product_pins).await.map_err(|_| "error.wire.stale".to_string())??;
    {
        let _turn = faber.missions.lock().await;
        let mut runner = Runner::load(&journal).map_err(str::to_string)?;
        mission::remember_roles(&mut runner.mission, &pins);
        runner.save(&journal).map_err(str::to_string)?;
    }
    let installs = mission::confirmed_setup(&project).map_err(str::to_string)?;
    let folder = PathBuf::from(&project.path);
    let base = mission.base.clone();
    let (revision, data, pid) = (mission.grant_revision, faber.data_dir.clone(), project.id.clone());
    let notify: Arc<dyn Fn() + Send + Sync> = {
        let (app, id) = (app.clone(), mission_id.to_string());
        Arc::new(move || {
            let (app, id) = (app.clone(), id.clone());
            tauri::async_runtime::spawn(async move { publish(&app, &id).await });
        })
    };
    let grant: Arc<dyn Fn() -> Result<Option<Grant>, faber_engine::project::git::GitError> + Send + Sync> = {
        let (engine, id) = (engine.clone(), project.id.clone());
        Arc::new(move || Ok(engine.project(&id).ok().and_then(|p| p.grant)))
    };
    let owner = project.repo.clone();
    let (repository, setup) = tauri::async_runtime::spawn_blocking(move || {
        let mut process = Process::find().map_err(|e| e.key().to_string())?;
        let repository = enroll(&mut process, &folder, &base).map_err(|e| e.key().to_string())?;
        if owner.as_ref() != Some(&repository.identity) {
            return Err("reason.grant.otherRepo".to_string());
        }
        let launch = mission::Launch {
            repository: repository.clone(),
            commands,
            setup: installs,
            data,
            project: pid,
            pins,
            grant,
            revision,
            notify: Some(notify),
        };
        let setup = mission::setup(launch, &mut process, &faber_engine::project::mission::resolve_tool)?;
        Ok((repository, setup))
    })
    .await
    .map_err(|_| "error.wire.stale".to_string())??;
    let broker = Broker::open(repository, journal.clone(), mission_id, revision, Process::find().map_err(|e| e.key().to_string())?)
        .map_err(|e| e.key().to_string())?;
    let outcome = Coordinator::new(engine, journal, broker, setup)?.drive().await;
    if let coordinator::Outcome::Stopped(coordinator::Stop::Crash) = outcome {
        return Err("reason.interrupted".into());
    }
    Ok(())
}

#[tauri::command]
async fn project_proof(faber: State<'_, Faber>, project_id: String) -> Res<mission::ProofView> {
    project_commands::id(&project_id)?;
    Ok(mission::view(&faber.engine.project(&project_id)?))
}

/// The owner confirms the commands a mission proves its work with (R137): the project's declared list, or the edited one.
#[tauri::command]
async fn project_proof_confirm(
    faber: State<'_, Faber>,
    project_id: String,
    commands: Vec<String>,
    setup: Vec<String>,
    declared: Vec<String>,
) -> Res<mission::ProofView> {
    let _turn = faber.missions.lock().await;
    project_commands::id(&project_id)?;
    for command in commands.iter().chain(&setup).chain(&declared) {
        project_commands::text(command)?;
    }
    let before = faber.engine.project(&project_id)?;
    let after = mission::confirm(&before, &commands, &setup, &declared).map_err(str::to_string)?;
    faber.engine.update_project(&before, &after)?;
    Ok(mission::view(&after))
}

/// The agents a mission uses must hold fresh receipts. When one is missing, stale or for another policy, Faber earns them
/// itself with its own contained probes (a few cents at most, hidden) and tells the screen how it goes. Nothing is forged:
/// a failed probe means no mission, and the screen gets the probe's reason.
async fn ensure_agents(app: &AppHandle, faber: &Faber) -> Res<()> {
    use faber_engine::project::qualify::{self, AgentsView};
    if qualify::ready().is_ok() {
        return Ok(());
    }
    // The hidden test build runs fake CLIs: its screen tests assert the gate stays shut, so it never earns receipts.
    if cfg!(feature = "test-hooks") {
        return Err(faber_engine::project::policy::Blocker::Restrictions.key().into());
    }
    let _one = faber.qualifying.lock().await;
    if qualify::ready().is_ok() {
        return Ok(());
    }
    // The check runs real probes: an update install waits for it, and none begins once the install is decided.
    let _running = update::SetupRunning::begin()?;
    let tell = |view: AgentsView| {
        let _ = app.emit("agents", &view);
    };
    tell(AgentsView::checking(1, faber_engine::model::Vendor::Codex));
    let exe = std::env::current_exe().map_err(|_| "reason.qualify.failed".to_string())?;
    let announce = app.clone();
    let progress = move |step: usize, _total: usize, vendor: faber_engine::model::Vendor| {
        let _ = announce.emit("agents", &AgentsView::checking(step, vendor));
    };
    match qualify::earn(&faber.data_dir, &exe, &progress).await {
        Ok(()) => {
            tell(AgentsView::ready());
            Ok(())
        }
        Err(detail) => {
            tell(AgentsView::failed(detail));
            Err("reason.qualify.failed".into())
        }
    }
}

#[tauri::command]
async fn mission_start(app: AppHandle, faber: State<'_, Faber>, project_id: String, goal: String) -> Res<String> {
    refuse_while_installing()?;
    project_commands::id(&project_id)?;
    project_commands::text(&goal)?;
    // The cheap, certain refusals come first (a grant, the owner's confirmed checks), so no probe is spent on a mission
    // that could not start; then the agents are checked; then the mission is made under the lock.
    {
        let project = faber.engine.project(&project_id)?;
        let grant = project.grant.as_ref().ok_or("reason.grant.none")?;
        project.allows(Effect::Worktree, grant.revision).map_err(|k| k.to_string())?;
        // R137: a mission proves its work only with the commands the project declared and its owner confirmed.
        mission::confirmed(&project).map_err(str::to_string)?;
    }
    ensure_agents(&app, &faber).await?;
    let _turn = faber.missions.lock().await;
    let project = faber.engine.project(&project_id)?;
    let grant = project.grant.as_ref().ok_or("reason.grant.none")?;
    project.allows(Effect::Worktree, grant.revision).map_err(|k| k.to_string())?;
    Runner::role_vendor(&faber.engine)?;
    let commands = mission::confirmed(&project).map_err(str::to_string)?;
    let mut process = Process::find().map_err(|e| e.key().to_string())?;
    let repo = enroll(&mut process, Path::new(&project.path), "HEAD").map_err(|e| e.key().to_string())?;
    if project.repo.as_ref() != Some(&repo.identity) {
        return Err("reason.grant.otherRepo".into());
    }
    let mission = faber.engine.new_mission(&project, &goal, &repo.base)?;
    let journal = Journal::open(Path::new(&project.path), &faber.data_dir, &mission.id).map_err(|e| e.key().to_string())?;
    let runner = Runner::start(&journal, &faber.engine, mission)?;
    sync_mission_awake(&faber, &runner.mission.id, runner.mission.state);
    let view = build_mission_view(&faber, &runner.mission, &journal, grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    drive_mission(&app, runner.mission.id.clone(), commands);
    Ok(runner.mission.id)
}

#[tauri::command]
async fn mission_state(faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    build_mission_view(&faber, &mission, &journal, &grant)
}

#[tauri::command]
async fn mission_mode(app: AppHandle, faber: State<'_, Faber>, mission_id: String, mode: RunMode) -> Res<MissionView> {
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    runner.mode(&journal, mode, now).map_err(str::to_string)?;
    let mission = runner.mission;
    sync_mission_awake(&faber, &mission.id, mission.state);
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
}

#[tauri::command]
async fn mission_stop(app: AppHandle, faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    awake::stop_mission(&faber.awake, &mut runner, &journal, &faber.engine).await?;
    let mission = runner.mission;
    sync_mission_awake(&faber, &mission.id, mission.state);
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
}

#[tauri::command]
async fn mission_resume(app: AppHandle, faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    refuse_while_installing()?;
    {
        let (project, _, mission) = find_mission_and_journal(&faber, &mission_id)?;
        project_commands::editable(&mission)?;
        project.allows(Effect::Worktree, mission.grant_revision).map_err(str::to_string)?;
        mission::confirmed(&project).map_err(str::to_string)?;
    }
    ensure_agents(&app, &faber).await?;
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    project.allows(Effect::Worktree, mission.grant_revision).map_err(str::to_string)?;
    let commands = mission::confirmed(&project).map_err(str::to_string)?;
    if faber.engine.is_driving(&mission_id) {
        return Err("reason.mission.driving".into());
    }
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    runner.resume(&journal, &faber.engine)?;
    let mission = runner.mission;
    sync_mission_awake(&faber, &mission.id, mission.state);
    let grant = project.grant.ok_or("reason.grant.none")?;
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    drive_mission(&app, mission_id, commands);
    Ok(view)
}

#[tauri::command]
async fn mission_answer(
    app: AppHandle,
    faber: State<'_, Faber>,
    mission_id: String,
    decision_id: String,
    answer: String,
) -> Res<MissionView> {
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    project_commands::id(&decision_id)?;
    project_commands::text(&answer)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    runner.answer(&journal, &decision_id, &answer, now).map_err(str::to_string)?;
    let mission = runner.mission;
    sync_mission_awake(&faber, &mission.id, mission.state);
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
}

#[cfg(feature = "test-hooks")]
#[tauri::command]
async fn probe_mission_power(
    faber: State<'_, Faber>,
    mission_id: String,
    state: Option<faber_engine::project::MissionState>,
) -> Res<Value> {
    let _turn = faber.missions.lock().await;
    let (_, journal, _) = find_mission_and_journal(&faber, &mission_id)?;
    let mut runner = Runner::load(&journal).map_err(str::to_string)?;
    if let Some(state) = state {
        runner.mission.state = state;
        runner.save(&journal).map_err(str::to_string)?;
    }
    Ok(serde_json::json!({"awake": lock(&faber.awake).is_awake(), "state": runner.mission.state}))
}

#[cfg(feature = "test-hooks")]
#[tauri::command]
async fn probe_mission_attention(
    app: AppHandle,
    faber: State<'_, Faber>,
    mission_id: String,
    focused: bool,
    attention: String,
) -> Res<Value> {
    let _turn = faber.missions.lock().await;
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    let grant = project.grant.ok_or("reason.grant.none")?;
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    tray::probe_mission(&app, &view, focused, &attention)
}

/// Test builds only: the screen's checks (tests/ui/) send their result here. It goes to the file named by
/// `FABER_CHECKS_OUT`, written whole before it appears, and then the app ends.
#[cfg(feature = "test-hooks")]
#[tauri::command]
async fn report_checks(app: AppHandle, json: String) -> Res<()> {
    let out = PathBuf::from(std::env::var_os("FABER_CHECKS_OUT").ok_or("FABER_CHECKS_OUT is not set")?);
    let part = out.with_extension("part");
    std::fs::write(&part, json).and_then(|()| std::fs::rename(&part, &out)).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// Uses setup module to detect CLIs on this computer.
fn detect_clis() -> BTreeMap<Vendor, Cli> {
    setup::detect_clis()
}

/// Under the config folder (Roaming on Windows), never next to a program: the per-user installer's own default folder is
/// `%LOCALAPPDATA%\Faber`, so the data must not be there (`update.rs`).
pub(crate) fn default_data_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("Faber")
}

/// Same folder the Electron versions used, so conversations carry over.
fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    match std::env::var_os("FABER_DATA_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(default_data_dir(&app.path().config_dir()?)),
    }
}

/// Set while Codex is being asked for its limits and models, which starts two Codex processes: one asking at a time is enough.
static LOADING_MODELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Asks Codex for this account's models in the background; the picker fills in when it answers. A call while an asking is
/// under way does nothing: that asking answers for both.
pub(crate) fn load_models(app: &AppHandle, codex: Cli, cwd: PathBuf) {
    use std::sync::atomic::Ordering;
    struct Done;
    impl Drop for Done {
        fn drop(&mut self) {
            LOADING_MODELS.store(false, Ordering::Release);
        }
    }
    if LOADING_MODELS.swap(true, Ordering::AcqRel) {
        return;
    }
    let done = Done;
    let real = real_spawner();
    let prefix = codex.prefix.clone();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _done = done;
        let events = read_codex_limits(&codex.command, &cwd, cfg!(windows), spawn.clone(), Duration::from_secs(20)).await;
        app.state::<Faber>().engine.note_account_events(Vendor::Codex, events);
        let list = list_codex_models(&codex.command, &cwd, cfg!(windows), spawn, Duration::from_secs(20)).await;
        let faber = app.state::<Faber>();
        let recommended = list.iter().find(|m| m.is_default).map(|m| m.id.clone());
        faber.engine.set_codex_models(list.iter().map(|m| m.id.clone()).collect(), recommended);
        *lock(&faber.models) = list;
        let _ = app.emit("faber:event", AppEvent::List);
    });
}

/// What Quit does, and what the idle update install does before the installer ends the process.
pub(crate) fn shut_down(app: &AppHandle) {
    terminal::terminate_hidden();
    let faber = app.state::<Faber>();
    lock(&faber.awake).shutdown();
    // A development build parks its missions at a checkpoint first and records that it closed cleanly.
    if faber.local.is_some() {
        let _ = tauri::async_runtime::block_on(faber.engine.pause_missions_for_update());
    }
    faber.engine.shutdown();
    if let Some(dev) = &faber.local {
        let _ = dev.record_clean_exit();
    }
}

/// Brings the window back, building the screen again if it was closed to the tray.
fn show(app: &AppHandle) {
    if let Some(local) = &app.state::<Faber>().local
        && !local.request_open().unwrap_or(false)
    {
        return;
    }
    let existing = main_window(app);
    let fresh = existing.is_none();
    let window = existing.or_else(|| {
        let faber = app.state::<Faber>();
        create_window(app, faber.hidden, &faber.data_dir).ok()
    });
    // A window not shown yet (built just now, or a second launch before its first paint) shows through `reveal`, which also
    // brings it forward; the page paints its own static frame, so even the failsafe never shows a blank one (R157).
    if (fresh || !app.state::<Faber>().revealed.load(std::sync::atomic::Ordering::SeqCst)) && !app.state::<Faber>().hidden {
        return;
    }
    if let Some(w) = window {
        // Asked for by the person: the first-paint reveal has nothing left to do.
        app.state::<Faber>().revealed.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        app.state::<Faber>().engine.set_attention(true, true);
    }
}

/// Shows the window once, when the screen has painted its first frame or the failsafe says so, so a person never sees an
/// empty one. Automated runs build theirs shown (offscreen) and skip this.
fn reveal(app: &AppHandle) {
    let faber = app.state::<Faber>();
    if faber.revealed.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        faber.engine.set_attention(true, true);
    }
}

/// The screen painted its first frame: the window may show now.
#[tauri::command]
fn screen_ready(app: AppHandle) {
    reveal(&app);
}

/// T9 owns presentation; no screen or worker can fabricate local promotion evidence.
#[tauri::command]
fn local_update_state(faber: State<'_, Faber>) -> Res<Option<faber_engine::project::wire::BuildView>> {
    faber
        .local
        .as_ref()
        .map(|dev| {
            dev.view().map_err(|error| match error {
                faber_engine::project::update::UpdateError::Newer => "error.journal.newer".into(),
                faber_engine::project::update::UpdateError::Store(_) => error.key().into(),
                _ => "error.store.corrupt".into(),
            })
        })
        .unwrap_or(Ok(None))
}

/// The screen's view of the update (`update.rs`): its phase and the download's progress. Changes arrive as `faber:update`.
#[tauri::command]
fn update_status(app: AppHandle) -> update::Status {
    update::status(&app)
}

/// "Update ready, restart": installs the kept update now. Refused while an answer runs.
#[tauri::command]
async fn update_restart(app: AppHandle) -> Res<()> {
    update::restart_now(app).await
}

/// The screen saw the first input of the session (typing, a paste, a drop): from then on the install at open does not end
/// the window under it.
#[tauri::command]
fn input_seen(faber: State<'_, Faber>) {
    faber.touched.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The screen says whether any field holds an unsent draft (text or attachments); read when an install is decided.
#[tauri::command]
fn draft_dirty(faber: State<'_, Faber>, dirty: bool) {
    faber.draft_dirty.store(dirty, std::sync::atomic::Ordering::SeqCst);
}

/// Whether a game or another full-screen app is in front (R191). The screen asks while its window is shown without the
/// focus and goes quiet while the answer is yes. It runs off the main thread: the query switches its thread's DPI mode
/// for a moment. A test build always answers no, so a game on the PC that runs the checks never changes them.
#[tauri::command]
async fn game_in_front() -> bool {
    !cfg!(feature = "test-hooks") && tray::game_in_front()
}

/// The last "what's new" entry the person has seen (`ui/notes.ts`), in its own small file in the data folder. None means
/// no entry was ever shown, which the screen reads as a first install or as an install from before the screen existed. A
/// test build answers "all seen", so no scenario starts behind the screen.
#[tauri::command]
fn notes_seen(faber: State<'_, Faber>) -> Option<u32> {
    let saved = std::fs::read_to_string(faber.data_dir.join("notes-seen")).ok().and_then(|text| text.trim().parse().ok());
    saved.or(cfg!(feature = "test-hooks").then_some(u32::MAX))
}

#[tauri::command]
fn save_notes_seen(faber: State<'_, Faber>, id: u32) -> Res<()> {
    std::fs::write(faber.data_dir.join("notes-seen"), id.to_string()).map_err(|_| "error.settings.saveFailed".to_string())
}

fn create_window(app: &AppHandle, hidden: bool, data_dir: &Path) -> tauri::Result<tauri::WebviewWindow> {
    let handle = app.clone();
    app.state::<Faber>().revealed.store(hidden, std::sync::atomic::Ordering::SeqCst);
    // The first paint already has the theme's own colour.
    let [r, g, b] = look::frame_color(&look::stored(app.state::<Faber>().engine.settings().appearance.as_ref()));
    // A test build started with `FABER_CHECKS_OUT` opens the page that runs the screen's checks and reports them.
    #[cfg(feature = "test-hooks")]
    let url =
        if std::env::var_os("FABER_CHECKS_OUT").is_some() { WebviewUrl::App("index.html?checks".into()) } else { WebviewUrl::default() };
    #[cfg(not(feature = "test-hooks"))]
    let url = WebviewUrl::default();
    let mut builder = WebviewWindowBuilder::new(app, "main", url)
        .title("Faber")
        .inner_size(1280.0, 800.0)
        .min_inner_size(720.0, 480.0)
        .background_color(Color(r, g, b, 0xff))
        // Shown after the screen's first paint (`reveal`), so there is never a blank window.
        .visible(hidden)
        // The "Paste" of the right-click menu reads the clipboard; without this WebView2 asks the person every time.
        .enable_clipboard_access()
        // The page never leaves Faber; a web link that slips through opens in the browser instead.
        .on_navigation(move |url| {
            let inside = url.scheme() == "tauri"
                || url.host_str() == Some("tauri.localhost")
                || (cfg!(debug_assertions) && url.host_str() == Some("localhost"));
            if !inside && matches!(url.scheme(), "http" | "https") {
                let _ = handle.opener().open_url(url.as_str(), None::<&str>);
            }
            inside
        });
    // The top bar of the page is the title bar (ui/WindowControls.tsx); macOS keeps its traffic lights over it.
    #[cfg(target_os = "macos")]
    {
        builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.decorations(false);
    }
    // Automated runs (tests, judges) stay offscreen with their own browser profile, so they never steal the screen
    // or share state between runs.
    if hidden {
        builder = builder.position(-32000.0, -32000.0).skip_taskbar(true).focused(false);
    }
    if hidden || app.state::<Faber>().local.is_some() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    // An automated launch (hidden, or with a DevTools port) also asks the browser engine not to collect or upload crash
    // reports and not to ask SmartScreen about addresses. WebView2 has no documented switch for crash uploads and Tauri
    // does not expose its crash-reporting setting, so the Chromium switches are the best control there is; they limit
    // what a test sends, they are no firewall (see docs/dev/testing.md).
    let port = std::env::var("FABER_CDP_PORT").ok();
    if hidden || port.is_some() {
        let mut args =
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-breakpad --disable-crash-reporter".to_string();
        if let Some(port) = port {
            args.push_str(&format!(" --remote-debugging-port={port}"));
        }
        #[cfg(feature = "test-hooks")]
        if hidden && std::env::var("FABER_SOFTWARE_RENDERING").as_deref() == Ok("1") {
            args.push_str(" --disable-gpu");
        }
        // The sentinel measures frames. A window on a desktop no monitor shows may be counted as hidden, and the engine then
        // slows or stops animation frames; these switches ask it not to. The sentinel's clock check proves they held.
        #[cfg(feature = "test-hooks")]
        if hidden {
            args.push_str(
                " --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling",
            );
        }
        builder = builder.additional_browser_args(&args);
    }
    let window = builder.build()?;
    if !hidden {
        // The failsafe: a screen that never reports its first paint still shows up.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            reveal(&handle);
        });
    }
    Ok(window)
}

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // One qualification probe, run by the check of the agents in a contained child of this program: no window, no app.
    if let [mode, probe, data] = args.as_slice()
        && mode == "--faber-qualify"
    {
        std::process::exit(faber_engine::project::qualify::child(&probe.to_string_lossy(), Path::new(data)));
    }
    if let [mode, data, nonce, output, canary] = args.as_slice()
        && mode == "--faber-local-canary"
    {
        std::process::exit(
            if nonce
                .to_str()
                .zip(canary.to_str())
                .is_some_and(|(nonce, canary)| local_update::probe_canary(Path::new(data), nonce, Path::new(output), canary).is_ok())
            {
                0
            } else {
                1
            },
        );
    }
    if let [mode, data, output] = args.as_slice()
        && mode == "--faber-local-check-data"
    {
        std::process::exit(if local_update::probe_saved_data(std::path::Path::new(data), std::path::Path::new(output)).is_ok() {
            0
        } else {
            1
        });
    }
    if let [mode, output] = args.as_slice()
        && mode == "--faber-local-identity"
    {
        std::process::exit(if local_update::write_identity(std::path::Path::new(output)).is_ok() { 0 } else { 1 });
    }
    if args.iter().any(|arg| arg.to_str().is_some_and(|arg| arg.starts_with("--faber-local-")))
        && args.as_slice() != ["--faber-local-start"]
    {
        std::process::exit(1);
    }
    let autostart_launch = !creates_window(std::env::args());
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
    let local_start = std::env::args_os().any(|arg| arg == "--faber-local-start");
    let local = if std::env::var("FABER_CHANNEL").as_deref() == Ok("development") {
        match Development::from_environment() {
            Ok(local) => Some(local),
            Err(_) => std::process::exit(1),
        }
    } else if autostart_launch {
        match std::env::current_exe().ok().and_then(|exe| Development::from_slot(&exe).ok()) {
            Some(local) => local,
            None => std::process::exit(1),
        }
    } else {
        None
    };
    if autostart_launch && let Some(dev) = &local {
        let result = std::env::current_exe().ok().and_then(|exe| autostart::launch_command(&exe, Some(dev)).ok());
        std::process::exit(if result.is_some_and(|command| command.spawn().is_ok()) { 0 } else { 1 });
    }
    if local_start && local.is_none() {
        std::process::exit(1);
    }
    #[cfg(feature = "test-hooks")]
    if local_start && let Some(dev) = &local {
        let marker = dev.data().join("test-crash-next");
        if marker.exists() {
            if std::fs::remove_file(marker).is_err() {
                std::process::exit(1);
            }
            std::process::exit(23);
        }
    }
    let _slot_lease = if let Some(dev) = &local {
        match dev.startup_lease(local_start) {
            Ok(lease) => Some(lease),
            Err(_) => std::process::exit(1),
        }
    } else {
        None
    };
    let _data_lease = match local.as_ref().map(Development::data_lease).transpose() {
        Ok(lease) => lease,
        Err(_) => std::process::exit(1),
    };
    if local_start && let Some(dev) = &local {
        // Tao creates a visible event-target HWND even without a WebView; defer the entire GUI runtime until asked.
        match headless_start(dev) {
            Ok(true) => {}
            Ok(false) => return,
            Err(_) => std::process::exit(1),
        }
    }
    let mut builder = tauri::Builder::default();
    // A second launch brings the open window forward. Runs with their own data folder (tests) stay separate.
    if std::env::var_os("FABER_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _| {
            if creates_window(args) {
                show(app);
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            state,
            open,
            send,
            branch,
            warm,
            set_clock,
            quota_attention,
            answer_plan,
            approve_credits,
            move_turn,
            wait_turn,
            route_draft,
            interrupt,
            answer,
            answer_question,
            rename,
            remove,
            remove_queued,
            send_queued,
            remove_project,
            preview_look,
            save_settings,
            save_theme,
            open_theme,
            add_project,
            recents,
            open_folders,
            forget_recent,
            save_environment,
            forget_environment,
            start_environment,
            chat_folder,
            save_file,
            show_file,
            show_project,
            read_image,
            open_terminal,
            open_link,
            set_shell_texts,
            project_enroll,
            project_grant,
            project_revoke,
            project_proof,
            project_proof_confirm,
            mission_start,
            mission_state,
            mission_mode,
            mission_stop,
            mission_resume,
            mission_answer,
            local_update_state,
            update_status,
            screen_ready,
            update_restart,
            input_seen,
            draft_dirty,
            game_in_front,
            notes_seen,
            save_notes_seen,
            setup::setup_check,
            setup::setup_install,
            setup::setup_sign_in,
            #[cfg(feature = "test-hooks")]
            probe_mission_attention,
            #[cfg(feature = "test-hooks")]
            probe_mission_power,
            #[cfg(feature = "test-hooks")]
            report_checks
        ])
        .setup(move |app| {
            let data_dir = local.as_ref().map(|dev| Ok(dev.data())).unwrap_or_else(|| data_dir(app.handle()))?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(tray::Tray::default());
            let clis = detect_clis();
            let handle = app.handle().clone();
            let is_test = cfg!(feature = "test-hooks") || hidden || std::env::var_os("FABER_DATA_DIR").is_some();
            let power: Box<dyn PowerKeepAwake> = if is_test {
                Box::new(NoopPower)
            } else {
                #[cfg(windows)]
                {
                    Box::new(WindowsPower)
                }
                #[cfg(not(windows))]
                {
                    Box::new(NoopPower)
                }
            };
            let autostart_keeper: Arc<dyn AutostartKeeper> = if is_test {
                Arc::new(FakeAutostart::new(false))
            } else {
                #[cfg(windows)]
                {
                    Arc::new(WindowsAutostart::new(local.clone()))
                }
                #[cfg(not(windows))]
                {
                    Arc::new(NoopAutostart)
                }
            };
            let keep_awake = faber_engine::store::Store::new(&data_dir).settings().keep_awake();
            let awake = Arc::new(Mutex::new(CountedAwake::new(power, keep_awake)?));
            let update_requests = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let mission_updates = update_requests.clone();
            let mission_power = awake.clone();
            let mission_events = faber_engine::project::store::observe_missions(
                &data_dir,
                Arc::new(move |mission| {
                    lock(&mission_power).set_mission_awake(
                        &mission.id,
                        matches!(
                            mission.state,
                            faber_engine::project::MissionState::Running | faber_engine::project::MissionState::Planning
                        ),
                    );
                    mission_updates.store(true, std::sync::atomic::Ordering::Release);
                }),
            )
            .map_err(|e| std::io::Error::other(e.key()))?;
            let awake_for_emit = awake.clone();
            let chat_updates = update_requests.clone();
            let emit = Arc::new(move |e: AppEvent| {
                // The engine calls this with its lock held, and the tray waits for the main thread, which can be waiting
                // for that lock. So the tray works on its own thread; it keeps the newest list, whatever the order.
                if let AppEvent::Running { revision, items } = &e {
                    chat_updates.store(true, std::sync::atomic::Ordering::Release);
                    let active_turns: HashSet<String> = items.iter().map(|i| i.turn_id.clone()).collect();
                    lock(&awake_for_emit).sync_runs(active_turns);
                    if !hidden {
                        let (app, revision, items) = (handle.clone(), *revision, items.clone());
                        tauri::async_runtime::spawn_blocking(move || tray::on_running(&app, revision, &items));
                    }
                }
                let _ = handle.emit("faber:event", e);
            });
            let opts = EngineOptions {
                data_dir: data_dir.clone(),
                clis: clis.clone(),
                spawn: real_spawner(),
                windows: cfg!(windows),
                claude_env_billed: faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)),
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tauri::async_runtime::handle().inner().clone(),
                ask: None,
            };
            let engine = Engine::new(opts, emit);
            if let Some(dev) = &local {
                engine.set_local_update(dev.manager().map_err(|e| std::io::Error::other(e.key()))?.layout().clone());
            }
            let missions = engine.mission_lock();
            app.manage(Faber {
                engine,
                models: Mutex::default(),
                update: Mutex::default(),
                stage: Mutex::default(),
                revealed: std::sync::atomic::AtomicBool::new(false),
                touched: std::sync::atomic::AtomicBool::new(false),
                draft_dirty: std::sync::atomic::AtomicBool::new(false),
                data_dir: data_dir.clone(),
                hidden,
                missions,
                local: local.clone(),
                awake,
                qualifying: Default::default(),
                _mission_events: mission_events,
                autostart: Mutex::new(autostart_keeper),
            });
            // A mission the last run left running has no driver now: it waits, paused, for the owner's Resume.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = restore_mission_notices(&handle) {
                    eprintln!("faber: interrupted missions could not restore ({error})");
                }
            });
            app.state::<Faber>().engine.watch_accounts();
            app.state::<Faber>().engine.detect_claude_flags();
            if let Some(codex) = clis.get(&Vendor::Codex) {
                load_models(app.handle(), codex.clone(), data_dir.clone());
            }
            if !autostart_launch {
                create_window(app.handle(), hidden, &data_dir)?;
            }
            if !hidden {
                tray::create(app)?;
            }
            #[cfg(feature = "test-hooks")]
            if autostart_launch && let Some(out) = std::env::var_os("FABER_CHECKS_OUT") {
                std::fs::write(out, serde_json::json!({ "windowCount": app.webview_windows().len() }).to_string())?;
            }
            // Only installed builds update themselves; source runs and test runs never do.
            if !cfg!(debug_assertions)
                && local_update::release_updates(
                    std::env::var("FABER_CHANNEL").as_deref().unwrap_or("release"),
                    hidden,
                    std::env::var_os("FABER_DATA_DIR").is_some(),
                )
            {
                update::keep_updated(app.handle());
            }
            if let Some(dev) = &local {
                let (dev, handle) = (dev.clone(), app.handle().clone());
                tauri::async_runtime::spawn(async move {
                    loop {
                        let engine = handle.state::<Faber>().engine.clone();
                        if update_requests.swap(false, std::sync::atomic::Ordering::AcqRel) {
                            let awake = handle.state::<Faber>().awake.clone();
                            if matches!(
                                service_local_landing(dev.clone(), engine.clone(), awake, main_window(&handle).is_some()).await,
                                Err(faber_engine::project::update::UpdateError::ActiveOperations)
                            ) {
                                update_requests.store(true, std::sync::atomic::Ordering::Release);
                            }
                        }
                        if dev.service_quiescence(&engine).await.is_ok()
                            && std::env::current_exe().is_ok_and(|exe| dev.leaving_slot(&exe).unwrap_or(false))
                            && dev.ensure_missions_quiescent().is_ok()
                        {
                            handle.exit(0);
                            return;
                        }
                        if dev.open_pending().unwrap_or(false) && dev.resume_allowed().is_ok() {
                            let handle = handle.clone();
                            let _ = handle.clone().run_on_main_thread(move || show(&handle));
                        }
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Faber failed to start")
        .run(move |app, event| {
            let _keep_roots_owned = (&_slot_lease, &_data_lease);
            // Closing the window frees the screen (about 90 MB) but keeps Faber in the tray, so running answers
            // finish; only Quit, or a test run, ends it.
            if let RunEvent::ExitRequested { code: None, api, .. } = &event
                && (!hidden || autostart_launch)
            {
                api.prevent_exit();
                // The screen is gone: the quota is read at the slower pace until the window comes back.
                app.state::<Faber>().engine.set_attention(false, false);
            }
            if let RunEvent::Exit = event {
                shut_down(app);
                let faber = app.state::<Faber>();
                // On Windows this starts the installer, which replaces Faber and needs it closed.
                if faber.local.is_none()
                    && let Some((update, bytes)) = lock(&faber.update).take()
                {
                    let _ = update.install(bytes);
                }
            }
        });
}

async fn service_local_landing(
    dev: Development,
    engine: Engine,
    awake: Arc<Mutex<CountedAwake>>,
    active_window: bool,
) -> Result<bool, faber_engine::project::update::UpdateError> {
    let _turn = engine.mission_lock().lock_owned().await;
    tauri::async_runtime::spawn_blocking(move || {
        let mut git = Process::find().map_err(|_| faber_engine::project::update::UpdateError::InvalidState("git unavailable"))?;
        let data = dev.data();
        dev.service_landed_missions(&engine, active_window, &mut git, |mission, repo| {
            Ok(local_update::ProductionRunner::for_mission(repo, &data, mission)?.with_awake(awake.clone()))
        })
    })
    .await
    .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("local build worker failed"))?
}

fn headless_start(dev: &Development) -> Result<bool, faber_engine::project::update::UpdateError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("headless runtime failed"))?;
    let _entered = runtime.enter();
    local_update::validate_saved_data(&dev.data())?;
    let power: Box<dyn PowerKeepAwake> = {
        #[cfg(windows)]
        {
            Box::new(WindowsPower)
        }
        #[cfg(not(windows))]
        {
            Box::new(NoopPower)
        }
    };
    let keep_awake = faber_engine::store::Store::new(&dev.data()).settings().keep_awake();
    let awake = Arc::new(Mutex::new(
        CountedAwake::new(power, keep_awake)
            .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("power owner unavailable"))?,
    ));
    let update_requests = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mission_power = awake.clone();
    let mission_updates = update_requests.clone();
    let _events = faber_engine::project::store::observe_missions(
        &dev.data(),
        Arc::new(move |mission| {
            lock(&mission_power).set_mission_awake(
                &mission.id,
                matches!(mission.state, faber_engine::project::MissionState::Running | faber_engine::project::MissionState::Planning),
            );
            mission_updates.store(true, std::sync::atomic::Ordering::Release);
        }),
    )
    .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("mission observer unavailable"))?;
    let _ = dev.cleanup_obsolete();
    let engine = Engine::new(
        EngineOptions {
            data_dir: dev.data(),
            clis: detect_clis(),
            spawn: real_spawner(),
            windows: cfg!(windows),
            claude_env_billed: false,
            max_warm: 0,
            idle: Duration::from_secs(120),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    engine.set_local_update(dev.manager()?.layout().clone());
    dev.startup_ready(
        &std::env::current_exe().map_err(|_| faber_engine::project::update::UpdateError::InvalidState("startup executable missing"))?,
    )?;
    let result = runtime.block_on(async {
        engine.pause_missions_for_update().await.map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?;
        loop {
            if update_requests.swap(false, std::sync::atomic::Ordering::AcqRel)
                && matches!(
                    service_local_landing(dev.clone(), engine.clone(), awake.clone(), false).await,
                    Err(faber_engine::project::update::UpdateError::ActiveOperations)
                )
            {
                update_requests.store(true, std::sync::atomic::Ordering::Release);
            }
            dev.service_quiescence(&engine).await?;
            #[cfg(feature = "test-hooks")]
            if dev.data().join("test-mission-admission").exists() {
                std::fs::remove_file(dev.data().join("test-mission-admission"))
                    .map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?;
                let project = engine
                    .projects_checked()
                    .map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?
                    .into_iter()
                    .next()
                    .ok_or(faber_engine::project::update::UpdateError::ActiveOperations)?;
                let j = Journal::open(Path::new(&project.path), &dev.data(), "m1")
                    .map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?;
                let mission = Runner::load(&j).map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?.mission;
                let result = Runner::start(&j, &engine, mission).map(|_| "unexpected admission".into()).unwrap_or_else(|key| key);
                std::fs::write(dev.data().join("test-mission-admission-result"), result)
                    .map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?;
            }
            if dev.leaving_slot(&std::env::current_exe().map_err(|_| faber_engine::project::update::UpdateError::ActiveOperations)?)?
                && dev.ensure_missions_quiescent().is_ok()
            {
                return Ok(false);
            }
            #[cfg(feature = "test-hooks")]
            if dev.data().join("test-clean-exit-next").exists() {
                std::fs::remove_file(dev.data().join("test-clean-exit-next"))
                    .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("test exit marker failed"))?;
                dev.record_clean_exit()?;
                return Ok(false);
            }
            if dev.open_pending()? && dev.resume_allowed().is_ok() && dev.request_open()? {
                return Ok(true);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    engine.shutdown();
    lock(&awake).shutdown();
    result
}
