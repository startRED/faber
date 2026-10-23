//! The desktop shell: one window, a tray icon, and the commands the screen may call. The engine does the work.

#![cfg_attr(windows, windows_subsystem = "windows")]

// The test hooks (`report_checks`, the checks page) never ship: a release build cannot be made with them.
#[cfg(all(feature = "test-hooks", not(debug_assertions)))]
compile_error!("the `test-hooks` feature is for debug test builds only; it must not be enabled in a release build");

mod feed;
mod folder;
mod look;
#[cfg(windows)]
mod pane;
mod project_commands;
mod setup;
mod terminal;
mod tools;
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
use base64::Engine as _;
#[cfg(not(windows))]
use faber::autostart::NoopAutostart;
#[cfg(windows)]
use faber::autostart::WindowsAutostart;
use faber::autostart::{self, AutostartKeeper, FakeAutostart, creates_window};
use faber::awake;
use faber_engine::cli::Cli;
use faber_engine::codex::{ModelInfo, list_codex_models, read_codex_limits};
use faber_engine::model::{Conversation, Project, Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::project::checks;
use faber_engine::{AppEvent, AppState, BranchRequest, Engine, EngineOptions, SendRequest, WarmRequest};
use look::{Failure, Look, ThemeFile};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub(crate) awake: Arc<Mutex<CountedAwake>>,
    pub(crate) autostart: Mutex<Arc<dyn AutostartKeeper>>,
    /// What the screen last heard of each streaming reply, so an update carries only the steps that changed (`feed.rs`).
    pub(crate) feed: Arc<Mutex<feed::Feed>>,
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

fn shown_settings(settings: Settings, keeper: &dyn AutostartKeeper) -> Settings {
    let mut settings = settings.shown();
    settings.autostart = Some(autostart::shown(keeper, settings.autostart()));
    settings.appearance = look::usable(settings.appearance.take());
    settings
}

fn event_for_screen(event: AppEvent, keeper: &dyn AutostartKeeper) -> AppEvent {
    match event {
        AppEvent::Settings { settings } => AppEvent::Settings { settings: shown_settings(settings, keeper) },
        event => event,
    }
}

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
        let AppEvent::Settings { settings } = event_for_screen(AppEvent::Settings { settings: sparse }, &keeper) else {
            panic!("settings event changed its type");
        };
        assert_eq!(settings.notifications, Some(true));
        assert_eq!(settings.notification_sound, Some(true));
        assert_eq!(settings.keep_awake, Some(true));
        assert_eq!(settings.permissions, Some(Permissions::Ask));
        assert_eq!(settings.autostart, Some(false), "the switch follows the shell, not stale storage");
        assert!(settings.appearance.is_none(), "unusable saved appearances follow the ordinary response");

        let off = Settings { notifications: Some(false), notification_sound: Some(false), keep_awake: Some(false), ..Settings::default() };
        let AppEvent::Settings { settings } = event_for_screen(AppEvent::Settings { settings: off }, &keeper) else {
            panic!("settings event changed its type");
        };
        assert_eq!(settings.notifications, Some(false));
        assert_eq!(settings.notification_sound, Some(false));
        assert_eq!(settings.keep_awake, Some(false));
    }
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

/// The one window. Once the browser pane (R161 step 7) is a second webview in it, Tauri no longer counts it as a webview
/// window, so it is found as a window.
fn main_window(app: &AppHandle) -> Option<tauri::Window> {
    app.get_window("main")
}

// The screen calls the engine through this fixed list of commands; nothing else crosses over.

#[tauri::command]
async fn state(app: AppHandle, faber: State<'_, Faber>) -> Res<FullState> {
    lock(&faber.feed).forget();
    let mut state = faber.engine.state();
    state.settings = shown_settings(std::mem::take(&mut state.settings), lock(&faber.autostart).as_ref());
    let look = look::stored(state.settings.appearance.as_ref());
    Ok(FullState { state, look, models: lock(&faber.models).clone(), version: app.package_info().version.to_string() })
}

#[tauri::command]
async fn open(faber: State<'_, Faber>, id: String) -> Res<Option<Conversation>> {
    // Whatever it heard of a streaming reply, the screen now takes this snapshot: the next update of each goes whole.
    lock(&faber.feed).forget();
    Ok(faber.engine.open_for_screen(&id))
}

#[tauri::command]
async fn create_chat_group(faber: State<'_, Faber>, name: String) -> Res<faber_engine::engine::ChatGroup> {
    faber.engine.create_chat_group(&name)
}

#[tauri::command]
async fn save_chat_group(
    faber: State<'_, Faber>,
    id: String,
    name: Option<String>,
    instructions: Option<String>,
) -> Res<faber_engine::engine::ChatGroup> {
    faber.engine.save_chat_group(&id, name.as_deref(), instructions.as_deref())
}

#[tauri::command]
async fn add_chat_group_files(
    faber: State<'_, Faber>,
    id: String,
    files: Vec<faber_engine::workspace::Attachment>,
) -> Res<faber_engine::engine::ChatGroup> {
    faber.engine.add_chat_group_files(&id, &files)
}

