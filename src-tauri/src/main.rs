//! The desktop shell: one window, a tray icon, and the commands that need the window. Every other command is answered by
//! the shared table (`faber-host`), which this shell relays them to; the engine does the work.

#![cfg_attr(windows, windows_subsystem = "windows")]

// The test hooks (`report_checks`, the checks page) never ship: a release build cannot be made with them.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

#[cfg(windows)]
mod pane;
mod tray;
mod update;
#[cfg(target_os = "macos")]
#[path = "pane/native.rs"]
mod visual_native;
mod visual_network;
mod visual_page;

#[cfg(windows)]
use awake::WindowsPower;
use awake::{CountedAwake, NoopPower, PowerKeepAwake};
#[cfg(not(windows))]
use faber::autostart::NoopAutostart;
#[cfg(windows)]
use faber::autostart::WindowsAutostart;
use faber::autostart::{self, AutostartKeeper, FakeAutostart, creates_window};
use faber::awake;
use faber_engine::model::Settings;
use faber_engine::{AppEvent, Engine};
pub(crate) use faber_host::default_data_dir;
use faber_host::{BoxFuture, Filter, Host, Kind, PickError, Picked, Shell, StartOptions, look};
use serde_json::Value;
use std::collections::HashSet;
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
    /// The commands any front end shares (R459); the ones that need the window are below.
    pub(crate) host: Arc<Host>,
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
}

/// What the host calls back in this shell: the tray and keeping the PC awake hear each event, the window receives it, and the
/// dialogs, the file manager, the browser, the tray's words and the updater are this shell's to answer.
struct TauriShell {
    handle: AppHandle,
    hidden: bool,
    awake: Arc<Mutex<CountedAwake>>,
    autostart: Arc<dyn AutostartKeeper>,
    /// Saves of the settings go one at a time, so Windows' registration and the stored switch never disagree.
    saving: Mutex<()>,
}

/// A file dialog over the main window, offering `filter` when there is one.
fn file_dialog(app: &AppHandle, filter: Option<Filter>) -> tauri_plugin_dialog::FileDialogBuilder<tauri::Wry> {
    let mut dialog = app.dialog().file();
    if let Some(Filter { name, extensions }) = filter {
        dialog = dialog.add_filter(name, extensions);
    }
    if let Some(w) = main_window(app) {
        dialog = dialog.set_parent(&w);
    }
    dialog
}

/// What a dialog answered, as the host reads it. A dialog that could not run counts as cancelled.
fn picked(answer: Option<tauri_plugin_dialog::FilePath>) -> Picked {
    match answer {
        Some(path) => path.into_path().map(Some).map_err(|_| PickError::NotLocal),
        None => Ok(None),
    }
}

impl Shell for TauriShell {
    fn heard(&self, e: &AppEvent) {
        // The engine calls this with its lock held, and the tray waits for the main thread, which can be waiting
        // for that lock. So the tray works on its own thread; it keeps the newest list, whatever the order. The tray reads
        // Faber's state, which is managed just after the host starts: an event before that is the next one's to show.
        if let AppEvent::Running { revision, items } = e {
            let active_turns: HashSet<String> = items.iter().map(|i| i.turn_id.clone()).collect();
            lock(&self.awake).sync_runs(active_turns);
            if !self.hidden && self.handle.try_state::<Faber>().is_some() {
                let (app, revision, items) = (self.handle.clone(), *revision, items.clone());
                tauri::async_runtime::spawn_blocking(move || tray::on_running(&app, revision, &items));
            }
        }
        if let AppEvent::Halted { title, why, vendor, .. } = e
            && !self.hidden
            && self.handle.try_state::<Faber>().is_some()
        {
            let (app, title, why, vendor) = (self.handle.clone(), title.clone(), why.clone(), *vendor);
            tauri::async_runtime::spawn_blocking(move || tray::on_halted(&app, &title, &why, vendor));
        }
    }

    fn shown_settings(&self, settings: Settings) -> Settings {
        shown_settings(settings, self.autostart.as_ref())
    }

    fn emit(&self, event: &Value) {
        let _ = self.handle.emit("faber:event", event);
    }

    fn version(&self) -> String {
        self.handle.package_info().version.to_string()
    }

    fn persist_settings(&self, autostart: Option<bool>, persist: &mut dyn FnMut() -> Result<Settings, String>) -> Result<Settings, String> {
        let _saving = lock(&self.saving);
        autostart::save(self.autostart.as_ref(), autostart, persist)
    }

