//! The table as a conversation's agent reaches it through the `faber` tool (R585): the engine holds this through
//! `Engine::set_commands` and carries each call here. `find` and `run` share one rule, [`eligible`]: what an agent may never
//! send (the person's own commands, the screen's, `send`), what a read-only conversation may not (anything but a true read),
//! and what a Chat may not (a project's own commands) is neither listed nor run. What passes goes through
//! [`Host::dispatch_as`] as that conversation's agent, so every command keeps its reach (R499), its family rule and the asks
//! book, exactly as from the command line. Signing in, money and the agent's own permissions, trust and hooks are `person`
//! reach in the table, so this never runs them.

use crate::{Caller, CommandInfo, Failure, Host, Kind, Reach, reach};
use faber_engine::commands::{Commands, MAX_FOUND, Scope};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};

/// Reads in name only: their kind is `read`, but they run a program, prepare files or open something on the person's
/// screen, so Plan and Isolated, which change nothing, refuse them too.
pub(crate) const READS_IN_NAME_ONLY: &[&str] =
    &["setup_check", "tools_list", "tools_prepare_replacement", "project_memory", "show_file", "show_project", "cli_sessions"];

/// A project's own commands beyond the `project_` ones: a Chat's agent is never offered them.
pub(crate) const PROJECT_ONLY: &[&str] = &[
    "cli_sessions",
    "pick_up",
    "start_conversation",
    "message_conversation",
    "resume_conversation",
    "mark_waiting",
    "wake_wait",
    "mcp_servers",
    "remove_project",
    "unfollow_pr",
    "save_environment",
    "forget_environment",
    "start_environment",
];

/// A Chat's agent asked for a project's own command.
pub(crate) const PROJECT_ONLY_KEY: &str = "error.reach.projectOnly";
/// An agent asked for `send`: it speaks in its own conversation by answering, and to another through `message_conversation`.
const USE_SESSION: &str = "error.session.useSession";

/// Whether an agent in `scope` may send `command`: the one rule both `find` and `run` apply, before the host's own.
pub(crate) fn eligible(command: &CommandInfo, scope: Scope) -> Result<(), &'static str> {
    if command.reach == Reach::Person.word() {
        return Err(reach::PERSON_ONLY);
    }
    if command.reach == Reach::Screen.word() {
        return Err(reach::SCREEN_ONLY);
    }
    if command.name == "send" {
        return Err(USE_SESSION);
    }
    if scope.read_only && (command.kind != Kind::Read || READS_IN_NAME_ONLY.contains(&command.name)) {
        return Err(reach::READ_ONLY);
    }
    if !scope.project && (command.name.starts_with("project_") || PROJECT_ONLY.contains(&command.name)) {
        return Err(PROJECT_ONLY_KEY);
    }
    Ok(())
}

/// What an agent in `scope` may send.
pub(crate) fn offered(scope: Scope) -> impl Iterator<Item = &'static CommandInfo> {
    Host::commands().iter().filter(move |command| eligible(command, scope).is_ok())
}

/// Lowercase words of two letters or more.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|word| word.chars().count() >= 2).map(str::to_lowercase).collect()
}

/// How well one command matches one word: a whole word of its name counts most, the start of one next, its description least.
fn matches(command: &CommandInfo, word: &str) -> usize {
    let near = |part: &str| word.len() >= 3 && part.starts_with(word) || part.len() >= 4 && word.starts_with(part);
    if command.name.split('_').any(|part| part == word) {
        4
    } else if command.name.split('_').any(near) {
        2
    } else if word.len() >= 3 && words_of(&command.about).iter().any(|part| part.starts_with(word)) {
        1
    } else {
        0
    }
}

/// The commands that best match `words`, at most [`MAX_FOUND`], in the table's order where they match as well. A word few
/// commands match weighs more than one many match, so "rename a conversation" finds `rename` before every `conversation_*`.
pub(crate) fn find(words: &str, scope: Scope) -> Vec<Value> {
    let commands: Vec<&CommandInfo> = offered(scope).collect();
    let mut scores = vec![0; commands.len()];
    for word in words_of(words) {
        let hits: Vec<usize> = commands.iter().map(|command| matches(command, &word)).collect();
        let matching = hits.iter().filter(|hit| **hit > 0).count().max(1);
        for (score, hit) in scores.iter_mut().zip(hits) {
            *score += hit * 256 / matching;
        }
    }
    let mut found: Vec<(usize, usize)> =
        scores.into_iter().enumerate().filter(|(_, score)| *score > 0).map(|(at, score)| (score, at)).collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().take(MAX_FOUND).map(|(_, at)| shown(commands[at])).collect()
}

/// One command as the agent reads it. `by` is left out: the host sets it to the agent's own conversation whatever is sent.
fn shown(command: &CommandInfo) -> Value {
    let mut schema = command.input_schema.clone();
    if let Some(properties) = schema["properties"].as_object_mut() {
        properties.remove("by");
    }
    if let Some(required) = schema["required"].as_array_mut() {
        required.retain(|name| name != "by");
    }
    json!({"name": command.name, "about": command.about, "kind": command.kind, "reach": command.reach, "inputSchema": schema})
}

/// Runs `command` as the agent of `conversation`: [`eligible`] first, then the host's own rule, as for any other caller.
pub(crate) async fn run(host: &Host, conversation: String, command: &str, args: Value, scope: Scope) -> Result<Value, Failure> {
    if let Some(info) = Host::commands().iter().find(|info| info.name == command) {
        eligible(info, scope).map_err(Failure::key)?;
    }
    host.dispatch_as(&Caller::Agent { conversation: Some(conversation) }, command, args).await
}

/// The failure as the command line receives it: the key alone, or with what it is about (the request id of an ask).
fn failure_text(failure: &Failure) -> String {
    serde_json::to_string(failure).unwrap_or_else(|_| failure.key.clone())
}

/// The host as the engine holds it: weakly, since the host owns the engine.
struct AgentCommands(Weak<Host>);

impl Commands for AgentCommands {
    fn find(&self, words: &str, scope: Scope) -> Vec<Value> {
        find(words, scope)
    }

    fn run(
        &self,
        conversation: String,
        command: String,
        args: Value,
        scope: Scope,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>> {
        let host = self.0.clone();
        Box::pin(async move {
            let Some(host) = host.upgrade() else { return Err(failure_text(&Failure::key("error.channel.closed"))) };
            run(&host, conversation, &command, args, scope).await.map_err(|failure| failure_text(&failure))
        })
    }
}

impl Host {
    /// The table for the engine's `faber` tool (`Engine::set_commands`).
    pub fn agent_commands(host: &Arc<Host>) -> Arc<dyn Commands> {
        Arc::new(AgentCommands(Arc::downgrade(host)))
    }
}
