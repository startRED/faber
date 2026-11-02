//! The shell of the sidecar (`faber-sidecar`): what a window-less process decides for the shell that draws its window. The
//! tray, the notifications, keeping the PC awake and starting with the system are decided here, from the engine's events
//! and the settings, and leave as orders (`wire::Order`) for the shell to carry out with its own calls; the shell draws and
//! decides nothing. Tauri's shell decides with the same parts (`tray`, `awake`, `autostart`) and carries them out itself.
//!
//! What only the shell knows reaches this process when it is started: the version of the program and whether it is
//! registered to start with the system now. A setting saved later updates this copy and sends an `autostart` order, since
//! this process cannot register the shell's own program.

use crate::tray::decide::{Attention, ShellTexts};
use crate::tray::{self, Deck, Moment};
use crate::wire::{Order, PickFilter, PickMode, ShellNote};
use crate::{BoxFuture, Filter, PickError, Picked, Res, Shell, lock};
use faber_engine::AppEvent;
use faber_engine::model::{RunningItem, Settings, Vendor};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Handle;
use tokio::sync::oneshot;

/// What [`SidecarShell::new`] needs.
pub struct SidecarOptions {
    pub data_dir: PathBuf,
    /// The version of the program that runs, as the shell that started it knows it (`FABER_VERSION`); this crate's own when
    /// the shell does not say.
    pub version: Option<String>,
    /// Whether the program is registered to start with the system right now (`FABER_AUTOSTART`).
    pub autostart: bool,
    /// A test or automated run: it notifies nobody.
    pub hidden: bool,
    /// Where the work that must not wait for the engine's lock runs.
    pub runtime: Handle,
    /// Where an order goes (the process's one writer).
    pub send: Arc<dyn Fn(Order) + Send + Sync>,
    /// How a notification is delivered at this moment, given the process that owns Faber's window
    /// (`tray::front::attention` in the program).
    pub attention: fn(u32) -> Attention,
    /// The process that owns Faber's window (`FABER_SHELL_PID`): its windows in front are Faber, never a game. This
    /// process when the shell does not say.
    pub shell_process: Option<u32>,
    /// Plays the attention chime (`tray::chime::play` in the program; tests stay quiet).
    pub chime: fn(&std::path::Path),
}

/// The sidecar's answer to every call the host makes back: the tray, the awake request and the start with the system are
/// orders; everything that needs a window keeps the host's default answer.
pub struct SidecarShell(Arc<Inner>);

struct Inner {
    version: String,
    hidden: bool,
    data_dir: PathBuf,
    runtime: Handle,
    send: Arc<dyn Fn(Order) + Send + Sync>,
    attention: fn(u32) -> Attention,
    shell_process: u32,
    chime: fn(&std::path::Path),
    /// Whether Faber's window has the focus, as the shell last said: a question seen there is never notified.
    focused: AtomicBool,
    /// Dialogs the shell was asked for and has not answered, by id; None once the program closes, so no dialog is asked
    /// after that and none is left waiting.
    asks: Mutex<Option<HashMap<u64, oneshot::Sender<Picked>>>>,
    next_ask: AtomicU64,
    /// The newest running list that came before the engine was attached, shown once it is.
    early: Mutex<Option<(u64, Vec<RunningItem>)>>,
    /// The engine's settings, known once the engine exists ([`SidecarShell::attach`]).
    settings: OnceLock<Box<dyn Fn() -> Settings + Send + Sync>>,
    /// Everything an order is decided from. An order is sent while this is held, so orders leave in the order they were decided.
    state: Mutex<State>,
    /// Saves of the settings go one at a time, so the stored switch and the order for it never disagree.
    saving: Mutex<()>,
}

struct State {
    deck: Deck,
    awake: crate::awake::Policy,
    /// What the shell was last told about keeping the PC awake.
    awake_on: bool,
    /// Whether the program is registered to start with the system, as the shell said and as saved settings changed it.
    autostart: bool,
}

