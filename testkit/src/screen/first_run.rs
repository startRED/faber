//! The first run, past its first step: the stepper, step 2 (the look and the question about AI), the tutorial that
//! follows, and what stays out of the way once it is over. Step 1 itself (the accounts) is in `setup.rs`.
//!
//! Screenshots of the checkpoints go to `target/faber-test/shots/` (`b7-*.png`).

use super::{Result, Scenario, claude_chat, ensure, eventually, no_cli, pass_look, pause, same, skip_tour};
use crate::app::{Ctx, Opened};
use crate::page::{Page, css};
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "first run, step 1: a stepper, a focused heading, and no way out but its buttons", run: step_one },
        Scenario {
            name: "first run, step 2: three looks side by side preview at once, Back keeps the choices and Continue saves them",
            run: step_two,
        },
        Scenario {
            name: "first run, tutorial: a beginner gets three cards, focus stays on the card and finishing is saved",
            run: tutorial_beginner,
        },
        Scenario {
            name: "first run, step 2: a refused save keeps the step open with the error, and starts no tutorial",
            run: refused_save_keeps_step_two,
        },
        Scenario {
            name: "first run: an old appearance with a saved conversation opens at step 2 once, with step 1 done",
            run: old_appearance_opens_step_two_once,
        },
        Scenario { name: "first run, tutorial: someone experienced gets two cards, and Esc skips them", run: tutorial_experienced },
        Scenario {
            name: "first run, keyboard only: step 2, the beginner tutorial and an Esc skip, with focus back in the composer",
            run: keyboard_only,
        },
        Scenario { name: "first run, tutorial: waits for an AI, then comes once and never again", run: tutorial_waits_for_an_ai },
        Scenario { name: "first run, tutorial: the card stays inside the window from 720 to 1440 wide", run: tutorial_fits_the_window },
        Scenario {
            name: "reconnecting an account later is a plain dialog and restarts nothing of the first run",
            run: manage_mode_is_plain,
        },
        Scenario {
            name: "first run, step 2: while the save is on its way Back and the choices are shut, and the tutorial follows it",
            run: save_in_flight_shuts_the_step,
        },
        Scenario {
            name: "first run, step 2: a preview still on its way when the step closed never lands on the next one",
            run: stale_preview_never_lands,
        },
        Scenario {
            name: "first run: Continue lands on a new conversation, not on the one remembered from an older version",
            run: continue_lands_on_a_fresh_chat,
        },
        Scenario {
            name: "first run, tutorial: an installed CLI that is signed out starts none, and signing in does",
            run: signed_out_cli_starts_no_tutorial,
        },
    ]
}

/// Holds the first `count` requests of the engine command `command` for `ms`. The page's `invoke` cannot be replaced, but
/// its requests go through `fetch`, which can.
fn hold(page: &mut Page, command: &str, ms: u32, count: u32) -> Result {
    page.eval(&format!(
        "(() => {{ const real = window.fetch.bind(window); window.__left = {count}; \
         window.fetch = (url, init) => String(url).endsWith('/{command}') && window.__left-- > 0 \
           ? new Promise((r) => setTimeout(r, {ms})).then(() => real(url, init)) : real(url, init); return true; }})()"
    ))
    .map(drop)
}

fn size(page: &mut Page, width: u32, height: u32) -> Result {
    let metrics = json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false });
    page.cdp.call("Emulation.setDeviceMetricsOverride", metrics)?;
    page.wait_for("the window to follow", &format!("window.innerWidth === {width}"))
}

/// A screenshot taken once the screen has stopped moving: cards fade in and the ring travels for about half a second.
fn shot(ctx: &Ctx, app: &mut Opened, name: &str) -> Result {
    pause(0.7);
    ctx.shot(app, name)
}

fn settings(ctx: &Ctx) -> Value {
    std::fs::read_to_string(ctx.sb.data.join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null)
}

/// Waits for `settings.json` to say `what`; saves go through the engine and reach the disk a moment later.
fn saved(ctx: &Ctx, what: &str, check: impl Fn(&Value) -> bool) -> Result {
    eventually(what, Duration::from_secs(15), || check(&settings(ctx))).map_err(|e| format!("{e}; settings.json holds {}", settings(ctx)))
}

/// Focus is on the tutorial card's Next (or Got it) button.
fn focus_on_next(page: &mut Page) -> Result {
    let on = page.eval("document.activeElement === document.querySelector('.coach .btn.primary')")?;
    ensure(on == json!(true), || "focus should start on the card's Next button".to_string())
}

