//! The commands Faber answers, in one table (R459). The window calls them, and so do the command line and the tests; each
//! front end only carries a name and its arguments here and the answer back, so a rule lives once, in Rust, whichever shell
//! draws the screen. A command's arguments are a JSON object with camelCase keys, as the screen's `ui/api.ts` sends them,
//! and a failure is an error key (`error.*`), never a sentence.
//!
//! What only a window can do (dialogs, revealing a file, the tray, updating) is a method of [`Shell`], which the front end
//! implements and a front end without a window leaves at its default, `error.host.noWindow`. The few commands that are
//! the window's whole business (the browser pane, its cookies) are listed here as [`Kind::Window`] so every caller sees the
//! whole list, and the front end answers them itself.

#[macro_use]
mod macros;
pub mod autostart;
pub mod awake;
mod failure;
mod feed;
pub mod folder;
pub mod look;
pub mod pane;
mod project_commands;
mod queues;
pub mod reach;
pub mod setup;
pub mod sidecar;
mod table;
pub mod terminal;
mod tools;
pub mod tray;
pub mod views;
pub mod visual;
pub mod wire;

pub use failure::Failure;
pub use feed::Feed;
pub use reach::{Caller, Reach};

use faber_engine::codex::{ModelInfo, list_codex_models, read_codex_limits};
use faber_engine::engine::Emit;
use faber_engine::model::{Settings, Vendor};
use faber_engine::proc::{Spawner, real_spawner};
use faber_engine::{AppEvent, Engine, EngineOptions};
use serde::Serialize;
use serde_json::{Value, json};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;
use tokio::runtime::Handle;
use tokio::sync::broadcast;

pub type Res<T> = Result<T, String>;

/// What every window-only answer says when the front end has no window.
pub const NO_WINDOW: &str = "error.host.noWindow";

/// What a command does, for callers that decide by it: the command line refuses [`Kind::StartsTurn`] past a set depth of
/// agents calling agents, and lists [`Kind::Read`] ones as safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Changes nothing a person would miss: reading, or telling the engine what the screen shows.
    Read,
    Write,
    /// Starts an agent's turn, which spends the person's quota.
    StartsTurn,
    /// Needs a window; the front end answers it.
    Window,
}

/// One command as the table writes it.
pub(crate) struct Entry {
    pub name: &'static str,
    pub args: &'static [(&'static str, &'static str)],
    pub kind: Kind,
    pub reach: Reach,
    /// The doc comment's lines, each with its leading space.
    pub doc: &'static [&'static str],
}

/// One command as callers see it. `args` are the JSON keys and the Rust type each takes, as written in the table; `reach`
/// says who may send it (`free`, `own`, `ask`, `person`, `screen`, see [`Reach`]) and `conversation` where its arguments
/// name the conversation it acts on (a JSON pointer, for `own`); `about` is what it does, in a sentence or two.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub name: &'static str,
    pub args: Vec<(String, &'static str)>,
    pub kind: Kind,
    pub reach: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<&'static str>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub about: String,
}

/// Names the table never holds: a connection answers them itself (the command line's greeting and its event stream).
pub const RESERVED: [&str; 2] = ["hello", "subscribe"];

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Why a dialog gave no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickError {
    /// There is no window to show it in.
    NoWindow,
    /// The person picked something that is not on this computer (a share, a cloud placeholder).
    NotLocal,
}

/// What a dialog answers: the path picked, none when the person cancelled, or why there was no answer.
pub type Picked = Result<Option<PathBuf>, PickError>;

/// The kind of file a dialog offers: its name and the extensions, without the dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
}

