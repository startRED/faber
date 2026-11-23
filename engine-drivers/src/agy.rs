//! Antigravity's CLI over its ACP server (`agy acp`, docs/dev/antigravity-cli.md#over-acp): a pure reader that turns what it
//! says, one JSON-RPC message per line, into the reply events the engine already knows, and the messages a driver writes to
//! it. No process here: the session driver feeds `AcpReader` every line it reads and every message it writes.

use crate::events::{EngineEvent, TOOL_OUTPUT_CHARS};
use crate::tools::Launch;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

/// The client's name in `initialize`.
pub const CLIENT_NAME: &str = "faber";

/// `initialize`, declaring forms so a question comes as `elicitation/create` instead of reply text.
pub fn initialize(id: u64, version: &str) -> Value {
    request(
        id,
        "initialize",
        json!({ "protocolVersion": 1, "clientCapabilities": { "elicitation": { "form": {} } }, "clientInfo": { "name": CLIENT_NAME, "version": version } }),
    )
}

/// `session/new` in `cwd` with Faber's MCP servers.
pub fn session_new(id: u64, cwd: &str, servers: &[Launch]) -> Value {
    request(id, "session/new", json!({ "cwd": cwd, "mcpServers": mcp_servers(servers) }))
}

/// `session/load`: the conversation comes back, replayed first (which the reader does not show again).
pub fn session_load(id: u64, session: &str, cwd: &str, servers: &[Launch]) -> Value {
    request(id, "session/load", json!({ "sessionId": session, "cwd": cwd, "mcpServers": mcp_servers(servers) }))
}

/// `session/set_model` with a model id that holds the effort too (`gemini-3.8-flash-low`).
pub fn set_model(id: u64, session: &str, model: &str) -> Value {
    request(id, "session/set_model", json!({ "sessionId": session, "modelId": model }))
}

/// `session/set_config_option`: `model` (the family) or `thought_level` (the effort) on its own.
pub fn set_config_option(id: u64, session: &str, config: &str, value: &str) -> Value {
    request(id, "session/set_config_option", json!({ "sessionId": session, "configId": config, "value": value }))
}

/// `session/set_mode`: `default` asks before a command or an MCP tool, `accept-edits`, `auto`, `plan`.
pub fn set_mode(id: u64, session: &str, mode: &str) -> Value {
    request(id, "session/set_mode", json!({ "sessionId": session, "modeId": mode }))
}

/// `session/prompt` with the person's text.
pub fn prompt(id: u64, session: &str, text: &str) -> Value {
    request(id, "session/prompt", json!({ "sessionId": session, "prompt": [{ "type": "text", "text": text }] }))
}

/// The notification `session/cancel`: the turn ends `cancelled`. A pending request still needs its answer (`cancelled`).
pub fn cancel(session: &str) -> Value {
    json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": session } })
}

/// The answer to `session/request_permission`: the chosen option, or `None` for a cancelled request.
pub fn permission_answer(request: &Value, option: Option<&str>) -> Value {
    let outcome = match option {
        Some(option) => json!({ "outcome": "selected", "optionId": option }),
        None => json!({ "outcome": "cancelled" }),
    };
    json!({ "jsonrpc": "2.0", "id": request, "result": { "outcome": outcome } })
}

/// The answer to `elicitation/create`: the form's fields (`{"q1": "2"}`), or `None` to decline.
pub fn elicitation_answer(request: &Value, content: Option<Value>) -> Value {
    let result = match content {
        Some(content) => json!({ "action": "accept", "content": content }),
        None => json!({ "action": "decline" }),
    };
    json!({ "jsonrpc": "2.0", "id": request, "result": result })
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn mcp_servers(servers: &[Launch]) -> Value {
    servers
        .iter()
        .map(|s| {
            let env: Vec<Value> = s.env.iter().map(|(name, value)| json!({ "name": name, "value": value })).collect();
            json!({ "name": s.name, "command": s.command, "args": s.args, "env": env })
        })
        .collect()
}

/// A request agy is waiting on, kept until it is answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    Permission {
        /// The JSON-RPC id, echoed as it came (agy sends strings).
        id: Value,
        call: String,
        allow: Option<String>,
        reject: Option<String>,
    },
    Question {
        id: Value,
        /// Each form field (`q1`), the text the card shows for it, its choices (value, label) and its free-text field.
        fields: Vec<Field>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub text: String,
    pub choices: Vec<(String, String)>,
    pub other: Option<String>,
}

