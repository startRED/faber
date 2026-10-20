//! The engine: conversations, live CLI sessions, permissions, branches and attachments.

use crate::account::{self, AccountStatus};
use crate::budget::{Billing, Purpose, Reach, Why, family_of};
use crate::cadence::{Cadence, TICK};
use crate::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
use crate::cli::Cli;
use crate::codex::read_codex_limits;
use crate::codex::{CodexServer, CodexServerOptions, CodexThread};
use crate::events::{EngineEvent, Session, UserTurn};
use crate::learn::Span;
use crate::ledger::{Ledger, Work, plausible};
use crate::model::*;
use crate::nudge;
use crate::proc::Spawner;
use crate::recents::RecentsView;
use crate::router::{Choice, Pick, pick_model};
use crate::signals::Signals;
use crate::store::Store;
use crate::usage;
use crate::workspace::{Attachment, inside_folder, new_files, save_files};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod answer;
mod archive;
mod aside;
mod auto;
mod auto_archive;
mod browse;
mod by_itself;
mod changes;
mod compact;
mod copies;
mod ended;
pub use aside::Aside;
pub use copies::GitRunner;
pub mod facts;
mod helpers;
mod install_gate;
mod keep_going;
mod parked;
mod plan;
mod pr_watch;
mod quota_watch;
mod recovery;
mod siblings;
mod stuck_watch;
mod unbounded;
mod usage_view;
mod verifying;
pub use pr_watch::{PrReader, gh_reader};
pub use usage_view::{VendorUsage, WindowUsage};
pub use verifying::{FIX, MAX_FIXES};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppEvent {
    Message {
        conversation_id: String,
        message: Box<Message>,
    },
    Queue {
        conversation_id: String,
        queued: Vec<QueuedTurn>,
    },
    Quota {
        vendor: Vendor,
        window: String,
        used: f64,
        resets_at: f64,
    },
    Usage {
        vendors: Vec<VendorUsage>,
    },
    Running {
        revision: u64,
        items: Vec<RunningItem>,
    },
    List,
    /// The agent's plan changed (None: it cleared it), for the checklist under the newest reply.
    Plan {
        conversation_id: String,
        plan: Option<Plan>,
    },
    /// How full a conversation's own agent's context is now (None: not known).
    Context {
        conversation_id: String,
        context: Option<ContextFill>,
    },
    /// "Keep going" stopped by itself in a conversation, and why (a `reason.keepGoing.*` key): the tray tells the person.
    Halted {
        conversation_id: String,
        title: String,
        why: String,
        vendor: Option<Vendor>,
    },
    /// The quick questions on the side of a conversation (R250), all of them each time one changes.
    Asides {
        conversation_id: String,
        asides: Vec<aside::Aside>,
    },
}

pub type Emit = Arc<dyn Fn(AppEvent) + Send + Sync>;

/// The time in Unix seconds. Tests replace it to place quota readings in time.
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Answers a routing question in place of the CLIs; tests use it so no real model is called.
pub type Ask = Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

