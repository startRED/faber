//! The form a command and its answer take outside the process that holds the [`Host`](crate::Host): one JSON object per
//! line. A request is `{"id":n,"cmd":"name","args":{...}}`, its answer `{"id":n,"ok":value}` or `{"id":n,"err":"error.key"}`,
//! and an event, which answers nothing, `{"event":{...}}`. The window's sidecar (if R459 phase 1 picks one) and the command
//! line's pipe speak it alike, so one reader and one writer serve both.

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
    Err { id: u64, err: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub event: Value,
}

impl Reply {
    pub fn of(id: u64, result: Result<Value, String>) -> Self {
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
    fn a_request_reads_back_and_its_answers_are_one_line_each() {
        let r = request("{\"id\":7,\"cmd\":\"open\",\"args\":{\"id\":\"c1\"}}\r\n").unwrap();
        assert_eq!(r, Request { id: 7, cmd: "open".into(), args: json!({"id":"c1"}) });
        assert_eq!(line(&Reply::of(7, Ok(json!({"a":"x\ny"})))), "{\"id\":7,\"ok\":{\"a\":\"x\\ny\"}}\n");
        assert_eq!(line(&Reply::of(7, Err("error.x".into()))), "{\"id\":7,\"err\":\"error.x\"}\n");
        assert_eq!(line(&Event { event: json!({"type":"list"}) }), "{\"event\":{\"type\":\"list\"}}\n");
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
