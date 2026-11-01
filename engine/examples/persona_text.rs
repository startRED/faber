//! Prints the words each CLI gets from Faber for a new Chat conversation, as JSON, the way the drivers build them:
//! `{"version", "claude", "codex", "languages": {"<code>": {"claude", "codex"}}}`, the top level for a screen that follows
//! the system and one entry per language chosen for the screen. `scripts/persona-eval.mjs` reads it, so the evaluation
//! sends what Faber sends.
//!
//!     node scripts/cargo.mjs run -q -p faber-engine --example persona_text

use faber_engine::claude::{ClaudeLaunch, claude_args};
use faber_engine::codex::{CodexThread, thread_params};
use faber_engine::model::{Area, Permissions, Tuning};
use serde_json::{Value, json};

fn words(language: Option<&str>) -> Value {
    let tuning = Tuning { language: language.map(String::from), ..Default::default() };
    let launch = ClaudeLaunch {
        area: Area::Chat,
        model: None,
        effort: None,
        resume: None,
        user_hooks: false,
        permissions: Permissions::Ask,
        instructions: None,
        user_name: None,
        research: None,
        group_instructions: None,
        flags: Default::default(),
        tuning: tuning.clone(),
        helper: Default::default(),
        browse: false,
        conversations: false,
        leads: false,
        verify: false,
        progress: false,
        tools: Default::default(),
    };
    let args = claude_args(&launch);
    let claude = args[args.iter().position(|a| a == "--append-system-prompt").expect("the persona is appended") + 1].clone();
    let thread = CodexThread {
        cwd: ".".into(),
        model: None,
        effort: None,
        resume: None,
        instructions: None,
        user_name: None,
        research: None,
        group_instructions: None,
        permissions: Permissions::Ask,
        leave_plan: false,
        tuning,
        helper: Default::default(),
        browse: false,
        conversations: false,
        leads: false,
        verify: false,
        progress: false,
    };
    let (_, params) = thread_params(Area::Chat, &thread);
    let codex = params["developerInstructions"].as_str().expect("the persona is the developer instructions").to_string();
    json!({ "claude": claude, "codex": codex })
}

fn main() {
    let mut out = words(None);
    out["version"] = json!(faber_engine::persona::VERSION);
    let languages: serde_json::Map<String, Value> =
        ["en", "pt-BR", "es", "fr", "de"].into_iter().map(|code| (code.to_string(), words(Some(code)))).collect();
    out["languages"] = Value::Object(languages);
    println!("{out}");
}