pub struct EngineOptions {
    pub data_dir: PathBuf,
    pub clis: BTreeMap<Vendor, Cli>,
    pub spawn: Spawner,
    pub windows: bool,
    /// Whether the shell found an environment override that bills Claude requests per use.
    pub claude_env_billed: bool,
    /// At most this many idle Claude processes stay ready; Codex conversations share one process and do not count.
    pub max_warm: usize,
    /// An idle Claude process closes after this long; its next turn resumes the same session.
    pub idle: Duration,
    /// Where session and timer tasks run, so the engine can be called from any thread.
    pub runtime: tokio::runtime::Handle,
    /// None asks the fastest cheap connected model; see `route`.
    pub ask: Option<Ask>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub area: Area,
    #[serde(default)]
    pub project_id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub files: Vec<Attachment>,
    pub choice: Choice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchRequest {
    pub conversation_id: String,
    pub message_id: String,
    pub text: String,
    pub choice: Choice,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarmRequest {
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub area: Area,
    #[serde(default)]
    pub project_id: Option<String>,
    pub choice: Choice,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppState {
    pub conversations: Vec<Summary>,
    pub projects: Vec<Project>,
    pub vendors: Vec<Vendor>,
    pub settings: Settings,
    /// The running list as last emitted, with its revision.
    pub running: Running,
    /// The latest reading of each vendor's quota windows, with the time each resets. The screen decides which count as
    /// expired; the engine reports what the CLIs said.
    pub quotas: Vec<QuotaReading>,
    pub usage: Vec<VendorUsage>,
}

/// How to continue a conversation in the CLI's own terminal interface.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalCommand {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

struct Live {
    vendor: Vendor,
    /// The model asked for (none = the account's default).
    model: Option<String>,
    /// The model the CLI says it runs, once it has said so.
    running: Option<String>,
    effort: String,
    /// Tells this process's events apart from those of a replaced one.
    sid: u64,
    session: Session,
    /// The answer being written right now.
    turn: Option<String>,
    break_pending: bool,
    answer_started: bool,
    used: u64,
    /// The CLI this process was started with; a replaced CLI starts a new process on the next turn.
    cli: Cli,
    /// The process continues a session an earlier process began.
    resumed: bool,
    permissions: Permissions,
    /// What the web and Codex settings were when the process started (R140).
    tuning: Tuning,
    /// The mode the CLI really runs: not `permissions` when the CLI lacks it (Claude Code without `auto`).
    runs: Permissions,
    /// The folder the process was started in (checked); a turn that finds the folder resolving elsewhere starts a new process.
    cwd: PathBuf,
    by_hand: bool,
    approved: bool,
    started: f64,
    /// A turn has finished on this process, so the next one finds the CLI's cache warm.
    served: bool,
    /// Queued messages handed to the running turn and not yet taken in, oldest first.
    steering: Vec<String>,
    /// A Claude turn ended while a line handed to it was still unread (`finish`): its keep-going reading, kept until that
    /// line's own turn takes over or the line fails.
    held: Option<Option<String>>,
    /// The session id the CLI reported at start, kept out of the conversation until a turn really ran on it: a CLI saves a
    /// session only once it has had a turn, so an id from a warm-up that never ran must not be resumed by a later launch.
    unsaved_session: Option<String>,
}

/// Messages waiting behind a running turn. Each may carry images, which stay in memory and go out with every queue event.
const MAX_QUEUED: usize = 10;

/// A model that joins a conversation midway, or a fresh session that replaces a lost one, reads what was said before as plain
/// text: every message, the names and places of the files attached, a count of the images it cannot see, and each tool's result
/// shortened as `handover::transcript` does. The reply that carries it says so (`note.historyResent`).
pub fn with_history(history: &[Message], text: &str, place: &dyn Fn(&str) -> String) -> String {
    let lines = history
        .iter()
        .map(|m| {
            let mut line = crate::handover::transcript(std::slice::from_ref(m));
            for file in m.attachments.iter().flatten() {
                line += &format!("\n[attached file: {}]", place(file));
            }
            match m.images.as_ref().map_or(0, |i| i.len()) {
                0 => {}
                n => line += &format!("\n[{n} image(s) sent here are not included]"),
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("Earlier in this conversation, with another assistant:\n\n{lines}\n\nNow the user says:\n{text}")
}

/// Where a stored attachment can be opened: the working folder's own `attachments` in Chat, the data folder's in Projects.
pub(crate) fn attachment_place(area: Area, data_dir: &Path, conversation: &str, name: &str) -> String {
    if area == Area::Chat {
        format!("attachments/{name}")
    } else {
        data_dir.join("attachments").join(conversation).join(name).to_string_lossy().into_owned()
    }
}

/// This vendor already answered in the conversation, yet has no session to continue: the session was lost.
fn session_was_lost(c: &Conversation, vendor: Vendor) -> bool {
    c.resumable(vendor).is_none() && c.messages.iter().any(|m| m.role == Role::Assistant && m.vendor == Some(vendor))
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn unix_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// The lifetime of the cache counts from when the request began, and the stream shows the answer only after its first word:
/// this much is taken off the expiry so the note never promises more warmth than there is.
const WARM_MARGIN_MS: u64 = 5_000;

/// How often the quota ledger is written while readings keep coming.
const LEDGER_EVERY: f64 = 30.0;

/// The readings the ledger remembers that have not reset yet, so Settings has numbers before the first answer.
fn remembered(ledger: &Ledger, now: f64) -> HashMap<(Vendor, String), QuotaReading> {
    Vendor::ALL
        .into_iter()
        .flat_map(|vendor| {
            ledger.of(vendor).readings.iter().filter(move |(_, r)| r.resets_at > now).map(move |(window, r)| {
                ((vendor, window.clone()), QuotaReading { vendor, window: window.clone(), used: r.used, resets_at: r.resets_at })
            })
        })
        .collect()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// A message no running CLI owns: nothing streams and no permission waits any more.
fn settle(m: &mut Message) {
    m.permissions = None;
    if m.status == Some(Status::Streaming) {
        m.status = Some(Status::Stopped);
    }
}

struct Core {
    opts: EngineOptions,
    store: Store,
    emit: Emit,
    me: Weak<Mutex<Core>>,
    /// Conversations in use, the source of truth while the app runs; every change is written through to disk.
    convs: HashMap<String, Conversation>,
    /// Runs git for the copies of project conversations (R201), given by the shell; without it they work in the folder.
    git: Option<copies::GitRunner>,
    /// Faber's own browser (R161 step 7), given by the shell where it has one; without it the tool is not offered.
    browser: Option<std::sync::Arc<dyn crate::browser::Browser>>,
    /// Runs a project's checks (`crate::verify`), given by the shell; without it the tool is not offered.
    verifier: Option<std::sync::Arc<crate::verify::Verifier>>,
    /// What each conversation's folder held when its reply began, for Faber's checks (`verifying::Before`).
    before: HashMap<String, verifying::Before>,
    live: HashMap<String, Live>,
    /// Set while an install of Faber closes the engine's admission (`Engine::install_admission`): nothing new starts.
    installing: bool,
    /// Compactions the agents asked for, done once their turn ends (`compact.rs`).
    compacts: HashMap<String, compact::Plan>,
    /// A new conversation still being typed: its CLI starts before the first message is sent.
    draft: Option<String>,
    /// Streaming text goes to the screen at most ~30 times a second; the message waiting per conversation.
    pending: HashMap<String, String>,
    next_sid: u64,
    tick: u64,
    /// The shared Codex processes, one per area, hooks choice and permissions. Faber's internal questions (routing) use an
    /// Isolated process of their own, so nothing the person's setting allows ever reaches them.
    codex: HashMap<(Area, bool, Permissions), CodexServer>,
    /// The ChatGPT model the account recommends, once the CLI has listed the models.
    codex_recommended: Option<String>,
    /// Every model the account can run on Codex, once listed.
    codex_models: Vec<String>,
    /// The latest reading of each quota window, per vendor. Auto prices the models by the share used.
    quotas: HashMap<(Vendor, String), QuotaReading>,
    /// The running list as last emitted.
    running: Running,
    /// Auto's route for the latest first message or draft.
    guess: Option<auto::Guess>,
    next_draft: Option<auto::Draft>,
    spare: Option<auto::Spare>,
    /// What Faber remembers about each vendor's quota (`budget.json`).
    ledger: Ledger,
    ledger_saved: f64,
    /// Safety changes cannot admit work after a failed ledger save.
    ledger_failed: bool,
    /// No ownership or unreadable authority: keep Chat in memory and leave the durable budget intact.
    projects_closed: bool,
    /// When each vendor's quota is read next (cadence.rs), and which readings are under way.
    cadence: Cadence,
    reading: HashSet<Vendor>,
    /// The watcher runs (the shell started it and the screen said the PC's offset): only then do events start readings.
    watching: bool,
    /// The engine is shutting down: no new quota reading starts.
    closing: bool,
    /// `/usage` turned out to ask a model: Claude's quota is then read from its own answers only.
    usage_command_spent: bool,
    /// The newer flags the installed Claude Code takes; none until `detect_claude_flags` has read them.
    claude_flags: crate::claude::ClaudeFlags,
    /// The `Cli::stamp` the flags above were read under (or are being read under): a different one means the CLI was updated or
    /// downgraded since, and what it takes is unknown until the help is read again.
    claude_stamp: Option<String>,
    /// When a failed Codex read last counted against the plan check.
    codex_failed_at: f64,
    /// The PC's offset from UTC in seconds, as the screen reports it.
    utc_offset: i64,
    offset_known: bool,
    watch_requested: bool,
    clock: Option<Clock>,
    /// Scales the fixed waits of "keep going" (retries, rests, the question timer); 1 outside tests.
    keep_pace: f64,
    transfers: HashMap<String, recovery::Transfer>,
    /// First messages whose separate copy is still being made, by conversation.
    copying: HashMap<String, copies::Held>,
    credit: Option<(String, String, Vendor)>,
    /// Helpers at work (R161), by the agent's tool call that started each.
    helpers: HashMap<String, helpers::Run>,
    /// The quick questions on the side (R250), by conversation, and the ones still asking, by question.
    asides: HashMap<String, Vec<aside::Aside>>,
    asking: HashMap<String, aside::Asking>,
    /// The subagents of each running reply, followed for signs of being stuck.
    watched: HashMap<String, stuck_watch::Watched>,
    /// The pull request each project conversation follows, by conversation (R200), and what reads one.
    follows: HashMap<String, pr_watch::Watch>,
    pr_reader: PrReader,
}

/// How often Claude's plan is checked again.
const PLAN_EVERY: f64 = 15.0 * 60.0;

/// A handle to the engine; clones share the same engine.
#[derive(Clone)]
pub struct Engine {
    core: Arc<Mutex<Core>>,
    runtime: tokio::runtime::Handle,
}

impl Engine {
    pub fn new(opts: EngineOptions, emit: Emit) -> Engine {
        let mut store = Store::new(&opts.data_dir);
        // A crash can leave an unsaved draft behind. Visible records keep their scratch; orphan UUID folders do not.
        let scratch = opts.data_dir.join("chat");
        if scratch.symlink_metadata().is_ok_and(|m| !m.file_type().is_symlink()) {
            for entry in std::fs::read_dir(&scratch).into_iter().flatten().flatten() {
                let id = entry.file_name().to_string_lossy().into_owned();
                if Uuid::parse_str(&id).is_ok() && !opts.data_dir.join("conversations").join(format!("{id}.json")).exists() {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        let owned = store.claim_budget().is_ok();
        let mut ledger = store.ledger();
        let projects_closed = !owned
            || store.ledger_checked().is_err()
            || match crate::project::budget::recover_unbounded(&mut ledger) {
                Ok(true) => store.save_ledger(&ledger).is_err(),
                Ok(false) => false,
                Err(_) => true,
            };
        let quotas = remembered(&ledger, unix_secs());
        let runtime = opts.runtime.clone();
        let pr_reader = gh_reader(&opts.data_dir);
        let core = Arc::new_cyclic(|me| {
            Mutex::new(Core {
                opts,
                store,
                emit,
                me: me.clone(),
                convs: HashMap::new(),
                live: HashMap::new(),
                installing: false,
                compacts: HashMap::new(),
                draft: None,
                pending: HashMap::new(),
                next_sid: 0,
                tick: 0,
                codex: HashMap::new(),
                codex_recommended: None,
                codex_models: vec![],
                quotas,
                running: Running::default(),
                guess: None,
                next_draft: None,
                spare: None,
                ledger,
                ledger_saved: 0.0,
                ledger_failed: false,
                projects_closed,
                cadence: Cadence::default(),
                reading: HashSet::new(),
                watching: false,
                closing: false,
                usage_command_spent: false,
                claude_flags: Default::default(),
                claude_stamp: None,
                codex_failed_at: f64::MIN,
                utc_offset: 0,
                offset_known: false,
                watch_requested: false,
                clock: None,
                keep_pace: 1.0,
                transfers: HashMap::new(),
                copying: HashMap::new(),
                credit: None,
                helpers: HashMap::new(),
                asides: HashMap::new(),
                asking: HashMap::new(),
                watched: HashMap::new(),
                follows: HashMap::new(),
                pr_reader,
                browser: None,
                verifier: None,
                before: HashMap::new(),
                git: None,
            })
        });
        Engine { core, runtime }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Core) -> T) -> T {
        let _inside = self.runtime.enter();
        f(&mut self.core.lock().unwrap_or_else(|e| e.into_inner()))
    }

    pub fn vendors(&self) -> Vec<Vendor> {
        self.with(|c| c.vendors())
    }
    /// The PC's offset from UTC in seconds (east is positive). The engine has no time zone database, so the screen says.
    pub fn set_utc_offset(&self, secs: i64) {
        let start_watching = self.with(|c| {
            let start = !c.offset_known && c.watch_requested;
            c.utc_offset = secs.clamp(-14 * 3600, 14 * 3600);
            c.offset_known = true;
            c.wake_waiting();
            c.emit_usage();
            start
        });
        if start_watching {
            self.watch_accounts();
        }
    }
    /// Replaces the clock; the quota arithmetic reads the time through it.
    pub fn set_time_source(&self, clock: Clock) {
        self.with(|c| {
            c.clock = Some(clock);
            // Which remembered readings are still current depends on the clock.
            for (key, reading) in remembered(&c.ledger, c.secs()) {
                c.quotas.entry(key).or_insert(reading);
            }
        })
    }
    /// Asks each connected CLI who is signed in and on which plan, and keeps the answers in the ledger: Claude Code through its
    /// status command, Codex through a short-lived server (which also refreshes its quota readings). A check that fails or
    /// times out refreshes nothing, and three in a row make Settings ask the user which plan they have.
    pub async fn check_plans(&self, timeout: Duration) {
        type Job = (Vendor, Cli, Spawner, PathBuf, bool);
        let jobs: Vec<Job> = self.with(|c| {
            c.opts.clis.iter().map(|(v, cli)| (*v, cli.clone(), c.opts.spawn.clone(), c.opts.data_dir.clone(), c.opts.windows)).collect()
        });
        let mut claude_quota = None;
        for (vendor, cli, spawn, cwd, windows) in jobs {
            match vendor {
                Vendor::Claude => claude_quota = self.check_claude_plan(timeout).await,
                Vendor::Codex => {
                    let events = read_codex_limits(&cli.command, &cwd, windows, spawner(&spawn, &cli), timeout).await;
                    self.with(|c| {
                        c.codex_plan_checked(events);
                        c.emit_usage();
                    });
                }
            }
        }
        // Optional quota work cannot hold up either vendor's completed account check.
        if let Some((cli, spawn, cwd)) = claude_quota {
            let events = account::claude_limits(&cli, &spawn, &cwd, timeout).await;
            self.with(|c| {
                c.zero_turn_checked(Vendor::Claude, events);
                c.emit_usage();
            });
        }
    }
    /// Checks the Claude plan; when the account is a subscription, returns what a zero-token quota read needs.
    async fn check_claude_plan(&self, timeout: Duration) -> Option<(Cli, Spawner, PathBuf)> {
        let (cli, spawn, cwd) =
            self.with(|c| c.opts.clis.get(&Vendor::Claude).map(|cli| (cli.clone(), c.opts.spawn.clone(), c.opts.data_dir.clone())))?;
        let found = account::claude_account(&cli, &spawn, &cwd, timeout).await;
        let subscription = found.as_ref().is_some_and(|s| s.billing == Some(Billing::Subscription));
        self.with(|c| {
            c.claude_plan_checked(found);
            c.emit_usage();
        });
        subscription.then_some((cli, spawn, cwd))
    }
    /// The newer flags the installed Claude Code is known to take (none until the help has been read).
    pub fn claude_flags(&self) -> crate::claude::ClaudeFlags {
        self.with(|c| c.claude_flags)
    }

    /// Reads once which newer flags the installed Claude Code lists in `--help` (`autocompact`, `permission-prompts`). Until it
    /// answers, and if it never does, every Claude process starts as it did before those flags existed. The shell starts it once.
    pub fn detect_claude_flags(&self) {
        self.with(|c| c.probe_claude_flags());
    }

    /// Once the screen supplies its local offset: checks Claude's plan every quarter of an hour, and reads both quotas on the
    /// schedule in `cadence.rs` (at start, every few minutes, after a wake). The shell starts it once.
    pub fn watch_accounts(&self) {
        let Some(me) = self.with(|c| {
            c.watch_requested = true;
            c.watching |= c.offset_known;
            c.offset_known.then(|| c.me.clone())
        }) else {
            return;
        };
        let runtime = self.runtime.clone();
        let handle = runtime.clone();
        runtime.spawn(async move {
            let mut plan_checked = f64::MIN;
            loop {
                let Some(core) = me.upgrade() else { return };
                let engine = Engine { core, runtime: handle.clone() };
                let now = engine.with(|c| {
                    c.quota_tick();
                    c.secs()
                });
                if !(0.0..PLAN_EVERY).contains(&(now - plan_checked)) {
                    plan_checked = now;
                    engine.check_claude_plan(Duration::from_secs(20)).await;
                }
                drop(engine);
                tokio::time::sleep(Duration::from_secs_f64(TICK)).await;
            }
        });
    }
    /// The plan the user named when it could not be detected. It never overrides how the login is billed, and a later
    /// detection that disagrees removes it.
    pub fn answer_plan(&self, vendor: Vendor, plan: &str) {
        let plan: String = plan.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
        self.with(|c| {
            if !plan.is_empty() {
                c.ledger.of_mut(vendor).typed_plan = Some(plan);
                c.save_ledger(true);
                c.emit_usage();
            }
        })
    }
    /// A copy of what the engine remembers about each vendor's quota, for Settings and for tests.
    pub fn ledger(&self) -> Ledger {
        self.with(|c| c.ledger.clone())
    }
    /// Quota readings and account facts that came from outside a conversation, such as a short-lived CLI process asked
    /// for them. Anything else in `events` is ignored.
    pub fn note_account_events(&self, vendor: Vendor, events: Vec<EngineEvent>) {
        self.with(|c| events.into_iter().for_each(|e| c.note_account_event(vendor, e)))
    }
    pub fn set_codex_models(&self, models: Vec<String>, recommended: Option<String>) {
        self.with(|c| {
            c.codex_models = models;
            c.codex_recommended = recommended;
        })
    }
    /// The first message of a conversation in Auto, while the user types it.
    pub fn route_draft(&self, req: WarmRequest, text: String) {
        self.with(|c| c.route_draft(req, text))
    }
    pub fn state(&self) -> AppState {
        self.with(|c| c.state())
    }
    pub fn settings(&self) -> Settings {
        self.with(|c| c.store.settings())
    }
    pub fn save_settings(&self, patch: Settings) -> Result<Settings, String> {
        self.with(|c| c.save_settings(patch))
    }
    pub fn open(&self, id: &str) -> Option<Conversation> {
        self.with(|c| c.load(id).cloned())
    }
    pub fn remove_queued(&self, id: &str, turn_id: &str) {
        self.with(|c| c.remove_queued(id, turn_id))
    }
    pub fn send_queued(&self, id: &str, turn_id: &str) -> Result<(), String> {
        self.with(|c| c.send_queued(id, turn_id))
    }
    /// Writes to a helper at work (R161 step 5): `call_id` is the agent's helper call in conversation `id`.
    pub fn tell_helper(&self, id: &str, call_id: &str, text: &str) -> Result<(), String> {
        self.with(|c| c.tell_helper(id, call_id, text))
    }
    /// A quick question on the side of conversation `id` (R250): the question's id, or why it cannot be asked (a key).
    pub fn ask_aside(&self, id: &str, question: &str, model: Option<String>) -> Result<String, String> {
        self.with(|c| c.ask_aside(id, question, model))
    }
    pub fn stop_aside(&self, aside_id: &str) {
        self.with(|c| c.stop_aside(aside_id))
    }
    pub fn asides(&self, id: &str) -> Vec<aside::Aside> {
        self.with(|c| c.asides_of(id))
    }
    pub fn warm(&self, req: WarmRequest) {
        self.with(|c| c.warm(req))
    }
    pub fn interrupt(&self, id: &str) {
        // One lock for all of it: the end of Faber's checks waits for it, and never sends a turn in between.
        self.with(|c| {
            if let Some(conv) = c.convs.get_mut(id) {
                conv.stopped = true;
            }
            c.keep_going_off(id);
            c.compacts.remove(id);
            let compacting = c.convs.get(id).is_some_and(|c| c.compacting);
            match c.live.get(id).filter(|l| l.turn.is_some() || compacting) {
                Some(l) => {
                    l.session.interrupt();
                    c.stop_helpers(id, false);
                }
                None => {
                    // Faber's checks of the reply that just ended stop too; the reply then keeps no row.
                    c.stop_checks(id);
                    c.stop_waiting(id)
                }
            }
        })
    }
    pub fn rename(&self, id: &str, title: &str) {
        self.with(|c| c.rename(id, title))
    }
    /// A folder is chosen by the person, before any messages or files belong to the conversation.
    pub fn chat_folder(&self, id: Option<&str>, folder: Option<&Path>) -> Result<Conversation, String> {
        self.with(|c| {
            let folder = folder
                .map(|p| {
                    // Any folder can be the working folder in both modes (R121), except the system folders and the ones that hold
                    // sign-ins and keys (R124).
                    if crate::permissions::forbidden_folder(p) {
                        return Err("error.workingFolder");
                    }
                    crate::permissions::working_root(p).filter(|p| p.is_dir()).ok_or("error.chatFolder")
                })
                .transpose()?
                .map(|p| p.to_string_lossy().into_owned());
            let id = match id {
                Some(id) => {
                    let conv = c.load(id).ok_or("error.conversationGone")?;
                    if conv.area != Area::Chat || !conv.messages.is_empty() {
                        return Err("error.chatFolderLocked".into());
                    }
                    id.to_string()
                }
                None => {
                    c.drop_draft();
                    c.blank(Area::Chat, None)
                }
            };
            if let Some(live) = c.retire(&id) {
                live.session.close();
            }
            if let Some(f) = &folder {
                c.note_folder(f);
            }
            let conv = c.convs.get_mut(&id).expect("loaded conversation");
            conv.working_folder = folder;
            conv.sessions.clear();
            conv.tentative.clear();
            conv.cost_base.clear();
            let result = conv.clone();
            c.save(&id);
            c.list_changed();
            Ok(result)
        })
    }
    /// Deletes a conversation, unless its copy holds work found nowhere else (see `remove_with`).
    pub fn remove(&self, id: &str) {
        let _ = self.remove_with(id, false);
    }
    /// `@` in the composer (parity T4): the entries of the working folder of conversation `id`, or of a new conversation's
    /// project, that match `query`. A new chat has no folder yet, so it has none. The folder is checked and git runs outside
    /// the engine's lock, since either may wait on a slow disk.
    pub fn mentions(&self, id: Option<&str>, project_id: Option<&str>, query: &str) -> Vec<crate::mention::Mention> {
        let Some((folder, git)) = self.with(|c| {
            let folder = match id.and_then(|id| c.convs.get(id)) {
                Some(conv) => c.folder_of(conv).ok(),
                None => project_id.and_then(|p| c.store.projects().into_iter().find(|x| x.id == p)).map(|p| PathBuf::from(p.path)),
            };
            folder.map(|f| (f, c.git.clone()))
        }) else {
            return vec![];
        };
        let Some(folder) = crate::permissions::checked_folder(&folder) else { return vec![] };
        let mut git = git.and_then(|run| run());
        crate::mention::find(&folder, git.as_deref_mut().map(|g| g as &mut dyn crate::project::git::Executor), query)
    }
    pub fn terminal(&self, id: &str) -> Option<TerminalCommand> {
        self.with(|c| c.terminal(id))
    }
    /// The absolute path of a file in a chat workspace, or None if the path points outside it or is missing.
    /// Gives the engine Faber's own browser (R161 step 7). Sessions that start afterwards offer it to a project's agent.
    pub fn set_browser(&self, browser: std::sync::Arc<dyn crate::browser::Browser>) {
        self.with(|c| c.browser = Some(browser));
    }
    /// The picture Faber's browser took for a call, when there is one.
    pub fn site_picture(&self, id: &str, call: &str) -> Option<PathBuf> {
        self.with(|c| c.site_picture(id, call))
    }
    pub fn workspace_file(&self, id: &str, path: &str) -> Option<PathBuf> {
        self.with(|c| c.store.get(id).is_some().then(|| inside_folder(&c.workspace_of(id), path)).flatten().filter(|p| p.is_file()))
    }
    pub fn add_project(&self, path: &str) -> Result<Project, String> {
        self.with(|c| c.add_project(path))
    }
    pub fn project(&self, id: &str) -> Result<Project, String> {
        self.with(|c| {
            c.store
                .projects_checked()
                .map_err(|e| e.key().to_string())?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or_else(|| "error.project.notFound".into())
        })
    }

    pub fn projects_checked(&self) -> Result<Vec<Project>, String> {
        self.with(|c| c.store.projects_checked().map_err(|e| e.key().to_string()))
    }
    pub fn update_project(&self, before: &Project, project: &Project) -> Result<(), String> {
        self.with(|c| {
            let mut projects = c.store.projects_checked().map_err(|e| e.key().to_string())?;
            if let Some(pos) = projects.iter().position(|p| p.id == project.id) {
                if &projects[pos] != before {
                    return Err("error.project.changed".into());
                }
                if before == project {
                    return Ok(());
                }
                projects[pos] = project.clone();
                if c.store.save_projects(&projects).is_ok() {
                    c.list_changed();
                    Ok(())
                } else {
                    Err("error.store.write".into())
                }
            } else {
                Err("error.project.notFound".into())
            }
        })
    }
    /// A folder picked in a dialog or taken from the recents, as a project. A refused or missing folder adds nothing.
    pub fn open_folder(&self, path: &str) -> Result<Project, String> {
        let Some(path) = crate::recents::resolved(path) else { return Err("error.workingFolder".into()) };
        if !Path::new(&path).is_dir() {
            return Err("error.recents.gone".into());
        }
        self.with(|c| c.add_project(&path))
    }
    pub fn recents(&self) -> RecentsView {
        self.with(|c| c.store.recents().view())
    }
    pub fn forget_recent(&self, path: &str) -> Result<(), String> {
        self.change_recents(|r| {
            r.forget(path);
            Ok(())
        })
    }
    pub fn save_environment(&self, name: &str, folders: &[String]) -> Result<(), String> {
        self.change_recents(|r| r.save_environment(new_id(), name, folders).map_err(String::from))
    }
    pub fn forget_environment(&self, id: &str) -> Result<(), String> {
        self.change_recents(|r| {
            r.forget_environment(id);
            Ok(())
        })
    }
    /// Reads the recents, changes them and writes them back; a failed write is an error the screen can say, never a silent success.
    fn change_recents(&self, change: impl FnOnce(&mut crate::recents::Recents) -> Result<(), String>) -> Result<(), String> {
        self.with(|c| {
            let mut recents = c.store.recents();
            change(&mut recents)?;
            c.store.save_recents(&recents).map_err(|_| "error.recents.saveFailed".to_string())
        })
    }
    /// Opens every folder of an environment that can still be opened; fails only when none can.
    pub fn start_environment(&self, id: &str) -> Result<Vec<Project>, String> {
        let folders = self.with(|c| c.store.recents().environment(id).map(|e| e.folders.clone())).ok_or("error.environment.gone")?;
        self.open_folders(&folders)
    }
    /// Opens each folder that can be opened as a project, the first of the list ending up the most recent.
    pub fn open_folders(&self, folders: &[String]) -> Result<Vec<Project>, String> {
        let mut projects = Vec::new();
        let mut failure = String::from("error.environment.folders");
        for folder in folders.iter().rev() {
            match self.open_folder(folder) {
                Ok(p) => projects.push(p),
                Err(e) => failure = e,
            }
        }
        projects.reverse();
        if projects.is_empty() { Err(failure) } else { Ok(projects) }
    }
    /// Takes a project out of the list. A conversation still answering in it stops the removal with `error.project.busy`:
    /// the person stops it first. The person's folder stays as it is, with whatever older versions left in it, and so does
    /// whatever an older version kept in Faber's data folder.
    pub fn remove_project(&self, id: &str) -> Result<(), String> {
        self.with(|c| {
            let mut projects = c.store.projects_checked().map_err(|e| e.key().to_string())?;
            if !projects.iter().any(|p| p.id == id) {
                return Err("error.project.notFound".into());
            }
            if c.convs.values().any(|conv| conv.project_id.as_deref() == Some(id) && running_item(conv).is_some()) {
                return Err("error.project.busy".into());
            }
            projects.retain(|p| p.id != id);
            c.store.save_projects(&projects).map_err(|_| "error.store.write".to_string())?;
            c.list_changed();
            Ok(())
        })
    }
    /// Swaps the CLIs in use, for example after one was installed or signed in to. Idle processes of a changed CLI
    /// close and the shared Codex processes retire, so the next turn starts the new one; a turn in flight finishes
    /// on the old process first.
    pub fn replace_clis(&self, clis: BTreeMap<Vendor, Cli>) {
        self.with(|c| c.replace_clis(clis))
    }
    /// Asks the CLI whether it is signed in. The answer is the verdict only; the CLI's output is not kept.
    pub async fn account_status(&self, vendor: Vendor, timeout: Duration) -> AccountStatus {
        let Some((cli, spawn, cwd)) =
            self.with(|c| Some((c.opts.clis.get(&vendor)?.clone(), c.opts.spawn.clone(), c.opts.data_dir.clone())))
        else {
            return AccountStatus::Unknown;
        };
        account::check(vendor, &cli, &spawn, &cwd, timeout).await
    }
    /// The CLI's own sign-in, to run in a terminal the user sees.
    pub fn login_command(&self, vendor: Vendor) -> Option<TerminalCommand> {
        self.with(|c| {
            let cli = c.opts.clis.get(&vendor)?.clone();
            let args = cli.prefix.into_iter().chain(account::login_args(vendor).iter().map(|a| a.to_string())).collect();
            Some(TerminalCommand { command: cli.command, args, cwd: c.opts.data_dir.clone() })
        })
    }
    /// Whether any answer is streaming or waits for the user (the running list is not empty).
    /// The shell uses it to hold back anything that would end the process (installing an update).
    pub fn is_working(&self) -> bool {
        self.with(|c| {
            !c.running.items.is_empty()
                // A conversation that goes on by itself works between turns and while it waits too (R199).
                || c.convs.values().any(|conv| conv.keep_going.is_some())
        })
    }
    /// Takes up the conversations "keep going" was on in when Faber last closed, once the shell has its CLIs and accounts: each
    /// goes on after a short wait, or when its saved wait ends. Call it once at start.
    pub fn resume_keep_going(&self) {
        self.with(|c| c.resume_keep_going())
    }
    /// Scales the fixed waits of "keep going" (a retry after an error, a rest, the question timer); only tests change it.
    #[doc(hidden)]
    pub fn set_keep_going_pace(&self, scale: f64) {
        self.with(|c| c.keep_pace = scale)
    }
    pub fn shutdown(&self) {
        self.with(|c| {
            c.closing = true;
            let active: Vec<_> = c.live.keys().cloned().collect();
            for id in active {
                c.note_turn(&id, None, true);
            }
            let ids: Vec<_> = c.convs.keys().cloned().collect();
            for id in ids {
                c.stop_waiting(&id);
            }
            c.transfers.clear();
            c.credit = None;
            for (_, l) in c.live.drain() {
                l.session.close();
            }
            c.close_helpers();
            c.close_asides(None);
            for (_, server) in c.codex.drain() {
                server.retire();
            }
            c.close_spare();
            c.save_ledger(true);
            c.store.release_budget();
        })
    }
}

impl Core {
    /// The time in Unix seconds.
    fn secs(&self) -> f64 {
        self.clock.as_ref().map_or_else(unix_secs, |clock| clock())
    }

    /// Writes the ledger, but at most every half minute unless `force`: readings come with every answer. A failed write is
    /// tried again with the next reading.
    fn save_ledger(&mut self, force: bool) {
        if self.projects_closed {
            return;
        }
        let now = self.secs();
        if force || !(0.0..LEDGER_EVERY).contains(&(now - self.ledger_saved)) {
            self.ledger_failed = self.store.save_ledger(&self.ledger).is_err();
            if !self.ledger_failed {
                self.ledger_saved = now;
            }
        }
    }

    /// A reading of one quota window: kept for the screen, remembered by the ledger, and told to the screen.
    fn note_quota(&mut self, vendor: Vendor, window: &str, used: f64, resets_at: f64) {
        // A reading that cannot be real (a CLI that changed its format) is shown nowhere and learned from by no one.
        if !plausible(used, resets_at) {
            return;
        }
        let now = self.secs();
        let reading = QuotaReading { vendor, window: window.to_string(), used, resets_at };
        self.quotas.insert((vendor, window.to_string()), reading);
        self.ledger.of_mut(vendor).record(window, used, resets_at, now, self.utc_offset);
        self.save_ledger(false);
        (self.emit)(AppEvent::Quota { vendor, window: window.to_string(), used, resets_at });
        self.stop_unapproved_paid(vendor, false);
        self.emit_usage();
        self.keep_going_recheck();
    }

    /// A whole reading of the account (every window of it noted just now) with room in every vendor-wide window overrules a
    /// limit the CLI reported without a reset, which before only a turn could clear; keep going waiting on it goes on.
    pub(super) fn reading_done(&mut self, vendor: Vendor) {
        let now = self.secs();
        let v = self.ledger.of_mut(vendor);
        if v.reach == Reach::Out(None) && v.snap(now).windows_show_room(now) {
            v.reach = Reach::Clear;
            self.save_ledger(true);
            self.emit_usage();
            self.keep_going_recheck();
        }
    }

    /// What a CLI said about the account: merged into what the ledger knows, field by field.
    fn apply_signals(&mut self, vendor: Vendor, s: Signals) {
        let now = self.secs();
        let v = self.ledger.of_mut(vendor);
        let spent = matches!((v.credit_balance, s.credit_balance), (Some(before), Some(after)) if after < before);
        if s.credit_balance.is_some() {
            v.credit_balance = s.credit_balance;
        }
        if let Some(reach) = s.reach {
            v.reach = reach;
        }
        if let Some(on) = s.overage_active {
            v.overage_active = on;
        }
        if let Some(possible) = s.paid_fallback {
            v.paid_fallback = Some(possible);
            v.paid_fallback_at = Some(now);
        }
        if let Some(billing) = s.billing {
            v.billing = billing;
        }
        if s.plan.is_some() {
            v.plan_detected(s.plan, now);
        }
        self.stop_unapproved_paid(vendor, spent);
        self.emit_usage();
        self.keep_going_recheck();
    }

    fn stop_unapproved_paid(&mut self, vendor: Vendor, spent: bool) {
        let v = self.ledger.of(vendor);
        let paid = spent || v.overage_active || (v.paid_fallback.unwrap_or(true) && v.snap(self.secs()).exhausted(self.secs()));
        let unknown = v.billing == Billing::Unknown;
        let billed = v.billing == Billing::UsageBilled;
        if unknown || billed || paid {
            self.stop_paid_helpers(vendor);
        }
        self.stop_paid_asides(vendor, paid && !billed);
        let stopped: Vec<String> = self
            .live
            .iter()
            .filter(|(_, l)| {
                l.vendor == vendor
                    && l.turn.is_some()
                    && ((!l.by_hand && (unknown || billed)) || (paid && !l.approved && !(l.by_hand && billed)))
            })
            .map(|(id, _)| id.clone())
            .collect();
        if paid && !stopped.is_empty() {
            self.ledger.of_mut(vendor).reach = Reach::Out(None);
        }
        for id in stopped {
            self.note_turn(&id, None, true);
            self.stop_helpers(&id, true);
            if let Some(l) = self.retire(&id) {
                l.session.close();
            }
            // With "keep going" on, a turn the person picked by hand moves too: nobody is there to click it.
            let unattended = self.convs.get(&id).is_some_and(|c| c.keep_going.is_some());
            if paid && !unknown && !billed && self.transfers.get(&id).is_some_and(|t| !t.by_hand || unattended) {
                if let Err(error) = self.recover(&id, false)
                    && let Some(reply) = self.transfers.get(&id).map(|t| t.reply.clone())
                {
                    self.fail_reply(&id, &reply, error);
                }
                continue;
            }
            if let Some(m) = self.convs.get_mut(&id).and_then(|c| c.messages.last_mut()) {
                m.status = Some(Status::Stopped);
                m.permissions = None;
                m.error = Some(
                    if unknown {
                        "error.billingUnknown"
                    } else if billed {
                        "error.billedPerUse"
                    } else {
                        "error.quotaOut"
                    }
                    .into(),
                );
                let reply_id = m.id.clone();
                self.finish(&id, Some(reply_id));
            }
        }
    }

    /// A request Faber made itself (an answer, a router question, a pulse): the learning has to know when it ran, so that use
    /// it did not cause can be told from use it did.
    fn note_span(&mut self, vendor: Vendor, start: f64, model: &str, cost: Option<f64>, incomplete: bool) {
        let now = self.secs();
        let v = self.ledger.of(vendor);
        let end =
            v.primary(now).and_then(|name| v.readings.get(name)).map(|r| r.read_at).filter(|at| *at >= start && *at <= now).unwrap_or(now);
        self.ledger.of_mut(vendor).spend(cost, incomplete, now, self.utc_offset);
        self.ledger.of_mut(vendor).span(Span { start, end, model: model.to_string(), cost, incomplete });
        self.ledger.of_mut(vendor).worked(Work { start, finished: now, model: model.to_string(), cost, incomplete }, now);
        self.quota_event();
        self.ledger.of_mut(vendor).learn_weights(now);
        if crate::project::budget::finish_unbounded(&mut self.ledger, vendor).is_err() {
            self.projects_closed = true;
        }
        self.save_ledger(true);
        self.emit_usage();
    }

    fn note_turn(&mut self, conv: &str, cost: Option<f64>, incomplete: bool) {
        let Some(l) = self.live.get(conv).filter(|l| l.started > 0.0) else { return };
        let (vendor, start, model) = (l.vendor, l.started, l.running.clone().or_else(|| l.model.clone()).unwrap_or_default());
        self.note_span(vendor, start, &model, cost, incomplete);
        // The turn ended: the quick questions asked before it began are over.
        self.drop_asides(conv, start);
        if let Some(l) = self.live.get_mut(conv) {
            l.started = 0.0;
        }
    }

    fn claude_plan_checked(&mut self, found: Option<Signals>) {
        let Some(mut s) = found else { return self.ledger.claude.plan_failed() };
        if self.opts.claude_env_billed && s.billing == Some(Billing::Subscription) {
            s.billing = Some(Billing::UsageBilled);
        }
        let (named, by_use) = (s.plan.is_some(), s.billing == Some(Billing::UsageBilled));
        self.apply_signals(Vendor::Claude, s);
        if !named && !by_use {
            self.ledger.claude.plan_missing();
        }
    }

    fn codex_plan_checked(&mut self, events: Vec<EngineEvent>) {
        if events.is_empty() {
            return self.ledger.codex.plan_failed();
        }
        self.zero_turn_checked(Vendor::Codex, events);
        if self.ledger.codex.plan.is_none() && self.ledger.codex.billing != Billing::UsageBilled {
            self.ledger.codex.plan_missing();
        }
    }

    fn zero_turn_checked(&mut self, vendor: Vendor, events: Vec<EngineEvent>) {
        for event in events {
            if let EngineEvent::Quota { window, used, resets_at } = &event {
                let now = self.secs();
                if plausible(*used, *resets_at) && *resets_at > now {
                    self.ledger.of_mut(vendor).zero_turn_readings.insert(
                        window.clone(),
                        crate::ledger::Reading {
                            used: (used * 1000.0).round() / 1000.0,
                            resets_at: resets_at.round(),
                            read_at: now.round(),
                        },
                    );
                }
            }
            self.note_account_event(vendor, event);
        }
        self.save_ledger(true);
    }

    fn note_account_event(&mut self, vendor: Vendor, event: EngineEvent) {
        match event {
            EngineEvent::Quota { window, used, resets_at } => self.note_quota(vendor, &window, used, resets_at),
            EngineEvent::Signals(s) => self.apply_signals(vendor, s),
            _ => {}
        }
    }

    fn vendors(&self) -> Vec<Vendor> {
        self.opts.clis.keys().copied().collect()
    }

    fn replace_clis(&mut self, clis: BTreeMap<Vendor, Cli>) {
        let changed = |v: Vendor| self.opts.clis.get(&v) != clis.get(&v);
        let stale: Vec<String> = self
            .live
            .iter()
            .filter(|(id, l)| l.turn.is_none() && !self.compacting(id) && changed(l.vendor))
            .map(|(id, _)| id.clone())
            .collect();
        if changed(Vendor::Codex) {
            for (_, server) in self.codex.drain() {
                server.retire();
            }
        }
        for id in stale {
            if let Some(l) = self.retire(&id) {
                l.session.close();
            }
        }
        self.opts.clis = clis;
    }

    fn scratch_of(&self, id: &str) -> PathBuf {
        self.opts.data_dir.join("chat").join(id)
    }

    fn workspace_of(&self, id: &str) -> PathBuf {
        let folder = match self.convs.get(id) {
            Some(c) => c.working_folder.clone(),
            None => self.store.get(id).and_then(|c| c.working_folder),
        };
        folder.map(PathBuf::from).unwrap_or_else(|| self.scratch_of(id))
    }

    fn cwd_of(&self, c: &Conversation) -> Result<PathBuf, String> {
        let dir = self.folder_of(c)?;
        if c.area == Area::Chat && c.working_folder.is_none() {
            std::fs::create_dir_all(&dir).map_err(|_| "error.projectSandbox")?;
        }
        // The private folder and a chosen one are checked alike on every launch: a link put there since may lead somewhere
        // that is not allowed. The CLI starts in the path that was checked.
        crate::permissions::checked_folder(&dir).ok_or_else(|| "error.workingFolder".to_string())
    }

    /// Where conversation `c` works, before it is checked (`cwd_of`). Reads only the engine's own state.
    fn folder_of(&self, c: &Conversation) -> Result<PathBuf, String> {
        if c.area == Area::Chat {
            return Ok(self.workspace_of(&c.id));
        }
        self.store
            .projects()
            .into_iter()
            .find(|p| Some(&p.id) == c.project_id.as_ref())
            // A conversation with a copy of its own works there; one removed after its work was merged comes back at the
            // next message, before anything runs.
            .map(|p| c.copy.as_ref().map_or_else(|| PathBuf::from(p.path), |copy| copy.folder()))
            .ok_or_else(|| "error.projectGone".to_string())
    }

    fn load(&mut self, id: &str) -> Option<&mut Conversation> {
        if !self.convs.contains_key(id) {
            let mut c = self.store.get(id)?;
            // Read from disk, so no CLI runs it: an answer cut off by a crash shows as stopped, not typing forever.
            c.messages.iter_mut().for_each(settle);
            c.queued.iter_mut().for_each(|q| q.sending = false);
            self.restore_recovery(&mut c);
            self.convs.insert(id.to_string(), c);
        }
        let c = self.convs.get_mut(id)?;
        // A Codex session saved before R121 carries the isolated host command tool, which no mode the person can pick uses
        // any more: it restarts once, with visible history, without it.
        if c.command_gate {
            c.forget_session(Vendor::Codex);
            c.cost_base.remove(&Vendor::Codex);
            c.command_gate = false;
        }
        Some(c)
    }

    fn save(&self, id: &str) {
        if let Some(c) = self.convs.get(id) {
            if c.messages
                .last()
                .is_none_or(|m| !matches!(m.error.as_deref(), Some("error.quotaOut" | "error.quotaWait")) && m.recovery.is_none())
            {
                self.store.save(c);
                return;
            }
            let mut saved = c.clone();
            if let Some(m) = saved.messages.last_mut() {
                m.recovery = self
                    .transfers
                    .get(id)
                    .filter(|t| {
                        t.reply == m.id
                            && m.status == Some(Status::Error)
                            && matches!(m.error.as_deref(), Some("error.quotaOut" | "error.quotaWait"))
                    })
                    .map(|t| Recovery {
                        pick: t.pick.clone(),
                        text: t.text.clone(),
                        by_hand: t.by_hand,
                        retried: t.retried,
                        check: t.check.into(),
                    });
            }
            self.store.save(&saved);
        }
    }

    fn state(&self) -> AppState {
        let mut quotas: Vec<QuotaReading> = self.quotas.values().cloned().collect();
        quotas.sort_by(|a, b| (a.vendor, &a.window).cmp(&(b.vendor, &b.window)));
        AppState {
            conversations: self.store.list().into_iter().filter(|c| !c.title.is_empty()).collect(),
            projects: self.store.projects(),
            vendors: self.vendors(),
            settings: self.store.settings(),
            running: self.running.clone(),
            quotas,
            usage: self.usage(),
        }
    }

    /// Whether the person's "Always allow" for this tool in this conversation still counts: it must have been made under the
    /// settings' current grants epoch (no epoch on either side matches: an install that never revoked anything).
    fn grant_stands(&self, conv: &str, tool: &str) -> bool {
        let Some(c) = self.convs.get(conv) else { return false };
        if !c.allow_always.as_ref().is_some_and(|a| a.iter().any(|t| t == tool)) {
            return false;
        }
        c.allow_always_epochs.get(tool) == self.store.settings().grants_epoch.as_ref()
    }

    /// Keeps the merged settings, or fails with `error.settings.saveFailed` before anything live changes.
    fn save_settings(&mut self, patch: Settings) -> Result<Settings, String> {
        let before = self.store.settings();
        let before_tuning = before.tuning();
        let mut settings = Settings {
            appearance: patch.appearance.or(before.appearance),
            user_hooks: patch.user_hooks.or(before.user_hooks),
            permissions: patch.permissions.or(before.permissions),
            web_search: patch.web_search.or(before.web_search),
            reasoning_summary: patch.reasoning_summary.or(before.reasoning_summary),
            verbosity: patch.verbosity.or(before.verbosity),
            notifications: patch.notifications.or(before.notifications),
            notification_sound: patch.notification_sound.or(before.notification_sound),
            language: patch.language.or(before.language),
            setup: patch.setup.or(before.setup),
            experience: patch.experience.or(before.experience),
            tutorial_done: patch.tutorial_done.or(before.tutorial_done),
            user_name: patch.user_name.map_or(before.user_name.clone(), |name| clean_name(&name)),
            keep_awake: patch.keep_awake.or(before.keep_awake),
            autostart: patch.autostart.or(before.autostart),
            grants_epoch: before.grants_epoch,
            previews: patch.previews.or(before.previews),
        };
        // A stricter mode revokes every remembered "Always allow" in the very save that changes the mode, by replacing the grants
        // epoch: a grant made under another one never counts again, even if a conversation file below fails to update.
        // Settings that could not be read already came with a fresh epoch (see `Store::settings`), and this save keeps it.
        let (was, to) = (before.permissions.unwrap_or_default(), settings.permissions());
        if to.strictness() > was.strictness() {
            settings.grants_epoch = Some(crate::model::new_epoch());
        }
        self.store.save_settings(&settings).map_err(|_| "error.settings.saveFailed".to_string())?;
        // A turn keeps the mode it started with until it ends, so moving to a stricter mode stops the turns under way (a turn
        // that began under a stricter mode keeps it; moving to a looser one does not touch it).
        let stricter = to.strictness() > was.strictness() || settings.tuning().web.strictness() > before_tuning.web.strictness();
        // A stricter mode never inherits what a looser one was told: every "Always allow" is forgotten.
        if to.strictness() > was.strictness() {
            let ids: Vec<String> = self.convs.iter().filter(|(_, c)| c.allow_always.is_some()).map(|(id, _)| id.clone()).collect();
            for id in ids {
                if let Some(c) = self.convs.get_mut(&id) {
                    c.allow_always = None;
                    c.allow_always_epochs.clear();
                }
                self.save(&id);
            }
            self.store.clear_grants(&|id| self.convs.contains_key(id));
        }
        if stricter {
            let active: Vec<String> = self.live.iter().filter(|(_, live)| live.turn.is_some()).map(|(id, _)| id.clone()).collect();
            for id in active {
                if let Some(live) = self.retire(&id) {
                    live.session.interrupt();
                    live.session.close();
                    if let Some(message) =
                        live.turn.as_ref().and_then(|turn| self.convs.get_mut(&id)?.messages.iter_mut().find(|m| &m.id == turn))
                    {
                        settle(message);
                    }
                    self.finish(&id, live.turn);
                }
            }
        }
        // The blank conversation being typed was started with the old name; the next one starts with the new one.
        if settings.user_name != before.user_name {
            self.drop_draft();
        }
        // Idle CLIs restart on their next turn when permissions change; hooks remain a Projects choice.
        let tuned = settings.tuning() != before_tuning;
        if settings.user_hooks != before.user_hooks || settings.permissions != before.permissions || tuned {
            let idle: Vec<String> = self
                .live
                .iter()
                .filter(|(id, l)| {
                    l.turn.is_none()
                        && !self.compacting(id)
                        && (settings.permissions != before.permissions
                            || tuned
                            || self.convs.get(*id).is_some_and(|c| c.area == Area::Project))
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in idle {
                if let Some(l) = self.retire(&id) {
                    l.session.close();
                }
            }
            let stale: Vec<_> = self
                .codex
                .keys()
                .filter(|(area, _, p)| {
                    *p != Permissions::Isolated && (*area == Area::Project || settings.permissions != before.permissions)
                })
                .copied()
                .collect();
            for key in stale {
                if let Some(server) = self.codex.remove(&key) {
                    server.retire();
                }
            }
        }
        Ok(settings)
    }

    /// Starts the hidden `--help` read (see `Engine::detect_claude_flags`); its answer replaces the flags known so far.
    fn probe_claude_flags(&mut self) {
        let Some(cli) = self.opts.clis.get(&Vendor::Claude).cloned() else { return };
        // The answer belongs to the CLI as it is now: if it changes again before the read ends, the newer read decides.
        let stamp = cli.stamp();
        self.claude_stamp = Some(stamp.clone());
        let (me, spawn, cwd) = (self.me.clone(), self.opts.spawn.clone(), self.opts.data_dir.clone());
        self.opts.runtime.spawn(async move {
            let Some(flags) = account::claude_flags(&cli, &spawn, &cwd, Duration::from_secs(20)).await else { return };
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if core.claude_stamp.as_ref() == Some(&stamp) {
                    core.claude_flags = flags;
                }
            }
        });
    }

    /// The newer flags to pass to a Claude Code launch. When the CLI is not the one the flags were read from (an update, or a
    /// downgrade to a version that lacks one), none are passed on this launch and the help is read again for the next ones. Before
    /// the first read there is nothing to compare with, and nothing is known either.
    fn claude_flags_for(&mut self, cli: &Cli) -> crate::claude::ClaudeFlags {
        if self.claude_stamp.as_ref().is_some_and(|read| *read != cli.stamp()) {
            self.claude_flags = Default::default();
            self.probe_claude_flags();
        }
        self.claude_flags
    }

    /// A tool of the conversation's replies. A message steered into the turn starts a new reply, so a tool that began in one
    /// before still ends there, however many messages came since (a helper in the background outlives several); the current
    /// reply is looked at first, then the others from the newest.
    fn tool_of(&mut self, conv: &str, id: &str) -> Option<&mut ToolUse> {
        let current = self.live.get(conv).and_then(|l| l.turn.clone());
        tool_in(&mut self.convs.get_mut(conv)?.messages, current.as_deref(), id)
    }

    /// A subagent's call that just ended gets the facts of what it did (R161 step 4); any other step is left as it is.
    fn settle_facts(&mut self, conv: &str, id: &str) {
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(owner) = c.messages.iter().rposition(|m| m.tools.iter().flatten().any(|t| t.id == id && t.ok.is_some())) else { return };
        // A message steered into the running turn starts a new reply, and the subagent's later steps land there: its facts
        // count the steps of its own reply and of every reply after it.
        let tools: Vec<ToolUse> = c.messages[owner..].iter().flat_map(|m| m.tools.iter().flatten().cloned()).collect();
        let Some(call) = tools.iter().find(|t| t.id == id) else { return };
        let facts = crate::engine::facts::of(&tools, call);
        let m = &mut c.messages[owner];
        if let Some(call) = m.tools.iter_mut().flatten().find(|t| t.id == id) {
            call.facts = facts;
        }
        let (owner, live) = (m.id.clone(), self.live.get(conv).and_then(|l| l.turn.clone()));
        // Sent at once and apart from the live reply's own update, which a pending one would swallow.
        if live.as_ref() != Some(&owner) {
            self.emit_message(conv, &owner);
        }
    }

    fn turn(&mut self, conv: &str) -> Option<&mut Message> {
        let id = self.live.get(conv)?.turn.clone()?;
        self.convs.get_mut(conv)?.messages.iter_mut().find(|m| m.id == id)
    }

    fn handle(&mut self, conv: &str, sid: u64, event: EngineEvent) {
        let Some(entry) = self.live.get(conv) else { return };
        if entry.sid != sid {
            return;
        }
        let (vendor, resumed) = (entry.vendor, entry.resumed);
        let turn_id = entry.turn.clone();
        // A turn ran on the session: from here on it is the conversation's session for this vendor.
        if !matches!(
            event,
            EngineEvent::Started { .. }
                | EngineEvent::TurnStarted
                | EngineEvent::Quota { .. }
                | EngineEvent::Signals(_)
                | EngineEvent::Context { .. }
                | EngineEvent::Error { .. }
        ) && let Some(id) = self.live.get_mut(conv).and_then(|l| l.unsaved_session.take())
            && let Some(c) = self.convs.get_mut(conv)
        {
            c.sessions.insert(vendor, id);
            c.tentative.remove(&vendor);
            if vendor == Vendor::Codex {
                c.command_gate = self.live.get(conv).is_some_and(|l| l.permissions == Permissions::Isolated);
            }
            // a draft is saved only once something is sent
            if !c.messages.is_empty() {
                self.save(conv);
            }
        }
        // While the context is compacted only its end matters (`compact.rs`).
        if self.convs.get(conv).is_some_and(|c| c.compacting) && self.compaction_event(conv, &event) {
            return;
        }
        // What the reply says or does moves it: its age counts from here, and more words after the agent finished mean it was not done.
        if let Some(main) = event.moves_reply()
            && let Some(m) = self.turn(conv)
        {
            m.moved_at = Some(now());
            if main {
                m.finished_at = None;
            }
        }
        match event {
            EngineEvent::Context { used, window } => return self.note_context(conv, vendor, used, window),
            EngineEvent::Compacted { after } => return self.note_compacted(conv, vendor, after),
            // Codex accepted the turn: if Faber dies now, the next launch resumes this session instead of starting one that never
            // heard of the turn.
            EngineEvent::TurnStarted => {
                let fresh = self.live.get(conv).and_then(|l| l.unsaved_session.clone());
                if let (Some(id), Some(c)) = (fresh, self.convs.get_mut(conv))
                    && c.sessions.get(&vendor) != Some(&id)
                {
                    c.tentative.insert(vendor, id);
                    if !c.messages.is_empty() {
                        self.save(conv);
                    }
                }
                return;
            }
            EngineEvent::Started { session_id, model } => {
                // A CLI started while the user typed reports its model before any answer exists; later answers use it too.
                if let Some(l) = self.live.get_mut(conv) {
                    l.running = Some(model.clone());
                }
                if let Some(m) = self.turn(conv) {
                    m.model = Some(model);
                }
                // Not saved yet: the id becomes resumable with the first sign that a turn ran on it (above).
                if let Some(l) = self.live.get_mut(conv) {
                    l.unsaved_session = Some(session_id);
                }
                // The turn's reply now names its model: the screen hears it now, not with the first word.
                self.refresh_running();
                return self.push(conv, turn_id.as_deref(), true);
            }
            EngineEvent::Break { final_answer } => {
                if final_answer && self.live.get(conv).is_some_and(|l| !l.answer_started) {
                    if let Some(m) = self.turn(conv) {
                        m.activity_end = Some(m.text.encode_utf16().count());
                    }
                    if let Some(l) = self.live.get_mut(conv) {
                        l.answer_started = true;
                    }
                }
                let has_text = self.turn(conv).is_some_and(|m| !m.text.is_empty());
                if let (true, Some(l)) = (has_text, self.live.get_mut(conv)) {
                    l.break_pending = true;
                }
                return;
            }
            // The main thread said all it will say; a tool call still open (a helper in the background) means it is not done.
            EngineEvent::Settled => {
                if let Some(m) = self.turn(conv)
                    && m.tools.iter().flatten().all(|t| t.ok.is_some() || t.parent.is_some())
                {
                    m.finished_at = Some(now());
                }
            }
            EngineEvent::Text(delta) => {
                let pending = self.live.get(conv).is_some_and(|l| l.break_pending);
                let Some(m) = self.turn(conv) else { return };
                if pending && !delta.is_empty() {
                    m.text.push_str("\n\n");
                    m.text_breaks.get_or_insert_default().push(m.text.encode_utf16().count());
                }
                m.text.push_str(&delta);
                if pending
                    && !delta.is_empty()
                    && let Some(l) = self.live.get_mut(conv)
                {
                    l.break_pending = false;
                }
            }
            EngineEvent::Thinking(delta) => {
                if let Some(m) = self.turn(conv) {
                    m.thinking.get_or_insert_default().push_str(&delta);
                }
            }
            EngineEvent::Tool { id, name, input, parent } => {
                let final_answer = self.live.get(conv).is_some_and(|l| l.answer_started);
                if let Some(m) = self.turn(conv) {
                    // A subagent works while the agent waits for it, so its tools never move where the answer begins.
                    if !final_answer && parent.is_none() {
                        m.activity_end = Some(m.text.encode_utf16().count());
                    }
                    let mut tool = ToolUse {
                        id: id.clone(),
                        name,
                        input,
                        ok: None,
                        output: None,
                        declined: false,
                        parent,
                        sign: None,
                        facts: None,
                        told: vec![],
                        check: false,
                        started: Some(now()),
                    };
                    tool.check = crate::nudge::is_check(&tool);
                    m.tools.get_or_insert_default().push(tool);
                }
                self.watch_started(conv, &id);
            }
            EngineEvent::ToolResult { id, ok, declined, text } => {
                if let Some(tool) = self.tool_of(conv, &id) {
                    // A helper sent to the background answers its call as soon as it starts; its end comes as `HelperDone`.
                    if !declined && crate::model::runs_in_background(tool) {
                        return;
                    }
                    tool.ok = Some(ok);
                    tool.declined = declined;
                    tool.output = Some(text.chars().take(4000).collect());
                }
                self.settle_facts(conv, &id);
                self.watch_ended(conv, &id);
                self.pr_seen(conv, &id, &text);
            }
            EngineEvent::HelperDone { id, ok, text } => {
                // A helper that ran with its call is already settled by the call's own result, its full report included.
                if let Some(tool) = self.tool_of(conv, &id).filter(|t| t.ok.is_none()) {
                    tool.ok = Some(ok);
                    tool.output = Some(text.chars().take(4000).collect());
                }
                self.settle_facts(conv, &id);
                self.watch_ended(conv, &id);
            }
            EngineEvent::HelperCall { id, input } => return self.start_helper(conv, id, input),
            EngineEvent::BrowseCall { id, input } => return self.browse(conv, id, input),
            EngineEvent::ConversationCall { id, input } => return self.conversation_call(conv, id, input),
            EngineEvent::VerifyCall { id, input } => return self.verify_call(conv, id, input),
            EngineEvent::MapCall { id, input } => return self.map_call(conv, id, input),
            EngineEvent::ProgressCall { id, tool, input } => return self.progress_call(conv, id, tool, input),
            EngineEvent::Permission { request_id, tool, input } => {
                // A request that touches a credential folder is refused in both modes, with no card (best effort for a command
                // line: see `touches_credentials`). Full access never shows a card. The answer is decided here, under the lock
                // that also saves the mode, so a request that was already on its way when the person switched to Ask finds the
                // session gone (and is never accepted).
                // A question the agent asks the person is always a card, in every mode: only the person can answer it.
                let question = tool == "AskUserQuestion";
                let cwd = self.convs.get(conv).and_then(|c| self.cwd_of(c).ok());
                if let Some(l) = self.live.get(conv)
                    && !question
                    && crate::permissions::touches_credentials(&input, cwd.as_deref().and_then(Path::to_str), &|name| {
                        std::env::var(name).ok()
                    })
                {
                    drop(l.session.answer(&request_id, false));
                    return;
                }
                if !question && self.live.get(conv).is_some_and(|l| l.permissions == Permissions::FullAccess) {
                    // Claude Code asks the host in Full access only for what bypass mode still holds back (the person's own ask
                    // rules, protected paths): on a CLI that has `--permission-prompts` it is denied and parked, never allowed on its own, as
                    // `--permission-prompts none` did; an older CLI keeps the yes it always had (R124).
                    if vendor == Vendor::Claude && self.claude_flags.permission_prompts {
                        if let Some(l) = self.live.get(conv) {
                            drop(l.session.answer(&request_id, false));
                        }
                        self.park(conv, request_id, tool, input);
                        return self.push(conv, turn_id.as_deref(), true);
                    }
                    return self.auto_answer(conv, vec![Permission { request_id, tool, input, epoch: None }]);
                }
                // Plan never takes a remembered grant: nothing may change under it without a card.
                let plan = self.live.get(conv).is_some_and(|l| l.permissions == Permissions::Plan);
                let allowed = !question && !plan && self.grant_stands(conv, &tool);
                if allowed {
                    return self.auto_answer(conv, vec![Permission { request_id, tool, input, epoch: None }]);
                }
                let epoch = self.store.settings().grants_epoch;
                let asked = question.then(|| request_id.clone());
                if let Some(m) = self.turn(conv) {
                    m.permissions.get_or_insert_default().push(Permission { request_id, tool, input, epoch });
                }
                self.weigh(conv);
                if let (Some(request), Some(turn)) = (asked, turn_id.as_deref()) {
                    self.watch_question(conv, turn, &request);
                }
                return self.push(conv, turn_id.as_deref(), true);
            }
            EngineEvent::CacheWarm { secs, started_ms } => {
                // The lifetime counts from the start of the request, not from the result: a long answer has used part of it.
                if let (Some(m), Some(started)) = (self.turn(conv), started_ms) {
                    m.cache_warm_until = Some(started.saturating_add(secs.saturating_mul(1000)).saturating_sub(WARM_MARGIN_MS));
                }
            }
            EngineEvent::Parked { tool_use_id, tool, input } => self.park(conv, tool_use_id, tool, input),
            EngineEvent::Injected { tag } => return self.taken_in(conv, tag),
            EngineEvent::SteerFailed { tag } => return self.steer_failed(conv, &tag),
            EngineEvent::Quota { window, used, resets_at } => return self.note_quota(vendor, &window, used, resets_at),
            EngineEvent::Signals(s) => return self.apply_signals(vendor, s),
            EngineEvent::Done {
                ok,
                cost_total_usd,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                cache_write_1h_tokens,
                incomplete,
                reason,
            } => {
                // Claude reports a conversation total; Codex has already priced this turn's individual requests.
                let session = self.convs.get(conv).and_then(|c| c.sessions.get(&vendor)).cloned().unwrap_or_default();
                let area = self.convs.get(conv).map_or(Area::Chat, |c| c.area);
                let fresh = self.live.get(conv).is_some_and(|l| !l.resumed);
                let first = self.live.get_mut(conv).is_some_and(|l| !std::mem::replace(&mut l.served, true));
                let before = self.convs.get(conv).and_then(|c| c.cost_base.get(&vendor));
                let settled = if vendor == Vendor::Codex {
                    usage::Settled { cost: cost_total_usd.filter(|cost| cost.is_finite() && *cost >= 0.0), base: None }
                } else {
                    usage::settle(before, &session, fresh, cost_total_usd)
                };
                if let (Some(base), Some(c)) = (settled.base, self.convs.get_mut(conv)) {
                    c.cost_base.insert(vendor, base);
                }
                let model = self.live.get(conv).and_then(|l| l.running.as_deref().or(l.model.as_deref())).unwrap_or_default();
                let native = settled.cost;
                let cost = native.or_else(|| {
                    usage::api_price_for_prompt(
                        model,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cache_write_tokens,
                        cache_write_1h_tokens,
                        if vendor == Vendor::Codex { 0 } else { input_tokens },
                    )
                });
                // A first turn after a resume starts from whatever the CLI's cache still holds, which is what an idle close or a
                // change of effort costs; it is logged (resumed) where a fresh process's first turn, always cold, is not.
                let resumed = first && self.live.get(conv).is_some_and(|l| l.resumed);
                if usage::worth_logging(input_tokens, cache_read_tokens, first && !resumed) {
                    let effort = self.live.get(conv).map(|l| l.effort.as_str());
                    self.store.log_cache_health(&serde_json::json!({
                        "at": now(), "conv": conv, "vendor": vendor.name(), "model": model, "effort": effort, "input": input_tokens,
                        "read": cache_read_tokens, "written": cache_write_tokens, "written1h": cache_write_1h_tokens,
                        "share": usage::cache_share(input_tokens, cache_read_tokens).map(|s| (s * 1000.0).round() / 1000.0),
                        "resumed": resumed,
                    }));
                }
                self.unwatch(conv);
                let missing = cost.is_none() || ((native.is_none() || vendor == Vendor::Codex) && incomplete);
                self.note_turn(conv, cost, missing);
                let can_run = crate::route::candidates(&self.vendors(), &self.codex_models);
                if let Some(m) = self.turn(conv) {
                    m.status = Some(if ok { Status::Done } else { Status::Stopped });
                    // Only a turn that ended well is done: a failed one may still be waited on or moved, and works again then.
                    if ok {
                        m.finished_at.get_or_insert_with(now);
                    } else {
                        m.finished_at = None;
                    }
                    m.cost_usd = cost.map(|cost| m.cost_usd.unwrap_or(0.0) + cost).or(m.cost_usd);
                    m.input_tokens = Some(input_tokens);
                    m.output_tokens = Some(output_tokens);
                    m.cache_read_tokens = Some(cache_read_tokens);
                    m.cache_write_tokens = Some(cache_write_tokens);
                    m.cache_write_1h_tokens = Some(cache_write_1h_tokens);
                    m.incomplete = (incomplete || m.incomplete.unwrap_or(false)).then_some(true);
                    m.permissions = None;
                    m.nudge = nudge::of(m, area, &can_run);
                }
                // A limit hit again after a move goes to recovery when keep going is on, which moves again (up to `MAX_MOVES`).
                let unattended = self.convs.get(conv).is_some_and(|c| c.keep_going.is_some());
                if !ok && self.transfers.get(conv).is_some_and(|t| t.moving) && !(unattended && recovery::plan_limit(&reason)) {
                    self.stop_helpers(conv, true);
                    if let Some(reply) = turn_id {
                        self.move_failed(conv, &reply);
                    }
                    return;
                }
                if !ok && recovery::plan_limit(&reason) {
                    self.ledger.of_mut(vendor).reach = Reach::Out(None);
                    self.save_ledger(true);
                    self.stop_helpers(conv, true);
                    if let Some(l) = self.retire(conv) {
                        l.session.close();
                    }
                    if let Err(e) = self.recover(conv, false)
                        && let Some(reply) = turn_id
                    {
                        self.fail_reply(conv, &reply, e);
                    }
                    return;
                }
                return self.finish(conv, turn_id);
            }
            EngineEvent::Error { message, fatal } => {
                // The CLI no longer has the session this launch resumed (a warm-up that never ran a turn, or a cleaned-up history).
                let mut message = message;
                if fatal && resumed && recovery::session_lost(&message) {
                    // Only a session that never took a turn may be replaced silently: not one whose turn was accepted, and not
                    // a reply that already holds work (partial text, tool actions, a retry after the quota ran out).
                    let silent = !self.convs.get(conv).is_some_and(|c| c.tentative.contains_key(&vendor))
                        && !self.reply_has_work(conv, turn_id.as_deref())
                        && !self.said_on(conv, vendor, turn_id.as_deref());
                    if let Some(c) = self.convs.get_mut(conv) {
                        c.forget_session(vendor);
                    }
                    // A new session with the same settings runs this turn once; a fresh launch is never `resumed`, so it cannot loop.
                    if silent && self.rerun_fresh(conv, turn_id.as_deref()) {
                        return;
                    }
                    // Turns did run there: say so, and let the next message start a session that is given the visible history.
                    message = "error.sessionLost".into();
                }
                // A CLI that refuses a flag Faber passed (it was changed under a running Faber): forget the newer flags at once, so the
                // next launch is the old one, and read the new help.
                if fatal && recovery::flag_rejected(&message) && self.claude_flags != Default::default() {
                    self.claude_flags = Default::default();
                    self.probe_claude_flags();
                }
                let limited = fatal && recovery::plan_limit(&message);
                if let Some(m) = self.turn(conv) {
                    m.error = Some(message);
                    if fatal {
                        m.status = Some(Status::Error);
                        m.permissions = None;
                    }
                }
                if fatal {
                    self.unwatch(conv);
                    self.note_turn(conv, None, true);
                    self.stop_helpers(conv, true);
                    if let Some(l) = self.retire(conv) {
                        l.session.close();
                    }
                    // What was handed to the turn and never taken in died with the process: it waits in the queue again.
                    if let Some(c) = self.convs.get_mut(conv).filter(|c| c.queued.iter().any(|q| q.sending)) {
                        c.queued.iter_mut().for_each(|q| q.sending = false);
                        self.emit_queue(conv);
                    }
                    let unattended = self.convs.get(conv).is_some_and(|c| c.keep_going.is_some());
                    if self.transfers.get(conv).is_some_and(|t| t.moving) && !(unattended && limited) {
                        if let Some(reply) = turn_id {
                            self.move_failed(conv, &reply);
                        }
                        return;
                    }
                    if limited {
                        self.ledger.of_mut(vendor).reach = Reach::Out(None);
                        self.save_ledger(true);
                        if let Err(e) = self.recover(conv, false)
                            && let Some(reply) = turn_id
                        {
                            self.fail_reply(conv, &reply, e);
                        }
                        return;
                    }
                    return self.finish(conv, turn_id);
                }
            }
        }
        self.push(conv, turn_id.as_deref(), false);
    }

    fn finish(&mut self, conv: &str, turn_id: Option<String>) {
        self.unwatch(conv);
        self.stop_helpers(conv, true);
        // Claude Code takes a line handed to its turn at the turn's next step; one still unread when the turn ends is the next
        // turn by itself, after a stop too (fixture `steer-stopped`). The conversation stays busy until that turn takes it in, so
        // nothing starts before it: not the queue, Faber's checks or keep going, and a plan limit there still continues it. The
        // reply is read for keep going now (its stop line leaves it, a stop is said on it); if the line fails after all, the turn
        // ends from that reading (`steer_failed`). The checks of the reply that follows cover this one's changes too.
        if self.live.get(conv).is_some_and(|l| l.vendor == Vendor::Claude && !l.steering.is_empty()) {
            let read = self.keep_going_ended(conv, turn_id.as_deref());
            if let Some(l) = self.live.get_mut(conv) {
                l.held = Some(read);
            }
            if let Some(c) = self.convs.get_mut(conv) {
                c.updated_at = now();
            }
            self.save(conv);
            self.save_ledger(false);
            self.push(conv, turn_id.as_deref(), true);
            self.list_changed();
            return;
        }
        self.end_turn(conv, turn_id, None);
    }

    /// The rest of a turn's end, with the keep-going reading `finish` already made when it held the turn.
    fn end_turn(&mut self, conv: &str, turn_id: Option<String>, read: Option<Option<String>>) {
        self.transfers.remove(conv);
        if let Some(l) = self.live.get_mut(conv) {
            l.turn = None;
            l.held = None;
            // Whatever was handed to the turn and not taken in stays in the queue, and goes out like the rest of it.
            l.steering.clear();
        }
        let mut unsent = false;
        if let Some(c) = self.convs.get_mut(conv) {
            unsent = c.queued.iter().any(|q| q.sending);
            c.queued.iter_mut().for_each(|q| q.sending = false);
        }
        let workspace = self.workspace_of(conv);
        if self.convs.get(conv).is_some_and(|c| c.area == Area::Chat && c.working_folder.is_none()) {
            crate::permissions::clean_tmp(&workspace);
        }
        let completed_ok = if let Some(c) = self.convs.get_mut(conv) {
            if let Some(m) = turn_id.as_ref().and_then(|id| c.messages.iter_mut().find(|m| &m.id == id))
                && c.area == Area::Chat
            {
                let files = new_files(&workspace, m.at);
                if !files.is_empty() {
                    m.files = Some(files);
                }
            }
            c.updated_at = now();
            turn_id.as_ref().and_then(|id| c.messages.iter().find(|m| &m.id == id)).is_some_and(|m| m.status == Some(Status::Done))
        } else {
            return;
        };
        // A turn that failed or was stopped compacts nothing: the agent's request ended with it.
        if !completed_ok {
            self.compacts.remove(conv);
        }
        let keep_going = match read {
            Some(read) => read,
            None => self.keep_going_ended(conv, turn_id.as_deref()),
        };
        // A project reply that changed files is done once Faber's checks say so (R146): the queue and keep going wait.
        let checking = completed_ok && turn_id.as_deref().is_some_and(|reply| self.check_done(conv, reply, keep_going.clone()));
        self.save(conv);
        self.save_ledger(false);
        self.push(conv, turn_id.as_deref(), true);
        self.list_changed();
        if unsent {
            self.emit_queue(conv);
        }
        if self.live.get(conv).is_some_and(|l| self.opts.clis.get(&l.vendor) != Some(&l.cli))
            && let Some(l) = self.retire(conv)
        {
            l.session.close();
        }
        // After the CLI check above: a changed CLI is replaced before the next turn starts, never under it.
        if completed_ok && !checking {
            self.drain(conv);
        }
        if !checking {
            self.keep_going_unless_compacting(conv, keep_going);
            // Faber's checks end the turn later (`verifying.rs`), and the report waits for them. An idle process that
            // exits ends no reply, so it reports nothing twice.
            if turn_id.is_some() {
                self.tell_parent(conv);
            }
        }
        self.close_when_idle(conv);
    }

    /// A Claude process costs about 120 MB just waiting, so it closes when it has been idle for a while; the next
    /// turn resumes the same session. Any newer use cancels the timer.
    fn close_when_idle(&self, conv: &str) {
        let Some(l) = self.live.get(conv).filter(|l| l.vendor == Vendor::Claude) else { return };
        let (sid, used, idle) = (l.sid, l.used, self.opts.idle);
        let (me, conv) = (self.me.clone(), conv.to_string());
        tokio::spawn(async move {
            tokio::time::sleep(idle).await;
            let Some(core) = me.upgrade() else { return };
            let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
            // A compaction is the CLI at work with no turn of Faber's: its end closes the process when it is idle (`compaction_ended`).
            if core.live.get(&conv).is_some_and(|l| l.sid == sid && l.used == used && l.turn.is_none())
                && !core.compacting(&conv)
                && let Some(l) = core.retire(&conv)
            {
                l.session.close();
            }
        });
    }

    fn push(&mut self, conv: &str, message_id: Option<&str>, immediate: bool) {
        let Some(message_id) = message_id else { return };
        if immediate {
            self.pending.remove(conv);
            self.emit_message(conv, message_id);
            return;
        }
        if self.pending.contains_key(conv) {
            return;
        }
        self.pending.insert(conv.to_string(), message_id.to_string());
        let me = self.me.clone();
        let conv = conv.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(33)).await;
            if let Some(core) = me.upgrade() {
                let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(id) = core.pending.remove(&conv) {
                    core.emit_message(&conv, &id);
                }
            }
        });
    }

    fn emit_message(&mut self, conv: &str, message_id: &str) {
        if let Some(m) = self.convs.get(conv).and_then(|c| c.messages.iter().find(|m| m.id == message_id)) {
            (self.emit)(AppEvent::Message { conversation_id: conv.to_string(), message: Box::new(m.clone()) });
        }
        self.refresh_running();
    }

    /// Tells the screen the conversations changed, and the running list with them.
    fn list_changed(&mut self) {
        (self.emit)(AppEvent::List);
        self.refresh_running();
    }

    fn requeue(&mut self, id: &str, item: QueuedTurn) {
        if let Some(c) = self.convs.get_mut(id) {
            c.queued.insert(0, item);
            self.save(id);
            self.emit_queue(id);
        }
    }

    fn emit_queue(&mut self, conv: &str) {
        if let Some(c) = self.convs.get(conv) {
            (self.emit)(AppEvent::Queue { conversation_id: conv.to_string(), queued: c.queued.clone() });
        }
        // A message waiting keeps a finished reply's row at work (`settled`).
        self.refresh_running();
    }

    /// A message written while a turn runs. The running CLI takes it into that turn (Codex `turn/steer`, Claude Code a line on
    /// its input) and the message becomes part of the conversation when the CLI says it has; every other case waits here and
    /// goes out as the next turn. Either way it is saved with the conversation first, so a crash or a refusal cannot lose it.
    fn enqueue(&mut self, id: &str, req: SendRequest) -> Result<String, String> {
        let c = self.convs.get(id).ok_or("error.conversationGone")?;
        if c.queued.len() >= MAX_QUEUED {
            return Err("error.queueFull".into());
        }
        let root = if c.area == Area::Chat { self.cwd_of(c)? } else { self.opts.data_dir.clone() };
        let attachments_dir = if c.area == Area::Chat { root.join("attachments") } else { root.join("attachments").join(id) };
        let saved = if req.files.is_empty() { vec![] } else { save_files(&root, &attachments_dir, &req.files) };
        if saved.len() != req.files.len() {
            return Err("error.projectSandbox".into());
        }
        let names: Vec<String> =
            saved.iter().map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()).collect();
        let at = now();
        let mut item = QueuedTurn {
            id: new_id(),
            text: req.text,
            images: (!req.images.is_empty()).then_some(req.images),
            attachments: (!names.is_empty()).then_some(names),
            choice: req.choice,
            at,
            sending: false,
            wake: None,
        };
        let c = self.convs.get(id).ok_or("error.conversationGone")?;
        // Steering waits behind nothing: a message that is already queued would be passed by one sent straight in. A reply whose
        // agent already finished takes nothing in: the message waits for the CLI to close the turn and goes out as the next one.
        let finished = c.messages.last().is_some_and(|m| m.role == Role::Assistant && m.finished_at.is_some());
        let steer = !finished
            && c.queued.iter().all(|q| q.sending)
            && self.live.get(id).is_some_and(|l| l.turn.is_some() && [l.vendor.name(), "auto"].contains(&item.choice.vendor.as_str()));
        if steer {
            self.steer(id, &mut item);
        }
        if let Some(c) = self.convs.get_mut(id) {
            c.queued.push(item);
            c.updated_at = at;
        }
        self.save(id);
        self.emit_queue(id);
        Ok(id.to_string())
    }

    /// Hands a message to the running turn; the CLI takes it in at its next step (`taken_in`) or refuses it (`steer_failed`).
    fn steer(&mut self, id: &str, item: &mut QueuedTurn) {
        item.sending = true;
        let turn = UserTurn { text: self.model_text(id, item), images: item.images.clone().unwrap_or_default() };
        if let Some(l) = self.live.get_mut(id) {
            l.steering.push(item.id.clone());
            l.session.steer(turn, &item.id);
        }
    }

    /// What the CLI is told for a message: what the person wrote, with the note about the files they attached.
    fn model_text(&self, id: &str, item: &QueuedTurn) -> String {
        let area = self.convs.get(id).map_or(Area::Chat, |c| c.area);
        let shown: Vec<String> = item
            .attachments
            .iter()
            .flatten()
            .map(|name| {
                if area == Area::Chat {
                    format!("attachments/{name}")
                } else {
                    self.opts.data_dir.join("attachments").join(id).join(name).to_string_lossy().into_owned()
                }
            })
            .collect();
        recovery::with_attachments(&item.text, area, &shown)
    }

    /// The CLI took the oldest message handed to the running turn in (or the one named): it joins the conversation, and the
    /// answer goes on in a new reply under it, because that is what answers it.
    fn taken_in(&mut self, conv: &str, tag: Option<String>) {
        let secs = self.secs();
        let Some(l) = self.live.get_mut(conv) else { return };
        let Some(at) = tag.and_then(|t| l.steering.iter().position(|s| *s == t)).or((!l.steering.is_empty()).then_some(0)) else { return };
        let queued_id = l.steering.remove(at);
        let before = l.turn.clone();
        let (vendor, model) = (l.vendor, l.running.clone().or_else(|| l.model.clone()));
        // A line taken in after its turn ended (`finish` held it) is a turn of the CLI's own: it is counted from now.
        if l.held.take().is_some() && l.started == 0.0 {
            l.started = secs;
        }
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(pos) = c.queued.iter().position(|q| q.id == queued_id) else { return };
        let item = c.queued.remove(pos);
        // A reply the person stopped keeps saying so; Claude Code still answers the line it was handed (fixture `steer-stopped`).
        if let Some(m) = before.as_ref().and_then(|b| c.messages.iter_mut().find(|m| &m.id == b && m.status == Some(Status::Streaming))) {
            m.status = Some(Status::Done);
        }
        let now = now();
        let mut question = Message::new(new_id(), Role::User, item.text.clone(), now);
        question.images = item.images.clone();
        question.attachments = item.attachments.clone();
        question.wake = item.wake.clone();
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), now);
        reply.vendor = Some(vendor);
        reply.model = model;
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
        reply.finished_at = None;
        let (question_id, reply_id) = (question.id.clone(), reply.id.clone());
        c.messages.push(question);
        c.messages.push(reply);
        c.updated_at = now;
        let text = self.model_text(conv, &item);
        self.note_before(conv);
        if let Some(l) = self.live.get_mut(conv) {
            l.turn = Some(reply_id.clone());
            l.answer_started = false;
            l.break_pending = false;
        }
        // A plan limit now continues this reply, not the one the turn began in.
        if let Some(t) = self.transfers.get_mut(conv) {
            (t.reply, t.text, t.images) = (reply_id.clone(), text, item.images.unwrap_or_default());
        }
        self.save(conv);
        self.list_changed();
        self.emit_queue(conv);
        self.emit_message(conv, &question_id);
        if let Some(before) = before {
            self.push(conv, Some(&before), true);
        }
        self.push(conv, Some(&reply_id), true);
    }

    /// Codex did not take the message in: it stays in the queue and goes out as the next turn.
    fn steer_failed(&mut self, conv: &str, tag: &str) {
        let mut ended = None;
        if let Some(l) = self.live.get_mut(conv) {
            l.steering.retain(|s| s != tag);
            // The turn was held for this line (`finish`), and the CLI never got it: the turn ends now.
            if l.steering.is_empty()
                && let Some(read) = l.held.take()
            {
                ended = Some((l.turn.clone(), read));
            }
        }
        if let Some((turn, read)) = ended {
            return self.end_turn(conv, turn, Some(read));
        }
        if let Some(q) = self.convs.get_mut(conv).and_then(|c| c.queued.iter_mut().find(|q| q.id == tag)) {
            q.sending = false;
        }
        self.save(conv);
        self.emit_queue(conv);
        self.drain(conv);
    }

    /// After an answer that finished well, the next message in the queue goes out. Anything else (a stop, a failure) leaves
    /// the queue for the person to decide.
    fn drain(&mut self, conv: &str) {
        // A compaction the agent asked for comes first; the queue goes out when it ended (`compact.rs`).
        if self.compact_due(conv) {
            return;
        }
        let idle = !self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let Some(c) = self.convs.get_mut(conv) else { return };
        if !idle || c.messages.last().is_none_or(|m| m.status != Some(Status::Done)) || c.queued.is_empty() || verifying::checking(c) {
            return;
        }
        let next = c.queued.remove(0);
        self.save(conv);
        self.emit_queue(conv);
        let _ = self.dispatch_queued(conv, next);
    }

    fn remove_queued(&mut self, id: &str, turn_id: &str) {
        if let Some(c) = self.convs.get_mut(id) {
            let before = c.queued.len();
            // One already handed to the running turn cannot be taken back.
            c.queued.retain(|q| q.id != turn_id || q.sending);
            if c.queued.len() != before {
                self.save(id);
                self.emit_queue(id);
            }
        }
    }

    fn send_queued(&mut self, id: &str, turn_id: &str) -> Result<(), String> {
        // While an answer runs, the message goes into its turn, ahead of the ones still waiting: the person chose it.
        let running = self.live.get(id).filter(|l| l.turn.is_some()).map(|l| l.vendor.name());
        if let Some(vendor) = running {
            let c = self.convs.get_mut(id).ok_or("error.conversationGone")?;
            let pos = c.queued.iter().position(|q| q.id == turn_id).ok_or("error.messageGone")?;
            if c.queued[pos].sending {
                return Ok(());
            }
            let mut item = c.queued.remove(pos);
            let at = c.queued.iter().rposition(|q| q.sending).map_or(0, |p| p + 1);
            // One for the other AI cannot go into this turn: it goes first among those that wait.
            if [vendor, "auto"].contains(&item.choice.vendor.as_str()) {
                self.steer(id, &mut item);
            }
            if let Some(c) = self.convs.get_mut(id) {
                c.queued.insert(at, item);
            }
            self.save(id);
            self.emit_queue(id);
            return Ok(());
        }
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some())
            || self
                .convs
                .get(id)
                .is_some_and(|c| verifying::checking(c) || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming)));
        let Some(c) = self.convs.get_mut(id) else { return Err("error.conversationGone".into()) };
        let pos = c.queued.iter().position(|q| q.id == turn_id).ok_or("error.messageGone")?;
        let item = c.queued.remove(pos);
        // Something with no turn to take it runs (Faber's checks, a wait for quota, the pick of an AI): it goes first.
        if busy {
            let at = c.queued.iter().rposition(|q| q.sending).map_or(0, |p| p + 1);
            c.queued.insert(at, item);
            self.save(id);
            self.emit_queue(id);
            return Ok(());
        }
        self.save(id);
        self.emit_queue(id);
        self.dispatch_queued(id, item)
    }

    /// Starts a queued message as a turn. If nothing can answer it, it goes back to the front of the queue, so no message is
    /// lost; once the turn exists, a failure shows on that turn's reply like any other.
    fn dispatch_queued(&mut self, id: &str, item: QueuedTurn) -> Result<(), String> {
        self.load(id).ok_or("error.conversationGone")?;
        if self.installing {
            self.requeue(id, item);
            return Err("error.update.activeOperations".into());
        }
        let Some(fallback) = pick_model(&item.choice, &self.vendors(), self.codex_recommended.as_deref()) else {
            self.requeue(id, item);
            return Err("error.noSubscription".into());
        };
        if !self.convs.contains_key(id) {
            return Err("error.conversationGone".into());
        }
        let model_text = self.model_text(id, &item);
        let auto = item.choice.vendor == "auto";
        // A turn Faber sends by itself (`by_itself.rs`) is never one the person picked: an AI billed per use is not spent on.
        let by_hand = !auto && item.wake.is_none();
        let route = if auto { self.next_route(id) } else { None };
        let c = self.convs.get_mut(id).expect("checked above");
        // A turn that starts ends any wait of keep going: its end decides again, and no timer or reading sends a second one.
        if let Some(kg) = c.keep_going.as_mut() {
            kg.wait_until = None;
        }
        c.choice = Some(Choice { effort: (!auto).then(|| item.choice.effort.unwrap_or_default()), ..item.choice.clone() });
        let routing = route.is_some();
        let pick = match &route {
            Some(Some(before)) => before.clone(),
            Some(None) => fallback.clone(),
            None if auto => {
                same_as_before(c, self.live.get(id)).filter(|p| self.opts.clis.contains_key(&p.vendor)).unwrap_or_else(|| fallback.clone())
            }
            None => fallback.clone(),
        };
        let data_dir = self.opts.data_dir.clone();
        let joining_late = c.resumable(pick.vendor).is_none() && !c.messages.is_empty();
        let history = c.messages.clone();
        let at = now();
        let resent = joining_late && session_was_lost(c, pick.vendor);
        let area = c.area;
        let mut question = Message::new(new_id(), Role::User, item.text.clone(), at);
        question.images = item.images.clone();
        question.attachments = item.attachments.clone();
        question.wake = item.wake.clone();
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), at);
        if !routing {
            reply.vendor = Some(pick.vendor);
            reply.model = pick.model.clone();
        }
        reply.reason = Some(if routing { "reason.routing".into() } else { pick.reason.clone() });
        reply.note = resent.then(|| "note.historyResent".to_string());
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
        reply.finished_at = None;
        let reply_id = reply.id.clone();
        let question_id = question.id.clone();
        c.messages.push(question);
        c.messages.push(reply);
        c.updated_at = at;
        self.save(id);
        self.list_changed();
        self.emit_message(id, &question_id);
        let images = item.images.unwrap_or_default();
        let attachments_len = item.attachments.as_ref().map_or(0, |a| a.len());
        self.transfers.insert(
            id.to_string(),
            recovery::Transfer {
                reply: reply_id.clone(),
                pick: pick.clone(),
                text: model_text.clone(),
                images: images.clone(),
                by_hand,
                retried: false,
                moving: false,
                check: "glance",
            },
        );
        let text =
            if joining_late { with_history(&history, &model_text, &|n| attachment_place(area, &data_dir, id, n)) } else { model_text };
        if let Some(before) = route {
            let only = before.as_ref().map(|p| p.vendor);
            let route = self.route_for(&item.text, images.len() + attachments_len, only);
            self.push(id, Some(&reply_id), true);
            let (me, conv, images) = (self.me.clone(), id.to_string(), images);
            tokio::spawn(async move {
                let verdict = tokio::time::timeout(auto::ROUTE_BUDGET, route).await.ok().and_then(|r| r.ok()).flatten();
                if let Some(core) = me.upgrade() {
                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                    let routed = verdict.and_then(|v| core.table(&v, only)).filter(|r| before.as_ref().is_none_or(|p| !r.weaker_than(p)));
                    core.after_route(&conv, &reply_id, routed, pick, text, images);
                }
            });
            return Ok(());
        }
        if let Err(error) = self.dispatch(id, &reply_id, &pick, text, images, by_hand) {
            self.fail_reply(id, &reply_id, error.clone());
            return Err(error);
        }
        Ok(())
    }

