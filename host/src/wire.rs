//! The form a command and its answer take outside the process that holds the [`Host`](crate::Host): one JSON object per
//! line. A request is `{"id":n,"cmd":"name","args":{...}}`, its answer `{"id":n,"ok":value}` or `{"id":n,"err":"error.key"}`
//! (the few failures that name a field carry `{"key","detail"}` instead of the string), and an event, which answers nothing,
//! `{"event":{...}}`. The window's sidecar (`faber-sidecar`) and the command line's pipe speak it alike, so one reader and one
//! writer serve both.
//!
//! The sidecar also tells the shell what to show with a line of its own, `{"order":{"kind":"tray",...}}`: the tray, a
//! notification, keeping the PC awake, starting with the system. An order is the whole decision (the shell draws it and
//! decides nothing), and it answers nothing. The shell, in turn, tells the sidecar what only it can see with
//! `{"shell":{"kind":"focus",...}}`, which answers nothing either.

use crate::Failure;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A line longer than this is refused before it is parsed: no command takes more (attachments travel as paths).
pub const MAX_LINE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Ok { id: u64, ok: Value },
    Err { id: u64, err: Failure },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event: Value,
}

/// What the shell must show or do, decided in Rust (the tray, notices, keep-awake, start with the system; R459). An order
/// says what is true now, so the shell may apply it again without harm; the sidecar sends one only when it changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Order {
    /// The tray icon: sent at start and whenever any of it changes, never twice the same.
    Tray {
        /// The tooltip, in the person's language ("Faber" when nothing runs).
        tooltip: String,
        /// How many questions wait for the person; the badge on the icon says it, and 0 means none.
        needs_you: usize,
        /// The 32 px icon as PNG, base64, with the badge drawn on it.
        picture: String,
        /// The 16 px icon as PNG, base64, with the badge drawn on it.
        picture16: String,
        menu: TrayMenu,
    },
    /// One notification, exactly when the tray should deliver it: the notifications setting, a game in front, one per
    /// question and the "keep going stopped" notice all decided already. The shell drops it while its window is focused
    /// (the sidecar cannot see the focus) and opens `conversation_id` when it is clicked.
    Notify {
        /// What the notification says; it has no separate body.
        title: String,
        body: String,
        conversation_id: Option<String>,
        /// Show the notification (silently: the sound is the next field's).
        toast: bool,
        /// Play Faber's attention chime.
        sound: bool,
    },
    /// Keep the PC awake (`on`) or let it sleep, as the running turns and the setting say.
    Awake { on: bool },
    /// Register Faber to start with the system (`on`) or remove it. The sidecar cannot register the shell's own program.
    Autostart { on: bool },
    /// Ask the person for a path in a dialog that belongs to the window; the shell answers with
    /// [`ShellNote::Picked`] under the same `id`.
    Pick {
        id: u64,
        mode: PickMode,
        filter: Option<PickFilter>,
        /// The file name a save dialog offers.
        name: Option<String>,
    },
    /// Show a file or folder in the file manager, selected and never opened. The host checked the path already.
    Reveal { path: String },
}

/// What a dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PickMode {
    Folder,
    File,
    Save,
}

/// The kind of file a dialog offers: its name and the extensions, without the dot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PickFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

/// The words on the tray's menu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayMenu {
    /// Brings the window back.
    pub show: String,
    pub quit: String,
}

/// What only the shell can see, said to the sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ShellNote {
    /// Faber's window gained (`true`) or lost the focus; a closed window has none.
    Focus { focused: bool },
    /// The answer to [`Order::Pick`]: the path picked, none when the person cancelled, or `no_window` when no dialog
    /// could be shown.
    Picked {
        id: u64,
        path: Option<String>,
        #[serde(default)]
        no_window: bool,
    },
}

/// A shell's note as it travels: `{"shell":{...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Noted {
    pub shell: ShellNote,
}

/// A line the sidecar reads: a command, or a note from its shell.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Request(Request),
    Note(ShellNote),
}

/// An order as it travels: `{"order":{...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ordered {
    pub order: Order,
}

impl Reply {
    pub fn of(id: u64, result: Result<Value, Failure>) -> Self {
        match result {
            Ok(ok) => Reply::Ok { id, ok },
            Err(err) => Reply::Err { id, err },
        }
    }
}

/// Parses one line as a request. A malformed or oversized line is `error.host.badRequest`: the peer gets that and the
/// connection goes on, since one bad line says nothing about the next.
pub fn request(line: &str) -> Result<Request, String> {
    if line.len() > MAX_LINE {
        return Err("error.host.badRequest".into());
    }
    serde_json::from_str(line.trim_end_matches(['\r', '\n'])).map_err(|_| "error.host.badRequest".into())
}

/// Parses one line the sidecar reads. A note is told by how it starts, as its shell writes it, so a request (which may be
/// large) is parsed once.
pub fn incoming(line: &str) -> Result<Incoming, String> {
    if line.starts_with(r#"{"shell":"#) {
        if line.len() > MAX_LINE {
            return Err("error.host.badRequest".into());
        }
        return serde_json::from_str::<Noted>(line.trim_end_matches(['\r', '\n']))
            .map(|noted| Incoming::Note(noted.shell))
            .map_err(|_| "error.host.badRequest".into());
    }
    request(line).map(Incoming::Request)
}

