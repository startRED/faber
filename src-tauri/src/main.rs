//! The desktop shell: one window, a tray icon, and the commands the screen may call. The engine does the work.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod terminal;

use base64::Engine as _;
use faber_engine::cli::{Cli, find_cli};
use faber_engine::codex::{ModelInfo, list_codex_models};
use faber_engine::model::{Conversation, Project, Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::{AppEvent, AppState, BranchRequest, Engine, EngineOptions, SendRequest, WarmRequest};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Wry};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

struct Faber {
    engine: Engine,
    /// ChatGPT models this account can use; empty until the Codex CLI answers.
    models: Mutex<Vec<ModelInfo>>,
    /// The tray menu's two items, renamed when the screen changes language.
    tray: Mutex<Option<[MenuItem<Wry>; 2]>>,
    /// A newer version, already downloaded; it installs when Faber quits.
    update: Mutex<Option<(Update, Vec<u8>)>>,
    data_dir: PathBuf,
    hidden: bool,
}

type Res<T> = Result<T, String>;

#[derive(Serialize)]
struct FullState {
    #[serde(flatten)]
    state: AppState,
    models: Vec<ModelInfo>,
    version: String,
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
    Ok(FullState { state: faber.engine.state(), models: lock(&faber.models).clone(), version: app.package_info().version.to_string() })
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
async fn warm(faber: State<'_, Faber>, req: WarmRequest) -> Res<()> {
    faber.engine.warm(req);
    Ok(())
}

#[tauri::command]
async fn interrupt(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.interrupt(&id);
    Ok(())
}

#[tauri::command]
async fn answer(faber: State<'_, Faber>, id: String, request_id: String, allow: bool, always: bool) -> Res<()> {
    faber.engine.answer(&id, &request_id, allow, always);
    Ok(())
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

#[tauri::command]
async fn save_settings(faber: State<'_, Faber>, patch: Settings) -> Res<Settings> {
    Ok(faber.engine.save_settings(patch))
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

#[tauri::command]
async fn set_tray(faber: State<'_, Faber>, open: String, quit: String) -> Res<()> {
    if let Some([o, q]) = &*lock(&faber.tray) {
        o.set_text(open).map_err(|e| e.to_string())?;
        q.set_text(quit).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Each CLI is found on PATH. FABER_CLI_<NAME> (a JSON Cli, or empty for none) replaces it, so end-to-end tests
/// can replay a recorded session through a fake CLI instead of a real subscription.
fn detect_clis() -> BTreeMap<Vendor, Cli> {
    Vendor::ALL
        .into_iter()
        .filter_map(|v| {
            let cli = match std::env::var(format!("FABER_CLI_{}", v.name().to_uppercase())) {
                Ok(json) => serde_json::from_str(&json).ok(),
                Err(_) => find_cli(v.name()),
            };
            cli.map(|c| (v, c))
        })
        .collect()
}

/// Same folder the Electron versions used, so conversations carry over.
fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    match std::env::var_os("FABER_DATA_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(app.path().config_dir()?.join("Faber")),
    }
}

/// Asks Codex for this account's models in the background; the picker fills in when it answers.
fn load_models(app: &AppHandle, codex: Cli, cwd: PathBuf) {
    let real = real_spawner();
    let prefix = codex.prefix.clone();
    let spawn: Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let list = list_codex_models(&codex.command, &cwd, cfg!(windows), spawn, Duration::from_secs(20)).await;
        *lock(&app.state::<Faber>().models) = list;
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
    let mut builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
        .title("Faber")
        .inner_size(1280.0, 800.0)
        .min_inner_size(720.0, 480.0)
        .background_color(Color(0x15, 0x17, 0x1b, 0xff))
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
    // Automated runs (tests, judges) stay offscreen with their own browser profile, so they never steal the screen
    // or share state between runs.
    if hidden {
        builder = builder.position(-32000.0, -32000.0).skip_taskbar(true).focused(false).data_directory(data_dir.join("webview"));
    }
    if let Ok(port) = std::env::var("FABER_CDP_PORT") {
        builder = builder.additional_browser_args(&format!(
            "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
        ));
    }
    builder.build()
}

fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Faber", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Faber", true, None::<&str>)?;
    let mut tray = TrayIconBuilder::new()
        .tooltip("Faber")
        .menu(&Menu::with_items(app, &[&open, &quit])?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    *lock(&app.state::<Faber>().tray) = Some([open, quit]);
    Ok(())
}

fn main() {
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
    let mut builder = tauri::Builder::default();
    // A second launch brings the open window forward. Runs with their own data folder (tests) stay separate.
    if std::env::var_os("FABER_DATA_DIR").is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            state,
            open,
            send,
            branch,
            warm,
            interrupt,
            answer,
            rename,
            remove,
            remove_project,
            save_settings,
            add_project,
            save_file,
            show_file,
            read_image,
            open_terminal,
            open_link,
            set_tray
        ])
        .setup(move |app| {
            let data_dir = data_dir(app.handle())?;
            std::fs::create_dir_all(&data_dir)?;
            let clis = detect_clis();
            let handle = app.handle().clone();
            let emit = Arc::new(move |e: AppEvent| {
                let _ = handle.emit("faber:event", e);
            });
            let opts = EngineOptions {
                data_dir: data_dir.clone(),
                clis: clis.clone(),
                spawn: real_spawner(),
                windows: cfg!(windows),
                max_warm: 3,
                idle: Duration::from_secs(120),
                runtime: tauri::async_runtime::handle().inner().clone(),
            };
            app.manage(Faber {
                engine: Engine::new(opts, emit),
                models: Mutex::default(),
                tray: Mutex::default(),
                update: Mutex::default(),
                data_dir: data_dir.clone(),
                hidden,
            });
            if let Some(codex) = clis.get(&Vendor::Codex) {
                load_models(app.handle(), codex.clone(), data_dir.clone());
            }
            create_window(app.handle(), hidden, &data_dir)?;
            if !hidden {
                create_tray(app)?;
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
                && !hidden
            {
                api.prevent_exit();
            }
            if let RunEvent::Exit = event {
                let faber = app.state::<Faber>();
                faber.engine.shutdown();
                // On Windows this starts the installer, which replaces Faber and needs it closed.
                if let Some((update, bytes)) = lock(&faber.update).take() {
                    let _ = update.install(bytes);
                }
            }
        });
}