    /// The conversations to show in the running list, in the order the screen draws them: blocks that hold a question
    /// first, then the sidebar's project order with Chat last; inside a block the questions first, oldest first.
    fn running_items(&self) -> Vec<RunningItem> {
        let mut items: Vec<RunningItem> = self.convs.values().filter_map(running_item).collect();
        if items.is_empty() {
            return items;
        }
        // ponytail: projects.json is read on every refresh while something runs; keep the order in memory if a profile shows it.
        let projects = self.store.projects();
        let block = |i: &RunningItem| match (i.area, &i.project_id) {
            (Area::Chat, _) => (usize::MAX, String::new()),
            (Area::Project, Some(id)) => (projects.iter().position(|p| &p.id == id).unwrap_or(usize::MAX - 1), id.clone()),
            (Area::Project, None) => (usize::MAX - 1, String::new()),
        };
        let asking: HashSet<(usize, String)> = items.iter().filter(|i| i.state == RunState::NeedsYou).map(block).collect();
        items.sort_by_cached_key(|i| {
            let block = block(i);
            (!asking.contains(&block), block, i.state == RunState::Working, i.since, i.conversation_id.clone())
        });
        items
    }

    /// Recomputes the running list and emits it with the next revision, but only if it differs from the last one emitted.
    /// It runs wherever the engine emits a `Message` or a `List`, so every change of a turn or a permission passes here.
    fn refresh_running(&mut self) {
        let items = self.running_items();
        if items == self.running.items {
            return;
        }
        self.mark_ended(&items);
        self.running = Running { revision: self.running.revision + 1, items: items.clone() };
        (self.emit)(AppEvent::Running { revision: self.running.revision, items });
    }

