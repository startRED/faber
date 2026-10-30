//! What the screen hears of a reply while it streams. The engine reports the whole message on every update, up to 30 times a
//! second, and a long agent reply holds thousands of steps (11 MB on 2026-10-22). Sent whole, each update was a script of
//! that size for the webview to parse, and the window grew to 8 GB in a few hours. So an update of a streaming reply
//! carries only the steps that changed since the last one the screen heard: `toolsFrom` steps are as they were, and after
//! them a step that did not change is only its id with `same`. The screen puts the rest back from what it holds
//! (`ui/opening.ts`). A reply that stops streaming goes whole, and so does every reply once the screen opens a conversation.

use faber_engine::AppEvent;
use faber_engine::model::{Message, Status, ToolUse};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::hash::Hasher;

/// Past this many streaming replies, the oldest record is dropped: its next update simply goes whole.
const KEPT: usize = 64;

/// What the screen last heard of each streaming reply: one fingerprint per step, in order.
#[derive(Default)]
pub struct Feed {
    heard: HashMap<String, Vec<(String, u64)>>,
    order: Vec<String>,
}

/// A step's fingerprint: a hash of everything the screen would receive for it.
fn fingerprint(tool: &ToolUse) -> u64 {
    struct Hashing(std::collections::hash_map::DefaultHasher);
    impl std::io::Write for Hashing {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.write(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut hashing = Hashing(Default::default());
    // Writing into a hasher never fails; a step that could not be written would only go whole.
    let _ = serde_json::to_writer(&mut hashing, tool);
    hashing.0.finish()
}

impl Feed {
    /// The screen asked for a conversation or for the whole state: it holds what that answer says, so every reply goes
    /// whole once more.
    pub fn forget(&mut self) {
        self.heard.clear();
        self.order.clear();
    }

    /// The event as the screen gets it.
    pub fn shrink(&mut self, event: AppEvent) -> Value {
        let AppEvent::Message { conversation_id, mut message } = event else {
            return serde_json::to_value(&event).unwrap_or(Value::Null);
        };
        let tools = message.tools.take().unwrap_or_default();
        let prints: Vec<(String, u64)> = tools.iter().map(|t| (t.id.clone(), fingerprint(t))).collect();
        let streaming = message.status == Some(Status::Streaming);
        let before = if streaming { self.heard.get(&message.id) } else { None };
        let sent = match before {
            Some(before) => {
                let from = before.iter().zip(&prints).take_while(|(a, b)| a == b).count();
                let unchanged: HashMap<&str, u64> = before.iter().map(|(id, print)| (id.as_str(), *print)).collect();
                let rest: Vec<Value> = tools[from..]
                    .iter()
                    .zip(&prints[from..])
                    .map(|(tool, (id, print))| {
                        if unchanged.get(id.as_str()) == Some(print) {
                            json!({ "id": id, "same": true })
                        } else {
                            serde_json::to_value(tool).unwrap_or(Value::Null)
                        }
                    })
                    .collect();
                Some((from, rest))
            }
            None => None,
        };
        let id = message.id.clone();
        if streaming {
            self.remember(id, prints);
        } else if self.heard.remove(&id).is_some() {
            self.order.retain(|o| *o != id);
        }
        let mut value = screen_message(conversation_id, message);
        let shown = &mut value["message"];
        match sent {
            Some((from, rest)) => {
                shown["tools"] = Value::Array(rest);
                shown["toolsFrom"] = from.into();
            }
            None if !tools.is_empty() => shown["tools"] = serde_json::to_value(&tools).unwrap_or(Value::Null),
            None => {}
        }
        value
    }

    fn remember(&mut self, id: String, prints: Vec<(String, u64)>) {
        if self.heard.insert(id.clone(), prints).is_none() {
            self.order.push(id);
        }
        if self.order.len() > KEPT {
            let oldest = self.order.remove(0);
            self.heard.remove(&oldest);
        }
    }
}

fn screen_message(conversation_id: String, message: Box<Message>) -> Value {
    serde_json::to_value(AppEvent::Message { conversation_id, message }).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str, output: &str) -> ToolUse {
        serde_json::from_value(json!({ "id": id, "name": "Read", "input": { "file_path": id }, "output": output })).unwrap()
    }

    fn reply(status: Status, tools: Vec<ToolUse>) -> AppEvent {
        let mut message: Message = serde_json::from_value(json!({ "id": "m1", "role": "assistant", "text": "", "at": 1 })).unwrap();
        message.status = Some(status);
        message.tools = Some(tools);
        AppEvent::Message { conversation_id: "c1".into(), message: Box::new(message) }
    }

    #[test]
    fn a_streaming_reply_sends_only_the_steps_that_changed_after_its_first_update() {
        let mut feed = Feed::default();
        let first = feed.shrink(reply(Status::Streaming, vec![tool("a", "1"), tool("b", "")]));
        assert_eq!(first["message"]["tools"].as_array().unwrap().len(), 2);
        assert!(first["message"].get("toolsFrom").is_none(), "the first update is whole: {first}");
        assert_eq!(first["type"], "message");
        assert_eq!(first["conversationId"], "c1");

        let next = feed.shrink(reply(Status::Streaming, vec![tool("a", "1"), tool("b", "2"), tool("c", "")]));
        assert_eq!(next["message"]["toolsFrom"], 1, "{next}");
        let rest = next["message"]["tools"].as_array().unwrap();
        assert_eq!(rest.len(), 2);
        assert_eq!((rest[0]["id"].as_str(), rest[0]["output"].as_str()), (Some("b"), Some("2")));
        assert_eq!(rest[1]["id"], "c");

        // A step that changed early leaves the later ones as their ids alone.
        let early = feed.shrink(reply(Status::Streaming, vec![tool("a", "1!"), tool("b", "2"), tool("c", "")]));
        assert_eq!(early["message"]["toolsFrom"], 0);
        let rest = early["message"]["tools"].as_array().unwrap();
        assert_eq!(rest[0]["output"], "1!");
        assert_eq!(rest[1], json!({ "id": "b", "same": true }));
        assert_eq!(rest[2], json!({ "id": "c", "same": true }));
    }

    #[test]
    fn an_ended_reply_and_the_first_update_after_an_open_go_whole() {
        let mut feed = Feed::default();
        feed.shrink(reply(Status::Streaming, vec![tool("a", "1")]));
        feed.forget();
        let reopened = feed.shrink(reply(Status::Streaming, vec![tool("a", "1")]));
        assert!(reopened["message"].get("toolsFrom").is_none(), "{reopened}");
        assert_eq!(reopened["message"]["tools"][0]["output"], "1");
        let done = feed.shrink(reply(Status::Done, vec![tool("a", "1")]));
        assert!(done["message"].get("toolsFrom").is_none(), "{done}");
        assert_eq!(done["message"]["tools"][0]["output"], "1");
        assert!(feed.heard.is_empty() && feed.order.is_empty(), "an ended reply is not remembered");
    }

    #[test]
    fn other_events_pass_as_they_are_and_old_records_are_dropped() {
        let mut feed = Feed::default();
        assert_eq!(feed.shrink(AppEvent::List), json!({ "type": "list" }));
        for i in 0..KEPT + 5 {
            feed.remember(format!("m{i}"), vec![]);
        }
        assert_eq!((feed.heard.len(), feed.order.len()), (KEPT, KEPT));
        assert!(!feed.heard.contains_key("m0"));
    }
}