/// Presses Tab until the focused element satisfies `test`, an expression over `a`, the active element.
fn tab_to(page: &mut Page, what: &str, test: &str) -> Result {
    for _ in 0..80 {
        if page.eval(&format!("(() => {{ const a = document.activeElement; return !!a && ({test}); }})()"))? == json!(true) {
            return Ok(());
        }
        page.press("Tab")?;
    }
    Err(format!("Tab never reached {what}"))
}

fn root_var(page: &mut Page, name: &str) -> Result<String> {
    let value = page.eval(&format!("getComputedStyle(document.documentElement).getPropertyValue('{name}').trim()"))?;
    Ok(value.as_str().unwrap_or_default().to_string())
}

fn current_step(page: &mut Page) -> Result<String> {
    page.inner_text(&css(".stepper li.cur"))
}

fn card_text(page: &mut Page) -> Result<String> {
    page.inner_text(&css(".coach"))
}

/// Which part of the screen holds the keyboard focus: the tutorial card, or somewhere else.
fn focus_in_card(page: &mut Page) -> Result<bool> {
    Ok(page.eval("!!document.activeElement && !!document.activeElement.closest('.coach')")? == json!(true))
}

fn choose_experience(app: &mut Opened, key: &str) -> Result {
    let text = app.page.text_of(key)?;
    let pick = css(".choice2 button").has_text(&text);
    app.page.click(&pick)?;
    same(app.page.attr(&pick, "aria-pressed")?.as_deref(), Some("true"), "the answer to the question about AI")
}

/// Continue on step 2, with the tutorial's first card on screen at the end.
fn continue_to_tutorial(app: &mut Opened) -> Result {
    let go = app.page.named("button", "first.continue")?;
    app.page.click(&go)?;
    app.page.wait_visible(&css(".coach"))
}

fn step_one(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en" }))?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let heading = page.named("heading", "setup.title")?;
    page.wait_visible(&heading)?;
    same(page.count(&css(".stepper li"))?, 3, "the steps")?;
    same(current_step(page)?, page.text_of("first.step.accounts")?, "the step that is on")?;
    same(page.count(&css(".shell"))?, 0, "the first run has no sidebar or shell")?;
    same(page.eval("document.activeElement && document.activeElement.id")?.as_str(), Some("setup-heading"), "focus starts on the heading")?;
    shot(ctx, &mut app, "b7-step1")?;
    let page = &mut app.page;
    // Esc and the shell's shortcuts do nothing here: the buttons are the only way on.
    for chord in ["Escape", "Control+n", "Control+,", "Control+."] {
        page.press(chord)?;
    }
    page.wait_visible(&heading)?;
    same(page.count(&css(".shell, .settings"))?, 0, "a shortcut must not open anything behind the first run")?;
    // Tab never leaves the dialog.
    for _ in 0..12 {
        page.press("Tab")?;
        same(page.eval("!!document.activeElement.closest('.fr-stage')")?, json!(true), "Tab stays in the first run")?;
    }
    ctx.close(app.id)
}