    /// Ends a reply that never got a CLI: it shows the error and leaves the running list.
    pub(super) fn fail_reply(&mut self, id: &str, reply_id: &str, error: String) {
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.status = Some(Status::Error);
            m.error = Some(error.clone());
        }
        self.save(id);
        self.push(id, Some(reply_id), true);
        self.keep_going_failed(id, reply_id, &error);
    }

    /// What the tool store hands this session: nothing where no tool is offered, and servers only where they are.
    fn tools_for(&self, area: Area, permissions: Permissions) -> crate::tools::Delivery {
        if !crate::claude::offers_tools(area, permissions) {
            return Default::default();
        }
        let mut tools = crate::tools::Store::new(&self.opts.data_dir).delivery();
        if !crate::claude::offers_servers(area, permissions) {
            tools.servers.clear();
        }
        tools
    }

    /// The shared Codex process for an area, hooks choice and permissions, started again if it died.
    fn codex_server(&mut self, area: Area, user_hooks: bool, permissions: Permissions, cli: &Cli) -> &CodexServer {
        let opts = CodexServerOptions {
            permissions,
            cli: cli.clone(),
            cwd: self.opts.data_dir.clone(),
            area,
            user_hooks,
            windows: self.opts.windows,
            spawn: spawner(&self.opts.spawn, cli),
            // What the store holds checked; a running server hears each change (`CodexServer::tools`).
            tools: self.tools_for(area, permissions),
        };
        let server = self.codex.entry((area, user_hooks, permissions)).or_insert_with(|| CodexServer::start(opts.clone()));
        if !server.is_alive() {
            *server = CodexServer::start(opts);
        } else {
            server.tools(opts.tools);
        }
        server
    }

    /// The gate before a user turn (3.4): the everyday default and the profile of an existing conversation are checked here
    /// too, not only what the table picked. A vendor the user picked by hand is allowed even when it is billed by use.
    fn gate_turn(&self, pick: &Pick, by_hand: bool) -> Result<(), String> {
        self.gate_turn_approved(pick, by_hand, false)
    }
    fn gate_turn_approved(&self, pick: &Pick, by_hand: bool, approved: bool) -> Result<(), String> {
        let family = pick.model.as_deref().and_then(family_of);
        self.allows(pick.vendor, family, Purpose::UserTurn { by_hand }, approved).map_err(|why| {
            match why {
                Why::Billing if self.ledger.of(pick.vendor).billing == Billing::Unknown => "error.billingUnknown",
                Why::Billing => "error.billedPerUse",
                Why::Exhausted | Why::Window | Why::Paid => "error.quotaOut",
            }
            .to_string()
        })
    }

    /// Hands a turn to the conversation's CLI, starting the CLI if needed.
    fn start(&mut self, id: &str, reply_id: &str, pick: &Pick, text: String, images: Vec<Image>, by_hand: bool) -> Result<(), String> {
        // The conversation's copy is still being made: nothing starts before it, wherever the turn comes from (a first
        // message, Auto's route, a queued message sent now). The latest turn is the one that starts once it is attached.
        if let Some(held) = self.copying.get_mut(id) {
            held.reply = reply_id.to_string();
            held.tail = Some(copies::Tail { pick: pick.clone(), text, images, by_hand });
            self.push(id, Some(reply_id), true);
            return Ok(());
        }
        if let Some(kg) = self.convs.get_mut(id).and_then(|c| c.keep_going.as_mut()) {
            kg.waiting = None;
        }
        self.note_away(id, reply_id);
        let approved = self.credit.as_ref().is_some_and(|(c, r, v)| c == id && r == reply_id && *v == pick.vendor);
        self.gate_turn_approved(pick, by_hand, approved)?;
        self.launch(id, pick)?;
        self.credit = None;
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.vendor = Some(pick.vendor);
            m.model = pick.model.clone();
            m.effort = Some(pick.effort.clone());
            m.wait_until = None;
            if m.reason.as_deref() == Some("reason.waiting") {
                m.reason = Some(pick.reason.clone());
            }
            m.error = None;
            m.status = Some(Status::Streaming);
            m.finished_at = None;
            m.permissions = None;
            m.mode = self.live.get(id).and_then(|l| l.permissions.fallback_note(l.runs));
        }
        if let Some(running) = self.live.get(id).and_then(|l| l.running.clone())
            && let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id))
        {
            m.model = Some(running);
        }
        let started = self.secs();
        if !self.live.contains_key(id) {
            return Err("error.conversationGone".into());
        }
        self.begin_unbounded(pick.vendor)?;
        self.note_before(id);
        if let Some(l) = self.live.get_mut(id) {
            l.started = started;
            l.turn = Some(reply_id.to_string());
            // Same-reply quota continuations must keep an already-visible answer outside activity.
            l.answer_started = self
                .convs
                .get(id)
                .and_then(|c| c.messages.last())
                .is_some_and(|m| m.id == reply_id && m.activity_end.unwrap_or(0) < m.text.encode_utf16().count());
            l.break_pending = false;
            l.by_hand = by_hand;
            l.approved = approved;
            l.break_pending |= self.convs.get(id).and_then(|c| c.messages.last()).is_some_and(|m| !m.text.is_empty());
            l.session.send(UserTurn { text, images });
        }
        self.push(id, Some(reply_id), true);
        Ok(())
    }

    fn launch(&mut self, conv: &str, pick: &Pick) -> Result<(), String> {
        self.tick += 1;
        let cli = self.opts.clis.get(&pick.vendor).cloned();
        let settings = self.store.settings();
        let (permissions, tuning) = (settings.permissions(), settings.tuning());
        let flags = match &cli {
            Some(cli) if pick.vendor == Vendor::Claude => self.claude_flags_for(cli),
            _ => Default::default(),
        };
        // Only Claude Code can lack a mode; its flags come from its own help, so a mode that appears later starts a new process.
        let runs = if pick.vendor == Vendor::Claude { permissions.for_claude(flags) } else { permissions };
        // Every turn checks the working folder again, also when the process of an earlier turn is reused: a link inside the path
        // may have been pointed somewhere else since.
        let cwd = match self.convs.get(conv).ok_or_else(|| "error.conversationGone".to_string()).and_then(|c| self.cwd_of(c)) {
            Ok(cwd) => cwd,
            Err(error) => {
                if let Some(l) = self.retire(conv) {
                    l.session.close();
                }
                return Err(error);
            }
        };
        if let Some(l) = self.live.get_mut(conv) {
            if l.vendor == pick.vendor
                && l.model == pick.model
                && l.effort == pick.effort
                && Some(&l.cli) == cli.as_ref()
                && l.permissions == permissions
                && l.tuning == tuning
                && l.runs == runs
                && l.cwd == cwd
            {
                l.used = self.tick; // most recently used
                return Ok(());
            }
            l.session.close();
            self.retire(conv);
        }
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let cwd_used = cwd.clone();
        let cli = cli.ok_or("error.notConnected")?;
        let spawn = spawner(&self.opts.spawn, &cli);
        self.next_sid += 1;
        let sid = self.next_sid;
        let me = self.me.clone();
        let key = conv.to_string();
        // The last driver callback also ends its run on panic or task cancellation.
        struct DriverEnd {
            me: std::sync::Weak<Mutex<Core>>,
            key: String,
            sid: u64,
            runtime: tokio::runtime::Handle,
        }
        impl Drop for DriverEnd {
            fn drop(&mut self) {
                let (me, key, sid) = (self.me.clone(), self.key.clone(), self.sid);
                // A callback can be dropped with Core locked, so cleanup must not wait here.
                self.runtime.spawn(async move {
                    if let Some(core) = me.upgrade() {
                        core.lock().unwrap_or_else(|e| e.into_inner()).handle(
                            &key,
                            sid,
                            EngineEvent::Error { message: "error.driverStopped".into(), fatal: true },
                        );
                    }
                });
            }
        }
        let end = DriverEnd { me, key, sid, runtime: self.opts.runtime.clone() };
        let sink: EventSink = Arc::new(move |e| {
            if let Some(core) = end.me.upgrade() {
                core.lock().unwrap_or_else(|e| e.into_inner()).handle(&end.key, end.sid, e);
            }
        });
        // The hooks switch matters only while asking; with full access the person's hooks always run, as in the CLI itself.
        let user_hooks = settings.user_hooks() && permissions.guarded();
        let resume = c.resumable(pick.vendor).cloned();
        let resumed = resume.is_some();
        let area = c.area;
        // The conversation's own name, fixed when it began: a rename in Settings never reaches a conversation under way.
        let user_name = c.user_name.clone();
        let (model, effort) = (pick.model.clone(), Some(pick.effort.clone()));
        let helper = self.offers_helper(area, pick.vendor);
        let browse = self.offers_browser(area, permissions);
        let conversations = siblings::offers(area);
        let verify = self.offers_verify(area, permissions);
        let progress = area == Area::Project;
        let session = match pick.vendor {
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch {
                        helper,
                        browse,
                        conversations,
                        verify,
                        progress,
                        area,
                        model,
                        effort,
                        resume,
                        user_hooks,
                        permissions,
                        instructions: None,
                        user_name,
                        flags,
                        tuning,
                        tools: self.tools_for(area, permissions),
                    },
                    spawn,
                    sandbox: self.opts.clis.get(&Vendor::Codex).cloned(),
                },
                sink,
            ),
            Vendor::Codex => self.codex_server(area, user_hooks, permissions, &cli).open(
                CodexThread {
                    helper,
                    browse,
                    conversations,
                    verify,
                    progress,
                    cwd,
                    model,
                    effort,
                    resume,
                    instructions: None,
                    user_name,
                    permissions,
                    tuning,
                },
                sink,
            ),
        };
        self.live.insert(
            conv.to_string(),
            Live {
                vendor: pick.vendor,
                model: pick.model.clone(),
                running: None,
                effort: pick.effort.clone(),
                sid,
                session,
                turn: None,
                break_pending: false,
                answer_started: false,
                used: self.tick,
                cli,
                resumed,
                permissions,
                tuning,
                runs,
                cwd: cwd_used,
                by_hand: false,
                approved: false,
                started: 0.0,
                served: false,
                steering: vec![],
                held: None,
                unsaved_session: None,
            },
        );
        // ponytail: count-based limit on idle Claude processes; a RAM budget across all CLIs comes with parallel workers.
        let claude = |l: &Live| l.vendor == Vendor::Claude;
        while self.live.values().filter(|l| claude(l)).count() > self.opts.max_warm {
            let Some(oldest) = self
                .live
                .iter()
                .filter(|(id, l)| claude(l) && l.turn.is_none() && l.sid != sid && !self.compacting(id))
                .min_by_key(|(_, l)| l.used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            if let Some(l) = self.retire(&oldest) {
                l.session.close();
            }
        }
        // A process started early (while the user types) closes too if the message never comes.
        self.close_when_idle(conv);
        Ok(())
    }

    fn blank(&mut self, area: Area, project_id: Option<String>) -> String {
        self.blank_as(new_id(), area, project_id)
    }

    fn blank_as(&mut self, id: String, area: Area, project_id: Option<String>) -> String {
        let at = now();
        let c = Conversation {
            id,
            title: String::new(),
            choice: None,
            area,
            project_id,
            working_folder: None,
            copy: None,
            sessions: BTreeMap::new(),
            tentative: BTreeMap::new(),
            command_gate: false,
            cost_base: BTreeMap::new(),
            allow_always: None,
            allow_always_epochs: BTreeMap::new(),
            branch_of: None,
            started_by: None,
            user_name: self.store.settings().user_name.and_then(|name| clean_name(&name)),
            persona_version: Some(crate::persona::VERSION),
            created_at: at,
            updated_at: at,
            queued: Vec::new(),
            keep_going: None,
            archived_at: None,
            ended: None,
            plan: None,
            context: None,
            messages: vec![],
            reported: (None, None),
            compacting: false,
            weighing: None,
            stopped: false,
        };
        let id = c.id.clone();
        self.convs.insert(id.clone(), c);
        id
    }

    fn send(&mut self, req: SendRequest) -> Result<String, String> {
        if self.installing {
            return Err("error.update.activeOperations".into());
        }
        let fallback = pick_model(&req.choice, &self.vendors(), self.codex_recommended.as_deref()).ok_or("error.noSubscription")?;
        let existing = req.conversation_id.as_deref().and_then(|id| self.load(id).map(|c| c.id.clone()));
        // Writing to an archived conversation brings it back.
        if let Some(id) = &existing
            && self.convs.get(id).is_some_and(|c| c.archived_at.is_some())
        {
            self.set_archived(id, false)?;
        }
        if let Some(id) = &existing
            && (self.live.get(id).is_some_and(|l| l.turn.is_some())
                || self
                    .convs
                    .get(id)
                    .is_some_and(|c| verifying::checking(c) || c.messages.last().is_some_and(|m| m.status == Some(Status::Streaming))))
        {
            return self.enqueue(id, req);
        }
        // Decided under the lock that takes the draft, so the copy is made for the conversation the message goes to.
        let plan = self.copy_plan(&req);
        let draft = self.draft.clone().filter(|d| self.convs.get(d).is_some_and(|c| c.area == req.area && c.project_id == req.project_id));
        let id = match existing {
            Some(id) => id,
            None => match draft {
                Some(d) => {
                    self.draft = None;
                    d
                }
                None => self.blank(req.area, req.project_id.clone()),
            },
        };
        self.load(&id).ok_or("error.conversationGone")?;
        let saved = if req.files.is_empty() {
            vec![]
        } else {
            let root = if req.area == Area::Chat {
                self.cwd_of(self.convs.get(&id).ok_or("error.conversationGone")?)?
            } else {
                self.opts.data_dir.clone()
            };
            let dir = if req.area == Area::Chat { root.join("attachments") } else { root.join("attachments").join(&id) };
            let files = save_files(&root, &dir, &req.files);
            if files.len() != req.files.len() {
                return Err("error.projectSandbox".into());
            }
            files
        };
        // Named on the conversation before its first message is saved, so a restart can never find the message without it.
        if let Some(plan) = &plan {
            self.name_copy(&id, plan);
        }
        let at = now();
        let auto = req.choice.vendor == "auto";
        let route = if auto { self.next_route(&id) } else { None };
        let c = self.convs.get_mut(&id).ok_or("error.conversationGone")?;
        c.choice = Some(Choice { effort: (!auto).then(|| req.choice.effort.unwrap_or_default()), ..req.choice.clone() });
        let routing = route.is_some();
        let pick = match &route {
            Some(Some(before)) => before.clone(),
            Some(None) => fallback.clone(),
            None if auto => {
                same_as_before(c, self.live.get(&id)).filter(|p| self.opts.clis.contains_key(&p.vendor)).unwrap_or_else(|| fallback.clone())
            }
            None => fallback.clone(),
        };
        if c.title.is_empty() {
            c.title = req.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect();
        }
        let data_dir = self.opts.data_dir.clone();
        let joining_late = c.resumable(pick.vendor).is_none() && !c.messages.is_empty();
        let resent = joining_late && session_was_lost(c, pick.vendor);
        let area = c.area;
        let history = c.messages.clone();
        // Attached files are saved where the model can open them with its own tools (PDFs, spreadsheets, archives...).
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let shown: Vec<String> = saved
            .iter()
            .map(|p| if c.area == Area::Chat { format!("attachments/{}", name(p)) } else { p.to_string_lossy().into_owned() })
            .collect();
        let text = recovery::with_attachments(&req.text, c.area, &shown);
        let mut question = Message::new(new_id(), Role::User, req.text.clone(), at);
        question.images = (!req.images.is_empty()).then(|| req.images.clone());
        question.attachments = Some(saved.iter().map(|p| name(p)).collect());
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), at);
        if !routing {
            reply.vendor = Some(pick.vendor);
            reply.model = pick.model.clone();
        }
        reply.reason = Some(if routing { "reason.routing".into() } else { pick.reason.clone() });
        reply.note = resent.then(|| "note.historyResent".to_string());
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
        reply.finished_at = None;
        let reply_id = reply.id.clone();
        let question_id = question.id.clone();
        c.messages.push(question);
        c.messages.push(reply);
        c.updated_at = at;
        self.save(&id);
        self.list_changed();
        // An open conversation only grows through these events, so the question goes out as well as the answer.
        self.emit_message(&id, &question_id);
        self.transfers.insert(
            id.clone(),
            recovery::Transfer {
                reply: reply_id.clone(),
                pick: pick.clone(),
                text: text.clone(),
                images: req.images.clone(),
                by_hand: !auto,
                retried: false,
                moving: false,
                check: "glance",
            },
        );
        let text = if joining_late { with_history(&history, &text, &|n| attachment_place(area, &data_dir, &id, n)) } else { text };
        // Git can take seconds: the message shows now, the copy is made away from the lock, and what starts the answer waits.
        match self.copying.get_mut(&id) {
            // The copy of an answer stopped meanwhile is still being made: this message takes its place and waits for it.
            Some(held) => {
                held.reply = reply_id.clone();
                held.tail = None;
            }
            None => {
                if let Some(plan) = plan {
                    self.hold_for_copy(&id, &reply_id, plan);
                }
            }
        }
        if let Some(before) = route {
            let only = before.as_ref().map(|p| p.vendor);
            let route = self.route_for(&req.text, req.images.len() + req.files.len(), only);
            self.push(&id, Some(&reply_id), true);
            let (me, conv, images) = (self.me.clone(), id.clone(), req.images);
            tokio::spawn(async move {
                let verdict = tokio::time::timeout(auto::ROUTE_BUDGET, route).await.ok().and_then(|r| r.ok()).flatten();
                if let Some(core) = me.upgrade() {
                    let mut core = core.lock().unwrap_or_else(|e| e.into_inner());
                    // The table runs now, on the quota of this moment, not on the quota the user saw while typing. A
                    // conversation under way only moves up.
                    let routed = verdict.and_then(|v| core.table(&v, only)).filter(|r| before.as_ref().is_none_or(|p| !r.weaker_than(p)));
                    core.after_route(&conv, &reply_id, routed, pick, text, images);
                }
            });
            return Ok(id);
        }
        if let Err(error) = self.dispatch(&id, &reply_id, &pick, text, req.images, !auto) {
            // The reply never got a CLI: it ends with the error instead of staying in the running list for good.
            self.fail_reply(&id, &reply_id, error.clone());
            return Err(error);
        }
        Ok(id)
    }

    // Edit a message or redo an answer with another model: a new conversation keeps everything before that message,
    // with the chat workspace copied, and sends the new text. The original stays as it was.
    fn branch(&mut self, req: BranchRequest, copy: Option<(String, crate::project::copy::WorkCopy)>) -> Result<String, String> {
        if self.installing {
            return Err("error.update.activeOperations".into());
        }
        let source = self.load(&req.conversation_id).cloned().ok_or("error.messageGone")?;
        let at = source.messages.iter().position(|m| m.id == req.message_id).ok_or("error.messageGone")?;
        let id = match copy {
            Some((id, copy)) => {
                let id = self.blank_as(id, source.area, source.project_id.clone());
                self.convs.get_mut(&id).expect("just created").copy = Some(copy);
                id
            }
            None => self.blank(source.area, source.project_id.clone()),
        };
        let from = self.workspace_of(&source.id);
        let to = self.workspace_of(&id);
        let c = self.convs.get_mut(&id).expect("just created");
        c.title = format!("↳ {}", source.title.trim_start_matches("↳ "));
        c.branch_of = Some(BranchOf { conversation_id: source.id.clone(), message_id: req.message_id.clone() });
        c.working_folder = source.working_folder.clone();
        c.messages = source.messages[..at]
            .iter()
            .cloned()
            .map(|mut m| {
                settle(&mut m);
                // The pull request stays the source's to follow: two conversations never wake on the same one.
                m.follow = None;
                m
            })
            .collect();
        if source.area == Area::Chat && source.working_folder.is_none() && from.is_dir() {
            copy_dir(&from, &to);
        }
        // The resend carries what the selected message carried: its images and attached files. A chat workspace copy already
        // holds the files, so they are taken out of it and saved again under the same names by the send.
        let picked = &source.messages[at];
        let images = picked.images.clone().unwrap_or_default();
        let kept =
            if source.area == Area::Chat { from.join("attachments") } else { self.opts.data_dir.join("attachments").join(&source.id) };
        let files: Vec<Attachment> = picked
            .attachments
            .iter()
            .flatten()
            .filter_map(|name| {
                let bytes = std::fs::read(kept.join(name)).ok()?;
                if source.area == Area::Chat {
                    let _ = std::fs::remove_file(to.join("attachments").join(name));
                }
                Some(Attachment { name: name.clone(), base64: base64::engine::general_purpose::STANDARD.encode(bytes) })
            })
            .collect();
        self.save(&id);
        self.send(SendRequest {
            conversation_id: Some(id),
            area: source.area,
            project_id: source.project_id,
            text: req.text,
            images,
            files,
            choice: req.choice,
        })
    }

    // Starts the CLI before the user finishes typing, so the first word arrives sooner.
    fn warm(&mut self, req: WarmRequest) {
        if self.installing {
            return;
        }
        let Some(fallback) = pick_model(&req.choice, &self.vendors(), self.codex_recommended.as_deref()) else { return };
        let id = self.draft_for(&req);
        if self.live.get(&id).is_some_and(|l| l.turn.is_some()) {
            return;
        }
        let pick = match (req.choice.vendor == "auto", self.convs.get(&id)) {
            // The route decides who answers a first message; until then only the router's own process gets ready.
            (true, Some(c)) if c.messages.is_empty() => return self.warm_router(),
            (true, Some(c)) => {
                let pick = same_as_before(c, self.live.get(&id)).filter(|p| self.opts.clis.contains_key(&p.vendor)).unwrap_or(fallback);
                if self.next_route(&id).is_some() {
                    self.warm_router();
                }
                pick
            }
            _ => fallback,
        };
        if self.copy_due(&id) {
            return;
        }
        let _ = self.launch(&id, &pick);
    }

    /// Throws away the blank conversation being typed, with the CLI that was started for it.
    fn drop_draft(&mut self) {
        if let Some(old) = self.draft.take() {
            if let Some(l) = self.retire(&old) {
                l.session.close();
            }
            self.convs.remove(&old);
            let _ = std::fs::remove_dir_all(self.scratch_of(&old));
        }
    }

    /// The conversation a warm-up or a draft route is for: the open one, or the new one being typed.
    fn draft_for(&mut self, req: &WarmRequest) -> String {
        let existing = req.conversation_id.as_deref().and_then(|id| self.load(id).map(|c| c.id.clone()));
        match existing {
            Some(id) => id,
            None => {
                let same = self
                    .draft
                    .as_ref()
                    .and_then(|d| self.convs.get(d))
                    .is_some_and(|c| c.area == req.area && c.project_id == req.project_id);
                if !same {
                    self.drop_draft();
                    self.draft = Some(self.blank(req.area, req.project_id.clone()));
                }
                self.draft.clone().expect("draft set above")
            }
        }
    }

    fn rename(&mut self, id: &str, title: &str) {
        let Some(c) = self.load(id) else { return };
        let title: String = title.chars().filter(|ch| !unseen(*ch)).collect::<String>().trim().chars().take(120).collect();
        if !title.is_empty() {
            c.title = title;
        }
        self.save(id);
        self.list_changed();
    }

    fn remove(&mut self, id: &str) {
        self.note_turn(id, None, true);
        self.stop_helpers(id, true);
        self.close_asides(Some(id));
        self.asides.remove(id);
        self.follows.remove(id);
        self.before.remove(id);
        if let Some(l) = self.retire(id) {
            l.session.close();
        }
        if self.store.get(id).is_some() || self.convs.contains_key(id) {
            self.store.remove(id);
            self.convs.remove(id);
            let _ = std::fs::remove_dir_all(self.scratch_of(id));
            let _ = std::fs::remove_dir_all(self.opts.data_dir.join("attachments").join(id));
        }
        self.list_changed();
    }

    fn terminal(&mut self, id: &str) -> Option<TerminalCommand> {
        let c = self.load(id)?.clone();
        let vendor = c.messages.iter().rev().find_map(|m| m.vendor)?;
        let session = c.sessions.get(&vendor)?.clone();
        // Session ids are UUIDs; anything else must never reach a shell.
        if !session.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
            return None;
        }
        let cli = self.opts.clis.get(&vendor)?.clone();
        let resume = match vendor {
            Vendor::Claude => vec!["--resume".to_string(), session],
            Vendor::Codex => vec!["resume".to_string(), session],
        };
        Some(TerminalCommand { command: cli.command, args: cli.prefix.into_iter().chain(resume).collect(), cwd: self.cwd_of(&c).ok()? })
    }

    /// A folder was used: it goes to the top of the recents (a refused one is not kept).
    fn note_folder(&self, path: &str) {
        let mut recents = self.store.recents();
        if recents.used(path).is_some() {
            // Remembering is a courtesy: opening the folder never fails because the list could not be written.
            let _ = self.store.save_recents(&recents);
        }
    }

    /// A damaged list is an error, never an empty one: writing it back would lose every grant and confirmed check.
    fn add_project(&mut self, path: &str) -> Result<Project, String> {
        let mut projects = self.store.projects_checked().map_err(|e| e.key().to_string())?;
        self.note_folder(path);
        if let Some(found) = projects.iter().find(|p| crate::recents::same_place(&p.path, path)) {
            return Ok(found.clone());
        }
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
        let project = Project::new(new_id(), name, path.to_string());
        projects.push(project.clone());
        self.store.save_projects(&projects).map_err(|e| e.key().to_string())?;
        self.list_changed();
        Ok(project)
    }
}