/// What the client sent that an answer refers to.
#[derive(Debug)]
enum Sent {
    Prompt,
    /// `session/new`, `session/load`, `session/resume`: the answer names the session.
    Open {
        session: Option<String>,
        replay: bool,
    },
    Model(String),
    Setting,
    Other,
}

/// One connection's state: which requests are in flight, the open tool calls, the questions waiting for an answer.
#[derive(Debug, Default)]
pub struct AcpReader {
    sent: HashMap<String, Sent>,
    session: String,
    /// The MCP servers given at the start, to split `mcp_<server>_<tool>`.
    servers: Vec<String>,
    open: HashSet<String>,
    done: HashSet<String>,
    declined: HashSet<String>,
    pending: HashMap<String, Asked>,
    after_tool: bool,
    bad_line: bool,
}

/// A JSON-RPC id as a map key: `"1"` and `1` stay apart.
fn key(id: &Value) -> String {
    id.to_string()
}

/// The `request_id` of a card: agy's string id as it is, any other id as JSON.
fn asked_key(id: &Value) -> String {
    id.as_str().map(String::from).unwrap_or_else(|| key(id))
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn cut(text: &str) -> String {
    text.chars().take(TOOL_OUTPUT_CHARS).collect()
}

impl AcpReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The session the last `session/new` or `session/load` opened.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// The request still waiting for the person's answer, by the `request_id` of its `Permission` event.
    pub fn asked(&self, request_id: &str) -> Option<&Asked> {
        self.pending.get(request_id)
    }

    /// Notes a message the driver wrote: a request, so its answer can be read, or an answer to agy's own request.
    pub fn sent(&mut self, msg: &Value) {
        let p = &msg["params"];
        if let Some(method) = msg["method"].as_str() {
            if msg["id"].is_null() {
                return;
            }
            if method == "session/new" || method == "session/load" {
                self.servers =
                    p["mcpServers"].as_array().into_iter().flatten().filter_map(|s| s["name"].as_str().map(String::from)).collect();
            }
            let sent = match method {
                "session/prompt" => Sent::Prompt,
                "session/new" => Sent::Open { session: None, replay: false },
                "session/load" | "session/resume" => {
                    Sent::Open { session: p["sessionId"].as_str().map(String::from), replay: method == "session/load" }
                }
                "session/set_model" => Sent::Model(text(&p["modelId"])),
                "session/set_mode" | "session/set_config_option" => Sent::Setting,
                _ => Sent::Other,
            };
            self.sent.insert(key(&msg["id"]), sent);
            return;
        }
        if let Some(Asked::Permission { call, reject, .. }) = self.pending.remove(&asked_key(&msg["id"]))
            && reject.is_some()
            && msg["result"]["outcome"]["optionId"].as_str() == reject.as_deref()
        {
            self.declined.insert(call);
        }
    }

    /// The answer for a permission card: the allow or the reject option agy offered. `None` once answered or unknown.
    pub fn answer_permission(&mut self, request_id: &str, allow: bool) -> Option<Value> {
        let Some(Asked::Permission { id, allow: yes, reject: no, .. }) = self.pending.get(request_id).cloned() else { return None };
        let msg = permission_answer(&id, if allow { yes.as_deref() } else { no.as_deref() });
        self.sent(&msg);
        Some(msg)
    }

    /// The answer for a question card: `reply` maps each shown question to the chosen label or free text, `None` skips.
    pub fn answer_question(&mut self, request_id: &str, reply: Option<&Value>) -> Option<Value> {
        let Some(Asked::Question { id, fields }) = self.pending.remove(request_id) else { return None };
        let mut content = Map::new();
        for f in &fields {
            let Some(answer) = reply.and_then(|r| r[&f.text].as_str()).filter(|a| !a.trim().is_empty()) else { continue };
            match (f.choices.iter().find(|(_, label)| label == answer), &f.other) {
                (Some((value, _)), _) => content.insert(f.name.clone(), value.as_str().into()),
                (None, Some(other)) => content.insert(other.clone(), answer.into()),
                (None, None) => continue,
            };
        }
        Some(elicitation_answer(&id, (!content.is_empty()).then_some(Value::Object(content))))
    }

    /// Answers every request still waiting as cancelled, as a stop needs (agy waits for them otherwise).
    pub fn cancel_pending(&mut self) -> Vec<Value> {
        self.pending
            .drain()
            .map(|(_, asked)| match asked {
                Asked::Permission { id, .. } => permission_answer(&id, None),
                Asked::Question { id, .. } => elicitation_answer(&id, None),
            })
            .collect()
    }

    /// Reads one line agy printed. A line that is not JSON is skipped, and said once on stderr.
    pub fn read(&mut self, line: &str) -> Vec<EngineEvent> {
        match serde_json::from_str::<Value>(line) {
            Ok(msg) => self.read_value(&msg),
            Err(_) => {
                if !std::mem::replace(&mut self.bad_line, true) {
                    eprintln!("faber: Antigravity printed a line that is not JSON: {}", line.chars().take(80).collect::<String>());
                }
                vec![]
            }
        }
    }

    pub fn read_value(&mut self, msg: &Value) -> Vec<EngineEvent> {
        match msg["method"].as_str() {
            Some("session/update") if !self.replaying() => self.update(&msg["params"]["update"]),
            Some("session/update") => vec![],
            Some("session/request_permission") if !msg["id"].is_null() => self.permission(msg),
            Some("elicitation/create") if !msg["id"].is_null() => self.question(msg),
            Some(_) => vec![],
            None => self.answer(msg),
        }
    }

    /// A `session/load` in flight: what comes now is the conversation's history, already shown.
    fn replaying(&self) -> bool {
        self.sent.values().any(|s| matches!(s, Sent::Open { replay: true, .. }))
    }

    fn answer(&mut self, msg: &Value) -> Vec<EngineEvent> {
        let Some(sent) = self.sent.remove(&key(&msg["id"])) else { return vec![] };
        let r = &msg["result"];
        let error = msg["error"].is_object().then(|| text(&msg["error"]["message"]));
        match (sent, error) {
            (Sent::Prompt, None) => self.end(r["stopReason"].as_str().unwrap_or_default(), &r["usage"]),
            (Sent::Prompt, Some(message)) => {
                let lost = message.starts_with("Session not found");
                let mut events = self.end_failed(msg, message);
                if lost {
                    events.insert(0, EngineEvent::Error { message: "error.sessionLost".into(), fatal: true });
                }
                events
            }
            (Sent::Open { session, .. }, None) => {
                let Some(id) = r["sessionId"].as_str().map(String::from).or(session) else { return vec![] };
                self.session = id.clone();
                vec![EngineEvent::Started { session_id: id, model: text(&r["models"]["currentModelId"]) }]
            }
            (Sent::Open { .. }, Some(_)) => vec![EngineEvent::Error { message: "error.sessionLost".into(), fatal: true }],
            (Sent::Model(model), None) => vec![EngineEvent::Started { session_id: self.session.clone(), model }],
            // A model, an effort or a mode agy lacks: the reply must not run on another one.
            (Sent::Model(_) | Sent::Setting, Some(message)) => self.end_failed(msg, message),
            _ => vec![],
        }
    }

    fn end_failed(&mut self, msg: &Value, message: String) -> Vec<EngineEvent> {
        let kind = crate::failure::from_text(&message).or((msg["error"]["code"] == -32602).then_some(crate::failure::Failure::Rejected));
        let mut events = vec![EngineEvent::Reason(crate::failure::reason("reason.failed", kind))];
        events.extend(self.close_open());
        events.push(done(false, message, &Value::Null));
        events
    }

    fn end(&mut self, stop: &str, usage: &Value) -> Vec<EngineEvent> {
        let ok = stop == "end_turn";
        let mut events: Vec<EngineEvent> =
            (!ok).then(|| EngineEvent::Reason(crate::events::failure_reason(Some(stop), None))).into_iter().collect();
        events.extend(self.close_open());
        events.push(done(ok, stop.into(), usage));
        events
    }

    /// A call still open when its turn ends never finishes (a question asked as text stays `pending`): it ends with the turn.
    fn close_open(&mut self) -> Vec<EngineEvent> {
        self.after_tool = false;
        let mut open: Vec<String> = self.open.drain().collect();
        open.sort();
        open.into_iter()
            .map(|id| {
                self.done.insert(id.clone());
                EngineEvent::ToolResult { id, ok: false, declined: false, exit_code: None, text: String::new() }
            })
            .collect()
    }

    fn update(&mut self, u: &Value) -> Vec<EngineEvent> {
        match u["sessionUpdate"].as_str().unwrap_or_default() {
            "agent_message_chunk" if u["content"]["type"] == "text" => {
                let delta = EngineEvent::Text(text(&u["content"]["text"]));
                if std::mem::take(&mut self.after_tool) { vec![EngineEvent::Break { final_answer: false }, delta] } else { vec![delta] }
            }
            "agent_thought_chunk" if u["content"]["type"] == "text" => vec![EngineEvent::Thinking(text(&u["content"]["text"]))],
            "usage_update" => vec![EngineEvent::Context { used: u["used"].as_u64(), window: u["size"].as_u64() }],
            "tool_call" | "tool_call_update" => self.tool(u),
            _ => vec![],
        }
    }

    fn tool(&mut self, u: &Value) -> Vec<EngineEvent> {
        let Some(id) = u["toolCallId"].as_str().map(String::from) else { return vec![] };
        if self.done.contains(&id) {
            return vec![];
        }
        let mut events = self.start(&id, u);
        let status = u["status"].as_str().unwrap_or_default();
        if matches!(status, "completed" | "failed") {
            self.open.remove(&id);
            self.done.insert(id.clone());
            let declined = self.declined.remove(&id);
            let command = self.step(u).0 == "Bash";
            let exit_code = command.then(|| u["rawOutput"]["exit_code"].as_i64()).flatten().and_then(|c| i32::try_from(c).ok());
            let ok = status == "completed" && !declined && exit_code.is_none_or(|c| c == 0);
            events.push(EngineEvent::ToolResult { id, ok, declined, exit_code, text: cut(&result_text(u, command)) });
        }
        events
    }

    /// The call's `Tool` event the first time it is seen.
    fn start(&mut self, id: &str, u: &Value) -> Vec<EngineEvent> {
        if !self.open.insert(id.to_string()) {
            return vec![];
        }
        self.after_tool = true;
        let (name, input) = self.step(u);
        vec![EngineEvent::Tool { id: id.into(), name, input, parent: None }]
    }

    /// The name and input the screen already draws: Claude Code's for a command, a file written or changed, a read, a search,
    /// a question and an MCP tool (`mcp__<server>__<tool>`); any other call keeps agy's own name.
    fn step(&self, u: &Value) -> (String, Value) {
        let name = u["name"].as_str().unwrap_or_default();
        let raw = &u["rawInput"];
        let path = || raw["Path"].as_str().or_else(|| u["locations"][0]["path"].as_str()).map(String::from);
        match (name, u["kind"].as_str().unwrap_or_default()) {
            ("run_command", _) | (_, "execute") if raw["CommandLine"].is_string() => {
                ("Bash".into(), json!({ "command": raw["CommandLine"] }))
            }
            ("write_file", _) | (_, "edit") => {
                let diff = u["content"].as_array().into_iter().flatten().find(|c| c["type"] == "diff");
                match (diff, path()) {
                    (Some(d), _) if d["oldText"].as_str().is_some_and(|t| !t.is_empty()) => {
                        ("Edit".into(), json!({ "file_path": d["path"], "old_string": d["oldText"], "new_string": d["newText"] }))
                    }
                    (Some(d), _) => ("Write".into(), json!({ "file_path": d["path"], "content": d["newText"] })),
                    (None, Some(p)) if raw["Content"].is_string() => ("Write".into(), json!({ "file_path": p, "content": raw["Content"] })),
                    _ => (name_or(name, u), raw.clone()),
                }
            }
            ("read_file", _) | (_, "read") if path().is_some() => ("Read".into(), json!({ "file_path": path() })),
            // shortcut: no search tool was recorded, so its input goes as agy names it; map its fields once one is.
            (_, "search") => ("Grep".into(), raw.clone()),
            ("ask_question", _) => ("AskUserQuestion".into(), question_input(raw)),
            _ => match self
                .servers
                .iter()
                .find_map(|s| name.strip_prefix("mcp_")?.strip_prefix(s.as_str())?.strip_prefix('_').map(|t| (s, t)))
            {
                Some((server, tool)) => (format!("mcp__{server}__{tool}"), raw.clone()),
                None => (name_or(name, u), raw.clone()),
            },
        }
    }

    fn permission(&mut self, msg: &Value) -> Vec<EngineEvent> {
        let call = &msg["params"]["toolCall"];
        let Some(id) = call["toolCallId"].as_str() else { return vec![] };
        let options = msg["params"]["options"].as_array().map(Vec::as_slice).unwrap_or_default();
        let option = |prefix: &str| {
            options
                .iter()
                .find(|o| o["kind"].as_str().is_some_and(|k| k.starts_with(prefix)))
                .and_then(|o| o["optionId"].as_str())
                .map(String::from)
        };
        let request_id = asked_key(&msg["id"]);
        self.pending.insert(
            request_id.clone(),
            Asked::Permission { id: msg["id"].clone(), call: id.into(), allow: option("allow"), reject: option("reject") },
        );
        let mut events = self.start(id, call);
        let (tool, input) = self.step(call);
        events.push(EngineEvent::Permission { request_id, tool, input });
        events
    }

    fn question(&mut self, msg: &Value) -> Vec<EngineEvent> {
        let props = msg["params"]["requestedSchema"]["properties"].as_object();
        let mut fields: Vec<Field> = Vec::new();
        for (name, p) in props.into_iter().flatten() {
            let Some(choices) = p["oneOf"].as_array() else { continue };
            let asked = text(&p["title"]);
            let mut shown = asked.clone();
            for n in 2.. {
                if !fields.iter().any(|f| f.text == shown) {
                    break;
                }
                shown = format!("{asked} ({n})");
            }
            let other = format!("{name}_other");
            fields.push(Field {
                name: name.clone(),
                text: shown,
                choices: choices.iter().filter_map(|c| Some((c["const"].as_str()?.into(), c["title"].as_str()?.into()))).collect(),
                other: props.is_some_and(|all| all.contains_key(&other)).then_some(other),
            });
        }
        if fields.is_empty() {
            return vec![];
        }
        let questions: Vec<Value> = fields
            .iter()
            .map(|f| {
                let options: Vec<Value> = f.choices.iter().map(|(_, label)| json!({ "label": label, "description": "" })).collect();
                json!({ "question": f.text, "header": "", "multiSelect": false, "options": options })
            })
            .collect();
        let request_id = asked_key(&msg["id"]);
        self.pending.insert(request_id.clone(), Asked::Question { id: msg["id"].clone(), fields });
        vec![EngineEvent::Permission { request_id, tool: "AskUserQuestion".into(), input: json!({ "questions": questions }) }]
    }
}

