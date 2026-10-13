//! The Settings scenarios: each section says its part, a look is a draft until "Use this look", and the account cards come
//! and go without losing it.

use super::shell::chat_with_future_resets;
use super::{Result, Scenario, ask, claude_chat, ensure, eventually, pause, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css, role_any};
use crate::sandbox::Launch;
use serde_json::json;
use std::time::Duration;

const QUESTION: &str = "In one sentence: why is the sky blue?";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "every section of Settings says its part: the accounts, the usage limits and what stays on this PC",
            run: walks_the_sections,
        },
        Scenario {
            name: "an installed CLI that is signed out is not a connected account, and the section asks again each time it opens",
            run: installed_is_not_connected,
        },
        Scenario { name: "Settings closed with focus inside it is not kept alive by the page", run: closed_settings_is_freed },
        Scenario {
            name: "the quota in the sidebar opens Settings on the usage limits, and a reload shows the same figures",
            run: opens_at_usage,
        },
        Scenario { name: "a click waits for the target's rendered layout before sending one pointer click", run: clicks_rendered_target },
        Scenario {
            name: "a look is a draft: the page wears it only after Use this look, and Back to default undoes it",
            run: look_is_a_draft,
        },
        Scenario {
            name: "an accent too close to a meaning colour is refused in words, and the draft goes back",
            run: refuses_a_close_accent,
        },
        Scenario {
            name: "the account cards open from Settings, and Close brings Settings back with the look not yet used",
            run: keeps_the_draft_across_accounts,
        },
    ]
}

/// The chat of the fixtures with resets in the future, run by a Claude CLI that says it is signed in or, with `signed_in`
/// false, one that is installed and says nobody is.
fn chat_with_cli(ctx: &Ctx, signed_in: bool) -> std::result::Result<Launch, String> {
    let mut launch = chat_with_future_resets(ctx)?;
    let setup = ctx.sb.data.join("claude.setup.json");
    std::fs::write(&setup, json!({ "vendor": "claude", "version": "2.1.296", "chat": "chat-future.jsonl" }).to_string())
        .map_err(|e| e.to_string())?;
    launch.recording = Some(setup);
    if signed_in {
        std::fs::create_dir_all(&ctx.sb.state).map_err(|e| e.to_string())?;
        std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    }
    Ok(launch)
}

fn open_settings(page: &mut Page) -> Result {
    let settings = page.named("button", "settings.open")?;
    page.click(&settings)?;
    let heading = page.named("heading", "set.general")?;
    page.wait_visible(&heading)
}

/// Opens a section from the list on the left and waits for its heading.
fn open_section(page: &mut Page, key: &str) -> Result {
    let item = page.named("button", key)?;
    page.click(&item)?;
    let heading = page.named("heading", key)?;
    page.wait_visible(&heading)
}

/// What the line of Claude in Connected accounts says.
const CLAUDE_STATE: &str =
    "[...document.querySelectorAll('.accounts li')].find((li) => li.textContent.includes('Claude'))?.querySelector('.state').textContent";
const BASE: &str = "document.documentElement.dataset.base";
/// The background the preview panel draws with.
const PREVIEW_BG: &str = "getComputedStyle(document.querySelector('.pvbox .mini')).getPropertyValue('--bg').trim()";