/// Runs the CLI's real executable with its fixed prefix (a script path for npm installs) before the arguments.
fn spawner(base: &Spawner, cli: &Cli) -> Spawner {
    let (base, prefix, command) = (base.clone(), cli.prefix.clone(), cli.command.clone());
    Arc::new(move |_bin, args, cwd| base(&command, &prefix.iter().chain(args).cloned().collect::<Vec<_>>(), cwd))
}

/// Auto keeps a conversation on the model its first message was routed to.
fn same_as_before(c: &Conversation, live: Option<&Live>) -> Option<Pick> {
    let last = c.messages.iter().rev().find(|m| m.role == Role::Assistant && m.vendor.is_some())?;
    let vendor = last.vendor?;
    let effort =
        live.filter(|l| l.vendor == vendor).map_or_else(|| last.effort.clone().unwrap_or_else(|| "medium".into()), |l| l.effort.clone());
    let reason = last.reason.clone().filter(|r| r != "reason.routing").unwrap_or_else(|| "reason.autoDefault".into());
    Some(Pick { vendor, model: last.model.clone(), effort, reason, pace: None })
}

/// A control character or an invisible formatting one (zero-width space, direction marks and overrides, line separators):
/// nothing a person can see, so a title never keeps one. The joiners U+200C and U+200D stay: they decide how Persian, Indic
/// and emoji text is spelled, and removing them changes the word.
fn unseen(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}' | '\u{61c}' | '\u{180e}' | '\u{200b}' | '\u{200e}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}' | '\u{fff9}'..='\u{fffb}'
        )
}

