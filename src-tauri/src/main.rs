//! The desktop shell: one window, a tray icon, and the commands the screen may call. The engine does the work.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The test hooks (`report_checks`, the checks page) never ship: a release build cannot be made with them.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

mod autostart;
mod awake;
mod look;
mod setup;
mod terminal;
mod tray;

#[cfg(not(windows))]
use autostart::NoopAutostart;
#[cfg(windows)]
use autostart::WindowsAutostart;
use autostart::{AutostartKeeper, FakeAutostart, creates_window};
#[cfg(windows)]
use awake::WindowsPower;
use awake::{CountedAwake, NoopPower, PowerKeepAwake};
use base64::Engine as _;
use faber_engine::cli::Cli;
use faber_engine::codex::{ModelInfo, list_codex_models, read_codex_limits};
use faber_engine::model::{Conversation, Project, Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
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

#[tauri::command]
async fn chat_folder(app: AppHandle, faber: State<'_, Faber>, id: Option<String>, scratch: bool) -> Res<Option<Conversation>> {
    if scratch {
        return faber.engine.chat_folder(id.as_deref(), None).map(Some);
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
        builder = builder.position(-32000.0, -32000.0).skip_taskbar(true).focused(false).data_directory(data_dir.join("webview"));
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
        builder = builder.additional_browser_args(&args);
    }
    builder.build()
}

fn main() {
    let autostart_launch = !creates_window(std::env::args());
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
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
            remove_project,
            preview_look,
            save_settings,
            save_theme,
            open_theme,
            add_project,
            chat_folder,
            save_file,
            show_file,
            read_image,
            open_terminal,
            open_link,
            set_shell_texts,
            setup::setup_check,
            setup::setup_install,
            setup::setup_sign_in,
            #[cfg(feature = "test-hooks")]
            report_checks
        ])
        .setup(move |app| {
            let data_dir = data_dir(app.handle())?;
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
                    Arc::new(WindowsAutostart::new())
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
            if !hidden && !cfg!(debug_assertions) && std::env::var_os("FABER_DATA_DIR").is_none() {
                fetch_update(app.handle());
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Faber failed to start")
        .run(move |app, event| {
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
                // On Windows this starts the installer, which replaces Faber and needs it closed.
                if let Some((update, bytes)) = lock(&faber.update).take() {
                    let _ = update.install(bytes);
                }
            }
        });
}