impl SidecarShell {
    /// The words and the keep-awake setting come from the data folder, as Tauri reads them, so a program started hidden with
    /// the system speaks the person's language and obeys their setting from its first order.
    pub fn new(options: SidecarOptions) -> Self {
        let SidecarOptions { data_dir, version, autostart, hidden, runtime, send, attention, shell_process, chime } = options;
        let keep_awake = faber_engine::store::Store::new(&data_dir).settings().keep_awake();
        let state = State {
            deck: Deck::new(tray::load_texts(&data_dir)),
            awake: crate::awake::Policy::new(keep_awake),
            awake_on: false,
            autostart,
        };
        Self(Arc::new(Inner {
            version: version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string()),
            hidden,
            data_dir,
            runtime,
            send,
            attention,
            shell_process: shell_process.unwrap_or_else(std::process::id),
            chime,
            focused: AtomicBool::new(false),
            early: Mutex::new(None),
            asks: Mutex::new(Some(HashMap::new())),
            next_ask: AtomicU64::new(1),
            settings: OnceLock::new(),
            state: Mutex::new(state),
            saving: Mutex::new(()),
        }))
    }

    /// The engine exists: from now on the settings it holds decide the notifications, and the tray is told for the first time.
    /// A running list that came before this is shown now.
    pub fn attach(&self, settings: impl Fn() -> Settings + Send + Sync + 'static) {
        let early = {
            let mut state = lock(&self.0.state);
            let _ = self.0.settings.set(Box::new(settings));
            let first = state.deck.tray();
            self.0.send_all(first);
            lock(&self.0.early).take()
        };
        // Outside the state's lock: reading the settings takes the engine's, which `heard` holds while it waits for this one.
        if let Some((revision, items)) = early {
            self.0.on_running(revision, &items);
        }
    }

    /// What the shell says about itself.
    pub fn note(&self, note: ShellNote) {
        match note {
            ShellNote::Focus { focused } => self.0.focused.store(focused, Ordering::SeqCst),
            ShellNote::Picked { id, path, no_window } => {
                // An answer nobody waits for (an id never asked, or asked by a closing program) changes nothing.
                if let Some(waiting) = lock(&self.0.asks).as_mut().and_then(|asks| asks.remove(&id)) {
                    let _ = waiting.send(answer(path, no_window));
                }
            }
        }
    }
}

/// A dialog's answer as the host reads it. Only a full path on this computer is one; anything else was not local.
fn answer(path: Option<String>, no_window: bool) -> Picked {
    if no_window {
        return Err(PickError::NoWindow);
    }
    match path.map(PathBuf::from) {
        Some(path) if path.is_absolute() => Ok(Some(path)),
        Some(_) => Err(PickError::NotLocal),
        None => Ok(None),
    }
}

