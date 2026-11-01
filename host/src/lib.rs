//! The commands Faber answers, in one table (R459). The window calls them, and so do the command line and the tests; each
//! front end only carries a name and its arguments here and the answer back, so a rule lives once, in Rust, whichever shell
//! draws the screen. A command's arguments are a JSON object with camelCase keys, as the screen's `ui/api.ts` sends them,
//! and a failure is an error key (`error.*`), never a sentence.
//!
//! What only a window can do (the browser pane, dialogs, the tray) is the front end's: such a command is listed here as
//! [`Kind::Window`] so every caller sees the whole list, and a front end without a window answers it `error.host.noWindow`.

mod feed;
pub mod wire;

pub use feed::Feed;

use faber_engine::engine::Emit;
use faber_engine::model::{Conversation, Settings, Vendor};
use faber_engine::{AppEvent, Engine, WarmRequest};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use tokio::sync::broadcast;

pub type Res<T> = Result<T, String>;

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

/// One command as callers see it. `args` are the JSON keys and the Rust type each takes, as written in the table.
#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    pub name: &'static str,
    pub args: Vec<(String, &'static str)>,
    pub kind: Kind,
}

/// Names the table never holds: a connection answers them itself (the command line's greeting and its event stream).
pub const RESERVED: [&str; 2] = ["hello", "subscribe"];

/// The part of the front end the host calls back. One with no window (the command line) keeps the defaults.
pub trait Shell: Send + Sync + 'static {
    /// Hears every engine event first, before the screen does (the tray, keeping the PC awake). The engine calls it with its
    /// lock held, so anything that may wait for that lock goes to another thread.
    fn heard(&self, _event: &AppEvent) {}
    /// The settings as the screen shows them, with what only the shell knows filled in, such as whether Faber starts with the
    /// system.
    fn shown_settings(&self, settings: Settings) -> Settings {
        settings
    }
    /// Sends one event, as the screen receives it, to the window if there is one.
    fn emit(&self, _event: &Value) {}
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
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

    pub fn host(self, engine: Engine) -> Host {
        Host { engine, feed: self.feed, shell: self.shell, events: self.events }
    }
}

/// The engine and what the commands share around it.
pub struct Host {
    pub engine: Engine,
    /// What the screen last heard of each streaming reply (`feed.rs`).
    pub feed: Arc<Mutex<Feed>>,
    pub shell: Arc<dyn Shell>,
    events: broadcast::Sender<Value>,
}