#[tauri::command]
async fn remove_chat_group_file(faber: State<'_, Faber>, id: String, file_id: String) -> Res<faber_engine::engine::ChatGroup> {
    faber.engine.remove_chat_group_file(&id, &file_id)
}

#[tauri::command]
async fn remove_chat_group(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.remove_chat_group(&id)
}

#[tauri::command]
async fn move_chat_to_group(faber: State<'_, Faber>, conversation_id: String, group_id: Option<String>) -> Res<()> {
    faber.engine.move_chat_to_group(&conversation_id, group_id.as_deref())
}

#[tauri::command]
async fn new_group_chat(faber: State<'_, Faber>, group_id: String) -> Res<Conversation> {
    faber.engine.new_group_chat(&group_id).map(Conversation::for_screen)
}

struct SearchRequest {
    replaced: tokio::sync::oneshot::Sender<()>,
    cancelled: Arc<AtomicBool>,
}

struct SearchQueue {
    scanning: tokio::sync::Mutex<()>,
    latest: Mutex<Option<SearchRequest>>,
}

impl SearchQueue {
    const fn new() -> Self {
        Self { scanning: tokio::sync::Mutex::const_new(()), latest: Mutex::new(None) }
    }

    async fn acquire(&self) -> Option<(tokio::sync::MutexGuard<'_, ()>, Arc<AtomicBool>)> {
        let (replaced, gone) = tokio::sync::oneshot::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some(older) = lock(&self.latest).replace(SearchRequest { replaced, cancelled: cancelled.clone() }) {
            older.cancelled.store(true, Ordering::Relaxed);
            let _ = older.replaced.send(());
        }
        tokio::select! {
            biased;
            _ = gone => None,
            one = self.scanning.lock() => Some((one, cancelled)),
        }
    }
}

#[tauri::command]
async fn search_messages(faber: State<'_, Faber>, query: String, area: faber_engine::model::Area) -> Res<faber_engine::search::Results> {
    // As with mentions: one scan runs and only the latest waits. Superseded scans stop between conversation files.
    static SEARCHES: SearchQueue = SearchQueue::new();
    let Some((_one, cancelled)) = SEARCHES.acquire().await else {
        return Ok(faber_engine::search::Results { ready: true, hits: vec![] });
    };
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.search_messages_cancellable(&query, area, &cancelled).map_err(str::to_string))
        .await
        .map_err(|_| "error.store.read".to_string())?
}

#[derive(Default)]
struct PromptReads {
    active: Option<(String, Arc<AtomicBool>)>,
    // A blur may arrive before its read starts on the async worker. Retain only recent request ids, never prompt text.
    cancelled: std::collections::VecDeque<String>,
}

impl PromptReads {
    fn start(&mut self, id: String) -> Option<Arc<AtomicBool>> {
        if self.cancelled.contains(&id) {
            return None;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Some((_, old)) = self.active.replace((id, cancelled.clone())) {
            old.store(true, Ordering::Relaxed);
        }
        Some(cancelled)
    }

    fn cancel(&mut self, id: String) {
        if self.active.as_ref().is_some_and(|(active, _)| *active == id)
            && let Some((_, token)) = self.active.take()
        {
            token.store(true, Ordering::Relaxed);
        }
        self.cancelled.push_back(id);
        if self.cancelled.len() > 32 {
            self.cancelled.pop_front();
        }
    }

    fn finish(&mut self, id: &str) {
        if self.active.as_ref().is_some_and(|(active, _)| active == id) {
            self.active = None;
        }
    }
}

#[tauri::command]
async fn prompt_history(
    faber: State<'_, Faber>,
    request_id: String,
    cancel: Option<bool>,
    conversation_id: Option<String>,
    area: faber_engine::model::Area,
    project_id: Option<String>,
) -> Res<Vec<String>> {
    static READS: std::sync::LazyLock<Mutex<PromptReads>> = std::sync::LazyLock::new(|| Mutex::new(PromptReads::default()));
    if cancel == Some(true) {
        lock(&READS).cancel(request_id);
        return Ok(vec![]);
    }
    let Some(cancelled) = lock(&READS).start(request_id.clone()) else { return Ok(vec![]) };
    let engine = faber.engine.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        engine.prompt_history_cancellable(conversation_id.as_deref(), area, project_id.as_deref(), &cancelled).map_err(str::to_string)
    })
    .await;
    lock(&READS).finish(&request_id);
    result.map_err(|_| "error.store.read".to_string())?
}

#[cfg(test)]
mod search_queue_tests {
    use super::{Ordering, PromptReads, SearchQueue};
    use std::future::Future;

    #[test]
    fn prompt_history_cancels_superseded_reads_and_only_the_blurred_request() {
        let mut reads = PromptReads::default();
        let first = reads.start("first".into()).unwrap();
        let second = reads.start("second".into()).unwrap();
        assert!(first.load(Ordering::Relaxed));
        reads.cancel("first".into());
        reads.finish("first");
        assert!(!second.load(Ordering::Relaxed));
        reads.cancel("second".into());
        assert!(second.load(Ordering::Relaxed));
    }