/// The part of the front end the host calls back. One with no window (the command line) keeps the defaults.
pub trait Shell: Send + Sync + 'static {
    /// Hears every engine event first, before the screen does (the tray, keeping the PC awake). The engine calls it with its
    /// lock held, so anything that may wait for that lock goes to another thread.
    fn heard(&self, _event: &AppEvent) {}
    /// The settings as the screen shows them, with what only the shell knows filled in, such as whether Faber starts with the
    /// system.
    fn shown_settings(&self, settings: Settings) -> Settings {
        shown(settings)
    }
    /// Sends one event, as the screen receives it, to the window if there is one.
    fn emit(&self, _event: &Value) {}
    /// The version of the program that runs; the screen shows it.
    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
    /// Stores settings through `persist`, together with whatever only the shell keeps for them (registering to start with the
    /// system, which is undone if `persist` fails). `autostart` is the patch's switch for it, if it carries one.
    fn persist_settings(&self, _autostart: Option<bool>, persist: &mut dyn FnMut() -> Res<Settings>) -> Res<Settings> {
        persist()
    }
    /// Settings were saved: the shell applies what it keeps itself (keeping the PC awake).
    fn settings_saved(&self, _settings: &Settings) {}
    /// A folder dialog.
    fn pick_folder(&self) -> BoxFuture<'_, Picked> {
        Box::pin(async { Err(PickError::NoWindow) })
    }
    /// A dialog to pick one file.
    fn pick_file(&self, _filter: Option<Filter>) -> BoxFuture<'_, Picked> {
        Box::pin(async { Err(PickError::NoWindow) })
    }
    /// A dialog to pick where a file is saved, offering `name`.
    fn pick_save(&self, _filter: Option<Filter>, _name: String) -> BoxFuture<'_, Picked> {
        Box::pin(async { Err(PickError::NoWindow) })
    }
    /// Shows a file or folder in the file manager, selected and never opened.
    fn reveal(&self, _path: PathBuf) -> Res<()> {
        Err(NO_WINDOW.into())
    }
    /// Opens a web address in the person's own browser, never inside Faber. The address is already checked to be `http(s)`.
    fn open_link(&self, _url: &str) -> Res<()> {
        Err(NO_WINDOW.into())
    }
    /// The words the shell shows by itself (tray menu, tooltip, notification), as the screen sends them.
    fn set_texts(&self, _texts: Value) -> Res<()> {
        Err(NO_WINDOW.into())
    }
    /// Where the update is, as the screen shows it. A shell that never updates itself has nothing to report.
    fn update_status(&self) -> Value {
        json!({ "phase": "none" })
    }
    /// "Update ready, restart": installs the kept update now.
    fn update_restart(&self) -> BoxFuture<'_, Res<()>> {
        Box::pin(async { Err(NO_WINDOW.into()) })
    }
    /// Held for the whole of a CLI or catalog install: while it lives, no update ends the process. Refused when one is
    /// already closing the program.
    fn hold_install(&self) -> Res<Box<dyn Send>> {
        Ok(Box::new(()))
    }
    /// The screen saw the first input of the session.
    fn input_seen(&self) {}
    /// Whether any field on the screen holds something typed or attached and not sent.
    fn draft_dirty(&self, _dirty: bool) {}
    /// Whether a game or another full-screen app is in front.
    fn game_in_front(&self) -> bool {
        false
    }
    /// Opens a lease in the window's page server for a visual answer.
    fn visual_open(&self) -> Res<String> {
        Err(NO_WINDOW.into())
    }
    /// Prepares one single-use visual page under lease `id`.
    fn visual_page(&self, _id: &str, _revision: u64, _document: String, _live: bool) -> Res<visual::Prepared> {
        Err(NO_WINDOW.into())
    }
    /// Closes lease `id` and drops any unconsumed visual page.
    fn visual_close(&self, _id: &str) -> Res<()> {
        Err(NO_WINDOW.into())
    }
    /// Consumes a prepared visual page by its URL.
    fn visual_document(&self, _path: &str) -> Res<visual::Document> {
        Err(NO_WINDOW.into())
    }
    /// Whether the shell's native network filter for live visual pages is active.
    #[cfg(feature = "test-hooks")]
    fn visual_policy(&self, _live: bool) -> Res<bool> {
        Err(NO_WINDOW.into())
    }
    /// The program is closing: release what the shell holds, such as the request to keep the PC awake.
    fn shut_down(&self) {}
}