    fn settings_saved(&self, settings: &Settings) {
        lock(&self.awake).set_enabled(settings.keep_awake());
    }

    fn pick_folder(&self) -> BoxFuture<'_, Picked> {
        let app = self.handle.clone();
        Box::pin(async move {
            let ask = move || file_dialog(&app, None).blocking_pick_folder();
            picked(tauri::async_runtime::spawn_blocking(ask).await.ok().flatten())
        })
    }

    fn pick_file(&self, filter: Option<Filter>) -> BoxFuture<'_, Picked> {
        let app = self.handle.clone();
        Box::pin(async move {
            let ask = move || file_dialog(&app, filter).blocking_pick_file();
            picked(tauri::async_runtime::spawn_blocking(ask).await.ok().flatten())
        })
    }

    fn pick_save(&self, filter: Option<Filter>, name: String) -> BoxFuture<'_, Picked> {
        let app = self.handle.clone();
        Box::pin(async move {
            let ask = move || file_dialog(&app, filter).set_file_name(name).blocking_save_file();
            picked(tauri::async_runtime::spawn_blocking(ask).await.ok().flatten())
        })
    }

    fn reveal(&self, path: PathBuf) -> Result<(), String> {
        self.handle.opener().reveal_item_in_dir(path).map_err(|_| "error.file.show".into())
    }

    fn open_link(&self, url: &str) -> Result<(), String> {
        self.handle.opener().open_url(url, None::<&str>).map_err(|_| "error.link.open".into())
    }

    fn set_texts(&self, texts: Value) -> Result<(), String> {
        let texts = serde_json::from_value(texts).map_err(|_| "error.host.badArgs".to_string())?;
        tray::set_texts(&self.handle, texts)
    }

    fn update_status(&self) -> Value {
        serde_json::to_value(update::status(&self.handle)).unwrap_or_default()
    }

    fn update_restart(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(update::restart_now(self.handle.clone()))
    }

    fn hold_install(&self) -> Result<Box<dyn Send>, String> {
        update::SetupRunning::begin().map(|running| Box::new(running) as Box<dyn Send>)
    }

    fn input_seen(&self) {
        if let Some(faber) = self.handle.try_state::<Faber>() {
            faber.touched.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn draft_dirty(&self, dirty: bool) {
        if let Some(faber) = self.handle.try_state::<Faber>() {
            faber.draft_dirty.store(dirty, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn game_in_front(&self) -> bool {
        tray::game_in_front()
    }

    fn shut_down(&self) {
        lock(&self.awake).shutdown();
    }
}

type Res<T> = Result<T, String>;

fn screen_navigation_allowed(url: &tauri::Url) -> bool {
    // WebKit reports children too; the native policy supplies the missing top-document distinction.
    url.as_str() == "about:blank"
        || (cfg!(any(target_os = "macos", target_os = "linux"))
            && visual_network::navigation_ready()
            && visual_page::navigation_allowed(url))
        || url.scheme() == "tauri"
        || url.host_str() == Some("tauri.localhost")
        || (cfg!(debug_assertions) && url.scheme() == "http" && url.host_str() == Some("localhost") && url.port() == Some(5173))
}

#[cfg(test)]
#[test]
fn visual_frame_disposal_allows_only_the_exact_empty_document_beside_the_existing_screen_urls() {
    assert_eq!(screen_navigation_allowed(&tauri::Url::parse("http://localhost:5173/").unwrap()), cfg!(debug_assertions));
    for local in ["about:blank", "tauri://localhost/index.html", "http://tauri.localhost/index.html"] {
        assert!(screen_navigation_allowed(&tauri::Url::parse(local).unwrap()), "{local}");
    }
    for refused in [
        "about:srcdoc",
        "blob:tauri://localhost/id",
        "about:blank#page",
        "about:blank?url=https://example.com",
        "https://example.com/",
        "file:///C:/page.html",
        "data:text/html,hello",
        "javascript:alert(1)",
        "https://localhost:5173/",
        "http://localhost:1234/",
    ] {
        assert!(!screen_navigation_allowed(&tauri::Url::parse(refused).unwrap()), "{refused}");
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    for visual in [
        "faber-visual://localhost/0123456789abcdef0123456789abcdef.html",
        "http://faber-visual.localhost/0123456789abcdef0123456789abcdef.html",
    ] {
        assert!(!screen_navigation_allowed(&tauri::Url::parse(visual).unwrap()), "{visual}");
    }
}

/// The settings as the screen shows them: the host's reading, and whether Windows will really start Faber for the switch.
fn shown_settings(settings: Settings, keeper: &dyn AutostartKeeper) -> Settings {
    let mut settings = faber_host::shown(settings);
    settings.autostart = Some(autostart::shown(keeper, settings.autostart()));
    settings
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The one window. Once the browser pane (R161 step 7) is a second webview in it, Tauri no longer counts it as a webview
/// window, so it is found as a window.
fn main_window(app: &AppHandle) -> Option<tauri::Window> {
    app.get_window("main")
}

// The commands below need the window, so this shell answers them itself. Every other command of the screen is in the shared
// table (`faber-host`), listed there with its kind; `Kind::Window` marks these.

#[cfg(test)]
mod settings_event_tests {
    use super::*;
    use faber_engine::model::Permissions;

    #[test]
    fn r160_settings_events_keep_screen_defaults_and_shell_values() {
        let keeper = FakeAutostart::new(false);
        let sparse = Settings {
            permissions: Some(Permissions::Ask),
            autostart: Some(true),
            appearance: Some(serde_json::from_value(serde_json::json!({"base":"unknown"})).unwrap()),
            ..Settings::default()
        };
        let settings = shown_settings(sparse, &keeper);
        assert_eq!(settings.notifications, Some(true));
        assert_eq!(settings.notification_sound, Some(true));
        assert_eq!(settings.keep_awake, Some(true));
        assert_eq!(settings.permissions, Some(Permissions::Ask));
        assert_eq!(settings.autostart, Some(false), "the switch follows the shell, not stale storage");
        assert!(settings.appearance.is_none(), "unusable saved appearances follow the ordinary response");

        let off = Settings { notifications: Some(false), notification_sound: Some(false), keep_awake: Some(false), ..Settings::default() };
        let settings = shown_settings(off, &keeper);
        assert_eq!(settings.notifications, Some(false));
        assert_eq!(settings.notification_sound, Some(false));
        assert_eq!(settings.keep_awake, Some(false));
    }
}

/// No authored page can call this command; the screen snapshots the saved choice for each opened version.
#[cfg(feature = "test-hooks")]
#[tauri::command]
fn visual_policy(faber: State<'_, Faber>) -> bool {
    visual_network::ready() && faber.engine.visual_network()
}

#[tauri::command]
fn visual_open(pages: State<'_, visual_page::Pages>) -> String {
    pages.open()
}

#[tauri::command]
async fn visual_page(
    faber: State<'_, Faber>,
    pages: State<'_, visual_page::Pages>,
    id: String,
    revision: u64,
    document: String,
) -> Res<visual_page::Prepared> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    if !visual_network::wait_navigation_ready(std::time::Duration::from_secs(5)).await {
        return Err("visual.failed".to_string());
    }
    pages.prepare(&id, revision, document, visual_network::ready() && faber.engine.visual_network()).map_err(str::to_string)
}

#[tauri::command]
fn visual_close(pages: State<'_, visual_page::Pages>, id: String) {
    pages.close(&id);
}

/// Faber's browser pane (R161 step 7) over the screen's own rectangle for it, in the page's logical pixels.
#[tauri::command]
async fn site_place(app: AppHandle, load: u32, box_id: u64, x: f64, y: f64, width: f64, height: f64) -> bool {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.place(load, box_id, x, y, width, height).await;
    }
    let _ = (app, load, box_id, x, y, width, height);
    false
}

/// The browser pane's page takes the corners of the card the screen shows it in: `radius` in logical pixels and the card's
/// colour as red, green and blue, drawn over the page's own corners; a radius of 0 leaves them square.
#[tauri::command]
async fn site_round(app: AppHandle, radius: f64, color: [u8; 3]) {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        pane.set_round(radius, color).await;
    }
    let _ = (app, radius, color);
}

/// The screen sizes Faber's browser can draw its pages at (`faber_engine::screens::SIZES`, the agent's own list) and the one in
/// use, by its `1920x1080` name; none is the pane's own. With `pick`, `size` becomes the pane's first: a name not in the list
/// (or none) is the pane's own. The agent's next calls use it too.
#[tauri::command]
async fn site_size(app: AppHandle, pick: bool, size: Option<String>) -> serde_json::Value {
    let sizes: Vec<_> = faber_engine::screens::SIZES
        .iter()
        .map(|v| serde_json::json!({ "name": v.name(), "screen": v.screen.name(), "width": v.width, "height": v.height }))
        .collect();
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        if pick {
            pane.set_size(size.as_deref().and_then(faber_engine::screens::named)).await;
        }
        return serde_json::json!({ "now": pane.size().map(|v| v.name()), "sizes": sizes });
    }
    let _ = (app, pick, size);
    serde_json::json!({ "now": null, "sizes": sizes })
}

/// The browser pane leaves the screen for the screen's box `box_id` and every older one; it keeps its page.
#[tauri::command]
async fn site_park(app: AppHandle, load: u32, box_id: u64) {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>() {
        pane.park(load, box_id);
    }
    let _ = (app, load, box_id);
}

/// The person opens a page from a reply's card in the browser pane; it is that conversation's page from then on.
#[tauri::command]
async fn site_open(app: AppHandle, id: String, url: String) -> bool {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.show_page(&id, &url).await;
    }
    let _ = (app, id, url);
    false
}

/// The person goes back, forward or reloads in the browser pane, on that conversation's page only.
#[tauri::command]
async fn site_step(app: AppHandle, id: String, step: String) -> bool {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.step(&id, &step).await;
    }
    let _ = (app, id, step);
    false
}

