//! `faber chat`: sends a message (or follows the turn already running) and streams the answer as it comes. A person at the
//! terminal answers the AI's cards right there; a program gets one JSON object per line and the card that waits for it.

use crate::out::{DONE, Out, REFUSED, STOPPED, Tone, UNREACHED, USAGE, WAITING};
use crate::thread::{self, StepState, Thread};
use crate::verbs::{self, Session, Stop};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{IsTerminal, Read};

pub async fn run(session: &mut Session) -> Result<i32, Stop> {
    let state = session.state().await?.clone();
    let state = if state.get("state").is_some() { &state["state"] } else { &state };
    let mut text = session.args.rest(1);
    if text == "-" || (text.is_empty() && !std::io::stdin().is_terminal()) {
        text.clear();
        let _ = std::io::stdin().read_to_string(&mut text);
    }
    let text = text.trim().to_string();
    let id = session.args.value("id").map(str::to_string);
    if text.is_empty() && id.is_none() {
        eprintln!("{}", session.out.words.fill("cli.error.usage", &[("usage", verbs::VERBS[3].1)]));
        return Err(USAGE);
    }
    // Events first, so nothing the answer says is missed between the send and the subscription.
    let Ok(mut events) = session.client.subscribe().await else { return Err(session.out.fail("cli.error.channel", UNREACHED)) };
    let known = id.as_deref().and_then(|id| state["conversations"].as_array()?.iter().find(|c| c["id"].as_str() == Some(id)).cloned());
    if id.is_some() && known.is_none() {
        return Err(session.out.fail("cli.error.noConversation", REFUSED));
    }
    let sent_at = now_ms();
    let conversation = if text.is_empty() {
        id.clone().unwrap_or_default()
    } else {
        let project =
            session.args.value("project").map(str::to_string).or_else(|| known.as_ref()?["projectId"].as_str().map(str::to_string));
        let area = known.as_ref().and_then(|c| c["area"].as_str()).unwrap_or(if project.is_some() { "project" } else { "chat" });
        let mut choice = json!({ "vendor": session.args.value("vendor").unwrap_or("auto") });
        if let Some(model) = session.args.value("model") {
            choice["model"] = json!(model);
        }
        if let Some(effort) = session.args.value("effort") {
            choice["effort"] = json!(effort);
        }
        let req = json!({ "conversationId": id, "area": area, "projectId": project, "text": text, "choice": choice });
        let sent = session.ask("send", json!({ "req": req })).await?;
        sent.as_str().map(str::to_string).or(id.clone()).unwrap_or_default()
    };
    if session.args.on("no-wait") {
        if session.out.json {
            session.out.json(&json!({ "conversationId": conversation }));
        } else {
            session.out.line(&conversation);
        }
        return Ok(DONE);
    }
    let opened = session.ask("open", json!({ "id": conversation })).await?;
    let mut follow = Follow::new(&conversation, Thread::from_conversation(&opened), if text.is_empty() { 0 } else { sent_at });
    if text.is_empty() && !is_running(state, &conversation) {
        // Nothing runs: the last answer is the result.
        return Ok(follow.finish(&session.out));
    }
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { return Err(session.out.fail("cli.error.gone", UNREACHED)) };
                match follow.hear(session, event).await? {
                    Some(code) => return Ok(code),
                    None => continue,
                }
            }
            _ = tokio::signal::ctrl_c() => {
                let _ = session.client.call("interrupt", json!({ "id": conversation })).await;
                session.out.line("");
                session.out.line(&session.out.paint(Tone::Quiet, &session.out.t("cli.chat.stopped")));
                return Ok(STOPPED);
            }
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn is_running(state: &Value, conversation: &str) -> bool {
    state["running"]["items"].as_array().into_iter().flatten().any(|r| r["conversationId"].as_str() == Some(conversation))
}

