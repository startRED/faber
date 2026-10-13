//! The engine: conversations, live CLI sessions, permissions, branches and attachments.

use crate::account::{self, AccountStatus};
use crate::budget::{Billing, Purpose, Reach, Why, family_of};
use crate::claude::{ClaudeLaunch, ClaudeOptions, EventSink, start_claude};
use crate::cli::Cli;
use crate::codex::read_codex_limits;
use crate::codex::{CodexServer, CodexServerOptions, CodexThread};
use crate::events::{EngineEvent, Session, UserTurn};
use crate::learn::Span;
use crate::ledger::{Ledger, plausible};
use crate::model::*;
use crate::proc::Spawner;
use crate::router::{Choice, Effort, Pick, pick_model};
use crate::signals::Signals;
use crate::store::Store;
use crate::usage;
use crate::workspace::{Attachment, inside_folder, new_files, save_files};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod answer;
mod auto;
mod pulse;
mod recovery;
mod usage_view;
pub use usage_view::{VendorUsage, WindowUsage};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AppEvent {
    Message { conversation_id: String, message: Box<Message> },
    Queue { conversation_id: String, queued: Vec<QueuedTurn> },
    Quota { vendor: Vendor, window: String, used: f64, resets_at: f64 },
    Usage { vendors: Vec<VendorUsage> },
    Running { revision: u64, items: Vec<RunningItem> },
    List,
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
    by_hand: bool,
    approved: bool,
    started: f64,
    /// Queued messages handed to the running turn (Codex) and not yet taken in, oldest first.
    steering: Vec<String>,
}

/// Messages waiting behind a running turn. Each may carry images, which stay in memory and go out with every queue event.
const MAX_QUEUED: usize = 10;

const HISTORY_LIMIT: usize = 20_000;

/// A model that joins a conversation midway reads what was said before, as plain text.
pub fn with_history(history: &[Message], text: &str) -> String {
    let lines = history
        .iter()
        .map(|m| format!("{}: {}", if m.role == Role::User { "User" } else { "Assistant" }, m.text))
        .collect::<Vec<_>>()
        .join("\n\n");
    let skip = lines.chars().count().saturating_sub(HISTORY_LIMIT);
    let lines: String = lines.chars().skip(skip).collect();
    format!("Earlier in this conversation, with another assistant:\n\n{lines}\n\nNow the user says:\n{text}")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn unix_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

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
    live: HashMap<String, Live>,
    /// A new conversation still being typed: its CLI starts before the first message is sent.
    draft: Option<String>,
    /// Streaming text goes to the screen at most ~30 times a second; the message waiting per conversation.
    pending: HashMap<String, String>,
    next_sid: u64,
    tick: u64,
    /// The shared Codex processes, one per area and hooks choice.
    codex: HashMap<(Area, bool), CodexServer>,
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
    /// When the last quota pulse started.
    last_pulse: f64,
    /// The PC's offset from UTC in seconds, as the screen reports it.
    utc_offset: i64,
    offset_known: bool,
    watch_requested: bool,
    clock: Option<Clock>,
    transfers: HashMap<String, recovery::Transfer>,
    credit: Option<(String, String, Vendor)>,
}

/// A handle to the engine; clones share the same engine.
#[derive(Clone)]
pub struct Engine {
    core: Arc<Mutex<Core>>,
    runtime: tokio::runtime::Handle,
}