// Refuse one open only in the hidden test build to exercise retry feedback.
#[cfg(all(windows, feature = "test-hooks"))]
static TEST_REFUSE_SITE_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(all(windows, feature = "test-hooks"))]
#[tauri::command]
fn test_site_refuse_once() {
    TEST_REFUSE_SITE_OPEN.store(true, std::sync::atomic::Ordering::Release);
}

/// The person loads an address they typed in the tab in front of the browser pane.
#[tauri::command]
async fn site_go(app: AppHandle, id: String, url: String) -> bool {
    #[cfg(all(windows, feature = "test-hooks"))]
    if TEST_REFUSE_SITE_OPEN.swap(false, std::sync::atomic::Ordering::AcqRel) {
        return false;
    }
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.go(&id, &url).await;
    }
    let _ = (app, id, url);
    false
}

/// What the person did to the browser pane's tabs, answered with the tabs and the tab it concerns; the tab in front shows.
#[derive(serde::Serialize)]
struct SiteTab {
    tabs: faber_engine::browser::tabs::Tabs,
    tab: Option<String>,
}

#[tauri::command]
async fn site_tab(app: AppHandle, id: String, step: String, tab: Option<String>) -> Option<SiteTab> {
    use faber_engine::TabStep;
    let step = match (step.as_str(), tab) {
        ("new", _) => TabStep::New,
        ("reopen", _) => TabStep::Reopen,
        ("bring", Some(tab)) => TabStep::Bring(tab),
        ("close", Some(tab)) => TabStep::Close(tab),
        ("closeOthers", Some(tab)) => TabStep::CloseOthers(tab),
        _ => return None,
    };
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.tab(&id, step).await.map(|(tabs, tab)| SiteTab { tabs, tab });
    }
    let _ = (app, id, step);
    None
}