    #[test]
    fn prompt_history_a_blur_before_worker_start_prevents_the_read_with_bounded_request_ids() {
        let mut reads = PromptReads::default();
        for id in 0..40 {
            reads.cancel(id.to_string());
        }
        assert_eq!(reads.cancelled.len(), 32);
        assert!(reads.start("39".into()).is_none());
        let token = reads.start("new".into()).unwrap();
        reads.finish("new");
        assert!(reads.active.is_none());
        assert!(!token.load(Ordering::Relaxed));
    }
    use std::task::{Context, Poll, Waker};

    #[tokio::test]
    async fn searches_cancel_the_running_scan_and_replace_only_the_waiting_query() {
        let queue = SearchQueue::new();
        let (running, cancelled) = queue.acquire().await.unwrap();
        let mut context = Context::from_waker(Waker::noop());
        let mut waiting = std::pin::pin!(queue.acquire());
        assert!(waiting.as_mut().poll(&mut context).is_pending());
        assert!(cancelled.load(Ordering::Relaxed), "the running scan must stop before reading another file");

        let mut newest = std::pin::pin!(queue.acquire());
        assert!(newest.as_mut().poll(&mut context).is_pending(), "the newer scan must wait for the running scan");
        assert!(matches!(waiting.as_mut().poll(&mut context), Poll::Ready(None)), "the replaced query must give up without scanning");
        drop(running);

        let (running, cancelled) = newest.await.unwrap();
        assert!(!cancelled.load(Ordering::Relaxed), "only the latest query gets to scan");
        drop(running);
        let (_, cancelled) = queue.acquire().await.unwrap();
        assert!(!cancelled.load(Ordering::Relaxed), "an idle queue must admit the next query");
    }
}

#[tauri::command]
async fn send(faber: State<'_, Faber>, req: SendRequest) -> Res<String> {
    // A project conversation's first message makes its copy after the message is recorded (R201), on the engine's own
    // blocking thread; this call waits only for the engine's lock, away from the async workers.
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.send(req)).await.map_err(|_| "error.copy.failed".to_string())?
}

/// The sessions started in a terminal in a project's folder (parity F5).
#[tauri::command]
async fn cli_sessions(faber: State<'_, Faber>, project_id: String) -> Res<Vec<faber_engine::model::CliSession>> {
    faber.engine.cli_sessions(&project_id, Duration::from_secs(20)).await
}

/// Opens a session started in a terminal as a project conversation, or the one already opened for it.
#[tauri::command]
async fn pick_up(faber: State<'_, Faber>, project_id: String, session: faber_engine::model::CliSession) -> Res<String> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.pick_up(&project_id, session))
        .await
        .map_err(|_| "error.conversationGone".to_string())?
}

#[tauri::command]
async fn branch(faber: State<'_, Faber>, req: BranchRequest) -> Res<String> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.branch(req)).await.map_err(|_| "error.copy.failed".to_string())?
}

/// "Branch from here" on an answer: a new conversation with every message up to it, where nothing is sent yet.
#[tauri::command]
async fn branch_here(faber: State<'_, Faber>, conversation_id: String, message_id: String) -> Res<String> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.branch_here(&conversation_id, &message_id))
        .await
        .map_err(|_| "error.copy.failed".to_string())?
}

/// The messages of a conversation that have other versions; the other conversations of its family are read from disk.
#[tauri::command]
async fn versions(faber: State<'_, Faber>, id: String) -> Res<Vec<faber_engine::model::Versions>> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.versions(&id)).await.map_err(|_| "error.messageGone".to_string())
}

/// Pages and drawings, with the versions already held in the conversation's saved replies.
#[tauri::command]
async fn visual_answers(
    faber: State<'_, Faber>,
    id: String,
    message_id: Option<String>,
    block: Option<usize>,
) -> Res<Vec<faber_engine::visual::VisualAnswer>> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.visual_answers_for_screen(&id, message_id.as_deref(), block))
        .await
        .map_err(|_| "error.messageGone".to_string())
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

#[tauri::command]
async fn visual_pieces(faber: State<'_, Faber>, id: String, message_id: String) -> Res<Vec<faber_engine::pieces::VisualPiece>> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.visual_pieces(&id, &message_id)).await.map_err(|_| "error.messageGone".to_string())
}

