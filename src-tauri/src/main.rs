//! The desktop shell: one window, a tray icon, and the commands the screen may call. The engine does the work.

#![cfg_attr(windows, windows_subsystem = "windows")]

// The test hooks (`report_checks`, the checks page) never ship: a release build cannot be made with them.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

mod awake;
mod look;
mod project_commands;
mod setup;
mod terminal;
mod tray;

#[cfg(windows)]
use awake::WindowsPower;
use awake::{CountedAwake, NoopPower, PowerKeepAwake};
use base64::Engine as _;
#[cfg(not(windows))]
use faber::autostart::NoopAutostart;
#[cfg(windows)]
use faber::autostart::WindowsAutostart;
use faber::autostart::{self, AutostartKeeper, FakeAutostart, creates_window};
use faber::local_update::{self, Development};
use faber_engine::cli::Cli;
use faber_engine::codex::{ModelInfo, list_codex_models, read_codex_limits};
use faber_engine::model::{Area, Conversation, Project, Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::project::git::{Process, enroll};
use faber_engine::project::store::Journal;
use faber_engine::project::wire::{DecisionView, EnrollView, MissionView, TaskView};
use faber_engine::project::{DecidedBy, Effect, GitMode, Grant, Mission, MissionState, RunMode};
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
use tauri_plugin_updater::{Update, UpdaterExt};

pub(crate) struct Faber {
    pub(crate) engine: Engine,
    /// ChatGPT models this account can use; empty until the Codex CLI answers.
    pub(crate) models: Mutex<Vec<ModelInfo>>,
    /// A newer version, already downloaded; it installs when Faber quits.
    pub(crate) update: Mutex<Option<(Update, Vec<u8>)>>,
    pub(crate) data_dir: PathBuf,
    pub(crate) hidden: bool,
    /// Serialize journal snapshots and mutations, so an answer cannot overwrite a newer view.
    missions: Mutex<()>,
    local: Option<Development>,
    pub(crate) awake: Arc<Mutex<CountedAwake>>,
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

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn main_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}

// The screen calls the engine through this fixed list of commands; nothing else crosses over.

fn refuse_direct_project(faber: &Faber, area: Area, project: Option<&str>, conversation: Option<&str>) -> Res<()> {
    if area == Area::Project
        || project.is_some()
        || conversation.and_then(|id| faber.engine.open(id)).is_some_and(|c| c.area == Area::Project)
    {
        return Err("error.project.missionRequired".into());
    }
    Ok(())
}

#[tauri::command]
async fn state(app: AppHandle, faber: State<'_, Faber>) -> Res<FullState> {
    let mut state = faber.engine.state();
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
    refuse_direct_project(&faber, req.area, req.project_id.as_deref(), req.conversation_id.as_deref())?;
    faber.engine.send(req)
}

#[tauri::command]
async fn branch(faber: State<'_, Faber>, req: BranchRequest) -> Res<String> {
    refuse_direct_project(&faber, Area::Chat, None, Some(&req.conversation_id))?;
    faber.engine.branch(req)
}

#[tauri::command]
fn set_clock(faber: State<'_, Faber>, utc_offset: i64) {
    faber.engine.set_utc_offset(utc_offset);
}

#[tauri::command]
fn answer_plan(faber: State<'_, Faber>, vendor: Vendor, plan: String) {
    faber.engine.answer_plan(vendor, &plan);
}
#[tauri::command]
fn approve_credits(faber: State<'_, Faber>, id: String, turn_id: String, vendor: Vendor) -> Res<()> {
    refuse_direct_project(&faber, Area::Chat, None, Some(&id))?;
    faber.engine.approve_credits(&id, &turn_id, vendor)
}
#[tauri::command]
fn move_turn(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    refuse_direct_project(&faber, Area::Chat, None, Some(&id))?;
    faber.engine.move_turn(&id, &turn_id)
}
#[tauri::command]
fn wait_turn(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    refuse_direct_project(&faber, Area::Chat, None, Some(&id))?;
    faber.engine.wait_turn(&id, &turn_id)
}

#[tauri::command]
async fn warm(faber: State<'_, Faber>, req: WarmRequest) -> Res<()> {
    refuse_direct_project(&faber, req.area, req.project_id.as_deref(), req.conversation_id.as_deref())?;
    faber.engine.warm(req);
    Ok(())
}

#[tauri::command]
async fn route_draft(faber: State<'_, Faber>, req: WarmRequest, text: String) -> Res<()> {
    refuse_direct_project(&faber, req.area, req.project_id.as_deref(), req.conversation_id.as_deref())?;
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
    faber.engine.remove_project(&id);
    Ok(())
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
    let mut settings = autostart::save(autostart.as_ref(), patch.autostart, || faber.engine.save_settings(patch))?;
    lock(&faber.awake).set_enabled(settings.keep_awake.unwrap_or(true));
    settings.appearance = look::usable(settings.appearance.take());
    let look = look::stored(settings.appearance.as_ref());
    Ok(SavedSettings { settings, look })
}

#[tauri::command]
async fn save_theme(app: AppHandle, appearance: Map<String, Value>, name: String) -> Result<bool, Failure> {
    let json = look::write_theme_file(&appearance, &name)?;
    let mut dialog = app.dialog().file().add_filter("JSON", &["json"]).set_file_name(format!("my-theme{}", look::theme_extension()));
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
    let path = picked.into_path().map_err(|e| e.to_string())?;
    Ok(Some(faber.engine.add_project(&path.to_string_lossy())))
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
    std::fs::copy(&source, target.into_path().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
async fn show_file(app: AppHandle, faber: State<'_, Faber>, id: String, path: String) -> Res<()> {
    let Some(source) = faber.engine.workspace_file(&id, &path) else { return Ok(()) };
    app.opener().reveal_item_in_dir(source).map_err(|e| e.to_string())
}

/// An image from a chat workspace as a data URL, so the page never reads the disk itself.
#[tauri::command]
async fn read_image(faber: State<'_, Faber>, id: String, path: String) -> Res<Option<String>> {
    let Some(source) = faber.engine.workspace_file(&id, &path) else { return Ok(None) };
    let ext = source.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let Some((_, kind)) = IMAGE_TYPES.iter().find(|(e, _)| Some(*e) == ext.as_deref()) else { return Ok(None) };
    if std::fs::metadata(&source).map_err(|e| e.to_string())?.len() > 8_000_000 {
        return Ok(None);
    }
    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
    Ok(Some(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}

#[tauri::command]
async fn open_terminal(faber: State<'_, Faber>, id: String) -> Res<bool> {
    let Some(cmd) = faber.engine.terminal(&id) else { return Ok(false) };
    terminal::open(&cmd).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Links in answers open in the user's browser, never inside Faber.
#[tauri::command]
async fn open_link(app: AppHandle, url: String) -> Res<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("only web links open".into());
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
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
        state: if mission.state == MissionState::Running { MissionState::Blocked } else { mission.state },
        mode: mission.mode,
        git_mode: grant.mode,
        goal: mission.goal.clone(),
        plan: None,
        tasks: task_views,
        cost,
        pace,
        decisions: decision_views,
        build: None,
        reason: if mission.state == MissionState::Running {
            Some("reason.mission.runnerUnavailable".into())
        } else {
            mission.reason.clone()
        },
    })
}

fn find_mission_and_journal(faber: &Faber, mission_id: &str) -> Res<(Project, Journal, Mission)> {
    project_commands::id(mission_id)?;
    let projects = faber.engine.projects_checked()?;
    for p in projects {
        let record = Path::new(&p.path).join(".faber/missions").join(mission_id).join("mission.json");
        if !record.is_file() {
            continue;
        }
        let journal = Journal::open(Path::new(&p.path), &faber.data_dir, mission_id).map_err(|e| e.key().to_string())?;
        let mission = journal.mission().map_err(|e| e.key().to_string())?.ok_or("error.journal.mismatch")?;
        if mission.id != mission_id || mission.project_id != p.id || p.repo.as_ref().is_none_or(|r| r.common_dir != mission.repo) {
            return Err("error.journal.mismatch".into());
        }
        return Ok((p, journal, mission));
    }
    Err("error.journal.mismatch".into())
}

fn restore_mission_notices(app: &AppHandle) -> Res<()> {
    let faber = app.state::<Faber>();
    let _turn = lock(&faber.missions);
    for project in faber.engine.projects_checked()? {
        let entries = match std::fs::read_dir(Path::new(&project.path).join(".faber/missions")) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("error.store.read".into()),
        };
        for entry in entries {
            let entry = entry.map_err(|_| "error.store.read")?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if !faber_engine::project::valid_id(&id) {
                continue;
            }
            let (owner, journal, mission) = find_mission_and_journal(&faber, &id)?;
            let grant = owner.grant.ok_or("reason.grant.none")?;
            let view = build_mission_view(&faber, &mission, &journal, &grant)?;
            tray::on_mission(app, &view);
        }
    }
    Ok(())
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
    project.accept_grant(grant.clone()).map_err(str::to_string)?;
    faber.engine.update_project(&before, &project)?;
    Ok(grant)
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

#[tauri::command]
async fn mission_start(app: AppHandle, faber: State<'_, Faber>, project_id: String, goal: String) -> Res<String> {
    project_commands::id(&project_id)?;
    let project = faber.engine.project(&project_id)?;
    let grant = project.grant.as_ref().ok_or("reason.grant.none")?;
    project.allows(Effect::Worktree, grant.revision).map_err(|k| k.to_string())?;
    project_commands::text(&goal)?;
    let _ = app;
    // T5 is absent on this branch. A journal state is not proof that a worker started.
    Err("reason.mission.runnerUnavailable".into())
}

#[tauri::command]
async fn mission_state(faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    let _turn = lock(&faber.missions);
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    build_mission_view(&faber, &mission, &journal, &grant)
}

#[tauri::command]
async fn mission_mode(app: AppHandle, faber: State<'_, Faber>, mission_id: String, mode: RunMode) -> Res<MissionView> {
    let _turn = lock(&faber.missions);
    let (project, journal, mut mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    mission.mode = mode;
    project_commands::changed(&mut mission, now)?;
    journal.save_mission(&mission).map_err(|e| e.key().to_string())?;
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
}

#[tauri::command]
async fn mission_stop(app: AppHandle, faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    let _turn = lock(&faber.missions);
    let (project, journal, mut mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    mission.state = MissionState::Paused;
    mission.reason = Some("reason.interrupted".into());
    project_commands::changed(&mut mission, now)?;
    journal.save_mission(&mission).map_err(|e| e.key().to_string())?;
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
}

#[tauri::command]
async fn mission_resume(app: AppHandle, faber: State<'_, Faber>, mission_id: String) -> Res<MissionView> {
    let _turn = lock(&faber.missions);
    let (project, journal, mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    project.allows(Effect::Worktree, mission.grant_revision).map_err(str::to_string)?;
    let _ = (app, journal);
    Err("reason.mission.runnerUnavailable".into())
}

#[tauri::command]
async fn mission_answer(
    app: AppHandle,
    faber: State<'_, Faber>,
    mission_id: String,
    decision_id: String,
    answer: String,
) -> Res<MissionView> {
    let _turn = lock(&faber.missions);
    let (project, journal, mut mission) = find_mission_and_journal(&faber, &mission_id)?;
    project_commands::editable(&mission)?;
    project_commands::id(&decision_id)?;
    project_commands::text(&answer)?;
    let mut decision = journal.decision(&decision_id).map_err(|e| e.key().to_string())?.ok_or("error.decision.option")?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    decision.answer(DecidedBy::Person, &answer, now).map_err(|k| k.to_string())?;
    journal.save_decision(&decision).map_err(|e| e.key().to_string())?;
    project_commands::changed(&mut mission, now)?;
    journal.save_mission(&mission).map_err(|e| e.key().to_string())?;
    let grant = project.grant.unwrap_or_else(|| Grant::new(&mission.repo, GitMode::Seguro, mission.grant_revision, mission.created_at));
    let view = build_mission_view(&faber, &mission, &journal, &grant)?;
    let _ = app.emit("mission", &view);
    tray::on_mission(&app, &view);
    Ok(view)
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
    let _turn = lock(&faber.missions);
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

/// Same folder the Electron versions used, so conversations carry over.
fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    match std::env::var_os("FABER_DATA_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(app.path().config_dir()?.join("Faber")),
    }
}

/// Asks Codex for this account's models in the background; the picker fills in when it answers.
pub(crate) fn load_models(app: &AppHandle, codex: Cli, cwd: PathBuf) {
    let real = real_spawner();
    let prefix = codex.prefix.clone();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
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

/// Looks for a newer release once per start and downloads it in the background, signed with Faber's own key.
fn fetch_update(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Ok(updater) = app.updater() else { return };
        let Ok(Some(update)) = updater.check().await else { return };
        if let Ok(bytes) = update.download(|_, _| {}, || {}).await {
            *lock(&app.state::<Faber>().update) = Some((update, bytes));
        }
    });
}

/// Brings the window back, building the screen again if it was closed to the tray.
fn show(app: &AppHandle) {
    if let Some(local) = &app.state::<Faber>().local
        && !local.request_open().unwrap_or(false)
    {
        return;
    }
    let window = main_window(app).or_else(|| {
        let faber = app.state::<Faber>();
        create_window(app, faber.hidden, &faber.data_dir).ok()
    });
    if let Some(w) = window {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
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

fn create_window(app: &AppHandle, hidden: bool, data_dir: &Path) -> tauri::Result<tauri::WebviewWindow> {
    let handle = app.clone();
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
    // what a test sends, they are no firewall (see docs/design.md).
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
    builder.build()
}

fn main() {
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
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
            answer_plan,
            approve_credits,
            move_turn,
            wait_turn,
            route_draft,
            interrupt,
            answer,
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
            save_file,
            show_file,
            read_image,
            open_terminal,
            open_link,
            set_shell_texts,
            project_enroll,
            project_grant,
            project_revoke,
            mission_start,
            mission_state,
            mission_mode,
            mission_stop,
            mission_resume,
            mission_answer,
            local_update_state,
            setup::setup_check,
            setup::setup_install,
            setup::setup_sign_in,
            #[cfg(feature = "test-hooks")]
            probe_mission_attention,
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
            let keep_awake = faber_engine::store::Store::new(&data_dir).settings().keep_awake.unwrap_or(true);
            let awake = Arc::new(Mutex::new(CountedAwake::new(power, keep_awake)?));
            let awake_for_emit = awake.clone();
            let emit = Arc::new(move |e: AppEvent| {
                // The engine calls this with its lock held, and the tray waits for the main thread, which can be waiting
                // for that lock. So the tray works on its own thread; it keeps the newest list, whatever the order.
                if let AppEvent::Running { revision, items } = &e {
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
            app.manage(Faber {
                engine: Engine::new(opts, emit),
                models: Mutex::default(),
                update: Mutex::default(),
                data_dir: data_dir.clone(),
                hidden,
                missions: Mutex::new(()),
                local: local.clone(),
                awake,
                autostart: Mutex::new(autostart_keeper),
            });
            app.state::<Faber>().engine.watch_accounts();
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
                fetch_update(app.handle());
            }
            if let Some(dev) = &local {
                let (dev, handle) = (dev.clone(), app.handle().clone());
                tauri::async_runtime::spawn(async move {
                    loop {
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
            }
            if let RunEvent::Exit = event {
                terminal::terminate_hidden();
                let faber = app.state::<Faber>();
                lock(&faber.awake).shutdown();
                faber.engine.shutdown();
                if let Some(dev) = &faber.local {
                    let _ = dev.record_clean_exit();
                }
                // On Windows this starts the installer, which replaces Faber and needs it closed.
                if faber.local.is_none()
                    && let Some((update, bytes)) = lock(&faber.update).take()
                {
                    let _ = update.install(bytes);
                }
            }
        });
}

fn headless_start(dev: &Development) -> Result<bool, faber_engine::project::update::UpdateError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|_| faber_engine::project::update::UpdateError::InvalidState("headless runtime failed"))?;
    let _entered = runtime.enter();
    local_update::validate_saved_data(&dev.data())?;
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
    dev.startup_ready(
        &std::env::current_exe().map_err(|_| faber_engine::project::update::UpdateError::InvalidState("startup executable missing"))?,
    )?;
    let result = runtime.block_on(async {
        loop {
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
    result
}