/// Whether Back and Forward of the browser pane can act on conversation `id`'s page: `[back, forward]`.
#[tauri::command]
async fn site_history(app: AppHandle, id: String) -> [bool; 2] {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        let (back, forward) = pane.history(&id).await;
        return [back, forward];
    }
    let _ = (app, id);
    [false, false]
}

/// Clears conversation `id`'s browsing data once the person said yes: tabs, closed tabs, sign-ins and the page's own data.
#[tauri::command]
async fn site_clear(app: AppHandle, id: String) {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        pane.clear(&id).await;
    }
    let _ = (app, id);
}

/// A Firefox profile the person may bring sign-ins from: its id (never a path), its name and whether it is the one used last.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CookieProfile {
    id: String,
    name: String,
    last_used: bool,
}

/// A site with cookies in that profile, for the person to pick: no cookie crosses into the screen.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CookieSite {
    name: String,
    cookies: u32,
    kind: Option<&'static str>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CookieSites {
    ticket: String,
    sites: Vec<CookieSite>,
}

/// The Firefox profiles on this computer whose sign-ins the browser pane can bring over (Windows only: elsewhere none).
#[tauri::command]
async fn cookie_profiles() -> Vec<CookieProfile> {
    #[cfg(windows)]
    let found = tauri::async_runtime::spawn_blocking(pane::firefox::profiles).await.unwrap_or_default();
    #[cfg(not(windows))]
    let found: Vec<CookieProfile> = Vec::new();
    #[cfg(windows)]
    let found = found.into_iter().map(|p| CookieProfile { id: p.id, name: p.name, last_used: p.last_used }).collect();
    found
}

