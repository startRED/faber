//! Conversations a project's agent starts by itself (self-development): the agent hands a separate piece of work to a new
//! conversation in the same project, which runs on its own beside it, shows in the sidebar like any other, and can ask the
//! person. Unlike a helper (`crate::helper`), the call does not wait for the work: it answers at once with the new
//! conversation's id, and the agent reads where that conversation stands with a second call when it needs to.

use serde_json::{Value, json};

/// The name both drivers give the call, whatever the CLI calls it.
pub const TOOL: &str = "Conversation";
/// The tool as Codex knows it, and as Faber's MCP server lists it to Claude Code.
pub const NAME: &str = "conversation";
/// The tool as Claude Code names it: `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__faber__conversation";
/// Conversations agents started that may run at once in one project; each is a CLI process.
pub const MAX_RUNNING: usize = 3;
/// The most of a conversation's latest answer a read returns.
const MAX_READ: usize = 4000;

const DESCRIPTION: &str = "Starts a new conversation in this project for a separate piece of work, so it runs in parallel \
with you. Pass `title` and `brief`: the new conversation's agent cannot see this conversation, so the brief must say \
everything it needs (the goal, the files involved, what done looks like). It answers at once with the new conversation's \
`id`; the person sees it in the sidebar and may talk to it. Later, pass only `id` to read whether it is still working and \
its latest answer. Use it for independent work that takes a while; never for small steps, and never for two pieces that \
change the same files.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "A few words naming the work; it becomes the conversation's title." },
            "brief": { "type": "string", "description": "Everything the new conversation needs to do the work on its own." },
            "id": { "type": "string", "description": "To read a conversation you started: its id, and nothing else." }
        },
        "additionalProperties": false
    })
}

/// The tool as a Codex `dynamicTools` entry.
pub fn codex_tool() -> Value {
    json!({ "type": "function", "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// The tool as Faber's MCP server lists it to Claude Code.
pub fn mcp_tool() -> Value {
    json!({ "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// What one call asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Start { title: String, brief: String },
    Read { id: String },
}

/// Reads a call's input. A call with both an id and a brief is refused, so a typo never starts a second conversation.
pub fn request(input: &Value) -> Result<Request, &'static str> {
    let text = |key: &str| input[key].as_str().map(str::trim).filter(|v| !v.is_empty()).map(String::from);
    match (text("id"), text("brief")) {
        (Some(_), Some(_)) => Err("pass either an id to read or a title and brief to start, not both"),
        (Some(id), None) => Ok(Request::Read { id }),
        (None, Some(brief)) => {
            let title: String = text("title").unwrap_or_else(|| "A piece of work".into()).chars().take(60).collect();
            Ok(Request::Start { title, brief })
        }
        (None, None) => Err("the brief was empty"),
    }
}

/// The first message of the new conversation: the agent's own brief, framed so the new agent knows where it comes from.
pub fn prompt(title: &str, brief: &str) -> String {
    format!(
        "Another AI agent working in this project started this conversation for a separate piece of work. It cannot see what \
         you write here, but the person can, and may answer your questions. Do the work, then end with a short report: what \
         you changed (files, checks you ran) and anything you could not do.\n\nTask: {title}\n\n{brief}"
    )
}

/// The call's answer once the conversation started.
pub fn started(title: &str, id: &str) -> String {
    format!(
        "Started the conversation \"{title}\" (id {id}). It runs on its own in this project and the person sees it in the \
         sidebar. Call this tool with {{\"id\": \"{id}\"}} to read where it stands."
    )
}

/// The call's answer when the conversation was made but its first turn could not start.
pub fn failed(title: &str, id: &str) -> String {
    format!(
        "The conversation \"{title}\" (id {id}) was made but could not start; its reply shows why. Do the work yourself or try again later."
    )
}

/// Where a conversation stands, as a read answers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Working,
    Waiting,
    Finished,
    Failed,
    Stopped,
}

/// The answer to a read: the state, then the conversation's latest answer, cut to a size the agent can take in.
pub fn read(title: &str, state: State, latest: &str) -> String {
    let said = match state {
        State::Working => "is still working",
        State::Waiting => "is waiting for the person to answer it",
        State::Finished => "finished its latest turn",
        State::Failed => "stopped with an error in its latest turn",
        State::Stopped => "was stopped by the person",
    };
    let latest = latest.trim();
    let mut out = format!("The conversation \"{title}\" {said}.");
    if !latest.is_empty() {
        let cut: String = latest.chars().take(MAX_READ).collect();
        let more = if cut.len() < latest.len() { " (cut short)" } else { "" };
        out.push_str(&format!("\n\nIts latest answer{more}:\n{cut}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_starts_with_a_brief_reads_with_an_id_and_never_both() {
        assert_eq!(
            request(&json!({"title":" Fix the build ","brief":"Make cargo build pass."})),
            Ok(Request::Start { title: "Fix the build".into(), brief: "Make cargo build pass.".into() })
        );
        assert_eq!(request(&json!({"id":"abc"})), Ok(Request::Read { id: "abc".into() }));
        assert!(request(&json!({"id":"abc","brief":"x"})).is_err());
        assert_eq!(request(&json!({"title":"x","brief":"  "})), Err("the brief was empty"));
        let Ok(Request::Start { title, .. }) = request(&json!({"brief":"x"})) else { panic!() };
        assert_eq!(title, "A piece of work");
        let Ok(Request::Start { title, .. }) = request(&json!({"title":"a".repeat(80),"brief":"x"})) else { panic!() };
        assert_eq!(title.chars().count(), 60);
    }

    #[test]
    fn the_answers_name_the_conversation_and_a_long_answer_is_cut() {
        assert!(started("Docs", "c1").contains("{\"id\": \"c1\"}"));
        assert!(prompt("Docs", "Write it.").ends_with("Task: Docs\n\nWrite it."));
        assert_eq!(read("Docs", State::Working, ""), "The conversation \"Docs\" is still working.");
        let long = read("Docs", State::Finished, &"x".repeat(5000));
        assert!(long.contains("finished its latest turn") && long.contains("(cut short)"), "{long}");
        assert!(!read("Docs", State::Failed, "short").contains("cut short"));
    }
}