/// The row of the running list for a conversation, if its latest reply is streaming, has unanswered permissions or has parked
/// requests (denied for want of anyone to answer; they wait for the person until the next message, and count as "needs you").
/// Faber's checks of it count too: running, it is at work; failing with no fix asked, it needs the person (R146).
/// Cache warmth is not here: it is a quiet note on the reply itself.
fn running_item(c: &Conversation) -> Option<RunningItem> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant)?;
    let permissions = reply.permissions.clone().unwrap_or_default();
    let parked = reply.parked.clone().unwrap_or_default();
    let parked_more = reply.parked_more.unwrap_or(0);
    let stuck: Vec<String> =
        reply.tools.iter().flatten().filter(|t| t.sign.is_some_and(crate::stuck::Sign::stuck)).map(|t| t.id.clone()).collect();
    let held = reply.follow.as_ref().filter(|f| f.held && f.state.open()).map(|f| (f.number, f.head.clone().unwrap_or_default()));
    let checks = reply.checks.as_ref().map(|k| k.state);
    let checks_need = checks == Some(ChecksState::NeedsYou);
    // Keep going between two turns (waiting for a reset, an error to pass, a rest) is still at work: the list says until when,
    // and no idle update restarts Faber under it.
    let kg_wait = c.keep_going.as_ref().and_then(|k| k.wait_until).filter(|_| reply.status != Some(Status::Streaming));
    let keep_wait = c.keep_going.as_ref().and_then(|k| k.waiting.clone());
    if reply.status != Some(Status::Streaming)
        && !c.compacting
        && c.weighing.is_none()
        && kg_wait.is_none()
        && keep_wait.is_none()
        && permissions.is_empty()
        && parked.is_empty()
        && parked_more == 0
        && held.is_none()
        && checks != Some(ChecksState::Running)
        && !checks_need
    {
        return None;
    }
    let state = if permissions.is_empty() && parked.is_empty() && parked_more == 0 && stuck.is_empty() && held.is_none() && !checks_need {
        RunState::Working
    } else {
        RunState::NeedsYou
    };
    // Only the CLI's turn is still closing: nothing else keeps the conversation at work (not Keep going, its checks, a compaction, a
    // message waiting), so the screen reads it as done.
    let settled = state == RunState::Working
        && reply.status == Some(Status::Streaming)
        && reply.finished_at.is_some()
        && reply.wait_until.is_none()
        && !c.compacting
        && c.weighing.is_none()
        && c.keep_going.is_none()
        && c.queued.is_empty()
        && checks != Some(ChecksState::Running);
    Some(RunningItem {
        conversation_id: c.id.clone(),
        turn_id: reply.id.clone(),
        title: c.title.clone(),
        area: c.area,
        project_id: c.project_id.clone(),
        vendor: reply.vendor,
        model: reply.model.clone(),
        state,
        since: reply.at,
        permissions,
        parked,
        parked_more,
        stuck,
        wait_until: reply.wait_until.filter(|_| reply.status == Some(Status::Streaming)).or(kg_wait),
        held,
        keep_going: c.keep_going.is_some(),
        keep_wait,
        keep_progress: c.keep_going.as_ref().map(|k| crate::model::KeepProgress {
            turns: k.work,
            since: k.since,
            last: keep_going::progress_line(&c.messages),
        }),
        weighing: c.weighing.is_some(),
        settled,
        plan: c.plan.as_ref().map(Plan::count),
        now: reply.now.clone().filter(|_| reply.status == Some(Status::Streaming)),
    })
}