#[tauri::command]
async fn save_piece_value(faber: State<'_, Faber>, id: String, message_id: String, block: usize, value: serde_json::Value) -> Res<()> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.save_piece_value(&id, &message_id, block, value))
        .await
        .map_err(|_| "error.store.write".to_string())?
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
/// The person read the requests held back in a reply: they leave the "needs you" list.
#[tauri::command]
fn parked_seen(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.parked_seen(&id, &turn_id)
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
async fn forget_auto_learned(faber: State<'_, Faber>) -> Res<()> {
    faber.engine.forget_auto_learned()
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
async fn approve_plan(
    faber: State<'_, Faber>,
    id: String,
    turn_id: String,
    request_id: String,
    mode: Option<faber_engine::model::Permissions>,
) -> Res<()> {
    faber.engine.approve_plan(&id, &turn_id, &request_id, mode).await
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
async fn remove(faber: State<'_, Faber>, id: String, discard: Option<bool>) -> Res<()> {
    // `error.copy.unsaved`: its copy holds work found nowhere else, and the screen asks before `discard` (R201).
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.remove_with(&id, discard.unwrap_or(false)))
        .await
        .map_err(|_| "error.copy.failed".to_string())?
}

/// Archives a conversation, or brings an archived one back.
#[tauri::command]
async fn archive(faber: State<'_, Faber>, id: String, on: bool) -> Res<()> {
    faber.engine.archive(&id, on)
}

#[tauri::command]
async fn pin(faber: State<'_, Faber>, id: String, on: bool) -> Res<()> {
    faber.engine.pin(&id, on)
}

#[tauri::command]
async fn move_to_project(faber: State<'_, Faber>, id: String, project_id: String) -> Res<Conversation> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.move_to_project(&id, &project_id).map(Conversation::for_screen))
        .await
        .map_err(|_| "error.store.write".to_string())?
}

#[tauri::command]
async fn keep_going(faber: State<'_, Faber>, id: String, on: bool) -> Res<()> {
    faber.engine.keep_going(&id, on)
}

/// The person has the conversation open on a visible window: the done card and the dot of its last run go.
#[tauri::command]
async fn seen(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.seen(&id)
}

#[tauri::command]
async fn remove_queued(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.remove_queued(&id, &turn_id);
    Ok(())
}

#[tauri::command]
async fn edit_queued(faber: State<'_, Faber>, id: String, turn_id: String, text: String) -> Res<()> {
    faber.engine.edit_queued(&id, &turn_id, &text)
}

#[tauri::command]
async fn send_queued(faber: State<'_, Faber>, id: String, turn_id: String) -> Res<()> {
    faber.engine.send_queued(&id, &turn_id)
}

/// What a subagent wrote between its steps (R161 step 3), for its side chat while it is open. The screen's copy of a message
/// carries only their length, so this is the one place they are read.
#[tauri::command]
async fn child_words(faber: State<'_, Faber>, id: String, call_id: String) -> Res<Vec<faber_engine::model::Words>> {
    Ok(faber.engine.child_words(&id, &call_id))
}

#[tauri::command]
async fn tell_helper(faber: State<'_, Faber>, id: String, call_id: String, text: String) -> Res<()> {
    faber.engine.tell_helper(&id, &call_id, &text)
}

/// `@` in the composer (parity T4): the entries of the working folder that match `query`, at most 50. Git may run, off the async
/// threads. One listing runs at a time, and a query that a newer one replaced while it waited gives up, so typing fast in a
/// big repository never piles up git runs or their output.
#[tauri::command]
async fn mentions(
    faber: State<'_, Faber>,
    id: Option<String>,
    project_id: Option<String>,
    query: String,
) -> Res<Vec<faber_engine::mention::Mention>> {
    // At most one query runs and one waits: a newer one answers the waiting one at once with nothing and takes its place.
    static LISTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    static WAITING: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>> = std::sync::Mutex::new(None);
    let (replaced, gone) = tokio::sync::oneshot::channel();
    if let Some(older) = WAITING.lock().unwrap_or_else(|e| e.into_inner()).replace(replaced) {
        let _ = older.send(());
    }
    let _one = tokio::select! {
        biased;
        _ = gone => return Ok(vec![]),
        one = LISTING.lock() => one,
    };
    let engine = faber.engine.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || engine.mentions(id.as_deref(), project_id.as_deref(), &query))
        .await
        .unwrap_or_default())
}

/// The last advertised commands, read from memory without starting a CLI.
#[tauri::command]
fn commands(faber: State<'_, Faber>, id: Option<String>, vendor: Option<Vendor>) -> Vec<faber_engine::model::AgentCommand> {
    faber.engine.commands(id.as_deref(), vendor)
}
/// "Compact now" in the context meter: the CLI's own compaction of the conversation's context.
#[tauri::command]
fn compact_now(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.compact_now(&id)
}
/// The context meter opened: asks what fills the context; the parts come as an event. False when no CLI can say now.
#[tauri::command]
fn context_parts(faber: State<'_, Faber>, id: String) -> bool {
    faber.engine.context_parts(&id)
}

#[tauri::command]
fn background_commands(faber: State<'_, Faber>) -> faber_engine::background::List {
    faber.engine.background_commands()
}

#[tauri::command]
async fn stop_background(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.stop_background(&id).await.map_err(str::to_string)
}
/// At most one git listing runs and one waits; a newer request replaces the waiting one.
#[tauri::command]
async fn changes(faber: State<'_, Faber>, id: String, cursor: Option<String>) -> Res<faber_engine::changes::ChangeList> {
    static LISTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    static WAITING: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>> = std::sync::Mutex::new(None);
    let (replaced, gone) = tokio::sync::oneshot::channel();
    if let Some(older) = lock(&WAITING).replace(replaced) {
        let _ = older.send(());
    }
    let _one = tokio::select! {
        biased;
        _ = gone => return Ok(faber_engine::changes::ChangeList::default()),
        one = LISTING.lock() => one,
    };
    let engine = faber.engine.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || engine.changes_page(&id, cursor.as_deref())).await.unwrap_or_default())
}