/// The settings as the screen shows them before the shell adds what only it knows.
pub fn shown(settings: Settings) -> Settings {
    let mut settings = settings.shown();
    settings.appearance = look::usable(settings.appearance.take());
    settings
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Under the config folder (Roaming on Windows), never next to a program: the per-user installer's own default folder is
/// `%LOCALAPPDATA%\Faber`, so the data must not be there.
pub fn default_data_dir(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("Faber")
}

/// The data folder: `FABER_DATA_DIR` when set (tests, a second copy), else the default under the config folder.
pub fn data_dir_from_env() -> Option<PathBuf> {
    match std::env::var_os("FABER_DATA_DIR") {
        Some(dir) => Some(PathBuf::from(dir)),
        None => dirs::config_dir().map(|config| default_data_dir(&config)),
    }
}

/// The installed Faber's data folder, whose crash file a standing goal of Faber's own repository reads (never writes). It is
/// the default folder whatever `FABER_DATA_DIR` says, so a development run with a folder of its own still reads the installed
/// one's. None for a test build and a hidden run, which never look at a real one.
pub fn installed_data_dir(hidden: bool) -> Option<PathBuf> {
    if cfg!(feature = "test-hooks") || hidden { None } else { dirs::config_dir().map(|config| default_data_dir(&config)) }
}

/// Where an event goes once it is not the engine's: to the window and to every subscriber.
#[derive(Clone)]
struct Voice {
    shell: Arc<dyn Shell>,
    events: broadcast::Sender<Value>,
}

impl Voice {
    fn say(&self, event: &impl Serialize) {
        if let Ok(value) = serde_json::to_value(event) {
            self.shell.emit(&value);
            // With nobody subscribed this keeps nothing.
            let _ = self.events.send(value);
        }
    }
}

/// What the engine's events pass through before the host exists: the engine is built with [`Wiring::emitter`], then handed
/// to [`Wiring::host`].
pub struct Wiring {
    shell: Arc<dyn Shell>,
    feed: Arc<Mutex<Feed>>,
    events: broadcast::Sender<Value>,
}

/// Events kept for a subscriber that reads late. One that falls further behind is told it lagged and should read `state` again.
const EVENTS_KEPT: usize = 256;

impl Wiring {
    pub fn new(shell: Arc<dyn Shell>) -> Self {
        Self { shell, feed: Arc::default(), events: broadcast::channel(EVENTS_KEPT).0 }
    }

    /// The engine's event sink: the shell hears the event, then the screen and every subscriber get it as the screen does
    /// (a streaming reply carries only its changed steps, `feed.rs`).
    pub fn emitter(&self) -> Emit {
        let (shell, feed, events) = (self.shell.clone(), self.feed.clone(), self.events.clone());
        Arc::new(move |event: AppEvent| {
            shell.heard(&event);
            let event = match event {
                AppEvent::Settings { settings } => AppEvent::Settings { settings: shell.shown_settings(settings) },
                event => event,
            };
            let value = lock(&feed).shrink(event);
            shell.emit(&value);
            // With nobody subscribed this keeps nothing.
            let _ = events.send(value);
        })
    }

    /// The host around `engine`. `hidden` is a test or automated run: no visible dialog may open in it.
    pub fn host(self, engine: Engine, data_dir: PathBuf, hidden: bool, runtime: Handle) -> Host {
        Host {
            engine,
            feed: self.feed,
            voice: Voice { shell: self.shell.clone(), events: self.events },
            shell: self.shell,
            data_dir,
            hidden,
            runtime,
            models: Arc::default(),
            loading_models: Arc::default(),
        }
    }
}

/// The engine and what the commands share around it.
pub struct Host {
    pub engine: Engine,
    /// What the screen last heard of each streaming reply (`feed.rs`).
    pub feed: Arc<Mutex<Feed>>,
    pub shell: Arc<dyn Shell>,
    pub data_dir: PathBuf,
    /// A test or automated run: it never opens a visible dialog.
    pub hidden: bool,
    /// Where the host's own background work runs.
    pub runtime: Handle,
    /// ChatGPT models this account can use; empty until the Codex CLI answers.
    models: Arc<Mutex<Vec<ModelInfo>>>,
    /// Set while Codex is being asked for its limits and models, which starts two Codex processes: one asking at a time is
    /// enough.
    loading_models: Arc<AtomicBool>,
    voice: Voice,
}

impl Host {
    /// Every event from now on, as the screen receives it. A reply already streaming arrives as its changed steps only
    /// (`feed.rs`), so a subscriber reads `open` (or `state`) first, and again after it lags.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.voice.events.subscribe()
    }

    /// Tells the window and every subscriber something that did not come from the engine (a setup step, the model list).
    pub(crate) fn announce(&self, event: &impl Serialize) {
        self.voice.say(event);
    }

    /// The whole table, built once.
    pub fn commands() -> &'static [CommandInfo] {
        static LIST: OnceLock<Vec<CommandInfo>> = OnceLock::new();
        LIST.get_or_init(|| {
            table::TABLE
                .iter()
                .map(|e| CommandInfo {
                    name: e.name,
                    args: e.args.iter().map(|(arg, ty)| (camel(arg), *ty)).collect(),
                    kind: e.kind,
                    reach: e.reach.word(),
                    conversation: if let Reach::Own(pointer) = e.reach { Some(pointer) } else { None },
                    about: e.doc.iter().map(|line| line.trim()).collect::<Vec<_>>().join(" "),
                })
                .collect()
        })
    }

    /// The commands anything but the screen may send: what the command line's help and the AI's tools list.
    pub fn exported() -> impl Iterator<Item = &'static CommandInfo> {
        Host::commands().iter().filter(|c| c.reach != Reach::Screen.word())
    }

    pub fn kind(command: &str) -> Option<Kind> {
        table::TABLE.iter().find(|e| e.name == command).map(|e| e.kind)
    }

    pub fn reach(command: &str) -> Option<Reach> {
        table::TABLE.iter().find(|e| e.name == command).map(|e| e.reach)
    }

    /// Runs one command for `caller`, after the rule of who may send it ([`reach::check`]): the screen's own `dispatch` is
    /// this for [`Caller::Screen`]. A name not in the table is `error.host.unknownCommand` whoever asks.
    pub async fn dispatch_as(&self, caller: &Caller, command: &str, args: Value) -> Result<Value, Failure> {
        if *caller != Caller::Screen {
            let reach = Host::reach(command).ok_or_else(|| Failure::key("error.host.unknownCommand"))?;
            reach::check(caller, command, reach, &args, |id| self.family_of(id)).map_err(Failure::key)?;
        }
        self.dispatch(command, args).await
    }

    /// The first conversation of `id`'s family (who started whom), None for a conversation that does not exist.
    pub fn family_of(&self, id: &str) -> Option<String> {
        let starters: std::collections::HashMap<String, Option<String>> =
            self.engine.state().conversations.into_iter().map(|c| (c.id, c.started_by)).collect();
        reach::root(id, |at| starters.get(at).cloned())
    }

    /// Asks Codex for this account's models in the background; the picker fills in when it answers. A call while an asking is
    /// under way does nothing: that asking answers for both.
    pub fn load_models(&self, codex: faber_engine::cli::Cli, cwd: PathBuf) {
        struct Done(Arc<AtomicBool>);
        impl Drop for Done {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        if self.loading_models.swap(true, Ordering::AcqRel) {
            return;
        }
        let done = Done(self.loading_models.clone());
        let real = real_spawner();
        let prefix = codex.prefix.clone();
        let spawn: Spawner = Arc::new(move |bin, args, cwd| real(bin, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd));
        let (engine, models, voice) = (self.engine.clone(), self.models.clone(), self.voice.clone());
        self.runtime.spawn(async move {
            let _done = done;
            let events = read_codex_limits(&codex.command, &cwd, cfg!(windows), spawn.clone(), Duration::from_secs(20)).await;
            engine.note_account_events(Vendor::Codex, events);
            let list = list_codex_models(&codex.command, &cwd, cfg!(windows), spawn, Duration::from_secs(20)).await;
            let recommended = list.iter().find(|m| m.is_default).map(|m| m.id.clone());
            engine.set_codex_models(list.iter().map(|m| m.id.clone()).collect(), recommended);
            *lock(&models) = list;
            voice.say(&AppEvent::List);
        });
    }

    /// What Quit does, and what the idle update install does before the installer ends the process. First the crash marker,
    /// since a Windows sign-out gives the shutdown below only a few seconds: Quit, a session end and the stop before an update
    /// installs all come here, and none of them counts as a crash at the next start.
    pub fn shut_down(&self) {
        faber_engine::crash::end_run(&self.data_dir);
        terminal::terminate_hidden();
        self.shell.shut_down();
        self.engine.shutdown();
    }
}

