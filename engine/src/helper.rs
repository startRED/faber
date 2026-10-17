//! Faber's "call a helper" tool (R161): a project's agent hands one piece of work or a review to the other vendor, which
//! runs it in a short-lived session of its own in the same folder and with the same permission mode. Both CLIs call the tool
//! in their own way (Claude Code through an MCP server the host answers over stdio, Codex as a dynamic tool), and both get
//! the helper's final report back as the tool's result.

use crate::model::Vendor;
use crate::route::Profile;
use serde_json::{Value, json};

/// The name both drivers give the call, whatever the CLI calls it, so the screen and the engine know one name.
pub const TOOL: &str = "Helper";
/// The tool as Codex knows it, and as Faber's MCP server lists it to Claude Code.
pub const NAME: &str = "helper";
/// The tool as Claude Code names it: `mcp__<server>__<tool>`.
pub const CLAUDE_NAME: &str = "mcp__faber__helper";
/// Faber's own MCP server for Claude Code: an `sdk` server lives in the host, which answers it over stdio.
pub const CLAUDE_MCP_CONFIG: &str = r#"{"mcpServers":{"faber":{"type":"sdk","name":"faber"}}}"#;
/// Helpers one conversation may run at once; each is a CLI process.
pub const MAX_PER_CONVERSATION: usize = 2;

/// Whether a session offers the tool, or is a helper itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Helper {
    #[default]
    Off,
    /// A project's own agent: it may call a helper.
    Offers,
    /// A helper: it leaves no history in the CLI and cannot call a helper of its own.
    Is,
}

const DESCRIPTION: &str = "Hands one self-contained piece of work, a review of your work, or a second opinion to a helper agent \
from another AI vendor. It works in the same folder with the same permissions, but it cannot see this conversation and cannot ask \
anyone questions, so the brief must say everything it needs: the goal, the files involved, and what done looks like. It returns \
the helper's final report. Use it when an independent look is worth a few minutes, never for trivial steps.";

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "A few words naming the piece of work, shown to the person." },
            "brief": { "type": "string", "description": "Everything the helper needs to do the work on its own." }
        },
        "required": ["title", "brief"],
        "additionalProperties": false
    })
}

/// The tool as a Codex `dynamicTools` entry.
pub fn codex_tool() -> Value {
    json!({ "type": "function", "name": NAME, "description": DESCRIPTION, "inputSchema": schema() })
}

/// What Faber's MCP server answers to one message Claude Code sends it.
#[derive(Debug, PartialEq)]
pub enum Mcp {
    /// The answer goes back at once (`mcp_response`).
    Answer(Value),
    /// A call of one of its tools (`NAME`, `crate::browser::NAME`, `crate::sibling::NAME` or `crate::verify::NAME`): the answer waits for the work. `id` is the tool
    /// call's own id, which names the call on screen.
    Call { tool: &'static str, id: String, rpc: Value, input: Value },
}

/// The tools Faber's MCP server holds for one session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tools {
    pub helper: bool,
    pub browse: bool,
    /// Conversations the agent starts by itself (`crate::sibling`).
    pub conversation: bool,
    /// The project's checks (`crate::verify`).
    pub verify: bool,
}

impl Tools {
    pub fn any(self) -> bool {
        self.helper || self.browse || self.conversation || self.verify
    }
}

/// Faber's MCP server, which holds the tools the session offers. Claude Code sends its JSON-RPC messages inside
/// `mcp_message` control requests.
pub fn mcp(message: &Value, tools: Tools) -> Mcp {
    let rpc = message["id"].clone();
    let answer = |result: Value| Mcp::Answer(json!({ "jsonrpc": "2.0", "id": rpc, "result": result }));
    match message["method"].as_str().unwrap_or_default() {
        "initialize" => answer(json!({
            "protocolVersion": message["params"]["protocolVersion"],
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "faber", "version": env!("CARGO_PKG_VERSION") }
        })),
        "tools/list" => {
            let mut list = vec![];
            if tools.helper {
                list.push(json!({ "name": NAME, "description": DESCRIPTION, "inputSchema": schema() }));
            }
            if tools.browse {
                list.push(crate::browser::mcp_tool());
            }
            if tools.conversation {
                list.push(crate::sibling::mcp_tool());
            }
            if tools.verify {
                list.push(crate::verify::mcp_tool());
            }
            answer(json!({ "tools": list }))
        }
        "tools/call" if let Some(tool) = called(&message["params"]["name"], tools) => {
            let meta = &message["params"]["_meta"]["claudecode/toolUseId"];
            // The tool call's id came in `_meta` on every CLI recorded (2.1.296); without it a fresh id tells calls apart, since the
            // JSON-RPC ids start again in every process and a resumed conversation would reuse them.
            let id = meta.as_str().map(String::from).unwrap_or_else(|| format!("{tool}-{}", uuid::Uuid::new_v4().simple()));
            Mcp::Call { tool, id, rpc, input: message["params"]["arguments"].clone() }
        }
        // A notification needs no answer, but the control request that carried it does.
        _ if rpc.is_null() => answer(json!({})),
        _ => Mcp::Answer(json!({ "jsonrpc": "2.0", "id": rpc, "error": { "code": -32601, "message": "Faber has no such method" } })),
    }
}