fn name_or(name: &str, u: &Value) -> String {
    [name, u["kind"].as_str().unwrap_or_default(), "tool"].into_iter().find(|n| !n.is_empty()).unwrap_or("tool").into()
}

/// `ask_question`'s input as the card Claude Code's `AskUserQuestion` takes.
fn question_input(raw: &Value) -> Value {
    let questions: Vec<Value> = raw["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| {
            let options: Vec<Value> = q["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|o| json!({ "label": o, "description": "" }))
                .collect();
            json!({ "question": q["question"], "header": "", "multiSelect": q["is_multi_select"] == true, "options": options })
        })
        .collect();
    json!({ "questions": questions })
}

/// What a finished call says: a command's own output (agy puts times and the exit code before it), else its text parts.
fn result_text(u: &Value, command: bool) -> String {
    if command && let Some(out) = u["rawOutput"]["output"].as_str() {
        return out.split_once("Command output:\n").map_or(out, |(_, rest)| rest).to_string();
    }
    let parts = u["content"].as_array().into_iter().flatten().filter_map(|c| c["content"]["text"].as_str()).filter(|t| !t.is_empty());
    parts.collect::<Vec<_>>().join("\n")
}

/// The turn's `Done` from its usage, which counts that turn alone: the prompt without its cache in `inputTokens`, and the
/// thinking apart from the output, so both are added back the way Claude Code counts them.
fn done(ok: bool, reason: String, usage: &Value) -> EngineEvent {
    let n = |name: &str| usage[name].as_u64().unwrap_or(0);
    EngineEvent::Done {
        ok,
        reason,
        cost_total_usd: None,
        input_tokens: n("inputTokens") + n("cachedReadTokens") + n("cachedWriteTokens"),
        output_tokens: n("outputTokens") + n("thoughtTokens"),
        cache_read_tokens: n("cachedReadTokens"),
        cache_write_tokens: n("cachedWriteTokens"),
        cache_write_1h_tokens: 0,
        incomplete: !usage.is_object(),
    }
}

#[cfg(test)]
mod tests;
