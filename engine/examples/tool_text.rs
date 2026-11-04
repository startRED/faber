//! Prints every tool Faber puts in front of a CLI, as JSON: `{"mcp": [...], "codex": [...]}`, each entry with its `name`,
//! `description` and `inputSchema`, the way the drivers send them with every tool on. `scripts/tool-tokens.mjs` counts them.
//!
//!     node scripts/cargo.mjs run -q -p faber-engine --example tool_text

use faber_engine::helper::{Mcp, Tools, mcp};
use serde_json::{Value, json};

fn main() {
    let all = Tools { helper: true, browse: true, conversation: true, verify: true, map: true, progress: true };
    let Mcp::Answer(list) = mcp(&json!({"method":"tools/list","jsonrpc":"2.0","id":1}), all) else { panic!("the list is an answer") };
    let mut codex: Vec<Value> = vec![
        faber_engine::helper::codex_tool(),
        faber_engine::browser::codex_tool(),
        faber_engine::sibling::codex_tool(),
        faber_engine::verify::codex_tool(),
        faber_engine::map::codex_tool(),
    ];
    codex.extend(faber_engine::progress::codex_tools());
    codex.extend(faber_engine::toolbox::codex_tools());
    println!("{}", json!({ "mcp": list["result"]["tools"], "codex": codex }));
}