#[tauri::command]
async fn change_patch(faber: State<'_, Faber>, id: String, path: String) -> Res<faber_engine::changes::ChangePatch> {
    // The newest read waiting for each (conversation, file); a newer one answers the older with nothing.
    type Waiting = std::collections::HashMap<(String, String), tokio::sync::oneshot::Sender<()>>;
    static READING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    static WAITING: std::sync::LazyLock<std::sync::Mutex<Waiting>> = std::sync::LazyLock::new(|| std::sync::Mutex::new(Waiting::new()));
    let key = (id.clone(), path.clone());
    let (replaced, gone) = tokio::sync::oneshot::channel();
    if let Some(older) = lock(&WAITING).insert(key.clone(), replaced) {
        let _ = older.send(());
    }
    let _one = tokio::select! {
        biased;
        _ = gone => return Ok(faber_engine::changes::ChangePatch::default()),
        one = READING.lock() => one,
    };
    lock(&WAITING).remove(&key);
    let engine = faber.engine.clone();
    Ok(tauri::async_runtime::spawn_blocking(move || engine.change_patch(&id, &path)).await.unwrap_or_default())
}

#[tauri::command]
async fn rewind_points(faber: State<'_, Faber>, id: String) -> Res<Vec<faber_engine::rewind::PointInfo>> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_points(&id)).await.map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_redos(faber: State<'_, Faber>, id: String) -> Res<Vec<faber_engine::rewind::RedoInfo>> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_redos(&id)).await.map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_redo_preview(faber: State<'_, Faber>, request: faber_engine::rewind::RedoRequest) -> Res<faber_engine::rewind::Preview> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_redo_preview(request))
        .await
        .map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_recover(faber: State<'_, Faber>, id: String) -> Res<()> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_recover(&id)).await.map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_preview(faber: State<'_, Faber>, request: faber_engine::rewind::RestoreRequest) -> Res<faber_engine::rewind::Preview> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_preview(request)).await.map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_restore(
    faber: State<'_, Faber>,
    request: faber_engine::rewind::RestoreRequest,
) -> Res<faber_engine::rewind::RestoreResult> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_restore(request)).await.map_err(|_| "error.rewind.storage".to_string())?
}

#[tauri::command]
async fn rewind_redo(faber: State<'_, Faber>, request: faber_engine::rewind::RedoRequest) -> Res<faber_engine::rewind::RestoreResult> {
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.rewind_redo(request)).await.map_err(|_| "error.rewind.storage".to_string())?
}

/// A quick question on the side of a conversation (R250); `model` is the tab's own pick, or the conversation's.
#[tauri::command]
async fn ask_aside(faber: State<'_, Faber>, id: String, question: String, model: Option<String>) -> Res<String> {
    faber.engine.ask_aside(&id, &question, model)
}

#[tauri::command]
async fn stop_aside(faber: State<'_, Faber>, aside_id: String) -> Res<()> {
    faber.engine.stop_aside(&aside_id);
    Ok(())
}

#[tauri::command]
async fn clear_asides(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.clear_asides(&id);
    Ok(())
}

#[tauri::command]
async fn asides(faber: State<'_, Faber>, id: String) -> Res<Vec<faber_engine::engine::Aside>> {
    Ok(faber.engine.asides(&id))
}

/// The person stops following the pull request a project conversation follows (R200).
#[tauri::command]
async fn unfollow_pr(faber: State<'_, Faber>, id: String) -> Res<()> {
    faber.engine.unfollow_pr(&id);
    Ok(())
}