/// The offered tool a call names, if any.
fn called(name: &Value, tools: Tools) -> Option<&'static str> {
    match name.as_str()? {
        NAME if tools.helper => Some(NAME),
        crate::browser::NAME if tools.browse => Some(crate::browser::NAME),
        crate::sibling::NAME if tools.conversation => Some(crate::sibling::NAME),
        crate::verify::NAME if tools.verify => Some(crate::verify::NAME),
        _ => None,
    }
}

/// The answer to a tool call, once the helper is done.
pub fn mcp_result(rpc: &Value, ok: bool, text: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": rpc, "result": { "content": [{ "type": "text", "text": text }], "isError": !ok } })
}

/// The other vendor: a helper always comes from the one that is not running the conversation.
pub fn other(vendor: Vendor) -> Vendor {
    match vendor {
        Vendor::Claude => Vendor::Codex,
        Vendor::Codex => Vendor::Claude,
    }
}

/// The model a helper runs: the cheapest of the vendor's profiles that clears a bar for multi-step agent work (Terminal-Bench
/// 44 and up), so a helper is good at the work and light on the quota.
pub fn profile(candidates: &[Profile], vendor: Vendor) -> Option<Profile> {
    const AGENT_BAR: u8 = 44;
    candidates
        .iter()
        .filter(|p| p.vendor == vendor && p.agent >= AGENT_BAR)
        .min_by(|a, b| a.cost.total_cmp(&b.cost))
        .or_else(|| candidates.iter().filter(|p| p.vendor == vendor).max_by_key(|p| p.agent))
        .copied()
}