/// What has been shown of the answer so far.
struct Follow {
    conversation: String,
    thread: Thread,
    /// Replies older than this are history, never the answer being followed.
    since: u64,
    /// The reply being followed, once it appears.
    reply: Option<String>,
    /// Characters of its text already shown, and the steps already shown as finished.
    shown_text: usize,
    shown_steps: HashSet<String>,
    /// Cards already answered or already told.
    cards: HashSet<String>,
    /// A step that started is told once, when it ends.
    starts: HashMap<String, StepState>,
}

impl Follow {
    fn new(conversation: &str, thread: Thread, since: u64) -> Follow {
        let reply = if since == 0 {
            thread.last().filter(|m| m["role"] == "assistant").and_then(|m| m["id"].as_str()).map(str::to_string)
        } else {
            None
        };
        let mut follow = Follow {
            conversation: conversation.into(),
            thread,
            since,
            reply,
            shown_text: 0,
            shown_steps: HashSet::new(),
            cards: HashSet::new(),
            starts: HashMap::new(),
        };
        // Following a turn already under way: what it said before is not repeated.
        if let Some(message) = follow.reply.as_ref().and_then(|id| follow.thread.get(id)).cloned() {
            follow.shown_text = message["text"].as_str().unwrap_or("").chars().count();
            for tool in message["tools"].as_array().into_iter().flatten() {
                if thread::step_state(tool) != StepState::Running {
                    follow.shown_steps.insert(tool["id"].as_str().unwrap_or("").into());
                }
            }
        }
        follow
    }

    /// One event; Some(exit code) when the turn is over or waits for someone this terminal cannot reach.
    async fn hear(&mut self, session: &Session, event: Value) -> Result<Option<i32>, Stop> {
        match event["type"].as_str() {
            Some("message") if event["conversationId"].as_str() == Some(&self.conversation) => {
                let message = event["message"].clone();
                let whole = match self.thread.take(message) {
                    Ok(whole) => whole.clone(),
                    Err(thread::Unknown) => {
                        // A reply this terminal never saw whole: opening the conversation again sends the next update whole.
                        let opened = session.ask("open", json!({ "id": self.conversation })).await?;
                        self.thread = Thread::from_conversation(&opened);
                        return Ok(None);
                    }
                };
                if whole["role"] != "assistant" || whole["at"].as_u64().unwrap_or(0) + 2000 < self.since {
                    return Ok(None);
                }
                let id = whole["id"].as_str().unwrap_or("").to_string();
                if self.reply.as_deref() != Some(&id) {
                    self.reply = Some(id);
                    self.shown_text = 0;
                    self.shown_steps.clear();
                }
                self.show(&session.out, &whole);
                if whole["status"].as_str().is_some_and(|s| s != "streaming") {
                    return Ok(Some(self.finish(&session.out)));
                }
            }
            Some("running") => {
                let items = event["items"].as_array().cloned().unwrap_or_default();
                return self.cards(session, &items).await;
            }
            // Events were dropped while this terminal was slow: the end of the turn or a card may be among them.
            Some("lagged") => {
                let opened = session.ask("open", json!({ "id": self.conversation })).await?;
                self.thread = Thread::from_conversation(&opened);
                if let Some(whole) = self.reply.as_ref().and_then(|id| self.thread.get(id)).cloned() {
                    self.show(&session.out, &whole);
                    if whole["status"].as_str().is_some_and(|s| s != "streaming") {
                        return Ok(Some(self.finish(&session.out)));
                    }
                }
                let state = session.ask("state", Value::Null).await?;
                let items = state["running"]["items"].as_array().cloned().unwrap_or_default();
                return self.cards(session, &items).await;
            }
            _ => {}
        }
        Ok(None)
    }

