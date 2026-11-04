//! Cross-vendor helper rounds use only recorded CLI traffic in the sandbox.

use super::{Result, Scenario, ask, ensure};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Duration;

const TOLD: &str = "Also say whether retries above 5 would be risky.";
pub const CLAUDE: &str = "helper_both: Claude asks Codex and receives its report";
pub const CODEX: &str = "helper_both: Codex asks Claude and receives its report";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: CLAUDE, run: claude }, Scenario { name: CODEX, run: codex }]
}

fn claude(ctx: &mut Ctx) -> Result {
    round(ctx, "claude")
}
fn codex(ctx: &mut Ctx) -> Result {
    round(ctx, "codex")
}

fn round(ctx: &mut Ctx, leader: &str) -> Result {
    ctx.write_settings(&json!({"language":"en", "setup":"done", "tutorialDone":true,
        "permissions":"dontAsk", "appearance":{"base":"sala", "motion":"none"}}))?;
    super::shell::seed_project(ctx, "Helper round")?;
    for vendor in ["claude", "codex"] {
        std::fs::write(ctx.sb.state.join(format!("{vendor}.signed-in")), "true").map_err(|e| e.to_string())?;
    }
    let source = std::fs::read_to_string(ctx.paths.fixtures.join(format!("{leader}/helper-call.jsonl"))).map_err(|e| e.to_string())?;
    let mut lines = Vec::new();
    let mut marked = false;
    for line in source.lines() {
        let mut entry: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let input = entry["dir"] == "in";
        let msg = &mut entry["msg"];
        if input
            && (msg["result"]["contentItems"].is_array() || msg["response"]["response"]["mcp_response"]["result"]["content"].is_array())
        {
            msg["_faberReportContains"] = json!("retries = 3");
            marked = true;
        }
        lines.push(entry.to_string());
    }
    ensure(marked, || "the leader recording has no helper result to check".into())?;
    let recording = ctx.sb.sandbox.join("leader.jsonl");
    std::fs::write(&recording, lines.join("\n")).map_err(|e| e.to_string())?;
    let helper = if leader == "claude" { "codex" } else { "claude" };
    let setup = |vendor: &str, chat: &std::path::Path| {
        json!({"vendor":vendor,
        "version":if vendor == "claude" {"2.1.296"} else {"0.162.1"},
        "chat":chat, "models":ctx.paths.fixtures.join("codex/models.jsonl")})
    };
    let leader_setup = ctx.sb.sandbox.join("leader.setup.json");
    let helper_setup = ctx.sb.sandbox.join("helper.setup.json");
    std::fs::write(&leader_setup, setup(leader, &recording).to_string()).map_err(|e| e.to_string())?;
    std::fs::write(&helper_setup, setup(helper, &ctx.paths.fixtures.join(format!("{helper}/helper-told.jsonl"))).to_string())
        .map_err(|e| e.to_string())?;
    let (claude_recording, codex_recording) = if leader == "claude" { (leader_setup, helper_setup) } else { (helper_setup, leader_setup) };
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Fake,
        claude_recording: Some(claude_recording),
        codex_recording: Some(codex_recording),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Helper round"))?;
    page.wait_for("both synthetic subscriptions and models", "window.__TAURI_INTERNALS__.invoke('state').then(s=>s.models.some(m=>m.id==='gpt-6.1-sol') && ['claude','codex'].every(v=>s.usage?.some(u=>u.vendor===v && !u.billingUnknown && !u.billedPerUse)))")?;
    let (model, prompt, reply, minimum) = if leader == "claude" {
        (
            "Claude Haiku",
            "Call the helper tool once with title \"Check\" and brief \"What is the answer?\". Then tell me in one line what it said.",
            "nothing risky",
            0.009,
        )
    } else {
        (
            "GPT-6.1 Sol",
            "Ask the helper tool to check config.toml for risky values (title \"Check config\"). Then tell me in one sentence what it found.",
            "não encontrou valores arriscados",
            0.006,
        )
    };
    super::select_model(page, model)?;
    ask(page, prompt)?;
    let row = css("article .agents .agent").has_text("Check config");
    page.wait_visible(&row)?;
    let box_ = css("#running .side-write textarea");
    // The thread is still moving while the reply streams, so a press can land beside the row (seen once in six runs on a
    // loaded PC, with the row on screen and nothing opened): press again until the helper's view is open. Opening twice is the same as once.
    for attempt in 0..3 {
        page.click(&row)?;
        let open = page.wait_for_within(
            "the helper's view to open",
            "!!document.querySelector('#running .side-write textarea')",
            Duration::from_secs(4),
        );
        if open.is_ok() || attempt == 2 {
            break;
        }
    }
    page.wait_visible(&box_)?;
    let (running_mb, webview) = ctx.memory_mb(app.id)?;
    ensure(webview && running_mb <= 400.0, || format!("helper round uses {running_mb:.1} MB while running"))?;
    page.fill(&box_, TOLD)?;
    page.click(&css("#running .side-write button[type=submit]"))?;
    page.wait_visible(&css("#running .side-told .you").has_text(TOLD))?;
    page.wait_visible(&css("#running .side-body .tools"))?;
    page.wait_visible(&css("article .agents .agent[data-state=done]").has_text("Check config"))?;
    page.wait_visible(&css("#running .agent-report").has_text("retries"))?;
    page.wait_visible(&css("article.ai .md").has_text(reply))?;
    page.wait_visible(&css("article.ai .actions .cost"))?;
    let cost = page
        .eval("Number(document.querySelector('article.ai .actions .cost').textContent.replace(/[^0-9.]/g,''))")?
        .as_f64()
        .unwrap_or(0.0);
    ensure(cost > minimum, || format!("reply cost {cost} omits the helper (expected above {minimum})"))?;
    let records = ctx.records()?;
    for vendor in [leader, helper] {
        let argument = if vendor == "claude" { "-p" } else { "app-server" };
        ensure(records.iter().any(|r| r.vendor == vendor && r.args.iter().any(|a| a == argument)), || {
            format!("{vendor} session never started")
        })?;
    }
    let (done_mb, webview) = ctx.memory_mb(app.id)?;
    ensure(webview && done_mb <= 400.0, || format!("helper round uses {done_mb:.1} MB after the reply"))?;
    ctx.notes.push(format!("{leader} helper round: {running_mb:.1} MB running, {done_mb:.1} MB after the reply"));
    ctx.close(app.id)
}