fn step_two(ctx: &mut Ctx) -> Result {
    // Step 1 is done and no look is chosen yet: the window opens at step 2.
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    size(&mut app.page, 1440, 900)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    same(current_step(page)?, page.text_of("first.step.look")?, "the step that is on")?;
    same(page.count(&css(".stepper li.done svg"))?, 1, "step 1 is marked done")?;
    same(page.count(&css(".tcard"))?, 3, "the three looks")?;
    same(page.eval("document.activeElement && document.activeElement.id")?.as_str(), Some("fr-title"), "focus starts on the heading")?;
    let cards: Vec<String> = ["sala", "oficina", "bancada"].iter().map(|id| format!("look.{id}.name")).collect();
    let mut tcards = Vec::new();
    for key in &cards {
        let text = page.text_of(key)?;
        tcards.push(css(".tcard").has_text(&text));
    }
    same(page.attr(&tcards[0], "aria-pressed")?.as_deref(), Some("true"), "the default look is chosen")?;
    // Each card draws its own look with its own tokens, not the look of the page.
    page.wait_for(
        "each picture to be drawn with its own look",
        "[...document.querySelectorAll('.tcard .mini')].map((m) => m.dataset.base).join() === 'sala,oficina,bancada'",
    )?;
    shot(ctx, &mut app, "b7-step2-sala")?;
    let page = &mut app.page;

    // Esc does nothing on this step either.
    page.press("Escape")?;
    page.wait_visible(&heading)?;
    // A choice shows on the whole screen at once, before anything is saved, and the text size outlives a change of theme.
    let large = page.named("button", "first.size.large")?;
    page.click(&large)?;
    page.wait_for("the large text", "parseFloat(getComputedStyle(document.body).fontSize) === 16")?;
    let ground = root_var(page, "--bg")?;
    page.click(&tcards[1])?;
    page.wait_for("the Workshop look on the page", "document.documentElement.dataset.base === 'oficina'")?;
    ensure(root_var(page, "--bg")? != ground, || format!("--bg did not change with the look: {ground}"))?;
    page.wait_for("the large text to stay", "parseFloat(getComputedStyle(document.body).fontSize) === 16")?;
    same(page.attr(&tcards[1], "aria-pressed")?.as_deref(), Some("true"), "the card chosen")?;
    same(page.attr(&tcards[0], "aria-pressed")?.as_deref(), Some("false"), "the card left")?;
    shot(ctx, &mut app, "b7-step2-oficina")?;
    let page = &mut app.page;
    page.click(&tcards[2])?;
    page.wait_for("the Workbench look on the page", "document.documentElement.dataset.base === 'bancada'")?;
    shot(ctx, &mut app, "b7-step2-bancada")?;
    let page = &mut app.page;
    same(settings(ctx).get("appearance").cloned().unwrap_or(Value::Null), Value::Null, "nothing is saved before Continue")?;

    // Back goes to step 1 and forward again finds the choices as they were.
    let back = page.named("button", "first.back")?;
    page.click(&back)?;
    let accounts = page.named("heading", "setup.title")?;
    page.wait_visible(&accounts)?;
    same(current_step(page)?, page.text_of("first.step.accounts")?, "the step that is on after Back")?;
    let next = page.named("button", "first.continue")?;
    page.click(&next)?;
    page.wait_visible(&heading)?;
    same(page.attr(&tcards[2], "aria-pressed")?.as_deref(), Some("true"), "Back and forward keep the look")?;

    // The rest of the first settings, and the question that decides the tutorial.
    page.click(&tcards[1])?;
    page.wait_for("the large text to stay", "parseFloat(getComputedStyle(document.body).fontSize) === 16")?;
    let compact = page.named("button", "set.densityCompact")?;
    page.click(&compact)?;
    page.wait_for("compact density", "parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--dens')) === 0.8")?;
    choose_experience(&mut app, "first.xp.beginner")?;
    shot(ctx, &mut app, "b7-step2-settings")?;
    continue_to_tutorial(&mut app)?;
    saved(ctx, "step 2 to be saved", |s| {
        s["appearance"]["base"] == "oficina"
            && s["appearance"]["textSize"].as_f64() == Some(16.0)
            && s["appearance"]["density"] == 0.8
            && s["experience"] == "beginner"
            && s["tutorialDone"] == false
            && s["setup"] == "done"
    })?;
    ctx.close(app.id)
}

fn start_tutorial(ctx: &mut Ctx, experience: &str) -> Result<Opened> {
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    size(&mut app.page, 1440, 900)?;
    let heading = app.page.named("heading", "first.look.title")?;
    app.page.wait_visible(&heading)?;
    choose_experience(&mut app, experience)?;
    continue_to_tutorial(&mut app)?;
    Ok(app)
}