fn walks_the_sections(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_cli(ctx, true)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.wait_for("the quota to show", "document.querySelector('.usage .share')?.textContent === '47%'")?;
    open_settings(page)?;

    // General: the language, hooks and both notification switches.
    let language = page.labelled("set.language")?;
    page.wait_visible(&language)?;
    same(page.count(&role_any("switch"))?, 3, "switches on General")?;
    for key in ["set.notifyMe", "set.playSound"] {
        let toggle = page.named("switch", key)?;
        same(page.is_checked(&toggle)?, true, "notifications default on")?;
    }
    ctx.shot(&mut app, "settings-general")?;

    // Appearance: the six looks are drawn before the picture is taken.
    let page = &mut app.page;
    open_section(page, "set.appearance")?;
    page.wait_for("the six looks to be drawn", "document.querySelectorAll('.preset .thumb .mini[style]').length === 6")?;
    page.wait_for("the preview to wear the look", &format!("{PREVIEW_BG} === '#0e0e10'"))?;
    pause(0.8);
    ctx.shot(&mut app, "settings-appearance")?;
    // The section scrolls in a 800 px window; a taller one shows all of it in one picture.
    let tall = json!({ "width": 1280, "height": 1160, "deviceScaleFactor": 1, "mobile": false });
    app.page.cdp.call("Emulation.setDeviceMetricsOverride", tall)?;
    pause(0.5);
    ctx.shot(&mut app, "settings-appearance-tall")?;
    // A narrow window stacks the preview under the controls: nothing sticks out sideways, and the bar of actions is there.
    let narrow = json!({ "width": 900, "height": 700, "deviceScaleFactor": 1, "mobile": false });
    app.page.cdp.call("Emulation.setDeviceMetricsOverride", narrow)?;
    pause(0.5);
    let fits = app.page.eval(
        "(() => { const m = document.querySelector('.s-main'); const p = document.querySelector('.s-prev').getBoundingClientRect(); const c = document.querySelector('.ap-main').getBoundingClientRect(); return document.documentElement.scrollWidth <= innerWidth && m.scrollWidth <= m.clientWidth && p.top >= c.bottom - 1; })()",
    )?;
    same(fits.as_bool(), Some(true), "Appearance at 900 px wide: no sideways scroll, and the preview under the controls")?;
    ctx.shot(&mut app, "settings-appearance-narrow")?;
    app.page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;

    // Connected accounts: Claude is connected, Codex is not.
    let page = &mut app.page;
    open_section(page, "set.subscriptions")?;
    let (on, off) = (page.text_of("set.connected")?, page.text_of("set.notConnected")?);
    same(page.count(&css(".accounts li"))?, 2, "the accounts listed")?;
    page.wait_for("Claude to be connected", &format!("({CLAUDE_STATE}) === {}", json!(on)))?;
    same(page.inner_text(&css(".accounts li").has_text("Codex").css(".state"))?, off, "Codex is not connected")?;
    ctx.shot(&mut app, "settings-accounts")?;

    // Usage limits: the 5-hour window then the 7-day one, with the shares the sidebar shows.
    let page = &mut app.page;
    open_section(page, "set.usage")?;
    let shares = page.eval("[...document.querySelectorAll('.usage-v .urow .share')].map((e) => e.textContent)")?;
    same(shares.to_string(), r#"["7%","47%"]"#.to_string(), "the shares of the two windows")?;
    let names = page.eval("[...document.querySelectorAll('.usage-v .urow .uname')].map((e) => e.textContent)")?;
    same(names.to_string(), r#"["5-hour window","7-day window"]"#.to_string(), "the windows")?;
    ctx.shot(&mut app, "settings-usage")?;

    // Privacy: three plain statements.
    let page = &mut app.page;
    open_section(page, "set.privacy")?;
    for id in ["telemetry", "history", "logins"] {
        let said = page.text_of(&format!("set.privacy.{id}"))?;
        page.wait_visible(&css(".facts li").has_text(&said))?;
    }
    same(page.count(&css(".facts li"))?, 3, "the statements")?;
    ctx.shot(&mut app, "settings-privacy")
}

fn opens_at_usage(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_future_resets(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.wait_for("the quota to show", "document.querySelector('.usage .share')?.textContent === '47%'")?;
    let vendor = css(".usage .vendor").has_text("Claude");
    page.click(&vendor)?;
    let heading = page.named("heading", "set.usage")?;
    page.wait_visible(&heading)?;
    let shares = "[...document.querySelectorAll('.usage-v .urow .share')].map((e) => e.textContent).join(' ')";
    same(page.eval(shares)?.as_str(), Some("7% 47%"), "the figures at the usage limits")?;
    // A screen that loads again asks the engine and shows the same readings.
    page.reload()?;
    page.wait_for("the quota after a reload", "document.querySelector('.usage .share')?.textContent === '47%'")?;
    open_settings(page)?;
    open_section(page, "set.usage")?;
    page.wait_for("the same figures", &format!("({shares}) === '7% 47%'"))
}

fn clicks_rendered_target(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    page.eval("(() => { const b = document.createElement('button'); b.id = 'layout-target'; b.style.cssText = 'position:fixed;left:300px;top:300px;width:80px;height:40px;z-index:9999'; b.onclick = () => window.__targetClicks = (window.__targetClicks ?? 0) + 1; b.scrollIntoView = () => requestAnimationFrame(() => { b.style.left = '600px'; }); document.body.append(b); window.__targetClicks = 0; })()")?;
    page.click(&css("#layout-target"))?;
    same(page.eval("window.__targetClicks")?.as_u64(), Some(1), "exactly one click reaches the target after its layout changes")
}

fn look_is_a_draft(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    open_settings(page)?;
    open_section(page, "set.appearance")?;
    page.wait_for("the preview to wear Dark Room", &format!("{PREVIEW_BG} === '#0e0e10'"))?;
    let apply = page.named("button", "set.use")?;
    same(page.is_disabled(&apply)?, true, "Use this look with nothing to use")?;
    let (pending, applied) = (page.text_of("set.previewPending")?, page.text_of("set.previewApplied")?);
    same(page.inner_text(&css(".s-prev .ph small"))?, applied.clone(), "the preview of the look on the page")?;

    // Morning: the preview wears it, the page does not.
    let morning = page.named("button", "look.manha.name")?;
    page.click(&morning)?;
    page.wait_for("the preview to wear Morning", &format!("{PREVIEW_BG} === '#f3f5f0'"))?;
    same(page.eval(BASE)?.as_str(), Some("sala"), "the page while the look is a draft")?;
    same(
        page.eval("getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()")?.as_str(),
        Some("#0e0e10"),
        "the page's background",
    )?;
    same(page.inner_text(&css(".s-prev .ph small"))?, pending, "the preview says it is not applied")?;
    same(page.is_disabled(&apply)?, false, "Use this look with a draft")?;

    // Back to default undoes the draft.
    let reset = page.named("button", "set.reset")?;
    page.click(&reset)?;
    page.wait_for("the preview to wear Dark Room", &format!("{PREVIEW_BG} === '#0e0e10'"))?;
    let sala = page.named("button", "look.sala.name")?;
    same(page.attr(&sala, "aria-pressed")?, Some("true".to_string()), "Dark Room is chosen again")?;
    same(page.is_disabled(&apply)?, true, "Use this look after Back to default")?;

    // Morning again, this time used.
    page.click(&morning)?;
    page.wait_for("the preview to wear Morning", &format!("{PREVIEW_BG} === '#f3f5f0'"))?;
    page.click(&apply)?;
    page.wait_for("the page to wear Morning", &format!("{BASE} === 'manha'"))?;
    page.wait_for(
        "the preview to say it is applied",
        &format!("document.querySelector('.s-prev .ph small').textContent === {}", serde_json::json!(applied)),
    )?;
    same(page.is_disabled(&apply)?, true, "Use this look once it is used")?;
    let file = ctx.sb.data.join("settings.json");
    eventually("the look to reach settings.json", Duration::from_secs(15), || {
        std::fs::read_to_string(&file).is_ok_and(|text| text.contains("\"base\": \"manha\""))
    })
}

fn refuses_a_close_accent(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    open_settings(page)?;
    open_section(page, "set.appearance")?;
    page.wait_for("the preview to wear Dark Room", &format!("{PREVIEW_BG} === '#0e0e10'"))?;
    let hex = page.labelled("set.hexLabel")?;
    let value = "document.querySelector('.hex').value";
    let before = page.eval(value)?;

    // Amber means "needs you": an accent next to it is refused, and the draft is what it was.
    page.fill(&hex, "#f0a030")?;
    page.press_on(&hex, "Enter")?;
    let sentence = page.text_of("error.look.nearAmber")?;
    page.wait_visible(&css(".ap-msg").has_text(&sentence))?;
    page.wait_for("the field to go back", &format!("{value} === {before}"))?;
    let apply = page.named("button", "set.use")?;
    same(page.is_disabled(&apply)?, true, "Use this look after a refused accent")?;
    same(page.eval(BASE)?.as_str(), Some("sala"), "the page after a refused accent")?;
    pause(0.4);
    ctx.shot(&mut app, "settings-accent-refused")?;

    // Something that is not a colour gets its own sentence.
    let page = &mut app.page;
    page.fill(&hex, "banana")?;
    page.press_on(&hex, "Enter")?;
    let invalid = page.text_of("set.hexInvalid")?;
    page.wait_visible(&css(".ap-msg").has_text(&invalid))?;
    page.wait_for("the field to go back", &format!("{value} === {before}"))?;

    // A colour in the short form is read, drawn, and can be used.
    page.fill(&hex, "36C")?;
    page.press_on(&hex, "Enter")?;
    page.wait_for("the field to show the colour", &format!("{value} === '#3366cc'"))?;
    page.wait_for("Use this look to be allowed", "!document.querySelector('.ap-bar .btn.primary').disabled")?;
    ensure(page.inner_text(&css(".ap-msg"))?.is_empty(), || "the sentence of the refusal should be gone".to_string())
}

fn keeps_the_draft_across_accounts(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    open_settings(page)?;
    open_section(page, "set.appearance")?;
    let morning = page.named("button", "look.manha.name")?;
    page.click(&morning)?;
    page.wait_for("the preview to wear Morning", &format!("{PREVIEW_BG} === '#f3f5f0'"))?;
    open_section(page, "set.subscriptions")?;
    let manage = page.named("button", "set.openSetup")?;
    page.click(&manage)?;
    let cards = page.named("heading", "setup.title")?;
    page.wait_visible(&cards)?;
    same(page.count(&css(".settings"))?, 0, "Settings while the cards are open")?;
    // The window's own Close button has the same name, so the one asked for is looked for inside the dialog.
    let close = css(".fr-stage").role("button", &page.text_of("setup.close")?).exact();
    page.click(&close)?;
    // Settings is back on the section it left, with the look still a draft.
    let heading = page.named("heading", "set.subscriptions")?;
    page.wait_visible(&heading)?;
    // Focus goes back to the button that opened the cards, so a keyboard user is where they were.
    page.wait_for("focus on Manage accounts", "document.activeElement?.dataset.restore === 'manage'")?;
    same(page.eval(BASE)?.as_str(), Some("sala"), "the page after the cards")?;
    open_section(page, "set.appearance")?;
    let morning = page.named("button", "look.manha.name")?;
    same(page.attr(&morning, "aria-pressed")?, Some("true".to_string()), "the draft after the cards")?;
    page.wait_for("the preview to wear Morning", &format!("{PREVIEW_BG} === '#f3f5f0'"))?;
    let apply = page.named("button", "set.use")?;
    same(page.is_disabled(&apply)?, false, "Use this look after the cards")?;
    // Esc leaves Settings and drops the draft.
    page.press("Escape")?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    same(page.eval(BASE)?.as_str(), Some("sala"), "the page after leaving Settings")
}

fn installed_is_not_connected(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_cli(ctx, false)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    open_settings(page)?;
    open_section(page, "set.subscriptions")?;
    let (on, off) = (page.text_of("set.connected")?, page.text_of("set.notConnected")?);
    // The CLI is installed, and says nobody is signed in.
    page.wait_for("Claude to be not connected", &format!("({CLAUDE_STATE}) === {}", json!(off)))?;
    // Signing in somewhere else; the section asks again the next time it opens.
    std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    let page = &mut app.page;
    open_section(page, "set.usage")?;
    open_section(page, "set.subscriptions")?;
    page.wait_for("Claude to be connected", &format!("({CLAUDE_STATE}) === {}", json!(on)))?;
    ctx.close(app.id)
}

fn closed_settings_is_freed(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    // The Projects area with no project open has no composer to take the focus when Settings closes, so the page's record
    // of the focus is what could keep the closed Settings alive.
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    let open = page.named("heading", "empty.openProjectTitle")?;
    page.wait_visible(&open)?;
    open_settings(page)?;
    open_section(page, "set.appearance")?;
    page.wait_for("the six looks to be drawn", "document.querySelectorAll('.preset .thumb .mini[style]').length === 6")?;
    // Back takes the focus when it is clicked, and the whole of Settings leaves the page with it.
    let back = page.named("button", "set.back")?;
    page.eval("window.__settings = new WeakRef(document.querySelector('.settings'))")?;
    page.click(&back)?;
    page.wait_visible(&open)?;
    let mut freed = false;
    for _ in 0..8 {
        app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
        pause(0.3);
        if app.page.eval("window.__settings.deref() === undefined")? == true {
            freed = true;
            break;
        }
    }
    ensure(freed, || "the page still holds the Settings it closed".to_string())?;
    ctx.close(app.id)
}