fn copy_dir(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".faber"
            || name == ".git"
            || entry.file_type().is_ok_and(|kind| kind.is_symlink())
            || !crate::permissions::allows(from, "Read", &serde_json::json!({ "file_path": name }))
        {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            let _ = std::fs::copy(entry.path(), target);
        }
    }
}

/// The tool `id` of the conversation's replies: the current reply first, then the others from the newest.
fn tool_in<'a>(messages: &'a mut [Message], current: Option<&str>, id: &str) -> Option<&'a mut ToolUse> {
    let has = |m: &Message| m.role == Role::Assistant && m.tools.iter().flatten().any(|t| t.id == id);
    let at = messages.iter().rposition(|m| Some(m.id.as_str()) == current && has(m)).or_else(|| messages.iter().rposition(has))?;
    messages[at].tools.as_mut()?.iter_mut().find(|t| t.id == id)
}

#[cfg(test)]
mod tool_in_tests {
    use super::*;
    use serde_json::json;

    fn reply(id: &str, tools: &[&str]) -> Message {
        let tools = tools.iter().map(|t| json!({ "id": t, "name": "Agent", "input": {} })).collect::<Vec<_>>();
        serde_json::from_value(json!({ "id": id, "role": "assistant", "text": "", "at": 0, "tools": tools })).unwrap()
    }

    #[test]
    fn a_helper_that_ends_after_many_messages_ends_on_its_own_call() {
        let mut messages = vec![reply("a1", &["helper"]), reply("a2", &[]), reply("a3", &[]), reply("a4", &[]), reply("a5", &["late"])];
        tool_in(&mut messages, Some("a5"), "helper").unwrap().ok = Some(true);
        assert_eq!(messages[0].tools.as_ref().unwrap()[0].ok, Some(true), "four replies later, the call is still found");
        assert!(tool_in(&mut messages, Some("a5"), "nobody").is_none());
        let mut twice = vec![reply("a1", &["t"]), reply("a2", &["t"])];
        tool_in(&mut twice, Some("a1"), "t").unwrap().ok = Some(false);
        assert_eq!(twice[0].tools.as_ref().unwrap()[0].ok, Some(false), "the current reply is looked at first");
    }
}