impl Engine {
    pub fn new(opts: EngineOptions, emit: Emit) -> Engine {
        let store = Store::new(&opts.data_dir);
        let ledger = store.ledger();
        let quotas = remembered(&ledger, unix_secs());
        let runtime = opts.runtime.clone();
        let core = Arc::new_cyclic(|me| {
            Mutex::new(Core {
                opts,
                store,
                emit,
                me: me.clone(),
                convs: HashMap::new(),
                live: HashMap::new(),
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
                last_pulse: 0.0,
                utc_offset: 0,
                offset_known: false,
                watch_requested: false,
                clock: None,
                transfers: HashMap::new(),
                credit: None,
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
        for (vendor, cli, spawn, cwd, windows) in jobs {
            match vendor {
                Vendor::Claude => {
                    let found = account::claude_account(&cli, &spawn, &cwd, timeout).await;
                    self.with(|c| {
                        c.claude_plan_checked(found);
                        c.emit_usage();
                    });
                }
                Vendor::Codex => {
                    let events = read_codex_limits(&cli.command, &cwd, windows, spawner(&spawn, &cli), timeout).await;
                    self.with(|c| {
                        c.codex_plan_checked(events);
                        c.emit_usage();
                    });
                }
            }
        }
    }
    /// Checks the plans once the screen supplies its local offset, then every quarter of an hour. The shell starts it once.
    pub fn watch_accounts(&self) {
        let Some(me) = self.with(|c| {
            c.watch_requested = true;
            c.offset_known.then(|| c.me.clone())
        }) else {
            return;
        };
        let runtime = self.runtime.clone();
        let handle = runtime.clone();
        runtime.spawn(async move {
            loop {
                let Some(core) = me.upgrade() else { return };
                Engine { core, runtime: handle.clone() }.check_plans(Duration::from_secs(20)).await;
                tokio::time::sleep(Duration::from_secs(15 * 60)).await;
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
    pub fn send(&self, req: SendRequest) -> Result<String, String> {
        self.with(|c| c.send(req))
    }
    pub fn branch(&self, req: BranchRequest) -> Result<String, String> {
        self.with(|c| c.branch(req))
    }
    pub fn remove_queued(&self, id: &str, turn_id: &str) {
        self.with(|c| c.remove_queued(id, turn_id))
    }
    pub fn send_queued(&self, id: &str, turn_id: &str) -> Result<(), String> {
        self.with(|c| c.send_queued(id, turn_id))
    }
    pub fn warm(&self, req: WarmRequest) {
        self.with(|c| c.warm(req))
    }
    pub fn interrupt(&self, id: &str) {
        self.with(|c| match c.live.get(id).filter(|l| l.turn.is_some()) {
            Some(l) => l.session.interrupt(),
            None => c.stop_waiting(id),
        })
    }
    pub fn rename(&self, id: &str, title: &str) {
        self.with(|c| c.rename(id, title))
    }
    pub fn remove(&self, id: &str) {
        self.with(|c| c.remove(id))
    }
    pub fn terminal(&self, id: &str) -> Option<TerminalCommand> {
        self.with(|c| c.terminal(id))
    }
    /// The absolute path of a file in a chat workspace, or None if the path points outside it or is missing.
    pub fn workspace_file(&self, id: &str, path: &str) -> Option<PathBuf> {
        self.with(|c| c.store.get(id).is_some().then(|| inside_folder(&c.workspace_of(id), path)).flatten().filter(|p| p.is_file()))
    }
    pub fn add_project(&self, path: &str) -> Project {
        self.with(|c| c.add_project(path))
    }
    pub fn remove_project(&self, id: &str) {
        self.with(|c| {
            let projects: Vec<Project> = c.store.projects().into_iter().filter(|p| p.id != id).collect();
            c.store.save_projects(&projects);
            c.list_changed();
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
    pub fn shutdown(&self) {
        self.with(|c| {
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
            for (_, server) in c.codex.drain() {
                server.retire();
            }
            c.close_spare();
            c.save_ledger(true);
        })
    }
}

impl Core {
    /// The time in Unix seconds.
    fn secs(&self) -> f64 {
        self.clock.as_ref().map_or_else(unix_secs, |clock| clock())
    }

    /// Writes the ledger, but at most every half minute unless `force`: readings come with every answer.
    fn save_ledger(&mut self, force: bool) {
        let now = self.secs();
        if force || !(0.0..LEDGER_EVERY).contains(&(now - self.ledger_saved)) {
            self.store.save_ledger(&self.ledger);
            self.ledger_saved = now;
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
        }
        if let Some(billing) = s.billing {
            v.billing = billing;
        }
        if s.plan.is_some() {
            v.plan_detected(s.plan, now);
        }
        self.stop_unapproved_paid(vendor, spent);
        self.emit_usage();
    }

    fn stop_unapproved_paid(&mut self, vendor: Vendor, spent: bool) {
        let v = self.ledger.of(vendor);
        let paid = spent || v.overage_active || (v.paid_fallback.unwrap_or(true) && v.snap(self.secs()).exhausted(self.secs()));
        let unknown = v.billing == Billing::Unknown;
        let billed = v.billing == Billing::UsageBilled;
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
            if let Some(l) = self.live.remove(&id) {
                l.session.close();
            }
            if paid && !unknown && !billed && self.transfers.get(&id).is_some_and(|t| !t.by_hand) {
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
        self.ledger.of_mut(vendor).learn_weights(now);
        self.save_ledger(true);
        self.emit_usage();
    }

    fn note_turn(&mut self, conv: &str, cost: Option<f64>, incomplete: bool) {
        let Some(l) = self.live.get(conv).filter(|l| l.started > 0.0) else { return };
        let (vendor, start, model) = (l.vendor, l.started, l.running.clone().or_else(|| l.model.clone()).unwrap_or_default());
        self.note_span(vendor, start, &model, cost, incomplete);
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
        events.into_iter().for_each(|e| self.note_account_event(Vendor::Codex, e));
        if self.ledger.codex.plan.is_none() && self.ledger.codex.billing != Billing::UsageBilled {
            self.ledger.codex.plan_missing();
        }
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
        let stale: Vec<String> =
            self.live.iter().filter(|(_, l)| l.turn.is_none() && changed(l.vendor)).map(|(id, _)| id.clone()).collect();
        if changed(Vendor::Codex) {
            for (_, server) in self.codex.drain() {
                server.retire();
            }
        }
        for id in stale {
            if let Some(l) = self.live.remove(&id) {
                l.session.close();
            }
        }
        self.opts.clis = clis;
    }

    // Each chat conversation gets a private workspace folder: code runs and files are created there, never elsewhere.
    fn workspace_of(&self, id: &str) -> PathBuf {
        self.opts.data_dir.join("chat").join(id)
    }

    fn cwd_of(&self, c: &Conversation) -> Result<PathBuf, String> {
        if c.area == Area::Chat {
            let dir = self.workspace_of(&c.id);
            let _ = std::fs::create_dir_all(&dir);
            return Ok(dir);
        }
        self.store
            .projects()
            .into_iter()
            .find(|p| Some(&p.id) == c.project_id.as_ref())
            .map(|p| PathBuf::from(p.path))
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
        self.convs.get_mut(id)
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
                    .map(|t| Recovery { pick: t.pick.clone(), text: t.text.clone(), by_hand: t.by_hand, retried: t.retried });
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

    /// Keeps the merged settings, or fails with `error.settings.saveFailed` before anything live changes.
    fn save_settings(&mut self, patch: Settings) -> Result<Settings, String> {
        let before = self.store.settings();
        let settings = Settings {
            appearance: patch.appearance.or(before.appearance),
            user_hooks: patch.user_hooks.or(before.user_hooks),
            notifications: patch.notifications.or(before.notifications),
            notification_sound: patch.notification_sound.or(before.notification_sound),
            language: patch.language.or(before.language),
            setup: patch.setup.or(before.setup),
            experience: patch.experience.or(before.experience),
            tutorial_done: patch.tutorial_done.or(before.tutorial_done),
            user_name: patch.user_name.map_or(before.user_name.clone(), |name| clean_name(&name)),
            keep_awake: patch.keep_awake.or(before.keep_awake),
            autostart: patch.autostart.or(before.autostart),
        };
        self.store.save_settings(&settings).map_err(|_| "error.settings.saveFailed".to_string())?;
        // The blank conversation being typed was started with the old name; the next one starts with the new one.
        if settings.user_name != before.user_name {
            self.drop_draft();
        }
        // Idle project CLIs restart on their next turn, so a changed hooks choice applies right away.
        if settings.user_hooks != before.user_hooks {
            let idle: Vec<String> = self
                .live
                .iter()
                .filter(|(id, l)| l.turn.is_none() && self.convs.get(*id).is_some_and(|c| c.area == Area::Project))
                .map(|(id, _)| id.clone())
                .collect();
            for id in idle {
                if let Some(l) = self.live.remove(&id) {
                    l.session.close();
                }
            }
            if let Some(server) = self.codex.remove(&(Area::Project, before.user_hooks == Some(true))) {
                server.retire();
            }
        }
        Ok(settings)
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
        let vendor = entry.vendor;
        let turn_id = entry.turn.clone();
        match event {
            EngineEvent::Started { session_id, model } => {
                // A CLI started while the user typed reports its model before any answer exists; later answers use it too.
                if let Some(l) = self.live.get_mut(conv) {
                    l.running = Some(model.clone());
                }
                if let Some(m) = self.turn(conv) {
                    m.model = Some(model);
                }
                let Some(c) = self.convs.get_mut(conv) else { return };
                c.sessions.insert(vendor, session_id);
                // a draft is saved only once something is sent
                if !c.messages.is_empty() {
                    self.save(conv);
                }
                // The turn's reply now names its model.
                self.refresh_running();
                return;
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
            EngineEvent::Tool { id, name, input } => {
                let final_answer = self.live.get(conv).is_some_and(|l| l.answer_started);
                if let Some(m) = self.turn(conv) {
                    if !final_answer {
                        m.activity_end = Some(m.text.encode_utf16().count());
                    }
                    m.tools.get_or_insert_default().push(ToolUse { id, name, input, ok: None, output: None });
                }
            }
            EngineEvent::ToolResult { id, ok, text } => {
                // A message steered into the turn starts a new reply, so a tool that began in the one before still ends here.
                let current = self.live.get(conv).and_then(|l| l.turn.clone());
                let replies =
                    self.convs.get_mut(conv).into_iter().flat_map(|c| c.messages.iter_mut().rev()).filter(|m| m.role == Role::Assistant);
                let mut recent = replies.take(3).collect::<Vec<_>>();
                recent.sort_by_key(|m| Some(&m.id) != current.as_ref());
                if let Some(tool) = recent.into_iter().filter_map(|m| m.tools.as_mut()).flatten().find(|t| t.id == id) {
                    tool.ok = Some(ok);
                    tool.output = Some(text.chars().take(4000).collect());
                }
            }
            EngineEvent::Permission { request_id, tool, input } => {
                let allowed = self.convs.get(conv).and_then(|c| c.allow_always.as_ref()).is_some_and(|a| a.contains(&tool));
                if allowed {
                    return self.auto_answer(conv, vec![Permission { request_id, tool, input }]);
                }
                if let Some(m) = self.turn(conv) {
                    m.permissions.get_or_insert_default().push(Permission { request_id, tool, input });
                }
                return self.push(conv, turn_id.as_deref(), true);
            }
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
                let fresh = self.live.get(conv).is_some_and(|l| !l.resumed);
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
                let missing = cost.is_none() || ((native.is_none() || vendor == Vendor::Codex) && incomplete);
                self.note_turn(conv, cost, missing);
                if let Some(m) = self.turn(conv) {
                    m.status = Some(if ok { Status::Done } else { Status::Stopped });
                    m.cost_usd = cost.map(|cost| m.cost_usd.unwrap_or(0.0) + cost).or(m.cost_usd);
                    m.input_tokens = Some(input_tokens);
                    m.output_tokens = Some(output_tokens);
                    m.cache_read_tokens = Some(cache_read_tokens);
                    m.cache_write_tokens = Some(cache_write_tokens);
                    m.cache_write_1h_tokens = Some(cache_write_1h_tokens);
                    m.incomplete = (incomplete || m.incomplete.unwrap_or(false)).then_some(true);
                    m.permissions = None;
                }
                if !ok && self.transfers.get(conv).is_some_and(|t| t.moving) {
                    if let Some(reply) = turn_id {
                        self.move_failed(conv, &reply);
                    }
                    return;
                }
                if !ok && recovery::plan_limit(&reason) {
                    self.ledger.of_mut(vendor).reach = Reach::Out(None);
                    self.save_ledger(true);
                    if let Some(l) = self.live.remove(conv) {
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
                let limited = fatal && recovery::plan_limit(&message);
                if let Some(m) = self.turn(conv) {
                    m.error = Some(message);
                    if fatal {
                        m.status = Some(Status::Error);
                        m.permissions = None;
                    }
                }
                if fatal {
                    self.note_turn(conv, None, true);
                    if let Some(l) = self.live.remove(conv) {
                        l.session.close();
                    }
                    if self.transfers.get(conv).is_some_and(|t| t.moving) {
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
        self.transfers.remove(conv);
        if let Some(l) = self.live.get_mut(conv) {
            l.turn = None;
            // Whatever was handed to the turn and not taken in stays in the queue, and goes out like the rest of it.
            l.steering.clear();
        }
        let mut unsent = false;
        if let Some(c) = self.convs.get_mut(conv) {
            unsent = c.queued.iter().any(|q| q.sending);
            c.queued.iter_mut().for_each(|q| q.sending = false);
        }
        let workspace = self.workspace_of(conv);
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
        self.save(conv);
        self.save_ledger(false);
        self.push(conv, turn_id.as_deref(), true);
        self.list_changed();
        if unsent {
            self.emit_queue(conv);
        }
        if self.live.get(conv).is_some_and(|l| self.opts.clis.get(&l.vendor) != Some(&l.cli))
            && let Some(l) = self.live.remove(conv)
        {
            l.session.close();
        }
        // After the CLI check above: a changed CLI is replaced before the next turn starts, never under it.
        if completed_ok {
            self.drain(conv);
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
            if core.live.get(&conv).is_some_and(|l| l.sid == sid && l.used == used && l.turn.is_none())
                && let Some(l) = core.live.remove(&conv)
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
    }

    /// A message written while a turn runs. Codex takes it into that turn (`turn/steer`) and the message becomes part of
    /// the conversation when Codex says it has; every other case waits here and goes out as the next turn. Either way it
    /// is saved with the conversation first, so a crash or a refusal cannot lose it.
    fn enqueue(&mut self, id: &str, req: SendRequest) -> Result<String, String> {
        let c = self.convs.get(id).ok_or("error.conversationGone")?;
        if c.queued.len() >= MAX_QUEUED {
            return Err("error.queueFull".into());
        }
        let attachments_dir =
            if c.area == Area::Chat { self.workspace_of(id).join("attachments") } else { self.opts.data_dir.join("attachments").join(id) };
        let saved = if req.files.is_empty() { vec![] } else { save_files(&attachments_dir, &req.files) };
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
        };
        let c = self.convs.get(id).ok_or("error.conversationGone")?;
        // Steering waits behind nothing: a message that is already queued would be passed by one sent straight in.
        let steer = c.queued.iter().all(|q| q.sending)
            && matches!(item.choice.vendor.as_str(), "auto" | "codex")
            && self.live.get(id).is_some_and(|l| l.vendor == Vendor::Codex && l.turn.is_some());
        if steer {
            item.sending = true;
            let turn = UserTurn { text: self.model_text(id, &item), images: item.images.clone().unwrap_or_default() };
            if let Some(l) = self.live.get_mut(id) {
                l.steering.push(item.id.clone());
                l.session.steer(turn, &item.id);
            }
        }
        if let Some(c) = self.convs.get_mut(id) {
            c.queued.push(item);
            c.updated_at = at;
        }
        self.save(id);
        self.emit_queue(id);
        Ok(id.to_string())
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

    /// Codex took the oldest message handed to the running turn in (or the one named): it joins the conversation, and the
    /// answer goes on in a new reply under it, because that is what answers it.
    fn taken_in(&mut self, conv: &str, tag: Option<String>) {
        let Some(l) = self.live.get_mut(conv) else { return };
        let Some(at) = tag.and_then(|t| l.steering.iter().position(|s| *s == t)).or((!l.steering.is_empty()).then_some(0)) else { return };
        let queued_id = l.steering.remove(at);
        let before = l.turn.clone();
        let model = l.running.clone().or_else(|| l.model.clone());
        let Some(c) = self.convs.get_mut(conv) else { return };
        let Some(pos) = c.queued.iter().position(|q| q.id == queued_id) else { return };
        let item = c.queued.remove(pos);
        if let Some(m) = before.as_ref().and_then(|b| c.messages.iter_mut().find(|m| &m.id == b)) {
            m.status = Some(Status::Done);
        }
        let now = now();
        let mut question = Message::new(new_id(), Role::User, item.text.clone(), now);
        question.images = item.images.clone();
        question.attachments = item.attachments.clone();
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), now);
        reply.vendor = Some(Vendor::Codex);
        reply.model = model;
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
        let (question_id, reply_id) = (question.id.clone(), reply.id.clone());
        c.messages.push(question);
        c.messages.push(reply);
        c.updated_at = now;
        let text = self.model_text(conv, &item);
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
        if let Some(l) = self.live.get_mut(conv) {
            l.steering.retain(|s| s != tag);
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
        let idle = !self.live.get(conv).is_some_and(|l| l.turn.is_some());
        let Some(c) = self.convs.get_mut(conv) else { return };
        if !idle || c.messages.last().is_none_or(|m| m.status != Some(Status::Done)) || c.queued.is_empty() {
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
        let busy = self.live.get(id).is_some_and(|l| l.turn.is_some())
            || self.convs.get(id).and_then(|c| c.messages.last()).is_some_and(|m| m.status == Some(Status::Streaming));
        if busy {
            return Err("error.busy".into());
        }
        let Some(c) = self.convs.get_mut(id) else { return Err("error.conversationGone".into()) };
        let pos = c.queued.iter().position(|q| q.id == turn_id).ok_or("error.messageGone")?;
        let item = c.queued.remove(pos);
        self.save(id);
        self.emit_queue(id);
        self.dispatch_queued(id, item)
    }

    /// Starts a queued message as a turn. If nothing can answer it, it goes back to the front of the queue, so no message is
    /// lost; once the turn exists, a failure shows on that turn's reply like any other.
    fn dispatch_queued(&mut self, id: &str, item: QueuedTurn) -> Result<(), String> {
        let Some(fallback) = pick_model(&item.choice, &self.vendors(), self.codex_recommended.as_deref()) else {
            self.requeue(id, item);
            return Err("error.noSubscription".into());
        };
        if !self.convs.contains_key(id) {
            return Err("error.conversationGone".into());
        }
        let model_text = self.model_text(id, &item);
        let auto = item.choice.vendor == "auto";
        let route = if auto { self.next_route(id) } else { None };
        let c = self.convs.get_mut(id).expect("checked above");
        c.choice = Some(Choice { effort: (!auto).then(|| item.choice.effort.unwrap_or(Effort::Medium)), ..item.choice.clone() });
        let routing = route.is_some();
        let pick = match &route {
            Some(Some(before)) => before.clone(),
            Some(None) => fallback.clone(),
            None if auto => {
                same_as_before(c, self.live.get(id)).filter(|p| self.opts.clis.contains_key(&p.vendor)).unwrap_or_else(|| fallback.clone())
            }
            None => fallback.clone(),
        };
        let joining_late = !c.sessions.contains_key(&pick.vendor) && !c.messages.is_empty();
        let history = c.messages.clone();
        let at = now();
        let mut question = Message::new(new_id(), Role::User, item.text.clone(), at);
        question.images = item.images.clone();
        question.attachments = item.attachments.clone();
        let mut reply = Message::new(new_id(), Role::Assistant, String::new(), at);
        if !routing {
            reply.vendor = Some(pick.vendor);
            reply.model = pick.model.clone();
        }
        reply.reason = Some(if routing { "reason.routing".into() } else { pick.reason.clone() });
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
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
                by_hand: !auto,
                retried: false,
                moving: false,
            },
        );
        let text = if joining_late { with_history(&history, &model_text) } else { model_text };
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
        if let Err(error) = self.dispatch(id, &reply_id, &pick, text, images, !auto) {
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
        self.running = Running { revision: self.running.revision + 1, items: items.clone() };
        (self.emit)(AppEvent::Running { revision: self.running.revision, items });
    }

    /// Ends a reply that never got a CLI: it shows the error and leaves the running list.
    pub(super) fn fail_reply(&mut self, id: &str, reply_id: &str, error: String) {
        if let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id)) {
            m.status = Some(Status::Error);
            m.error = Some(error);
        }
        self.save(id);
        self.push(id, Some(reply_id), true);
    }

    /// The shared Codex process for an area and hooks choice, started again if it died.
    fn codex_server(&mut self, area: Area, user_hooks: bool, cli: &Cli) -> &CodexServer {
        let opts = CodexServerOptions {
            bin: cli.command.clone(),
            cwd: self.opts.data_dir.clone(),
            area,
            user_hooks,
            windows: self.opts.windows,
            spawn: spawner(&self.opts.spawn, cli),
        };
        let server = self.codex.entry((area, user_hooks)).or_insert_with(|| CodexServer::start(opts.clone()));
        if !server.is_alive() {
            *server = CodexServer::start(opts);
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
            m.permissions = None;
        }
        if let Some(running) = self.live.get(id).and_then(|l| l.running.clone())
            && let Some(m) = self.convs.get_mut(id).and_then(|c| c.messages.iter_mut().find(|m| m.id == reply_id))
        {
            m.model = Some(running);
        }
        let started = self.secs();
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
        if let Some(l) = self.live.get_mut(conv) {
            if l.vendor == pick.vendor && l.model == pick.model && l.effort == pick.effort && Some(&l.cli) == cli.as_ref() {
                l.used = self.tick; // most recently used
                return Ok(());
            }
            l.session.close();
            self.live.remove(conv);
        }
        let c = self.convs.get(conv).ok_or("error.conversationGone")?;
        let cwd = self.cwd_of(c)?;
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
        let user_hooks = self.store.settings().user_hooks == Some(true);
        let resume = c.sessions.get(&pick.vendor).cloned();
        let resumed = resume.is_some();
        let area = c.area;
        // The conversation's own name, fixed when it began: a rename in Settings never reaches a conversation under way.
        let user_name = c.user_name.clone();
        let (model, effort) = (pick.model.clone(), Some(pick.effort.clone()));
        let session = match pick.vendor {
            Vendor::Claude => start_claude(
                ClaudeOptions {
                    bin: cli.command.clone(),
                    cwd,
                    launch: ClaudeLaunch { area, model, effort, resume, user_hooks, instructions: None, user_name },
                    spawn,
                },
                sink,
            ),
            Vendor::Codex => self
                .codex_server(area, user_hooks, &cli)
                .open(CodexThread { cwd, model, effort, resume, instructions: None, user_name }, sink),
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
                by_hand: false,
                approved: false,
                started: 0.0,
                steering: vec![],
            },
        );
        // ponytail: count-based limit on idle Claude processes; a RAM budget across all CLIs comes with parallel workers.
        let claude = |l: &Live| l.vendor == Vendor::Claude;
        while self.live.values().filter(|l| claude(l)).count() > self.opts.max_warm {
            let Some(oldest) = self
                .live
                .iter()
                .filter(|(_, l)| claude(l) && l.turn.is_none() && l.sid != sid)
                .min_by_key(|(_, l)| l.used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            if let Some(l) = self.live.remove(&oldest) {
                l.session.close();
            }
        }
        // A process started early (while the user types) closes too if the message never comes.
        self.close_when_idle(conv);
        Ok(())
    }

    fn blank(&mut self, area: Area, project_id: Option<String>) -> String {
        let at = now();
        let c = Conversation {
            id: new_id(),
            title: String::new(),
            choice: None,
            area,
            project_id,
            sessions: BTreeMap::new(),
            cost_base: BTreeMap::new(),
            allow_always: None,
            branch_of: None,
            user_name: self.store.settings().user_name.and_then(|name| clean_name(&name)),
            created_at: at,
            updated_at: at,
            queued: Vec::new(),
            messages: vec![],
        };
        let id = c.id.clone();
        self.convs.insert(id.clone(), c);
        id
    }

    fn send(&mut self, req: SendRequest) -> Result<String, String> {
        let fallback = pick_model(&req.choice, &self.vendors(), self.codex_recommended.as_deref()).ok_or("error.noSubscription")?;
        let existing = req.conversation_id.as_deref().and_then(|id| self.load(id).map(|c| c.id.clone()));
        if let Some(id) = &existing
            && (self.live.get(id).is_some_and(|l| l.turn.is_some())
                || self.convs.get(id).and_then(|c| c.messages.last()).is_some_and(|m| m.status == Some(Status::Streaming)))
        {
            return self.enqueue(id, req);
        }
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
        let attachments_dir = if req.area == Area::Chat {
            self.workspace_of(&id).join("attachments")
        } else {
            self.opts.data_dir.join("attachments").join(&id)
        };
        let at = now();
        let auto = req.choice.vendor == "auto";
        let route = if auto { self.next_route(&id) } else { None };
        let c = self.convs.get_mut(&id).ok_or("error.conversationGone")?;
        c.choice = Some(Choice { effort: (!auto).then(|| req.choice.effort.unwrap_or(Effort::Medium)), ..req.choice.clone() });
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
        let joining_late = !c.sessions.contains_key(&pick.vendor) && !c.messages.is_empty();
        let history = c.messages.clone();
        // Attached files are saved where the model can open them with its own tools (PDFs, spreadsheets, archives...).
        let saved = if req.files.is_empty() { vec![] } else { save_files(&attachments_dir, &req.files) };
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
        reply.tools = Some(vec![]);
        reply.status = Some(Status::Streaming);
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
            },
        );
        let text = if joining_late { with_history(&history, &text) } else { text };
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
    fn branch(&mut self, req: BranchRequest) -> Result<String, String> {
        let source = self.load(&req.conversation_id).cloned().ok_or("error.messageGone")?;
        let at = source.messages.iter().position(|m| m.id == req.message_id).ok_or("error.messageGone")?;
        let id = self.blank(source.area, source.project_id.clone());
        let from = self.workspace_of(&source.id);
        let to = self.workspace_of(&id);
        let c = self.convs.get_mut(&id).expect("just created");
        c.title = format!("↳ {}", source.title.trim_start_matches("↳ "));
        c.branch_of = Some(BranchOf { conversation_id: source.id.clone(), message_id: req.message_id.clone() });
        c.messages = source.messages[..at]
            .iter()
            .cloned()
            .map(|mut m| {
                settle(&mut m);
                m
            })
            .collect();
        if source.area == Area::Chat && from.is_dir() {
            copy_dir(&from, &to);
        }
        self.save(&id);
        self.send(SendRequest {
            conversation_id: Some(id),
            area: source.area,
            project_id: source.project_id,
            text: req.text,
            images: vec![],
            files: vec![],
            choice: req.choice,
        })
    }

    // Starts the CLI before the user finishes typing, so the first word arrives sooner.
    fn warm(&mut self, req: WarmRequest) {
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
        let _ = self.launch(&id, &pick);
    }

    /// Throws away the blank conversation being typed, with the CLI that was started for it.
    fn drop_draft(&mut self) {
        if let Some(old) = self.draft.take() {
            if let Some(l) = self.live.remove(&old) {
                l.session.close();
            }
            self.convs.remove(&old);
            let _ = std::fs::remove_dir_all(self.workspace_of(&old));
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
        let title: String = title.trim().chars().take(120).collect();
        if !title.is_empty() {
            c.title = title;
        }
        self.save(id);
        self.list_changed();
    }

    fn remove(&mut self, id: &str) {
        if let Some(l) = self.live.remove(id) {
            l.session.close();
        }
        if self.store.get(id).is_some() || self.convs.contains_key(id) {
            self.store.remove(id);
            self.convs.remove(id);
            let _ = std::fs::remove_dir_all(self.workspace_of(id));
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

    fn add_project(&mut self, path: &str) -> Project {
        let mut projects = self.store.projects();
        if let Some(found) = projects.iter().find(|p| p.path == path) {
            return found.clone();
        }
        let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string());
        let project = Project { id: new_id(), name, path: path.to_string() };
        projects.push(project.clone());
        self.store.save_projects(&projects);
        self.list_changed();
        project
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

/// The row of the running list for a conversation, if its latest reply is streaming or has unanswered permissions.
fn running_item(c: &Conversation) -> Option<RunningItem> {
    let reply = c.messages.last().filter(|m| m.role == Role::Assistant)?;
    let permissions = reply.permissions.clone().unwrap_or_default();
    if reply.status != Some(Status::Streaming) && permissions.is_empty() {
        return None;
    }
    Some(RunningItem {
        conversation_id: c.id.clone(),
        turn_id: reply.id.clone(),
        title: c.title.clone(),
        area: c.area,
        project_id: c.project_id.clone(),
        vendor: reply.vendor,
        model: reply.model.clone(),
        state: if permissions.is_empty() { RunState::Working } else { RunState::NeedsYou },
        since: reply.at,
        permissions,
        wait_until: reply.wait_until.filter(|_| reply.status == Some(Status::Streaming)),
    })
}

fn copy_dir(from: &Path, to: &Path) {
    let _ = std::fs::create_dir_all(to);
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            let _ = std::fs::copy(entry.path(), target);
        }
    }
}