fn tutorial_beginner(ctx: &mut Ctx) -> Result {
    let mut app = start_tutorial(ctx, "first.xp.beginner")?;
    let page = &mut app.page;
    same(page.inner_text(&css(".coach .coach-n"))?, "1 of 3".to_string(), "the first card's place")?;
    let next = page.named("button", "tour.next")?;
    focus_on_next(page)?;
    // The screen behind the card is out of reach for the keyboard and for assistive technology.
    same(page.eval("document.querySelector('.shell').inert")?, json!(true), "the shell is inert behind the tutorial")?;
    same(page.eval("document.querySelector('.coach').getAttribute('aria-modal')")?.as_str(), Some("true"), "a modal dialog")?;
    for _ in 0..6 {
        page.press("Tab")?;
        ensure(focus_in_card(page)?, || "Tab left the tutorial card".to_string())?;
    }
    page.press("Shift+Tab")?;
    ensure(focus_in_card(page)?, || "Shift+Tab left the tutorial card".to_string())?;
    // The shell's own shortcuts do nothing under it.
    page.press("Control+n")?;
    page.press("Control+,")?;
    same(page.count(&css(".settings"))?, 0, "Settings stays closed under the tutorial")?;
    same(page.count(&css("[role=dialog]"))?, 1, "the card is the only dialog")?;
    shot(ctx, &mut app, "b7-tour-1")?;
    let page = &mut app.page;

    page.click(&next)?;
    page.wait_for("the second card", "document.querySelector('.coach .coach-n')?.textContent === '2 of 3'")?;
    same(
        page.eval("document.activeElement === document.querySelector('.coach .btn.primary')")?,
        json!(true),
        "focus starts on Next again",
    )?;
    shot(ctx, &mut app, "b7-tour-2")?;
    let page = &mut app.page;
    page.click(&next)?;
    page.wait_for("the third card", "document.querySelector('.coach .coach-n')?.textContent === '3 of 3'")?;
    // The last card has Got it and no Skip.
    let skip = page.named("button", "tour.skip")?;
    same(page.count(&skip)?, 0, "no Skip on the last card")?;
    shot(ctx, &mut app, "b7-tour-3")?;
    let page = &mut app.page;
    let done = page.named("button", "tour.finish")?;
    page.click(&done)?;
    page.wait_gone(&css(".tour"))?;
    same(page.eval("document.querySelector('.shell').inert")?, json!(false), "the shell is usable again")?;
    // Focus goes back to a field the person can type in.
    page.wait_for("focus back in the composer", "document.activeElement === document.querySelector('.composer textarea')")?;
    saved(ctx, "tutorialDone to be saved", |s| s["tutorialDone"] == true && s["experience"] == "beginner")?;
    ctx.close(app.id)?;

    // The next launch has no tutorial, and no first-run step either.
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    pause(0.5);
    same(app.page.count(&css(".tour, .fr-stage"))?, 0, "nothing of the first run on the next launch")?;
    ctx.close(app.id)
}

fn tutorial_experienced(ctx: &mut Ctx) -> Result {
    let mut app = start_tutorial(ctx, "first.xp.experienced")?;
    let page = &mut app.page;
    same(page.inner_text(&css(".coach .coach-n"))?, "1 of 2".to_string(), "the first card's place")?;
    let first = card_text(page)?;
    ensure(first.contains(&page.text_of("tour.auto.title")?), || format!("an experienced person starts with Auto: {first}"))?;
    shot(ctx, &mut app, "b7-tour-experienced-1")?;
    let page = &mut app.page;
    let next = page.named("button", "tour.next")?;
    page.click(&next)?;
    page.wait_for("the second card", "document.querySelector('.coach .coach-n')?.textContent === '2 of 2'")?;
    shot(ctx, &mut app, "b7-tour-experienced-2")?;
    let page = &mut app.page;
    // Esc on the last card ends it the same way as the button.
    page.press("Escape")?;
    page.wait_gone(&css(".tour"))?;
    saved(ctx, "tutorialDone to be saved", |s| s["tutorialDone"] == true && s["experience"] == "experienced")?;
    ctx.close(app.id)
}

fn tutorial_waits_for_an_ai(ctx: &mut Ctx) -> Result {
    // The first run asked for the tutorial, but no AI is connected: nothing teaches the composer yet.
    ctx.write_settings(&json!({ "language": "en", "setup": "skipped", "appearance": { "base": "sala" }, "tutorialDone": false }))?;
    let mut app = ctx.launch(no_cli())?;
    let title = app.page.named("heading", "empty.noAiTitle")?;
    app.page.wait_visible(&title)?;
    pause(0.5);
    same(app.page.count(&css(".tour"))?, 0, "no tutorial without an AI")?;
    shot(ctx, &mut app, "b7-no-ai")?;
    ctx.close(app.id)?;
    // With an AI it begins, and Skip ends it for good.
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    app.page.wait_visible(&css(".coach"))?;
    let skip = app.page.named("button", "tour.skip")?;
    app.page.click(&skip)?;
    app.page.wait_gone(&css(".tour"))?;
    saved(ctx, "tutorialDone to be saved", |s| s["tutorialDone"] == true)?;
    ctx.close(app.id)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    pause(0.5);
    same(app.page.count(&css(".tour"))?, 0, "a skipped tutorial never comes back")?;
    ctx.close(app.id)
}