/// The sites with cookies in the profile `profile`, under a ticket the import names.
#[tauri::command]
async fn cookie_sites(app: AppHandle, profile: String) -> Res<CookieSites> {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        let (ticket, sites) = pane.list_sites(profile).await.map_err(str::to_string)?;
        let sites = sites
            .into_iter()
            .map(|s| CookieSite {
                name: s.name,
                cookies: s.cookies,
                kind: s.kind.map(|k| match k {
                    pane::firefox::Kind::Money => "money",
                    pane::firefox::Kind::Email => "email",
                    pane::firefox::Kind::SignIn => "signin",
                }),
            })
            .collect();
        return Ok(CookieSites { ticket, sites });
    }
    let _ = (app, profile);
    Err("error.cookies.missing".into())
}

/// Brings the person's sign-ins on the sites they picked into conversation `id`'s pages; answers how many sites came over.
#[tauri::command]
async fn cookie_import(app: AppHandle, id: String, ticket: String, sites: Vec<String>) -> Res<usize> {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        return pane.import(&id, &ticket, &sites).await.map_err(str::to_string);
    }
    let _ = (app, id, ticket, sites);
    Err("error.cookies.missing".into())
}

/// The sites whose sign-ins conversation `id` holds.
#[tauri::command]
async fn cookie_held(app: AppHandle, id: String) -> Vec<String> {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>() {
        return pane.held_sites(&id);
    }
    let _ = (app, id);
    Vec::new()
}

/// Forgets the sign-ins conversation `id` holds and signs its page out of them.
#[tauri::command]
async fn cookie_forget(app: AppHandle, id: String) {
    #[cfg(windows)]
    if let Some(pane) = app.try_state::<Arc<pane::Pane>>().map(|p| p.inner().clone()) {
        pane.forget(&id).await;
    }
    let _ = (app, id);
}

/// Debug test builds only: observed native child geometry and a bounded view of its fixture page.
#[cfg(all(windows, feature = "test-hooks"))]
#[tauri::command]
async fn test_site_probe(app: AppHandle, capture: bool, quick: Option<bool>) -> Res<serde_json::Value> {
    let pane = app.try_state::<Arc<pane::Pane>>().ok_or("the test browser is unavailable")?.inner().clone();
    pane.test_probe(capture, quick.unwrap_or(false)).await
}

/// Resizes the real hidden host, rather than emulating the main page's viewport.
#[cfg(all(windows, feature = "test-hooks"))]
#[tauri::command]
async fn test_host_size(app: AppHandle, width: u32, height: u32) -> Res<()> {
    if !(720..=1920).contains(&width) || !(600..=1080).contains(&height) {
        return Err("test host size is outside 720–1920 by 600–1080".into());
    }
    app.get_window("main")
        .ok_or("the test window is unavailable")?
        .set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| e.to_string())
}

/// Test builds only: the screen's checks send their result to `FABER_CHECKS_OUT`, written whole before the app ends.
#[cfg(feature = "test-hooks")]
#[tauri::command]
async fn report_checks(app: AppHandle, json: String) -> Res<()> {
    let out = PathBuf::from(std::env::var_os("FABER_CHECKS_OUT").ok_or("FABER_CHECKS_OUT is not set")?);
    let part = out.with_extension("part");
    std::fs::write(&part, json).and_then(|()| std::fs::rename(&part, &out)).map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}

/// Same folder the Electron versions used, so conversations carry over.
fn data_dir(app: &AppHandle) -> tauri::Result<PathBuf> {
    match std::env::var_os("FABER_DATA_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => Ok(default_data_dir(&app.path().config_dir()?)),
    }
}

/// What Quit does, and what the idle update install does before the installer ends the process.
pub(crate) fn shut_down(app: &AppHandle) {
    app.state::<Faber>().host.shut_down();
}

/// Where a real run keeps its crash record (a local file, never sent): none for a test build, a hidden run or one with its own
/// data folder. The folder is the one `data_dir` gives later, which Tauri reads from the same `dirs` call.
fn crash_record_dir(is_test: bool) -> Option<PathBuf> {
    if is_test { None } else { dirs::config_dir().map(|config| default_data_dir(&config)) }
}