/// One line, ending in `\n`, for anything the wire carries. JSON never holds a raw line break, so a line is one message.
pub fn line<T: Serialize>(message: &T) -> String {
    let mut text = serde_json::to_string(message).unwrap_or_else(|_| r#"{"err":"error.host.badReply"}"#.into());
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_line_is_a_request_or_a_note_from_the_shell() {
        assert_eq!(incoming("{\"shell\":{\"kind\":\"focus\",\"focused\":true}}\n"), Ok(Incoming::Note(ShellNote::Focus { focused: true })));
        assert_eq!(incoming(r#"{"id":3,"cmd":"state"}"#), Ok(Incoming::Request(Request { id: 3, cmd: "state".into(), args: Value::Null })));
        assert_eq!(incoming(r#"{"shell":{"kind":"nap"}}"#), Err("error.host.badRequest".into()));
        assert_eq!(
            incoming(r#"{"shell":{"kind":"picked","id":4,"path":"C:\\a","noWindow":false}}"#),
            Ok(Incoming::Note(ShellNote::Picked { id: 4, path: Some("C:\\a".into()), no_window: false }))
        );
        let pick = Order::Pick {
            id: 4,
            mode: PickMode::Save,
            filter: Some(PickFilter { name: "JSON".into(), extensions: vec!["json".into()] }),
            name: Some("a.json".into()),
        };
        assert_eq!(
            serde_json::to_value(&pick).unwrap(),
            json!({"kind":"pick","id":4,"mode":"save","filter":{"name":"JSON","extensions":["json"]},"name":"a.json"})
        );
    }

    #[test]
    fn a_request_reads_back_and_its_answers_are_one_line_each() {
        let r = request("{\"id\":7,\"cmd\":\"open\",\"args\":{\"id\":\"c1\"}}\r\n").unwrap();
        assert_eq!(r, Request { id: 7, cmd: "open".into(), args: json!({"id":"c1"}) });
        assert_eq!(line(&Reply::of(7, Ok(json!({"a":"x\ny"})))), "{\"id\":7,\"ok\":{\"a\":\"x\\ny\"}}\n");
        assert_eq!(line(&Reply::of(7, Err("error.x".into()))), "{\"id\":7,\"err\":\"error.x\"}\n");
        assert_eq!(line(&Event { event: json!({"type":"list"}) }), "{\"event\":{\"type\":\"list\"}}\n");
    }

    #[test]
    fn an_order_is_one_line_tagged_by_kind_with_camel_case_keys() {
        let tray = Order::Tray {
            tooltip: "Faber".into(),
            needs_you: 2,
            picture: "AA==".into(),
            picture16: "AQ==".into(),
            menu: TrayMenu { show: "Open Faber".into(), quit: "Quit Faber".into() },
        };
        assert_eq!(
            serde_json::to_value(&tray).unwrap(),
            json!({"kind":"tray","tooltip":"Faber","needsYou":2,"picture":"AA==","picture16":"AQ==","menu":{"show":"Open Faber","quit":"Quit Faber"}})
        );
        let notify = Order::Notify {
            title: "A needs you".into(),
            body: String::new(),
            conversation_id: Some("c1".into()),
            toast: true,
            sound: false,
        };
        assert_eq!(
            line(&Ordered { order: notify.clone() }),
            "{\"order\":{\"kind\":\"notify\",\"title\":\"A needs you\",\"body\":\"\",\"conversationId\":\"c1\",\"toast\":true,\"sound\":false}}\n"
        );
        assert_eq!(serde_json::to_value(Order::Awake { on: true }).unwrap(), json!({"kind":"awake","on":true}));
        assert_eq!(serde_json::to_value(Order::Autostart { on: false }).unwrap(), json!({"kind":"autostart","on":false}));
        for order in [tray, notify, Order::Awake { on: false }, Order::Autostart { on: true }] {
            assert_eq!(serde_json::from_str::<Ordered>(&line(&Ordered { order: order.clone() })).unwrap().order, order);
        }
    }

    #[test]
    fn missing_args_are_null_and_bad_lines_are_refused_by_key() {
        assert_eq!(request("{\"id\":1,\"cmd\":\"state\"}").unwrap().args, Value::Null);
        assert_eq!(request("not json"), Err("error.host.badRequest".into()));
        assert_eq!(request("{\"cmd\":\"state\"}"), Err("error.host.badRequest".into()));
        assert_eq!(request(&"x".repeat(MAX_LINE + 1)), Err("error.host.badRequest".into()));
    }

    #[test]
    fn an_answer_reads_back_as_itself() {
        for reply in [Reply::of(3, Ok(json!(null))), Reply::of(4, Err("error.y".into()))] {
            assert_eq!(serde_json::from_str::<Reply>(&line(&reply)).unwrap(), reply);
        }
    }
}