    /// The cards of this conversation in a list of what runs, each answered once.
    async fn cards(&mut self, session: &Session, items: &[Value]) -> Result<Option<i32>, Stop> {
        for item in items.iter().filter(|i| i["conversationId"].as_str() == Some(&self.conversation)) {
            let turn = item["turnId"].as_str().unwrap_or("").to_string();
            for permission in item["permissions"].as_array().into_iter().flatten() {
                let key = format!("{turn}/{}", permission["requestId"].as_str().unwrap_or(""));
                if !self.cards.insert(key) {
                    continue;
                }
                if let Some(code) = card(session, &self.conversation, &turn, permission).await? {
                    return Ok(Some(code));
                }
            }
        }
        Ok(None)
    }

    /// The new part of the answer: finished steps one line each, and the text that grew.
    fn show(&mut self, out: &Out, message: &Value) {
        for tool in message["tools"].as_array().into_iter().flatten() {
            let id = tool["id"].as_str().unwrap_or("").to_string();
            let now = thread::step_state(tool);
            if now == StepState::Running {
                if self.starts.insert(id, now).is_none() && out.json {
                    out.line(&json!({ "type": "step", "state": "running", "step": tool }).to_string());
                }
                continue;
            }
            if !self.shown_steps.insert(id) {
                continue;
            }
            if out.json {
                out.line(&json!({ "type": "step", "state": state_word(now), "step": tool }).to_string());
            } else {
                if self.shown_text > 0 {
                    out.line("");
                }
                out.line(&verbs::step(out, tool));
            }
        }
        let text = message["text"].as_str().unwrap_or("");
        let count = text.chars().count();
        if count > self.shown_text {
            let delta: String = text.chars().skip(self.shown_text).collect();
            self.shown_text = count;
            if out.json {
                out.line(&json!({ "type": "text", "delta": delta }).to_string());
            } else {
                out.part(&delta);
            }
        }
    }

    /// The end of the turn: the result line for a program, the quiet line under the answer for a person.
    fn finish(&self, out: &Out) -> i32 {
        let message = self.reply.as_ref().and_then(|id| self.thread.get(id)).or_else(|| self.thread.last()).cloned().unwrap_or(Value::Null);
        let status = message["status"].as_str().unwrap_or("done");
        let failed = status == "error";
        if out.json {
            out.line(
                &json!({
                    "type": "result",
                    "conversationId": self.conversation,
                    "messageId": message["id"],
                    "status": status,
                    "error": message["error"],
                    "text": message["text"],
                    "vendor": message["vendor"],
                    "model": message["model"],
                    "took": message["took"],
                    "inputTokens": message["inputTokens"],
                    "outputTokens": message["outputTokens"],
                    "costUsd": message["costUsd"],
                })
                .to_string(),
            );
        } else {
            if self.shown_text == 0 && self.since == 0 {
                verbs::print_text(out, message["text"].as_str().unwrap_or(""));
            }
            out.line("");
            if let Some(meta) = verbs::meta_line(out, &message) {
                out.line(&out.paint(if failed { Tone::Bad } else { Tone::Quiet }, &meta));
            }
            out.line(&out.paint(Tone::Quiet, &out.words.fill("cli.chat.continue", &[("id", &self.conversation)])));
        }
        if failed { REFUSED } else { DONE }
    }
}

fn state_word(state: StepState) -> &'static str {
    match state {
        StepState::Running => "running",
        StepState::Done => "done",
        StepState::Failed => "failed",
        StepState::Declined => "declined",
    }
}