/// Brings the window back, building the screen again if it was closed to the tray.
fn show(app: &AppHandle) {
    let existing = main_window(app);
    let fresh = existing.is_none();
    let window = existing.or_else(|| {
        let faber = app.state::<Faber>();
        create_window(app, faber.hidden, &faber.data_dir).ok().map(|w| w.as_ref().window())
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

/// When this process started, for `startup.log`.
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
/// Milliseconds from the start to the first window built, for `startup.log`.
static WINDOW_BUILT_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Only the first window of a run is written down.
static STARTUP_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn ms_since_start() -> u64 {
    STARTED.get().map_or(0, |at| at.elapsed().as_millis().try_into().unwrap_or(u64::MAX))
}

/// Shows the window once, when the screen has painted its first frame or the failsafe says so, so a person never sees an
/// empty one. Automated runs build theirs shown (offscreen) and skip this. The first one of a run writes down how long the
/// start took, so a slow one can be explained afterwards.
fn reveal(app: &AppHandle, by: &str) {
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
    if !STARTUP_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        let window = WINDOW_BUILT_MS.load(std::sync::atomic::Ordering::SeqCst);
        update::log_to(&faber.data_dir, update::STARTUP_LOG, "shown", &format!("window={window}ms shown={}ms by={by}", ms_since_start()));
    }
}

/// The screen painted its first frame: the window may show now.
#[tauri::command]
fn screen_ready(app: AppHandle) {
    reveal(&app, "paint");
}

fn create_window(app: &AppHandle, hidden: bool, data_dir: &Path) -> tauri::Result<tauri::WebviewWindow> {
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
        .initialization_script_for_all_frames(include_str!("visual.js"))
        .title("Faber")
        .inner_size(1280.0, 800.0)
        .min_inner_size(720.0, 480.0)
        .background_color(Color(r, g, b, 0xff))
        // Shown after the screen's first paint (`reveal`), so there is never a blank window.
        .visible(hidden)
        // The "Paste" of the right-click menu reads the clipboard; without this WebView2 asks the person every time.
        .enable_clipboard_access()
        // A file dragged over the window reaches the page as an ordinary drop (the composer's box, and the whole window's "Drop to attach"),
        // as it does on macOS and Linux. With Tauri's own drag and drop handler on, Windows keeps those events to itself.
        .disable_drag_drop_handler()
        // Only the screen's explicit open_link command opens a browser; an authored frame never may.
        .on_navigation(screen_navigation_allowed);
    // The page of the screen's checks never sees the system's reduced-motion preference (tests/ui/system-motion.js): a CI
    // runner asks for it, and the motion checks would stand aside or fail.
    #[cfg(feature = "test-hooks")]
    if std::env::var_os("FABER_CHECKS_OUT").is_some() {
        builder = builder.initialization_script(include_str!("../../tests/ui/system-motion.js"));
        if let Ok(filter) = std::env::var("FABER_CHECKS_FILTER") {
            builder = builder.initialization_script(format!("window.__faberChecksFilter = {};", serde_json::to_string(&filter)?));
        }
    }
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
    // macOS treats a window placed past the desktop as occluded, and WebKit suspends the page of an occluded window after a
    // while (the checks page's whole event loop froze mid-run on CI). The test build there sits on the screen's corner under
    // every other window, ignores the mouse, never takes focus and asks WebKit not to schedule its page as inactive.
    // Windows keeps the offscreen place above, and nothing here runs outside the test build.
    #[cfg(all(feature = "test-hooks", target_os = "macos"))]
    if hidden {
        builder = builder
            .position(0.0, 0.0)
            .always_on_bottom(true)
            .focusable(false)
            .shadow(false)
            .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    }
    if hidden {
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
    #[cfg(all(feature = "test-hooks", target_os = "macos"))]
    if hidden {
        let _ = window.set_ignore_cursor_events(true);
    }
    visual_network::install(&window);
    let _ = WINDOW_BUILT_MS.compare_exchange(
        0,
        ms_since_start().max(1),
        std::sync::atomic::Ordering::SeqCst,
        std::sync::atomic::Ordering::SeqCst,
    );
    if !hidden {
        // The failsafe: a screen that never reports its first paint still shows up.
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            reveal(&handle, "failsafe");
        });
    }
    Ok(window)
}

fn main() {
    STARTED.get_or_init(std::time::Instant::now);
    if let Some(code) = faber_engine::proc::guard_mode() {
        std::process::exit(code);
    }
    // A copy of Faber in a temporary folder that finishes an update install (`update/handoff.rs`): no window, no app.
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.first().is_some_and(|arg| arg == update::handoff::FLAG) {
            std::process::exit(update::handoff::finish(&args[1..]));
        }
    }
    let autostart_launch = !creates_window(std::env::args());
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
    let is_test = cfg!(feature = "test-hooks") || hidden || std::env::var_os("FABER_DATA_DIR").is_some();
    // A refused destination must not install a writer that a later startup panic could call.
    if let Some(dir) = crash_record_dir(is_test)
        && let Err(error) = faber_engine::crash::install_hook(dir, env!("CARGO_PKG_VERSION"), env!("FABER_COMMIT"))
    {
        eprintln!("Faber failed to start: {error}");
        std::process::exit(1);
    }
    // Before any HTTPS client, the updater's included: reqwest is built without a crypto provider of its own.
    faber_engine::tls::install();
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
        .register_uri_scheme_protocol("faber-visual", |context, request| {
            context.app_handle().state::<visual_page::Pages>().respond(context.webview_label(), request)
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler({
            let handler: fn(tauri::ipc::Invoke) -> bool = tauri::generate_handler![
                site_place,
                site_round,
                site_park,
                site_size,
                site_open,
                site_step,
                site_go,
                site_tab,
                site_clear,
                site_history,
                cookie_profiles,
                cookie_sites,
                cookie_import,
                cookie_held,
                cookie_forget,
                visual_open,
                visual_page,
                visual_close,
                #[cfg(feature = "test-hooks")]
                visual_policy,
                screen_ready,
                #[cfg(feature = "test-hooks")]
                report_checks,
                #[cfg(all(windows, feature = "test-hooks"))]
                test_site_probe,
                #[cfg(all(windows, feature = "test-hooks"))]
                test_host_size,
                #[cfg(all(windows, feature = "test-hooks"))]
                test_site_refuse_once
            ];
            // Only the screen calls Faber: a web page in the browser pane (R161 step 7) is refused before any command runs.
            move |invoke: tauri::ipc::Invoke| {
                if invoke.message.webview().label() != "main" {
                    invoke.resolver.reject("Faber answers only its own screen");
                    return true;
                }
                // A command of the shared table runs there (R459); the rest need the window and are this shell's own, listed above.
                if Host::kind(invoke.message.command()).is_some_and(|kind| kind != Kind::Window) {
                    let host = invoke.message.webview().state::<Faber>().host.clone();
                    let command = invoke.message.command().to_string();
                    let args = match invoke.message.payload() {
                        tauri::ipc::InvokeBody::Json(args) => args.clone(),
                        tauri::ipc::InvokeBody::Raw(_) => Value::Null,
                    };
                    invoke
                        .resolver
                        .respond_async(async move { host.dispatch(&command, args).await.map_err(tauri::ipc::InvokeError::from) });
                    return true;
                }
                handler(invoke)
            }
        })
        .setup(move |app| {
            app.manage(visual_page::Pages::default());
            let data_dir = data_dir(app.handle())?;
            app.manage(tray::Tray::default());
            // Only the first launch gets here (a second one ends in the single-instance plugin), so only it reads the marker.
            if crash_record_dir(is_test).is_some() {
                faber_engine::crash::start_run(&data_dir, env!("CARGO_PKG_VERSION"), env!("FABER_COMMIT"));
            }
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
                    Arc::new(WindowsAutostart)
                }
                #[cfg(not(windows))]
                {
                    Arc::new(NoopAutostart)
                }
            };
            let keep_awake = faber_engine::store::Store::new(&data_dir).settings().keep_awake();
            let awake = Arc::new(Mutex::new(CountedAwake::new(power, keep_awake)?));
            let shell =
                Arc::new(TauriShell { handle: app.handle().clone(), hidden, awake, autostart: autostart_keeper, saving: Mutex::new(()) });
            // R161 step 7: Faber's own browser, for a project's agent. Windows only for now: its page is driven through WebView2.
            #[cfg(windows)]
            let pane = pane::Pane::new(app.handle().clone());
            #[cfg(windows)]
            let browser: Option<Arc<dyn faber_engine::browser::Browser>> = Some(Arc::new(pane::PaneBrowser(pane.clone())));
            #[cfg(not(windows))]
            let browser: Option<Arc<dyn faber_engine::browser::Browser>> = None;
            // While a game is in front (R191) no new project check starts and a running one is held to a tenth of the processor.
            // The test build never waits for one, as the screen's own reading (`game_in_front`) never changes there.
            let game_guard: faber_engine::verify::Guard = Arc::new(|| !cfg!(feature = "test-hooks") && crate::tray::game_in_front());
            let host = faber_host::start(
                StartOptions {
                    data_dir: data_dir.clone(),
                    hidden,
                    runtime: tauri::async_runtime::handle().inner().clone(),
                    installed_data_dir: faber_host::installed_data_dir(hidden),
                    game_guard: Some(game_guard),
                    browser,
                },
                shell,
            )?;
            app.manage(Faber {
                engine: host.engine.clone(),
                host,
                update: Mutex::default(),
                stage: Mutex::default(),
                revealed: std::sync::atomic::AtomicBool::new(false),
                touched: std::sync::atomic::AtomicBool::new(false),
                draft_dirty: std::sync::atomic::AtomicBool::new(false),
                data_dir: data_dir.clone(),
                hidden,
            });
            #[cfg(windows)]
            app.manage(pane);
            if !autostart_launch {
                create_window(app.handle(), hidden, &data_dir)?;
            }
            if !hidden {
                tray::create(app)?;
            }
            #[cfg(feature = "test-hooks")]
            if autostart_launch && let Some(out) = std::env::var_os("FABER_CHECKS_OUT") {
                std::fs::write(out, serde_json::json!({ "windowCount": app.windows().len() }).to_string())?;
            }
            // Only installed builds update themselves; source runs and test runs never do.
            if !cfg!(debug_assertions) && !hidden && std::env::var_os("FABER_DATA_DIR").is_none() {
                update::keep_updated(app.handle());
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
                // The screen is gone: the quota is read at the slower pace until the window comes back.
                app.state::<Faber>().engine.set_attention(false, false);
            }
            if let RunEvent::Exit = event {
                shut_down(app);
                let faber = app.state::<Faber>();
                // On Windows this starts the installer, which replaces Faber and needs it closed.
                if let Some((update, bytes)) = lock(&faber.update).take() {
                    let _ = update.install(bytes);
                }
            }
        });
}

