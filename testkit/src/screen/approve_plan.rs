//! Plan approval through the real screen and both recorded CLI protocols.
use super::{Result, Scenario, ask, ensure, same};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::json;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "approve-plan: Claude approval changes the composer mode and starts the work", run: claude },
        Scenario { name: "approve-plan: Codex approval changes the composer mode and starts the work", run: codex },
        Scenario { name: "approve-plan: Claude keeps Stop visible while the plan closes and cancels the queued work", run: claude_stop },
    ]
}

fn claude(ctx: &mut Ctx) -> Result {
    run(ctx, false, false)
}
fn codex(ctx: &mut Ctx) -> Result {
    run(ctx, true, false)
}

fn claude_stop(ctx: &mut Ctx) -> Result {
    run(ctx, false, true)
}

fn run(ctx: &mut Ctx, codex: bool, stop: bool) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "tutorialDone": true,
        "permissions": "plan", "appearance": { "base": "sala", "motion": "none" } }))?;
    let vendor = if codex { "codex" } else { "claude" };
    let recorded = ctx.sb.sandbox.join("approve-plan.jsonl");
    let mut plan = std::fs::read_to_string(ctx.paths.fixtures.join(vendor).join("approve-plan.jsonl")).map_err(|e| e.to_string())?;
    if stop {
        let result = plan.find("{\"dir\":\"out\",\"msg\":{\"type\":\"result\"").ok_or("the plan recording has no result")?;
        plan.insert_str(result, "{\"dir\":\"out\",\"msg\":{\"faber_hold\":true}}\n");
    }
    std::fs::write(&recorded, plan).map_err(|e| e.to_string())?;
    let recording = if codex {
        // Model enumeration uses a separate app-server. The existing setup dispatcher keeps it out of the turn recording.
        let setup = ctx.sb.sandbox.join("approve-plan.setup.json");
        std::fs::write(
            &setup,
            json!({ "vendor": "codex", "version": "0.162.1", "chat": "approve-plan.jsonl",
            "models": ctx.paths.fixtures.join("codex/models.jsonl") })
            .to_string(),
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(ctx.sb.state.join("codex.signed-in"), "true").map_err(|e| e.to_string())?;
        setup
    } else {
        recorded.clone()
    };
    let mut app = ctx.launch(Launch {
        claude: if codex { CliSlot::Off } else { CliSlot::Fake },
        codex: if codex { CliSlot::Fake } else { CliSlot::Off },
        recording: Some(recording),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    if codex {
        page.wait_for("Codex models", "[...document.querySelectorAll('.composer select option')].some(o => o.text === 'GPT-6.1 Sol')")?;
    }
    page.select_option(&model, if codex { "GPT-6.1 Sol" } else { "Claude Haiku" })?;
    ask(page, "Plan this change.")?;
    let card = css("article [data-testid=plan-approval]");
    page.wait_visible(&card)?;
    let shared_mode = page.text_of("ask.plan.shared")?;
    page.wait_visible(&css("article .plan-approval .note").has_text(&shared_mode))?;
    if codex {
        same(
            page.eval(
                "[...document.querySelectorAll('article .md li')].filter(el => el.textContent.includes('Inspect the parser.')).length",
            )?
            .as_u64(),
            Some(1),
            "the streamed Codex plan is drawn only once",
        )?;
    } else {
        page.wait_visible(&css("article .plan-approval .md li").has_text("Inspect the parser."))?;
        page.wait_visible(&css("article .plan-approval .md li").has_text("Add the check."))?;
    }
    let mode = page.labelled("composer.mode")?;
    same(page.eval("document.querySelector('.composer .mode select').value")?.as_str(), Some("plan"), "still planning")?;
    ensure(
        !page.eval("document.querySelector('article').textContent.includes('Started the approved work.')")?.as_bool().unwrap_or(true),
        || "work started before approval".into(),
    )?;
    same(
        page.eval("document.querySelector('article .plan-approval select').value")?.as_str(),
        Some("ask"),
        "a conversation with unknown prior mode defaults to Ask",
    )?;
    // The replacement process reads this recording; the Plan process has already loaded its own turn.
    std::fs::copy(ctx.paths.fixtures.join(vendor).join("approve-plan-run.jsonl"), &recorded).map_err(|e| e.to_string())?;
    let approve = page.text_of("ask.plan.approve")?;
    page.click(&card.clone().role("button", &approve).exact())?;
    page.wait_for("the composer mode to change", "document.querySelector('.composer .mode select').value === 'ask'")?;
    if stop {
        page.wait_gone(&card)?;
        let cancel = css(".composer button.stop");
        page.wait_visible(&cancel)?;
        page.wait_visible(&css(".run-btn").has_text("1 running"))?;
        page.click(&cancel)?;
        std::fs::write(ctx.sb.state.join("release"), "").map_err(|e| e.to_string())?;
        page.wait_visible(&css("article .cost").has_text("0.03"))?;
        page.wait_gone(&cancel)?;
        let idle = page.text_of("run.none")?;
        page.wait_visible(&css(".run-btn").has_text(&idle))?;
        // A person's prompt is a .mine div; only an agent's reply is an article.
        same(page.count(&css(".mine:not(.queued)"))?, 1, "only the person's original prompt remains")?;
        same(page.count(&css("article.ai"))?, 1, "Stop does not send work after the held plan result arrives")?;
        same(page.count(&css(".mine.queued"))?, 0, "the approved work was removed from the queue")?;
        same(page.count(&css("article.ai").has_text("Started the approved work."))?, 0, "no approved work reached a replacement process")?;
        let approved = page.labelled("wake.approvedPlan")?;
        same(page.count(&approved)?, 0, "the approved work was cancelled before dispatch")?;
        return Ok(());
    }
    page.wait_visible(&css("article").has_text("Started the approved work."))?;
    page.wait_gone(&card)?;
    let approved = page.labelled("wake.approvedPlan")?;
    page.wait_visible(&approved)?;
    ensure(
        !page
            .eval("[...document.querySelectorAll('article')].some(el => el.textContent.includes('Go ahead with the plan.'))")?
            .as_bool()
            .unwrap_or(true),
        || "Faber's approved-plan instruction was drawn as a person's message".into(),
    )?;
    // The composer still offers the same value after a reload of the saved conversation.
    page.cdp.call("Page.reload", json!({}))?;
    page.wait_visible(&mode)?;
    page.wait_for("the saved mode", "document.querySelector('.composer .mode select').value === 'ask'")
}
