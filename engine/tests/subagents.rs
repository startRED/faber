//! A real Claude Code 2.1.296 run with two subagents in parallel (tests/fixtures/claude/subagents.jsonl): every tool a
//! subagent calls names the Agent step it runs in, so the work log can show one row per subagent.

use faber_engine::claude::parse_claude_message;
use faber_engine::events::EngineEvent;
use faber_engine::model::ToolUse;
use serde_json::{Value, json};

#[test]
fn each_subagent_tool_names_the_step_it_runs_in() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/claude/subagents.jsonl")).unwrap();
    let tools: Vec<(String, String, Option<String>)> = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|entry| entry["dir"] == "out")
        .flat_map(|entry| parse_claude_message(&entry["msg"]))
        .filter_map(|e| match e {
            EngineEvent::Tool { id, name, parent, .. } => Some((id, name, parent)),
            _ => None,
        })
        .collect();
    let steps: Vec<&(String, String, Option<String>)> = tools.iter().filter(|t| t.1 == "Agent").collect();
    assert_eq!(steps.len(), 2, "{tools:?}");
    assert!(steps.iter().all(|t| t.2.is_none()), "the reply's own steps have no parent");
    let reads: Vec<&Option<String>> = tools.iter().filter(|t| t.1 == "Read").map(|t| &t.2).collect();
    assert_eq!(reads.len(), 2, "{tools:?}");
    for parent in &reads {
        assert!(steps.iter().any(|s| Some(&s.0) == parent.as_ref()), "{parent:?} is one of the steps");
    }
    assert_ne!(reads[0], reads[1], "each subagent reads in its own step");
}

#[test]
fn a_saved_tool_keeps_its_step_and_older_files_read_as_before() {
    let tool: ToolUse = serde_json::from_value(json!({ "id": "t", "name": "Read", "input": {} })).unwrap();
    assert_eq!(tool.parent, None);
    assert!(serde_json::to_value(&tool).unwrap().get("parent").is_none(), "a reply's own tool is saved as before");
    let child = ToolUse { parent: Some("step".into()), ..tool };
    assert_eq!(serde_json::to_value(&child).unwrap()["parent"], "step");
}