impl Host {
    /// Every event from now on, as the screen receives it. A reply already streaming arrives as its changed steps only
    /// (`feed.rs`), so a subscriber reads `open` (or `state`) first, and again after it lags.
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// The whole table, built once.
    pub fn commands() -> &'static [CommandInfo] {
        static LIST: OnceLock<Vec<CommandInfo>> = OnceLock::new();
        LIST.get_or_init(|| {
            TABLE
                .iter()
                .map(|(name, args, kind)| CommandInfo { name, args: args.iter().map(|(arg, ty)| (camel(arg), *ty)).collect(), kind: *kind })
                .collect()
        })
    }

    pub fn kind(command: &str) -> Option<Kind> {
        TABLE.iter().find(|(name, _, _)| *name == command).map(|(_, _, kind)| *kind)
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

/// Builds the commands, `Host::dispatch` and the table from one list, so they cannot drift apart. Each entry is
/// `Kind fn name(host, arg: Type, ...) -> Answer { body }`, where the body returns `Res<Answer>`.
macro_rules! table {
    ($( $(#[$meta:meta])* $kind:ident fn $name:ident($h:ident $(, $arg:ident: $ty:ty)* $(,)?) -> $ret:ty $body:block )*) => {
        mod commands {
            #![allow(clippy::unused_async)]
            use super::*;
            $( $(#[$meta])* pub(super) async fn $name($h: &Host $(, $arg: $ty)*) -> Res<$ret> $body )*
        }

        const TABLE: &[(&str, &[(&str, &str)], Kind)] =
            &[$( (stringify!($name), &[$( (stringify!($arg), stringify!($ty)) ),*], Kind::$kind) ),*];

        impl Host {
            /// Runs one command. Arguments are an object of named values, as the screen sends them; anything else, or
            /// arguments that do not fit, is `error.host.badArgs`, and a name not in the table `error.host.unknownCommand`.
            pub async fn dispatch(&self, command: &str, args: Value) -> Res<Value> {
                let args = match args {
                    Value::Null => Value::Object(Default::default()),
                    Value::Object(_) => args,
                    _ => return Err("error.host.badArgs".into()),
                };
                match command {
                    $( stringify!($name) => {
                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Args { $( $arg: $ty, )* }
                        let Args { $( $arg, )* } = serde_json::from_value(args).map_err(|_| "error.host.badArgs".to_string())?;
                        let answer = commands::$name(self $(, $arg)*).await?;
                        serde_json::to_value(answer).map_err(|_| "error.host.badReply".to_string())
                    } )*
                    _ => Err("error.host.unknownCommand".into()),
                }
            }
        }
    };
}

table! {
    Read fn open(h, id: String) -> Option<Conversation> {
        // Whatever it heard of a streaming reply, the screen now takes this snapshot: the next update of each goes whole.
        lock(&h.feed).forget();
        Ok(h.engine.open_for_screen(&id))
    }
    Write fn create_chat_group(h, name: String) -> faber_engine::engine::ChatGroup {
        h.engine.create_chat_group(&name)
    }
    Write fn save_chat_group(h, id: String, name: Option<String>, instructions: Option<String>) -> faber_engine::engine::ChatGroup {
        h.engine.save_chat_group(&id, name.as_deref(), instructions.as_deref())
    }
    Write fn add_chat_group_files(h, id: String, files: Vec<faber_engine::workspace::Attachment>) -> faber_engine::engine::ChatGroup {
        h.engine.add_chat_group_files(&id, &files)
    }
    Write fn remove_chat_group_file(h, id: String, file_id: String) -> faber_engine::engine::ChatGroup {
        h.engine.remove_chat_group_file(&id, &file_id)
    }
    Write fn remove_chat_group(h, id: String) -> () {
        h.engine.remove_chat_group(&id)
    }
    Write fn move_chat_to_group(h, conversation_id: String, group_id: Option<String>) -> () {
        h.engine.move_chat_to_group(&conversation_id, group_id.as_deref())
    }
    StartsTurn fn new_group_chat(h, group_id: String) -> Conversation {
        h.engine.new_group_chat(&group_id).map(Conversation::for_screen)
    }
    Read fn set_clock(h, utc_offset: i64) -> () {
        h.engine.set_utc_offset(utc_offset);
        Ok(())
    }
    /// The window was shown, hidden or focused: the quota is read more often while it is open, and at once on focus.
    Read fn quota_attention(h, visible: bool, focused: bool) -> () {
        h.engine.set_attention(visible, focused);
        Ok(())
    }
    Write fn answer_plan(h, vendor: Vendor, plan: String) -> () {
        h.engine.answer_plan(vendor, &plan);
        Ok(())
    }
    Write fn approve_credits(h, id: String, turn_id: String, vendor: Vendor) -> () {
        h.engine.approve_credits(&id, &turn_id, vendor)
    }
    Write fn move_turn(h, id: String, turn_id: String) -> () {
        h.engine.move_turn(&id, &turn_id)
    }
    /// The person read the requests held back in a reply: they leave the "needs you" list.
    Write fn parked_seen(h, id: String, turn_id: String) -> () {
        h.engine.parked_seen(&id, &turn_id)
    }
    Write fn wait_turn(h, id: String, turn_id: String) -> () {
        h.engine.wait_turn(&id, &turn_id)
    }
    /// Go on with a reply Faber was closed under (R477): one fixed turn, refused once it is no longer the last.
    Write fn continue_closed(h, id: String, turn_id: String) -> () {
        h.engine.continue_closed(&id, &turn_id)
    }
    Read fn warm(h, req: WarmRequest) -> () {
        h.engine.warm(req);
        Ok(())
    }
    Write fn forget_auto_learned(h) -> () {
        h.engine.forget_auto_learned()
    }
    Read fn route_draft(h, req: WarmRequest, text: String) -> () {
        h.engine.route_draft(req, text);
        Ok(())
    }
    Write fn interrupt(h, id: String) -> () {
        h.engine.interrupt(&id);
        Ok(())
    }
    Write fn answer(h, id: String, turn_id: String, request_id: String, allow: bool, always: bool) -> () {
        h.engine.answer(&id, &turn_id, &request_id, allow, always).await
    }
    Write fn dismiss_unclean_exit(h) -> () {
        h.engine.dismiss_unclean_exit();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_each_command_once_and_never_a_reserved_name() {
        let names: Vec<&str> = Host::commands().iter().map(|c| c.name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "a command is listed twice");
        for reserved in RESERVED {
            assert!(!names.contains(&reserved), "{reserved} is answered by the connection, never by the table");
        }
    }

    #[test]
    fn arguments_are_listed_as_the_screen_sends_them() {
        let answer = Host::commands().iter().find(|c| c.name == "answer").unwrap();
        let keys: Vec<&str> = answer.args.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["id", "turnId", "requestId", "allow", "always"]);
        let group = Host::commands().iter().find(|c| c.name == "move_chat_to_group").unwrap();
        assert_eq!(group.args[1], ("groupId".to_string(), "Option<String>"));
        assert_eq!(Host::kind("new_group_chat"), Some(Kind::StartsTurn));
        assert_eq!(Host::kind("hello"), None);
    }
}
