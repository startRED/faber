//! One JSON object per line, in both directions (the format of the command host's own wire, so one codec serves both).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

/// The longest line either side reads. A long conversation opened whole is several megabytes; past this, the line is
/// refused instead of growing memory without end.
pub const MAX_LINE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

/// What a client reads: a reply to one of its requests, or an event.
#[derive(Debug, Clone, PartialEq)]
pub enum FromFaber {
    Reply { id: u64, result: Result<Value, String> },
    Event(Value),
}

pub fn request(id: u64, cmd: &str, args: Value) -> String {
    json!({ "id": id, "cmd": cmd, "args": args }).to_string()
}

pub fn reply(id: u64, result: &Result<Value, String>) -> String {
    match result {
        Ok(ok) => json!({ "id": id, "ok": ok }),
        Err(err) => json!({ "id": id, "err": err }),
    }
    .to_string()
}

pub fn event(event: &Value) -> String {
    json!({ "event": event }).to_string()
}

/// A line from Faber, or None when it is none of the three shapes.
pub fn parse_from_faber(line: &str) -> Option<FromFaber> {
    let mut value: Value = serde_json::from_str(line).ok()?;
    if let Some(event) = value.get_mut("event") {
        return Some(FromFaber::Event(event.take()));
    }
    let id = value.get("id")?.as_u64()?;
    if let Some(ok) = value.get_mut("ok") {
        return Some(FromFaber::Reply { id, result: Ok(ok.take()) });
    }
    let err = value.get("err")?.as_str()?.to_string();
    Some(FromFaber::Reply { id, result: Err(err) })
}

/// Reads one line without its end. None at the end of the stream; an error for a line past `limit` or not UTF-8.
pub async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R, buffer: &mut Vec<u8>, limit: usize) -> std::io::Result<Option<String>> {
    buffer.clear();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(if buffer.is_empty() { None } else { Some(text(std::mem::take(buffer))?) });
        }
        let (taken, done) = match chunk.iter().position(|b| *b == b'\n') {
            Some(at) => (at + 1, true),
            None => (chunk.len(), false),
        };
        buffer.extend_from_slice(&chunk[..taken]);
        reader.consume(taken);
        if buffer.len() > limit {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "line too long"));
        }
        if done {
            buffer.pop();
            if buffer.last() == Some(&b'\r') {
                buffer.pop();
            }
            return Ok(Some(text(std::mem::take(buffer))?));
        }
    }
}

fn text(bytes: Vec<u8>) -> std::io::Result<String> {
    String::from_utf8(bytes).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_and_events_read_back_as_written() {
        let ok = parse_from_faber(&reply(3, &Ok(json!({"a": 1})))).unwrap();
        assert_eq!(ok, FromFaber::Reply { id: 3, result: Ok(json!({"a": 1})) });
        let err = parse_from_faber(&reply(4, &Err("error.x".into()))).unwrap();
        assert_eq!(err, FromFaber::Reply { id: 4, result: Err("error.x".into()) });
        assert_eq!(parse_from_faber(&event(&json!({"type": "list"}))), Some(FromFaber::Event(json!({"type": "list"}))));
        assert_eq!(parse_from_faber(r#"{"id":1}"#), None);
        assert_eq!(parse_from_faber("not json"), None);
    }

    #[test]
    fn a_request_without_args_reads_as_null_args() {
        let request: Request = serde_json::from_str(r#"{"id":1,"cmd":"state"}"#).unwrap();
        assert_eq!(request, Request { id: 1, cmd: "state".into(), args: Value::Null });
    }

    #[tokio::test]
    async fn lines_end_at_newline_drop_cr_and_keep_the_last_unterminated_one() {
        let mut reader = tokio::io::BufReader::new(&b"one\r\ntwo\nthree"[..]);
        let mut buffer = Vec::new();
        assert_eq!(read_line(&mut reader, &mut buffer, MAX_LINE).await.unwrap().as_deref(), Some("one"));
        assert_eq!(read_line(&mut reader, &mut buffer, MAX_LINE).await.unwrap().as_deref(), Some("two"));
        assert_eq!(read_line(&mut reader, &mut buffer, MAX_LINE).await.unwrap().as_deref(), Some("three"));
        assert_eq!(read_line(&mut reader, &mut buffer, MAX_LINE).await.unwrap(), None);
    }
}