impl Inner {
    /// Asks the shell for a dialog and waits for its answer. A hidden run shows none; a shell that ends first answers
    /// that there was no window.
    fn ask(self: &Arc<Self>, mode: PickMode, filter: Option<Filter>, name: Option<String>) -> BoxFuture<'static, Picked> {
        let inner = self.clone();
        Box::pin(async move {
            if inner.hidden {
                return Err(PickError::NoWindow);
            }
            let id = inner.next_ask.fetch_add(1, Ordering::Relaxed);
            let (answer, answered) = oneshot::channel();
            match lock(&inner.asks).as_mut() {
                Some(asks) => asks.insert(id, answer),
                None => return Err(PickError::NoWindow),
            };
            let filter = filter.map(|Filter { name, extensions }| PickFilter {
                name: name.to_string(),
                extensions: extensions.iter().map(|extension| extension.to_string()).collect(),
            });
            (inner.send)(Order::Pick { id, mode, filter, name });
            answered.await.unwrap_or(Err(PickError::NoWindow))
        })
    }

    /// Sends orders, in the order given. A hidden run never notifies, nor does anyone while Faber's window has the focus.
    fn send_all(&self, orders: impl IntoIterator<Item = Order>) {
        for order in orders {
            // Looked at again here: the person may have come back to the window since the notice was decided.
            if matches!(order, Order::Notify { .. }) && (self.hidden || self.focused()) {
                continue;
            }
            // The chime is played here: the shell that shows the notice may have no way to play a sound without a window.
            if matches!(order, Order::Notify { sound: true, .. }) {
                (self.chime)(&self.data_dir);
            }
            (self.send)(order);
        }
    }

    /// Tells the shell to keep the PC awake, or to let it sleep, when the policy now says otherwise than it was told.
    fn sync_awake(&self, state: &mut State) {
        let wanted = state.awake.wanted();
        if wanted != state.awake_on {
            state.awake_on = wanted;
            self.send_all([Order::Awake { on: wanted }]);
        }
    }

    fn on_running(&self, revision: u64, items: &[RunningItem]) {
        // Looked at again under the state's lock, which `attach` holds as it sets the settings and takes the early list, so
        // a list is never kept after `attach` looked for one.
        if self.settings.get().is_none() {
            let _state = lock(&self.state);
            if self.settings.get().is_none() {
                let mut early = lock(&self.early);
                if early.as_ref().is_none_or(|(kept, _)| *kept < revision) {
                    *early = Some((revision, items.to_vec()));
                }
                return;
            }
        }
        let Some(settings) = self.settings.get().map(|settings| settings()) else { return };
        let mut state = lock(&self.state);
        let orders =
            state.deck.running(revision, items, &settings, &Moment { attention: &|| self.attention(), focused: &|| self.focused() });
        self.send_all(orders);
    }

    fn on_halted(&self, conversation_id: &str, title: &str, why: &str, ai: Option<Vendor>) {
        let Some(settings) = self.settings.get().map(|settings| settings()) else { return };
        let state = lock(&self.state);
        let orders = state.deck.halted(
            conversation_id,
            title,
            why,
            ai,
            &settings,
            &Moment { attention: &|| self.attention(), focused: &|| self.focused() },
        );
        self.send_all(orders);
    }

    fn attention(&self) -> Attention {
        (self.attention)(self.shell_process)
    }

    fn focused(&self) -> bool {
        self.focused.load(Ordering::SeqCst)
    }
}

impl Shell for SidecarShell {
    fn heard(&self, event: &AppEvent) {
        // The engine calls this with its lock held, and the settings are read under that lock, so the tray works on its own
        // thread; it keeps the newest list, whatever the order. Keeping the PC awake needs no engine state, so it is decided here.
        match event {
            AppEvent::Running { revision, items } => {
                {
                    let mut state = lock(&self.0.state);
                    state.awake.sync_runs(items.iter().map(|item| item.turn_id.clone()).collect::<HashSet<_>>());
                    self.0.sync_awake(&mut state);
                }
                let (inner, revision, items) = (self.0.clone(), *revision, items.clone());
                self.0.runtime.spawn_blocking(move || inner.on_running(revision, &items));
            }
            AppEvent::Halted { conversation_id, title, why, vendor } => {
                let (inner, conversation_id, title, why, vendor) =
                    (self.0.clone(), conversation_id.clone(), title.clone(), why.clone(), *vendor);
                self.0.runtime.spawn_blocking(move || inner.on_halted(&conversation_id, &title, &why, vendor));
            }
            _ => {}
        }
    }

    fn shown_settings(&self, settings: Settings) -> Settings {
        let mut settings = crate::shown(settings);
        settings.autostart = Some(lock(&self.0.state).autostart);
        settings
    }

    fn version(&self) -> String {
        self.0.version.clone()
    }

    fn persist_settings(&self, autostart: Option<bool>, persist: &mut dyn FnMut() -> Res<Settings>) -> Res<Settings> {
        let _saving = lock(&self.0.saving);
        let saved = persist()?;
        if let Some(on) = autostart {
            let mut state = lock(&self.0.state);
            if state.autostart != on {
                state.autostart = on;
                self.0.send_all([Order::Autostart { on }]);
            }
        }
        Ok(saved)
    }

    fn settings_saved(&self, settings: &Settings) {
        let mut state = lock(&self.0.state);
        state.awake.set_enabled(settings.keep_awake());
        self.0.sync_awake(&mut state);
    }

    fn set_texts(&self, texts: Value) -> Res<()> {
        let texts: ShellTexts = serde_json::from_value(texts).map_err(|_| "error.host.badArgs".to_string())?;
        let mut state = lock(&self.0.state);
        if state.deck.texts() != &texts
            && let Err(error) = tray::save_texts(&self.0.data_dir, &texts)
        {
            eprintln!("faber: the tray's words could not be kept ({error})");
        }
        let order = state.deck.set_texts(texts);
        self.0.send_all(order);
        Ok(())
    }