fn tutorial_fits_the_window(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "appearance": { "base": "sala" }, "tutorialDone": false }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    app.page.wait_visible(&css(".coach"))?;
    // The window's own buttons stay reachable: the dim does not cover the title bar, and the ring moves with a transform only.
    let reach = "(() => { const e = document.elementFromPoint(innerWidth - 20, 14); return !!e && !!e.closest('.top'); })()";
    app.page.wait_for("the title bar above the dim", reach)?;
    app.page.wait_for("the ring", "document.querySelectorAll('.tour-ring').length === 1")?;
    let moves = app.page.eval("getComputedStyle(document.querySelector('.tour-ring')).transitionProperty")?;
    same(moves.as_str(), Some("transform"), "what the ring animates")?;
    let inside = "(() => { const r = document.querySelector('.coach').getBoundingClientRect();                   return r.x >= 0 && r.y >= 0 && r.right <= innerWidth && r.bottom <= innerHeight && r.width > 0; })()";
    for step in 0..3 {
        for (width, height) in [(1440, 900), (1100, 700), (899, 700), (800, 600), (720, 480)] {
            size(&mut app.page, width, height)?;
            app.page.wait_for(&format!("card {} inside the {width}x{height} window", step + 1), inside)?;
            if step == 1 && width < 900 {
                // The sidebar is behind its button, so the Projects button is not there to point at: the card is centred, with no
                // pointer and no ring. The card follows the resize a moment after the window does.
                let centred = "(() => { const c = document.querySelector('.coach'); const r = c.getBoundingClientRect(); \
                       return c.getAttribute('data-side') === null && document.querySelectorAll('.tour-ring').length === 0 \
                         && Math.abs(r.x + r.width / 2 - innerWidth / 2) <= 1.5 && Math.abs(r.y + r.height / 2 - innerHeight / 2) <= 1.5; })()";
                app.page.wait_for(&format!("the Projects card centred, with no pointer, at {width} px"), centred)?;
            }
            if step == 1 && (width == 1100 || width == 720) {
                shot(ctx, &mut app, &format!("b7-tour-2-at-{width}"))?;
            }
        }
        if step < 2 {
            let next = css(".coach .btn.primary");
            app.page.click(&next)?;
            let expected = format!("document.querySelector('.coach .coach-n')?.textContent === '{} of 3'", step + 2);
            app.page.wait_for("the next card", &expected)?;
        }
    }
    ctx.close(app.id)
}

fn manage_mode_is_plain(ctx: &mut Ctx) -> Result {
    // A finished first run that asked for the tutorial, with no AI connected.
    let kept = json!({ "base": "oficina", "textSize": 16 });
    ctx.write_settings(
        &json!({ "language": "en", "setup": "done", "appearance": kept, "experience": "experienced", "tutorialDone": false }),
    )?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let connect = page.named("button", "empty.noAiButton")?;
    let heading = page.named("heading", "setup.title")?;
    page.click(&connect)?;
    page.wait_visible(&heading)?;
    same(page.count(&css(".stepper"))?, 0, "no stepper outside the first run")?;
    // The window's own Close button has the same name, so the one asked for is looked for inside the dialog.
    let close = css(".fr-stage").role("button", &page.text_of("setup.close")?).exact();
    page.wait_visible(&close)?;
    shot(ctx, &mut app, "b7-manage")?;
    let page = &mut app.page;
    // Esc closes it, and Close closes it too; neither hands over to step 2.
    page.press("Escape")?;
    page.wait_gone(&heading)?;
    // Focus goes back to the button that opened the dialog, whichever way it closed.
    let on_connect = "document.activeElement === document.querySelector('[data-restore=connect]')";
    page.wait_for("focus back on the Connect button after Esc", on_connect)?;
    page.click(&connect)?;
    page.wait_visible(&heading)?;
    page.click(&close)?;
    page.wait_gone(&heading)?;
    page.wait_for("focus back on the Connect button after Close", on_connect)?;
    let title = page.named("heading", "empty.noAiTitle")?;
    page.wait_visible(&title)?;
    same(page.count(&css(".fr-stage"))?, 0, "closing must not hand over to step 2")?;
    // Closing saves the setup choice and nothing else: the look, the answer and the tutorial's state are as they were.
    saved(ctx, "the setup choice to be saved", |s| {
        s["setup"] == "done" && s["appearance"] == kept && s["experience"] == "experienced" && s["tutorialDone"] == false
    })?;
    ctx.close(app.id)
}