/// What the helper's session is asked: the agent's own title and brief, framed so it knows nobody can answer it.
pub fn prompt(input: &Value) -> Option<String> {
    let brief = input["brief"].as_str().map(str::trim).filter(|b| !b.is_empty())?;
    let title = input["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("A piece of work");
    Some(format!(
        "You are a helper. Another AI agent working in this folder handed you the piece of work below. You cannot see its \
         conversation, and nobody can answer questions, so decide for yourself and do what you can. Finish with a short report \
         for that agent: what you found or changed (files, checks you ran) and anything you could not do.\n\nTask: {title}\n\n{brief}"
    ))
}

/// A message the person wrote to a helper at work (R161 step 5), as the helper reads it: it still reports to the agent.
pub fn from_person(text: &str) -> String {
    format!(
        "The person following your work writes to you:

{}

Take it into account, and still finish with your short report for the agent.",
        text.trim()
    )
}

/// What the agent reads under the report when the person wrote to the helper: words the agent itself never saw.
pub fn told_note(told: &[String]) -> String {
    let quoted: Vec<String> = told.iter().map(|t| format!("\"{}\"", t.trim())).collect();
    format!(
        "

While it worked, the person wrote to the helper: {}.",
        quoted.join("; ")
    )
}

/// What goes back to the agent when the helper ends: its report, plus what it was not allowed to do, which the agent can do
/// itself, where the person approves it.
pub fn report(ok: bool, text: &str, refused: &[String], error: Option<&str>) -> (bool, String) {
    let mut out = text.trim().to_string();
    if !ok || out.is_empty() {
        let why = error.filter(|e| !e.is_empty()).unwrap_or("it ended without a report");
        out = if out.is_empty() {
            format!("The helper stopped: {why}.")
        } else {
            format!("{out}\n\nThe helper stopped before it finished: {why}.")
        };
    }
    if !refused.is_empty() {
        out.push_str(&format!(
            "\n\nThe helper was not allowed to use these without the person's approval, so it did not: {}.",
            refused.join(", ")
        ));
    }
    (ok && !text.trim().is_empty(), out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mcp_server_lists_one_tool_and_hands_its_call_on_with_the_tool_call_id() {
        let mcp = |m: &Value| mcp(m, Tools { helper: true, browse: false, conversation: false, verify: false });
        let init = json!({"method":"initialize","params":{"protocolVersion":"2025-11-25"},"jsonrpc":"2.0","id":0});
        let Mcp::Answer(a) = mcp(&init) else { panic!() };
        assert_eq!((a["id"].clone(), a["result"]["protocolVersion"].clone()), (json!(0), json!("2025-11-25")));
        let Mcp::Answer(list) = mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1})) else { panic!() };
        assert_eq!(list["result"]["tools"][0]["name"], NAME);
        assert_eq!(
            mcp(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})),
            Mcp::Answer(json!({"jsonrpc":"2.0","id":null,"result":{}}))
        );
        let call = json!({"method":"tools/call","params":{"name":"helper","arguments":{"title":"Check","brief":"Look."},
            "_meta":{"claudecode/toolUseId":"toolu_1","progressToken":2}},"jsonrpc":"2.0","id":2});
        assert_eq!(
            mcp(&call),
            Mcp::Call { tool: NAME, id: "toolu_1".into(), rpc: json!(2), input: json!({"title":"Check","brief":"Look."}) }
        );
        let Mcp::Answer(unknown) = mcp(&json!({"method":"tools/call","params":{"name":"other"},"jsonrpc":"2.0","id":3})) else { panic!() };
        assert_eq!(unknown["error"]["code"], -32601);
    }

    // R161 step 7: the browser sits beside the helper, and a session lists and answers only the tools it offers.
    #[test]
    fn the_mcp_server_holds_only_the_tools_the_session_offers() {
        let list = |tools| match mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1}), tools) {
            Mcp::Answer(a) => {
                a["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>()
            }
            Mcp::Call { .. } => panic!(),
        };
        assert_eq!(list(Tools { helper: true, browse: true, conversation: false, verify: false }), ["helper", "browse"]);
        assert_eq!(list(Tools { helper: false, browse: true, conversation: false, verify: false }), ["browse"]);
        let call = json!({"method":"tools/call","params":{"name":"browse","arguments":{"url":"https://a.org"},
            "_meta":{"claudecode/toolUseId":"toolu_2"}},"jsonrpc":"2.0","id":4});
        assert_eq!(
            mcp(&call, Tools { helper: false, browse: true, conversation: false, verify: false }),
            Mcp::Call { tool: crate::browser::NAME, id: "toolu_2".into(), rpc: json!(4), input: json!({"url":"https://a.org"}) }
        );
        let Mcp::Answer(refused) = mcp(&call, Tools { helper: true, browse: false, conversation: false, verify: false }) else {
            panic!("a tool not offered is never called")
        };
        assert_eq!(refused["error"]["code"], -32601);
    }

    #[test]
    fn a_helper_runs_the_cheapest_model_of_the_other_vendor_that_is_good_at_agent_work() {
        let all = crate::route::PROFILES;
        assert_eq!(profile(&all, Vendor::Claude).map(|p| (p.model, p.effort)), Some(("claude-sonnet-5-5", "high")));
        assert_eq!(profile(&all, Vendor::Codex).map(|p| (p.model, p.effort)), Some(("gpt-6.1-sol", "medium")));
        let only_luna: Vec<_> = all.into_iter().filter(|p| p.model == "gpt-6-luna").collect();
        assert_eq!(profile(&only_luna, Vendor::Codex).map(|p| p.model), Some("gpt-6-luna"));
        assert_eq!(profile(&only_luna, Vendor::Claude), None);
    }

    #[test]
    fn the_report_says_what_the_helper_could_not_do_and_an_empty_brief_is_no_task() {
        assert_eq!(prompt(&json!({"title":"x","brief":"  "})), None);
        assert!(prompt(&json!({"title":"Check","brief":"Look at a.rs."})).unwrap().ends_with("Task: Check\n\nLook at a.rs."));
        assert_eq!(report(true, " Fine. ", &[], None), (true, "Fine.".into()));
        let (ok, text) = report(true, "Read it.", &["Bash".into()], None);
        assert!(ok && text.starts_with("Read it.") && text.contains("approval, so it did not: Bash."), "{text}");
        assert_eq!(report(false, "", &[], Some("error.quotaOut")), (false, "The helper stopped: error.quotaOut.".into()));
        assert_eq!(report(true, "", &[], None), (false, "The helper stopped: it ended without a report.".into()));
    }
}