    fn game_in_front(&self) -> bool {
        tray::front::game_in_front(self.0.shell_process)
    }

    fn pick_folder(&self) -> BoxFuture<'_, Picked> {
        self.0.ask(PickMode::Folder, None, None)
    }

    fn pick_file(&self, filter: Option<Filter>) -> BoxFuture<'_, Picked> {
        self.0.ask(PickMode::File, filter, None)
    }

    fn pick_save(&self, filter: Option<Filter>, name: String) -> BoxFuture<'_, Picked> {
        self.0.ask(PickMode::Save, filter, Some(name))
    }

    fn show_conversation(&self, id: &str) -> Res<()> {
        if self.0.hidden {
            return Err(crate::NO_WINDOW.into());
        }
        (self.0.send)(Order::ShowConversation { id: id.into() });
        Ok(())
    }

    fn reveal(&self, path: PathBuf) -> Res<()> {
        (self.0.send)(Order::Reveal { path: path.to_string_lossy().into_owned() });
        Ok(())
    }

    fn shut_down(&self) {
        // Dialogs still open are answered "no window": their commands end instead of waiting for a shell that is gone.
        lock(&self.0.asks).take();
        let mut state = lock(&self.0.state);
        state.awake.shutdown();
        self.0.sync_awake(&mut state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray::decide::Attention::{Normal, Silent};
    use faber_engine::model::{Area, Permission, RunState};
    use serde_json::json;
    use std::time::Duration;

    type Sink = Arc<Mutex<Vec<Order>>>;

    fn normal(_: u32) -> Attention {
        Normal
    }

    fn silent(_: u32) -> Attention {
        Silent
    }

    fn quiet(_: &std::path::Path) {}

    fn shell_in(dir: &std::path::Path, hidden: bool, autostart: bool, attention: fn(u32) -> Attention) -> (SidecarShell, Sink) {
        let sink: Sink = Arc::default();
        let into = sink.clone();
        let shell = SidecarShell::new(SidecarOptions {
            data_dir: dir.to_path_buf(),
            version: None,
            autostart,
            hidden,
            runtime: Handle::current(),
            send: Arc::new(move |order| lock(&into).push(order)),
            attention,
            shell_process: None,
            chime: quiet,
        });
        (shell, sink)
    }

    /// An attached shell with the default settings, and everything it has told so far taken out of the sink.
    fn attached(dir: &std::path::Path, hidden: bool, attention: fn(u32) -> Attention) -> (SidecarShell, Sink) {
        let (shell, sink) = shell_in(dir, hidden, false, attention);
        shell.attach(Settings::default);
        lock(&sink).clear();
        (shell, sink)
    }

    fn taken(sink: &Sink) -> Vec<Order> {
        std::mem::take(&mut *lock(sink))
    }

    #[tokio::test]
    async fn show_conversation_reaches_the_shell() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), false, normal);
        shell.show_conversation("c1").unwrap();
        assert_eq!(taken(&sink), vec![Order::ShowConversation { id: "c1".into() }]);
        let (hidden, _) = attached(dir.path(), true, normal);
        assert_eq!(hidden.show_conversation("c1"), Err(crate::NO_WINDOW.into()));
    }

    fn item(conversation: &str, turn: &str, title: &str, ask: bool) -> RunningItem {
        RunningItem {
            conversation_id: conversation.into(),
            turn_id: turn.into(),
            title: title.into(),
            area: Area::Chat,
            project_id: None,
            vendor: None,
            model: None,
            state: if ask { RunState::NeedsYou } else { RunState::Working },
            since: 0,
            wait_until: None,
            permissions: if ask {
                vec![Permission { request_id: "0".into(), tool: "Write".into(), input: json!({}), epoch: None }]
            } else {
                vec![]
            },
            parked: vec![],
            parked_more: 0,
            stuck: vec![],
            stall: None,
            held: None,
            keep_going: false,
            keep_wait: None,
            keep_progress: None,
            weighing: false,
            settled: false,
            plan: None,
            now: None,
        }
    }

    fn running(revision: u64, items: Vec<RunningItem>) -> AppEvent {
        AppEvent::Running { revision, items }
    }

    /// Waits for the work `heard` hands to another thread.
    async fn until(sink: &Sink, count: usize) -> Vec<Order> {
        for _ in 0..500 {
            if lock(sink).len() >= count {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        taken(sink)
    }

    #[tokio::test]
    async fn the_tray_is_told_when_the_engine_is_there_and_not_before() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = shell_in(dir.path(), false, false, normal);
        shell.0.on_running(2, &[item("a", "t1", "A", true)]);
        shell.0.on_running(1, &[]);
        assert!(taken(&sink).is_empty(), "no settings yet: the newest list waits for them");
        shell.attach(Settings::default);
        let orders = taken(&sink);
        assert!(
            matches!(orders.as_slice(), [Order::Tray { needs_you: 0, .. }, Order::Tray { needs_you: 1, .. }, Order::Notify { .. }]),
            "the first tray, then the list that came early: {orders:?}"
        );
    }

    #[tokio::test]
    async fn a_list_from_the_engine_reaches_the_shell_as_a_tray_and_a_notice() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), false, normal);
        shell.heard(&running(1, vec![item("conv", "t1", "Fix it", true)]));
        let orders = until(&sink, 3).await;
        assert_eq!(orders.len(), 3, "{orders:?}");
        assert_eq!(orders[0], Order::Awake { on: true }, "a turn runs");
        assert!(matches!(orders[1], Order::Tray { needs_you: 1, .. }), "{orders:?}");
        assert_eq!(
            orders[2],
            Order::Notify {
                title: "Fix it needs you".into(),
                body: String::new(),
                conversation_id: Some("conv".into()),
                toast: true,
                sound: true
            }
        );
        shell.heard(&AppEvent::Halted {
            conversation_id: "conv".into(),
            title: "Fix it".into(),
            why: "reason.keepGoing.done".into(),
            vendor: None,
        });
        let halted = until(&sink, 1).await;
        assert!(matches!(halted.as_slice(), [Order::Notify { conversation_id: Some(id), .. }] if id == "conv"), "{halted:?}");
    }

    #[tokio::test]
    async fn a_dialog_is_asked_of_the_shell_and_its_answer_reaches_the_command() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), false, normal);
        let shell = Arc::new(shell);
        let asking = {
            let shell = shell.clone();
            tokio::spawn(async move { shell.pick_save(Some(Filter { name: "JSON", extensions: &["json"] }), "a.json".into()).await })
        };
        let asked = until(&sink, 1).await;
        let [Order::Pick { id, mode: PickMode::Save, filter: Some(filter), name: Some(name) }] = asked.as_slice() else {
            panic!("{asked:?}")
        };
        assert_eq!((filter.name.as_str(), filter.extensions.as_slice(), name.as_str()), ("JSON", &["json".to_string()][..], "a.json"));
        let picked = dir.path().join("a.json");
        shell.note(ShellNote::Picked { id: id + 100, path: None, no_window: false });
        shell.note(ShellNote::Picked { id: *id, path: Some(picked.to_string_lossy().into()), no_window: false });
        assert_eq!(asking.await.unwrap(), Ok(Some(picked)), "an id nobody asked for changes nothing");
        // Cancelled, a path that is not a full one, no window, and a program that closes while a dialog is open.
        for (path, no_window, expected) in
            [(None, false, Ok(None)), (Some("a.json".to_string()), false, Err(PickError::NotLocal)), (None, true, Err(PickError::NoWindow))]
        {
            let asking = {
                let shell = shell.clone();
                tokio::spawn(async move { shell.pick_folder().await })
            };
            let asked = until(&sink, 1).await;
            let [Order::Pick { id, .. }] = asked.as_slice() else { panic!("{asked:?}") };
            shell.note(ShellNote::Picked { id: *id, path, no_window });
            assert_eq!(asking.await.unwrap(), expected);
        }
        let asking = {
            let shell = shell.clone();
            tokio::spawn(async move { shell.pick_file(None).await })
        };
        until(&sink, 1).await;
        shell.shut_down();
        assert_eq!(asking.await.unwrap(), Err(PickError::NoWindow));
        assert_eq!(shell.pick_folder().await, Err(PickError::NoWindow), "none is asked once the program closes");
        // A hidden run never shows one.
        let (hidden, sink) = attached(dir.path(), true, normal);
        assert_eq!(hidden.pick_folder().await, Err(PickError::NoWindow));
        assert!(!taken(&sink).iter().any(|order| matches!(order, Order::Pick { .. })));
    }

    #[tokio::test]
    async fn a_question_that_comes_while_the_window_has_the_focus_is_never_notified() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), false, normal);
        shell.note(ShellNote::Focus { focused: true });
        shell.0.on_running(1, &[item("a", "t1", "A", true)]);
        assert!(matches!(taken(&sink).as_slice(), [Order::Tray { needs_you: 1, .. }]), "seen in the window");
        shell.note(ShellNote::Focus { focused: false });
        shell.0.on_running(2, &[item("a", "t1", "A", true)]);
        assert!(taken(&sink).is_empty(), "the same question, still not told");
        shell.0.on_halted("a", "A", "reason.keepGoing.done", None);
        assert!(matches!(taken(&sink).as_slice(), [Order::Notify { .. }]), "away from the window, a notice again");
    }

    #[tokio::test]
    async fn a_notice_waits_for_what_tauris_tray_would_wait_for() {
        let dir = tempfile::tempdir().unwrap();
        // A game in front: the tray still follows the list, and nothing is shown or played.
        let (shell, sink) = attached(dir.path(), false, silent);
        shell.0.on_running(1, &[item("a", "t1", "A", true)]);
        assert!(matches!(taken(&sink).as_slice(), [Order::Tray { needs_you: 1, .. }]));
        shell.0.on_halted("a", "A", "reason.keepGoing.done", None);
        assert!(taken(&sink).is_empty());
        // The same question is not told again, and a hidden run never notifies at all.
        let (shell, sink) = attached(dir.path(), true, normal);
        shell.0.on_running(1, &[item("a", "t1", "A", true)]);
        assert!(matches!(taken(&sink).as_slice(), [Order::Tray { needs_you: 1, .. }]), "hidden: the tray only");
        shell.0.on_halted("a", "A", "reason.keepGoing.done", None);
        assert!(taken(&sink).is_empty());
    }

    #[tokio::test]
    async fn the_pc_is_kept_awake_while_a_turn_runs_and_the_setting_allows_it() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), true, normal);
        let awake = |sink: &Sink| taken(sink).into_iter().filter(|o| matches!(o, Order::Awake { .. })).collect::<Vec<_>>();
        shell.heard(&running(1, vec![item("a", "t1", "A", false)]));
        assert_eq!(awake(&sink), [Order::Awake { on: true }]);
        shell.heard(&running(2, vec![item("a", "t1", "A", false), item("b", "t2", "B", false)]));
        assert_eq!(awake(&sink), [], "already awake: not told twice");
        shell.heard(&running(3, vec![item("b", "t2", "B", false)]));
        assert_eq!(awake(&sink), [], "one turn is left");
        shell.settings_saved(&Settings { keep_awake: Some(false), ..Settings::default() });
        assert_eq!(awake(&sink), [Order::Awake { on: false }], "the setting ends it at once");
        shell.settings_saved(&Settings { keep_awake: Some(true), ..Settings::default() });
        assert_eq!(awake(&sink), [Order::Awake { on: true }], "and brings it back with the turn still running");
        shell.heard(&running(4, vec![]));
        assert_eq!(awake(&sink), [Order::Awake { on: false }]);
        shell.heard(&running(5, vec![item("c", "t3", "C", false)]));
        assert_eq!(awake(&sink), [Order::Awake { on: true }]);
        shell.shut_down();
        assert_eq!(awake(&sink), [Order::Awake { on: false }], "closing lets the PC sleep");
        shell.heard(&running(6, vec![item("c", "t3", "C", false)]));
        assert_eq!(awake(&sink), [], "and nothing late brings it back");
    }

    #[tokio::test]
    async fn the_keep_awake_setting_in_the_data_folder_decides_from_the_first_turn() {
        let dir = tempfile::tempdir().unwrap();
        let store = faber_engine::store::Store::new(dir.path());
        store.save_settings(&Settings { keep_awake: Some(false), ..Settings::default() }).unwrap();
        let (shell, sink) = attached(dir.path(), true, normal);
        shell.heard(&running(1, vec![item("a", "t1", "A", false)]));
        assert!(!until(&sink, 1).await.iter().any(|o| matches!(o, Order::Awake { .. })));
    }

    #[tokio::test]
    async fn the_words_the_screen_sends_reach_the_tray_are_kept_and_speak_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = attached(dir.path(), true, normal);
        let words = json!({
            "open": "Abrir", "quit": "Sair",
            "running": { "one": "Faber: {n} em andamento", "other": "Faber: {n} em andamento" },
            "needsYou": { "one": ", {n} precisa de você", "other": ", {n} precisam de você" },
            "notify": "{title} precisa de você"
        });
        assert_eq!(shell.set_texts(json!({ "nonsense": 1 })), Err("error.host.badArgs".into()));
        shell.set_texts(words.clone()).unwrap();
        let orders = taken(&sink);
        let [Order::Tray { menu, .. }] = orders.as_slice() else { panic!("{orders:?}") };
        assert_eq!((menu.show.as_str(), menu.quit.as_str()), ("Abrir", "Sair"));
        shell.set_texts(words).unwrap();
        assert!(taken(&sink).is_empty(), "the same words are not told again");

        let (again, sink) = shell_in(dir.path(), true, false, normal);
        again.attach(Settings::default);
        let first = taken(&sink);
        assert!(
            matches!(first.as_slice(), [Order::Tray { menu, .. }] if menu.show == "Abrir"),
            "a restart starts in the saved language: {first:?}"
        );
    }

    #[tokio::test]
    async fn start_with_the_system_follows_the_shell_and_is_ordered_only_when_a_save_changes_it() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, sink) = shell_in(dir.path(), true, true, normal);
        shell.attach(Settings::default);
        taken(&sink);
        assert_eq!(
            shell.shown_settings(Settings { autostart: Some(false), ..Settings::default() }).autostart,
            Some(true),
            "the shell's word, not stale storage"
        );
        let ok = |settings: Settings| move || Ok::<Settings, String>(settings.clone());
        let stored = Settings::default();

        assert_eq!(shell.persist_settings(Some(true), &mut ok(stored.clone())), Ok(stored.clone()));
        assert!(taken(&sink).is_empty(), "already registered");
        shell.persist_settings(None, &mut ok(stored.clone())).unwrap();
        assert!(taken(&sink).is_empty(), "a save that says nothing about it");
        shell.persist_settings(Some(false), &mut ok(stored.clone())).unwrap();
        assert_eq!(taken(&sink), [Order::Autostart { on: false }]);
        assert_eq!(shell.shown_settings(Settings::default()).autostart, Some(false));
        assert_eq!(
            shell.persist_settings(Some(true), &mut || Err("error.settings.saveFailed".into())),
            Err("error.settings.saveFailed".into())
        );
        assert!(taken(&sink).is_empty(), "a save that failed changes nothing");
        assert_eq!(shell.shown_settings(Settings::default()).autostart, Some(false));
        shell.persist_settings(Some(true), &mut ok(stored)).unwrap();
        assert_eq!(taken(&sink), [Order::Autostart { on: true }]);
    }

    #[tokio::test]
    async fn the_version_is_the_shells_when_it_says_and_this_programs_when_it_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let (shell, _) = shell_in(dir.path(), true, false, normal);
        assert_eq!(shell.version(), env!("CARGO_PKG_VERSION"));
        let shell = SidecarShell::new(SidecarOptions {
            data_dir: dir.path().to_path_buf(),
            version: Some("9.8.7".into()),
            autostart: false,
            hidden: true,
            runtime: Handle::current(),
            send: Arc::new(|_| {}),
            attention: normal,
            shell_process: None,
            chime: quiet,
        });
        assert_eq!(shell.version(), "9.8.7");
    }
}