#[tauri::command]
async fn remove_project(faber: State<'_, Faber>, id: String) -> Res<()> {
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
    let settings = autostart::save(autostart.as_ref(), patch.autostart, || faber.engine.save_settings(patch))?;
    let settings = shown_settings(settings, autostart.as_ref());
    lock(&faber.awake).set_enabled(settings.keep_awake());
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
async fn export_conversation(
    app: AppHandle,
    faber: State<'_, Faber>,
    id: String,
    words: faber_engine::export::Words,
) -> Result<bool, Failure> {
    use std::io::Write;
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (title, markdown) = engine.export_conversation(&id, &words).map_err(Failure::key)?;
        let stem: String = title.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(120).collect();
        let stem = if stem.trim().is_empty() { "Faber" } else { stem.trim() };
        let mut dialog = app.dialog().file().add_filter("Markdown", &["md"]).set_file_name(format!("{stem}.md"));
        if let Some(w) = main_window(&app) {
            dialog = dialog.set_parent(&w);
        }
        let Some(target) = dialog.blocking_save_file() else { return Ok(false) };
        let target = target.into_path().map_err(|_| Failure::key("error.export.write"))?;
        look::replace_file(&target, |file| file.write_all(markdown.as_bytes())).map_err(|_| Failure::key("error.export.write"))?;
        Ok(true)
    })
    .await
    .map_err(|_| Failure::key("error.export.write"))?
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
        return faber.engine.chat_folder(id.as_deref(), None).map(Conversation::for_screen).map(Some);
    }
    // A folder taken from the recents needs no dialog; the engine checks it like any chosen folder.
    if let Some(path) = path {
        return faber.engine.chat_folder(id.as_deref(), Some(Path::new(&path))).map(Conversation::for_screen).map(Some);
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
    faber.engine.chat_folder(id.as_deref(), Some(&path)).map(Conversation::for_screen).map(Some)
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
    let Some(source) = faber.engine.folder_file(&id, &path) else { return Ok(()) };
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

/// A file a reply names, read only around its line (parity P2), from inside the conversation's folder.
#[tauri::command]
async fn read_file_at(faber: State<'_, Faber>, id: String, path: String) -> Res<faber_engine::fileview::FileView> {
    let engine = faber.engine.clone();
    let read = tauri::async_runtime::spawn_blocking(move || engine.read_file_at(&id, &path)).await;
    Ok(read.unwrap_or_else(|_| faber_engine::fileview::FileView::failed("error.file.read")))
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

/// The person loads an address they typed in the tab in front of the browser pane.
#[tauri::command]
async fn site_go(app: AppHandle, id: String, url: String) -> bool {
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

/// A conversation's tabs in Faber's browser, kept in its file, so the pane opens the front one again.
#[tauri::command]
async fn site_tabs(faber: State<'_, Faber>, id: String) -> Res<Option<faber_engine::browser::tabs::Tabs>> {
    // Reading a conversation's file waits on the engine, never on the window's thread.
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.browser_tabs(&id)).await.map_err(|_| "error.conversationGone".to_string())
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

/// The picture the browser took for a call, as a data URL, so the page never reads the disk itself.
#[tauri::command]
async fn site_picture(faber: State<'_, Faber>, id: String, call: String) -> Res<Option<String>> {
    let Some(source) = faber.engine.site_picture(&id, &call) else { return Ok(None) };
    if !std::fs::metadata(&source).is_ok_and(|m| m.len() <= 2_000_000) {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(&source) else { return Ok(None) };
    Ok(Some(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
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

/// Settings > Projects: whether each new conversation of this project works in a separate copy (R201).
#[tauri::command]
async fn project_copies(faber: State<'_, Faber>, project_id: String, on: bool) -> Res<()> {
    project_commands::id(&project_id)?;
    let project = faber.engine.project(&project_id)?;
    let mut updated = project.clone();
    updated.separate_copies = on;
    faber.engine.update_project(&project, &updated)
}

/// Settings > Projects and the project page's Goals fold: a project's standing goals, the switch and the day's limit.
#[tauri::command]
async fn project_goals(faber: State<'_, Faber>, project_id: String) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    faber.engine.project_goals(&project_id)
}

/// The project's switch for starting fixes by itself. Off until the person turns it on.
#[tauri::command]
async fn project_goals_auto(faber: State<'_, Faber>, project_id: String, on: bool) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    faber.engine.set_project_goals(&project_id, on)
}

/// Adds a goal: the person's words and one of the project's confirmed checks.
#[tauri::command]
async fn project_goal_add(
    faber: State<'_, Faber>,
    project_id: String,
    words: String,
    check: String,
) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    project_commands::text(&words)?;
    project_commands::text(&check)?;
    faber.engine.add_project_goal(&project_id, &words, &check)
}

/// Adds a goal that breaks when the installed Faber's crash file gets a new line: Faber's own repository only.
#[tauri::command]
async fn project_goal_add_crashes(faber: State<'_, Faber>, project_id: String, words: String) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    project_commands::text(&words)?;
    faber.engine.add_project_crash_goal(&project_id, &words)
}

#[tauri::command]
async fn project_goal_remove(faber: State<'_, Faber>, project_id: String, goal_id: String) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    project_commands::id(&goal_id)?;
    faber.engine.remove_project_goal(&project_id, &goal_id)
}

/// A paused goal, or one that needs the person, is checked again.
#[tauri::command]
async fn project_goal_resume(faber: State<'_, Faber>, project_id: String, goal_id: String) -> Res<faber_engine::project::goals::View> {
    project_commands::id(&project_id)?;
    project_commands::id(&goal_id)?;
    faber.engine.resume_project_goal(&project_id, &goal_id)
}

#[tauri::command]
async fn project_proof(faber: State<'_, Faber>, project_id: String) -> Res<checks::ProofView> {
    project_commands::id(&project_id)?;
    Ok(checks::view(&faber.engine.project(&project_id)?))
}

#[tauri::command]
async fn project_memory(faber: State<'_, Faber>, project_id: String) -> Res<faber_engine::project::memory::MemoryList> {
    project_commands::id(&project_id)?;
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.project_memory(&project_id)).await.map_err(|_| "error.memory.read".to_string())?
}

#[tauri::command]
async fn edit_project_memory(faber: State<'_, Faber>, project_id: String, edit: faber_engine::project::memory::Edit) -> Res<()> {
    project_commands::id(&project_id)?;
    let engine = faber.engine.clone();
    tauri::async_runtime::spawn_blocking(move || engine.edit_project_memory(&project_id, &edit))
        .await
        .map_err(|_| "error.memory.write".to_string())?
}

/// The owner confirms the commands Faber checks a project's work with (R137): the project's declared list, or the edited one.
#[tauri::command]
async fn project_proof_confirm(
    faber: State<'_, Faber>,
    project_id: String,
    commands: Vec<String>,
    setup: Vec<String>,
    declared: Vec<String>,
) -> Res<checks::ProofView> {
    project_commands::id(&project_id)?;
    for command in commands.iter().chain(&setup).chain(&declared) {
        project_commands::text(command)?;
    }
    let before = faber.engine.project(&project_id)?;
    let after = checks::confirm(&before, &commands, &setup, &declared).map_err(str::to_string)?;
    faber.engine.update_project(&before, &after)?;
    Ok(checks::view(&after))
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

/// Test builds only: the checks page says which check starts, and the shell keeps that note next to the report
/// (`FABER_CHECKS_OUT` with the extension `progress`). The runner resets its wait whenever the check in it changes, so a run that
/// is slow but alive never times out while a page stuck inside one check still does.
#[cfg(feature = "test-hooks")]
#[tauri::command]
fn checks_progress(note: String) -> Res<()> {
    let out = PathBuf::from(std::env::var_os("FABER_CHECKS_OUT").ok_or("FABER_CHECKS_OUT is not set")?);
    // Written whole and renamed into place: the runner polls the file, and half a note would read as progress.
    let part = out.with_extension("progress.part");
    std::fs::write(&part, note).map_err(|e| e.to_string())?;
    std::fs::rename(&part, out.with_extension("progress")).map_err(|e| e.to_string())
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
    let faber = app.state::<Faber>();
    // First, since a Windows sign-out gives the shutdown below only a few seconds: Quit, a session end and the stop before an
    // update installs all come here, and none of them counts as a crash at the next start.
    faber_engine::crash::end_run(&faber.data_dir);
    terminal::terminate_hidden();
    lock(&faber.awake).shutdown();
    faber.engine.shutdown();
}

/// Where a real run keeps its crash record (a local file, never sent): none for a test build, a hidden run or one with its own
/// data folder. The folder is the one `data_dir` gives later, which Tauri reads from the same `dirs` call.
fn crash_record_dir(is_test: bool) -> Option<PathBuf> {
    if is_test { None } else { dirs::config_dir().map(|config| default_data_dir(&config)) }
}

/// The installed Faber's data folder, whose crash file a standing goal of Faber's own repository reads (never writes). It is the
/// default folder whatever `FABER_DATA_DIR` says, so a development run with a folder of its own still reads the installed
/// one's. None for a test build and a hidden run, which never look at a real one.
fn installed_data_dir(hidden: bool) -> Option<PathBuf> {
    if cfg!(feature = "test-hooks") || hidden { None } else { dirs::config_dir().map(|config| default_data_dir(&config)) }
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
    let autostart_launch = !creates_window(std::env::args());
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");
    let is_test = cfg!(feature = "test-hooks") || hidden || std::env::var_os("FABER_DATA_DIR").is_some();
    // From the first line on, so a panic before the app is set up is written too.
    if let Some(dir) = crash_record_dir(is_test) {
        faber_engine::crash::install_hook(dir, env!("CARGO_PKG_VERSION"), env!("FABER_COMMIT"));
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
                site_tabs,
                site_go,
                site_tab,
                site_clear,
                site_history,
                site_picture,
                cookie_profiles,
                cookie_sites,
                cookie_import,
                cookie_held,
                cookie_forget,
                state,
                open,
                create_chat_group,
                save_chat_group,
                add_chat_group_files,
                remove_chat_group_file,
                remove_chat_group,
                move_chat_to_group,
                new_group_chat,
                search_messages,
                prompt_history,
                send,
                branch,
                cli_sessions,
                pick_up,
                branch_here,
                versions,
                visual_answers,
                #[cfg(feature = "test-hooks")]
                visual_policy,
                visual_open,
                visual_page,
                visual_close,
                visual_pieces,
                save_piece_value,
                warm,
                set_clock,
                quota_attention,
                answer_plan,
                approve_credits,
                move_turn,
                wait_turn,
                parked_seen,
                route_draft,
                forget_auto_learned,
                interrupt,
                keep_going,
                seen,
                archive,
                pin,
                move_to_project,
                answer,
                answer_question,
                approve_plan,
                rename,
                remove,
                remove_queued,
                edit_queued,
                send_queued,
                tell_helper,
                child_words,
                mentions,
                commands,
                compact_now,
                context_parts,
                background_commands,
                stop_background,
                changes,
                change_patch,
                read_file_at,
                rewind_points,
                rewind_redos,
                rewind_redo_preview,
                rewind_recover,
                rewind_preview,
                rewind_restore,
                rewind_redo,
                ask_aside,
                stop_aside,
                clear_asides,
                asides,
                unfollow_pr,
                remove_project,
                preview_look,
                save_settings,
                save_theme,
                export_conversation,
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
                project_copies,
                project_goals,
                project_goals_auto,
                project_goal_add,
                project_goal_add_crashes,
                project_goal_remove,
                project_goal_resume,
                project_proof,
                project_memory,
                edit_project_memory,
                project_proof_confirm,
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
                tools::tools_list,
                tools::tools_inventory,
                tools::mcp_servers,
                tools::set_mcp_server,
                tools::tools_install,
                tools::tools_remove,
                tools::tools_set_enabled,
                tools::tools_restore,
                tools::tools_clear_removed,
                tools::tools_in_use,
                #[cfg(feature = "test-hooks")]
                report_checks,
                #[cfg(feature = "test-hooks")]
                checks_progress
            ];
            // Only the screen calls Faber: a web page in the browser pane (R161 step 7) is refused before any command runs.
            move |invoke: tauri::ipc::Invoke| {
                if invoke.message.webview().label() != "main" {
                    invoke.resolver.reject("Faber answers only its own screen");
                    return true;
                }
                handler(invoke)
            }
        })
        .setup(move |app| {
            app.manage(visual_page::Pages::default());
            let data_dir = data_dir(app.handle())?;
            std::fs::create_dir_all(&data_dir)?;
            app.manage(tray::Tray::default());
            let clis = detect_clis();
            let handle = app.handle().clone();
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
            let awake_for_emit = awake.clone();
            let autostart_for_emit = autostart_keeper.clone();
            let feed = Arc::new(Mutex::new(feed::Feed::default()));
            let feed_for_emit = feed.clone();
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
                if let AppEvent::Halted { title, why, vendor, .. } = &e
                    && !hidden
                {
                    let (app, title, why, vendor) = (handle.clone(), title.clone(), why.clone(), *vendor);
                    tauri::async_runtime::spawn_blocking(move || tray::on_halted(&app, &title, &why, vendor));
                }
                let event = lock(&feed_for_emit).shrink(event_for_screen(e, autostart_for_emit.as_ref()));
                let _ = handle.emit("faber:event", event);
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
            if let Some(dir) = installed_data_dir(hidden) {
                engine.set_installed_data(dir);
            }
            // The test build's agent installs come from the loopback server of FABER_SETUP_SOURCE, as `tools_install` does: they never reach the network.
            #[cfg(feature = "test-hooks")]
            if let Ok(source) = std::env::var("FABER_SETUP_SOURCE") {
                engine.set_setup_source(&source);
            }
            // Project conversations work in copies of their own (R201), made with the native git only, never a shell.
            engine.set_git(Arc::new(|| {
                faber_engine::project::git::Process::find()
                    .ok()
                    .map(|git| Box::new(git) as Box<dyn faber_engine::project::git::Executor + Send>)
            }));
            // A project's checks, which its agent runs with Faber's `verify` tool (R190): no process until it is called. While a
            // game is in front (R191) no new check starts and a running one is held to a tenth of the processor.
            // The test build never waits for one, as the screen's own reading (`game_in_front` above) never changes there.
            let guard: faber_engine::verify::Guard = Arc::new(|| !cfg!(feature = "test-hooks") && crate::tray::game_in_front());
            // Its results journal (failed first, flaky told) lives in the data folder.
            let journal = data_dir.join("verify").join("journal.jsonl");
            engine.set_verifier(faber_engine::verify::Verifier::real(Some(guard), Some(journal)));
            {
                // Once, at start: copies of deleted conversations, and merged clean ones, go.
                let engine = engine.clone();
                tauri::async_runtime::spawn_blocking(move || engine.sweep_copies());
            }
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
                awake,
                autostart: Mutex::new(autostart_keeper),
                feed,
            });
            // R161 step 7: Faber's own browser, for a project's agent. Windows only for now: its page is driven through WebView2.
            #[cfg(windows)]
            {
                let pane = pane::Pane::new(app.handle().clone());
                app.state::<Faber>().engine.set_browser(Arc::new(pane::PaneBrowser(pane.clone())));
                app.manage(pane);
            }
            app.state::<Faber>().engine.watch_accounts();
            app.state::<Faber>().engine.follow_open_prs();
            app.state::<Faber>().engine.watch_archive();
            app.state::<Faber>().engine.detect_claude_flags();
            // Work that went on by itself when Faber last closed goes on again, once the accounts have had a moment to be read.
            app.state::<Faber>().engine.resume_keep_going();
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
        assert!(main.contains("ifletSome(dir)=crash_record_dir(is_test){faber_engine::crash::install_hook(dir,"), "the hook is gated");
        assert!(main.contains("ifcrash_record_dir(is_test).is_some(){faber_engine::crash::start_run("), "the marker is gated");
    }

    #[test]
    fn quit_the_end_of_a_session_and_the_stop_before_an_update_all_end_the_run_cleanly() {
        // `shut_down` is what `RunEvent::Exit` calls, and tao sends that event on a Windows sign-out, restart or shutdown.
        assert!(body_of("pub(crate) fn shut_down(").contains("faber_engine::crash::end_run(&faber.data_dir);"));
        assert!(body_of("fn main() {").contains("RunEvent::Exit=event{shut_down(app);"));
    }
}
