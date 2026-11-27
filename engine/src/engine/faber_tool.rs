//! The `faber` tool (R585): a conversation's agent finds and runs Faber's own commands. Both actions go to the host away
//! from the engine's lock, in the conversation's scope: Plan and Isolated, which change nothing (`toolbox::may_change`),
//! run only true reads, and a Chat never reaches a project's own commands. The host applies each command's reach for this
//! conversation (R499), and the call is answered, under the lock only to hand the prepared answer over, when the host has.
use super::toolbox::may_change;
use super::*;
use crate::commands::Scope;
use crate::faber_tool::{Request, request};

/// The longest the agent waits for a command before it hears that it is still going and goes on. Only the wait ends: work
/// the command already handed to a blocking thread (a file read, a git call) cannot be cancelled and runs to its end.
const RUN_TIMEOUT: Duration = Duration::from_secs(120);
/// The most of an answer the agent reads; a longer one comes back as `{"truncated": true, ...}` with its start.
const MAX_ANSWER: usize = 20_000;

impl Core {
    /// Whether a conversation offers the `faber` tool: in Chat and Projects, wherever the front end gave the engine its table.
    pub(super) fn offers_faber(&self) -> bool {
        self.table.is_some()
    }

    pub(super) fn faber_call(&mut self, conv: &str, call: String, input: serde_json::Value) {
        let Some(live) = self.live.get(conv) else { return };
        let sid = live.sid;
        let scope =
            Scope { read_only: !may_change(live.permissions), project: self.convs.get(conv).is_some_and(|c| c.area == Area::Project) };
        let Some(commands) = self.table.clone() else {
            return self.answer_toolbox(conv, sid, &call, false, "Faber's commands are not available here.".into());
        };
        let asked = match request(&input) {
            Err(why) => return self.answer_toolbox(conv, sid, &call, false, why.into()),
            Ok(asked) => asked,
        };
        let (me, conv) = (self.me.clone(), conv.to_string());
        self.opts.runtime.spawn(async move {
            let (ok, text) = match asked {
                Request::Find(words) => {
                    let found = commands.find(&words, scope);
                    match found.is_empty() {
                        true => (true, "No Faber command matches these words here; try others.".to_string()),
                        false => (true, serde_json::Value::Array(found).to_string()),
                    }
                }
                Request::Run { command, args } => {
                    match tokio::time::timeout(RUN_TIMEOUT, commands.run(conv.clone(), command, args, scope)).await {
                        Ok(Ok(value)) => (true, value.to_string()),
                        Ok(Err(failure)) => (false, failure),
                        Err(_) => (false, format!("The command took longer than {} seconds; it may still finish.", RUN_TIMEOUT.as_secs())),
                    }
                }
            };
            let text = bounded(text);
            let Some(core) = me.upgrade() else { return };
            core.lock().unwrap_or_else(|e| e.into_inner()).answer_toolbox(&conv, sid, &call, ok, text);
        });
    }
}

/// The answer as the agent reads it: whole up to [`MAX_ANSWER`] characters, else a valid JSON envelope with the start of it,
/// cut before any data address it would split (a half picture is useless and misleads).
fn bounded(text: String) -> String {
    let Some((at, _)) = text.char_indices().nth(MAX_ANSWER) else { return text };
    let mut kept = &text[..at];
    if let Some(data) = kept.rfind("data:")
        && !kept[data..].contains('"')
    {
        kept = &kept[..data];
    }
    serde_json::json!({
        "truncated": true,
        "characters": text.chars().count(),
        "text": kept,
        "note": "The answer was too long; narrow the arguments (a smaller limit or range) to read the rest.",
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_answer_comes_back_as_valid_json_and_never_splits_a_data_address() {
        assert_eq!(bounded("short".into()), "short");
        let long = serde_json::json!({"text": "é".repeat(MAX_ANSWER + 5)}).to_string();
        let envelope: serde_json::Value = serde_json::from_str(&bounded(long.clone())).expect("valid JSON");
        assert_eq!(envelope["truncated"], true);
        assert_eq!(envelope["characters"], long.chars().count());
        assert!(envelope["text"].as_str().unwrap().chars().count() <= MAX_ANSWER);
        let picture =
            serde_json::json!({"before": "a", "picture": format!("data:image/png;base64,{}", "A".repeat(MAX_ANSWER))}).to_string();
        let envelope: serde_json::Value = serde_json::from_str(&bounded(picture)).unwrap();
        let kept = envelope["text"].as_str().unwrap();
        assert!(!kept.contains("data:") && kept.ends_with("\"picture\":\""), "{kept}");
    }
}