/// `turn_id` as the screen sends it: `turnId`.
fn camel(snake: &str) -> String {
    let mut out = String::with_capacity(snake.len());
    let mut upper = false;
    for c in snake.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// What [`start`] needs from the front end that is not the engine's own default.
pub struct StartOptions {
    pub data_dir: PathBuf,
    /// A test or automated run: no visible dialog, and the engine reads no real installed Faber.
    pub hidden: bool,
    /// Where the engine's sessions and timers run.
    pub runtime: Handle,
    /// The installed Faber's data folder ([`installed_data_dir`]), None where it is never read.
    pub installed_data_dir: Option<PathBuf>,
    /// Held back from a project's checks while a game is in front: no new check starts and a running one slows down.
    pub game_guard: Option<faber_engine::verify::Guard>,
    /// Faber's own browser for a project's agent, when the front end has one.
    pub browser: Option<Arc<dyn faber_engine::browser::Browser>>,
}

/// Builds the engine and everything around it, in the order Faber has always started it, and answers the host every front
/// end calls. `shell` hears the engine's events from the first one.
pub fn start(options: StartOptions, shell: Arc<dyn Shell>) -> std::io::Result<Arc<Host>> {
    let StartOptions { data_dir, hidden, runtime, installed_data_dir, game_guard, browser } = options;
    let _data_directory = faber_engine::workspace::DataDirectory::open(&data_dir)?;
    let clis = setup::detect_clis();
    let wiring = Wiring::new(shell);
    let opts = EngineOptions {
        data_dir: data_dir.clone(),
        // Private Chat work lives beside the data folder, never inside it (#383).
        #[cfg(windows)]
        chat_dir: {
            let mut name = data_dir.file_name().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?.to_os_string();
            name.push("-chat-work");
            data_dir.with_file_name(name)
        },
        clis: clis.clone(),
        spawn: real_spawner(),
        windows: cfg!(windows),
        claude_env_billed: faber_engine::signals::claude_env_billed(std::env::vars_os().map(|(name, _)| name)),
        max_warm: 3,
        idle: Duration::from_secs(120),
        runtime: runtime.clone(),
        ask: None,
    };
    let engine = Engine::new(opts, wiring.emitter())?;
    engine.measure_unix_worker_memory();
    if let Some(dir) = installed_data_dir {
        engine.set_installed_data(dir);
    }
    // The test build's agent installs come from the loopback server of FABER_SETUP_SOURCE, as `tools_install` does: they never reach the network.
    #[cfg(feature = "test-hooks")]
    if let Ok(source) = std::env::var("FABER_SETUP_SOURCE") {
        engine.set_setup_source(&source);
    }
    // Project conversations work in copies of their own (R201), made with the native git only, never a shell.
    engine.set_git(Arc::new(|| {
        faber_engine::project::git::Process::find().ok().map(|git| Box::new(git) as Box<dyn faber_engine::project::git::Executor + Send>)
    }));
    // A project's checks, which its agent runs with Faber's `verify` tool (R190): no process until it is called. Its results
    // journal (failed first, flaky told) lives in the data folder.
    let journal = data_dir.join("verify").join("journal.jsonl");
    engine.set_verifier(faber_engine::verify::Verifier::real(game_guard, Some(journal)));
    {
        // Once, at start: copies of deleted conversations, and merged clean ones, go.
        let engine = engine.clone();
        runtime.spawn_blocking(move || engine.sweep_copies());
    }
    let host = Arc::new(wiring.host(engine, data_dir.clone(), hidden, runtime));
    // R161 step 7: Faber's own browser, for a project's agent, set before any turn can resume.
    if let Some(browser) = browser {
        host.engine.set_browser(browser);
    }
    host.engine.watch_accounts();
    host.engine.follow_open_prs();
    host.engine.watch_archive();
    host.engine.detect_claude_flags();
    // Work that went on by itself when Faber last closed goes on again, once the accounts have had a moment to be read.
    host.engine.resume_keep_going();
    // And what the restart for an update cut, from a conversation not on Keep going.
    host.engine.resume_after_update();
    // And a team leader the close cut off with no member left to wake it.
    host.engine.resume_cut_leaders();
    if let Some(codex) = clis.get(&Vendor::Codex) {
        host.load_models(codex.clone(), data_dir);
    }
    Ok(host)
}

#[cfg(test)]
mod tests;