#[cfg(test)]
#[test]
fn the_build_carries_its_commit_or_says_it_does_not_know_it() {
    let commit = env!("FABER_COMMIT");
    assert!(commit == "unknown" || (commit.len() == 12 && commit.chars().all(|c| c.is_ascii_hexdigit())), "{commit}");
}

#[cfg(test)]
mod crash_record_wiring_tests {
    use super::*;

    /// The text of the top-level function that starts at `signature`, without spaces or line breaks.
    fn body_of(signature: &str) -> String {
        // A Windows checkout may end its lines with CR LF.
        let source = include_str!("main.rs").replace("\r\n", "\n");
        let from = source.find(signature).expect("the function is in this file");
        let end = source[from..].find("\n}\n").expect("it ends at the left margin");
        source[from..from + end].split_whitespace().collect()
    }

    #[test]
    fn a_test_build_a_hidden_run_or_one_with_its_own_data_folder_writes_no_crash_record() {
        assert_eq!(crash_record_dir(true), None);
        assert_eq!(crash_record_dir(false).and_then(|dir| dir.file_name().map(|n| n.to_owned())), Some("Faber".into()));
        // The three places that act on it: the hook at the top of main, the marker at the setup, and the end of the run.
        let main = body_of("fn main() {");
        assert!(
            main.contains("ifletSome(dir)=crash_record_dir(is_test)&&letErr(error)=faber_engine::crash::install_hook(dir,"),
            "the hook is gated"
        );
        assert!(main.contains("ifcrash_record_dir(is_test).is_some(){faber_engine::crash::start_run("), "the marker is gated");
    }

    #[test]
    fn quit_the_end_of_a_session_and_the_stop_before_an_update_all_end_the_run_cleanly() {
        // `shut_down` is what `RunEvent::Exit` calls, and tao sends that event on a Windows sign-out, restart or shutdown.
        // The host ends the crash marker first of all (its own test checks the order).
        assert!(body_of("pub(crate) fn shut_down(").contains("host.shut_down();"));
        assert!(body_of("fn main() {").contains("RunEvent::Exit=event{shut_down(app);"));
    }
}