fn refused_save_keeps_step_two(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    choose_experience(&mut app, "first.xp.experienced")?;
    let page = &mut app.page;
    // A folder where the temporary file goes makes every write of settings.json fail while the old file stays valid.
    let blocker = ctx.sb.data.join("settings.json.tmp");
    std::fs::create_dir(&blocker).map_err(|e| e.to_string())?;
    let go = page.named("button", "first.continue")?;
    page.click(&go)?;
    let toast = css(".toast[role=alert]");
    page.wait_visible(&toast)?;
    let sentence = page.text_of("error.settings.saveFailed")?;
    ensure(page.inner_text(&toast)?.contains(&sentence), || format!("the alert should say {sentence:?}"))?;
    // The step stays open, can be used again, and nothing was saved or started.
    page.wait_visible(&heading)?;
    page.wait_enabled(&go)?;
    pause(0.5);
    same(page.count(&css(".coach"))?, 0, "no tutorial after a refused save")?;
    let text = std::fs::read_to_string(ctx.sb.data.join("settings.json")).map_err(|e| e.to_string())?;
    ensure(!text.contains("tutorialDone") && !text.contains("experience"), || format!("settings.json should be as it was: {text}"))?;
    // Once the disk works again, the same Continue saves and the tutorial starts.
    std::fs::remove_dir(&blocker).map_err(|e| e.to_string())?;
    let page = &mut app.page;
    page.click(&go)?;
    page.wait_gone(&heading)?;
    page.wait_visible(&css(".coach"))?;
    saved(ctx, "step 2 to be saved", |s| s["experience"] == "experienced" && s["tutorialDone"] == false)?;
    ctx.close(app.id)
}