/// One card of the AI: answered here by a person, or told to a program, which answers it with `faber call`.
async fn card(session: &Session, conversation: &str, turn: &str, permission: &Value) -> Result<Option<i32>, Stop> {
    let out = &session.out;
    if !out.interactive {
        if out.json {
            out.line(&json!({ "type": "needsYou", "conversationId": conversation, "turnId": turn, "permission": permission }).to_string());
        } else {
            eprintln!("{}", out.paint(Tone::Warn, &out.words.fill("cli.card.elsewhere", &[("id", conversation)])));
        }
        return Ok(Some(WAITING));
    }
    let request = permission["requestId"].as_str().unwrap_or("");
    let input = &permission["input"];
    out.line("");
    match permission["tool"].as_str().unwrap_or("") {
        "AskUserQuestion" => {
            let mut answers = serde_json::Map::new();
            for q in input["questions"].as_array().into_iter().flatten() {
                let question = q["question"].as_str().unwrap_or("");
                out.line(&out.paint(Tone::Strong, &format!("? {question}")));
                let options: Vec<&str> = q["options"].as_array().into_iter().flatten().filter_map(|o| o["label"].as_str()).collect();
                for (i, option) in options.iter().enumerate() {
                    let about = q["options"][i]["description"].as_str().unwrap_or("");
                    out.line(&format!("  {} {option}  {}", out.paint(Tone::Accent, &format!("{}.", i + 1)), out.paint(Tone::Quiet, about)));
                }
                let multi = q["multiSelect"].as_bool() == Some(true);
                let hint = if multi { "cli.card.pickMany" } else { "cli.card.pickOne" };
                let said = ask_line(out, &out.t(hint)).await;
                answers.insert(question.into(), Value::String(chosen(&said, &options, multi)));
            }
            session.ask("answer_question", json!({ "id": conversation, "turnId": turn, "requestId": request, "answers": answers })).await?;
        }
        "ExitPlanMode" => {
            out.line(&out.paint(Tone::Strong, &out.t("cli.card.plan")));
            verbs::print_text(out, input["plan"].as_str().unwrap_or(""));
            let said = ask_line(out, &out.t("cli.card.planChoice")).await;
            let mode = input["mode"].as_str().unwrap_or("ask");
            let approve = said.trim() == "1";
            let mode = if approve { json!(mode) } else { Value::Null };
            session.ask("approve_plan", json!({ "id": conversation, "turnId": turn, "requestId": request, "mode": mode })).await?;
        }
        tool => {
            let line = thread::step_line(&json!({ "name": tool, "input": input }));
            out.line(&out.paint(Tone::Strong, &out.words.fill("cli.card.permission", &[("step", &line)])));
            let said = ask_line(out, &out.t("cli.card.permissionChoice")).await;
            let (allow, always) = match said.trim() {
                "1" => (true, false),
                "2" => (true, true),
                _ => (false, false),
            };
            session
                .ask("answer", json!({ "id": conversation, "turnId": turn, "requestId": request, "allow": allow, "always": always }))
                .await?;
        }
    }
    Ok(None)
}

/// What the person picked: numbers become their labels, anything else is their own answer.
fn chosen(said: &str, options: &[&str], multi: bool) -> String {
    let picks: Vec<&str> = said
        .split([',', ' '])
        .filter(|p| !p.is_empty())
        .map(|p| p.trim().parse::<usize>().ok().and_then(|n| options.get(n.wrapping_sub(1)).copied()))
        .collect::<Option<Vec<_>>>()
        .unwrap_or_default();
    match picks.as_slice() {
        [] => said.trim().to_string(),
        [one, ..] if !multi => one.to_string(),
        many => many.join(", "),
    }
}

/// One line typed by the person, read off the async runtime.
async fn ask_line(out: &Out, prompt: &str) -> String {
    out.part(&format!("{} ", out.paint(Tone::Accent, prompt)));
    tokio::task::spawn_blocking(|| {
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        line
    })
    .await
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_pick_their_options_and_words_are_an_answer_of_their_own() {
        let options = ["Yes", "No", "Later"];
        assert_eq!(chosen("2", &options, false), "No");
        assert_eq!(chosen("1, 3", &options, true), "Yes, Later");
        assert_eq!(chosen("1 3", &options, false), "Yes");
        assert_eq!(chosen("only on Fridays", &options, false), "only on Fridays");
        assert_eq!(chosen("9", &options, false), "9");
    }
}