fn old_appearance_opens_step_two_once(ctx: &mut Ctx) -> Result {
    // What an older version saved: the appearance cannot be read, so the screen has none. A saved conversation means step 1
    // is not due, so the first run starts at step 2 with step 1 shown as done.
    super::shell::seed(ctx, "old", "An old chat", None, 1_700_000_000)?;
    ctx.write_settings(&json!({
        "language": "en",
        "appearance": { "mode": "dark", "accent": "mono", "ui": "geistmono", "read": "geist", "density": "comfortable", "corners": "round" }
    }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    same(page.count(&css(".stepper li.done svg"))?, 1, "step 1 is shown as done")?;
    pass_look(page)?;
    skip_tour(page)?;
    saved(ctx, "step 2 to be saved", |s| s["appearance"]["base"] == "sala" && s["tutorialDone"] == true)?;
    ctx.close(app.id)?;
    // The next launch goes straight to the new conversation.
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    pause(0.5);
    same(app.page.count(&css(".fr-stage, .tour"))?, 0, "step 2 comes once")?;
    ctx.close(app.id)
}

fn keyboard_only(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    // Step 2: the Workshop card with Space, the answer with Enter, Continue with Enter.
    let workshop = json!(page.text_of("look.oficina.name")?);
    tab_to(page, "the Workshop card", &format!("a.classList.contains('tcard') && a.textContent.includes({workshop})"))?;
    page.press("Space")?;
    page.wait_for("the Workshop look", "document.documentElement.dataset.base === 'oficina'")?;
    let never = json!(page.text_of("first.xp.beginner")?);
    tab_to(page, "the answer Never", &format!("!!a.closest('.choice2') && a.textContent.includes({never})"))?;
    page.press("Enter")?;
    tab_to(page, "Continue", "a.classList.contains('big')")?;
    page.press("Enter")?;
    page.wait_visible(&css(".coach"))?;
    // The tutorial: Space presses Next, Tab stays on the card, the shortcuts of the shell do nothing, Esc skips.
    focus_on_next(page)?;
    page.press("Space")?;
    page.wait_for("the second card", "document.querySelector('.coach .coach-n')?.textContent === '2 of 3'")?;
    focus_on_next(page)?;
    page.press("Shift+Tab")?;
    ensure(focus_in_card(page)?, || "Shift+Tab left the tutorial card".to_string())?;
    page.press("Control+n")?;
    page.press("Control+,")?;
    same(page.count(&css(".settings"))?, 0, "Settings stays closed under the tutorial")?;
    page.press("Escape")?;
    page.wait_gone(&css(".tour"))?;
    page.wait_for("focus back in the composer", "document.activeElement === document.querySelector('.composer textarea')")?;
    saved(ctx, "the choices to be saved", |s| {
        s["appearance"]["base"] == "oficina" && s["experience"] == "beginner" && s["tutorialDone"] == true
    })?;
    ctx.close(app.id)
}

fn save_in_flight_shuts_the_step(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    hold(page, "save_settings", 2500, 1)?;
    let go = page.named("button", "first.continue")?;
    page.click(&go)?;
    // The save is on its way: Back would open step 1 under it, and a choice made now would be lost to it.
    let back = page.named("button", "first.back")?;
    same(page.is_disabled(&back)?, true, "Back while the save is on its way")?;
    same(page.eval("document.querySelector('.themes').inert && document.querySelector('.first-ctl').inert")?, json!(true), "the choices")?;
    page.wait_gone(&heading)?;
    page.wait_visible(&css(".coach"))?;
    ctx.close(app.id)
}

fn stale_preview_never_lands(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    // Workshop's preview takes 2.5 s; Back and forward again happen meanwhile, and Workbench is chosen on the new step.
    hold(page, "preview_look", 2500, 1)?;
    let workshop = css(".tcard").has_text(&page.text_of("look.oficina.name")?);
    let workbench = css(".tcard").has_text(&page.text_of("look.bancada.name")?);
    page.click(&workshop)?;
    let back = page.named("button", "first.back")?;
    page.click(&back)?;
    let accounts = page.named("heading", "setup.title")?;
    page.wait_visible(&accounts)?;
    let forward = page.named("button", "first.continue")?;
    page.click(&forward)?;
    page.wait_visible(&heading)?;
    page.click(&workbench)?;
    page.wait_for("the Workbench look", "document.documentElement.dataset.base === 'bancada'")?;
    // The answer of the old step arrives now, and must change nothing.
    pause(3.2);
    same(page.eval("document.documentElement.dataset.base")?.as_str(), Some("bancada"), "the look after the old preview answered")?;
    ctx.close(app.id)
}

fn continue_lands_on_a_fresh_chat(ctx: &mut Ctx) -> Result {
    // A person of an older version had opened a conversation, and the window remembers it.
    super::shell::seed(ctx, "old", "An old chat", None, 1_790_000_000_000)?;
    super::start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    app.page.click(&css(".list .item").has_text("An old chat"))?;
    app.page.wait_visible(&css("article").has_text("An old chat: the answer"))?;
    ctx.close(app.id)?;
    // The new version asks its first-run question once, because the saved appearance has no base.
    let older = json!({
        "language": "en",
        "appearance": { "mode": "dark", "accent": "mono", "ui": "geistmono", "read": "geist", "density": "comfortable", "corners": "round" }
    });
    std::fs::write(ctx.sb.data.join("settings.json"), older.to_string()).map_err(|e| e.to_string())?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    pass_look(&mut app.page)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    same(app.page.count(&css("article").has_text("An old chat: the answer"))?, 0, "the old conversation after Continue")?;
    // The first conversation is where the tutorial teaches.
    app.page.wait_visible(&css(".coach"))?;
    skip_tour(&mut app.page)?;
    ctx.close(app.id)
}

fn signed_out_cli_starts_no_tutorial(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "appearance": { "base": "sala" }, "tutorialDone": false }))?;
    // Installed, but the CLI says nobody is signed in: there is no marker file yet.
    let recording = ctx.paths.fixtures.join("setup").join("claude.setup.json");
    let launch = Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording: Some(recording), ..Launch::default() };
    let mut app = ctx.launch(launch)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    pause(1.0);
    same(app.page.count(&css(".tour"))?, 0, "no tutorial for an AI that is signed out")?;
    ensure(settings(ctx)["tutorialDone"] == false, || "the tutorial is still to come".to_string())?;
    // Signing in elsewhere, then closing Settings: the next look at the account says it is connected.
    std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    let page = &mut app.page;
    page.press("Control+,")?;
    page.wait_visible(&css(".settings"))?;
    page.press("Escape")?;
    page.wait_visible(&css(".coach"))?;
    skip_tour(page)?;
    ctx.close(app.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scenario_has_its_own_name() {
        let mut names: Vec<&str> = scenarios().iter().map(|s| s.name).collect();
        let all = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all);
    }
}
