//! The first-run guide, end to end: connecting an installed CLI, installing one from a loopback release, the choice to
//! have none, memory with the guide open, and that closing it leaves no process or file behind.
//!
//! The guide is step 1 of the first run. It opens because these scenarios start with settings that do not say the first
//! run is done; each one passes step 2 (the look) and, when an AI is connected, skips the tutorial. A scenario that
//! installs leaves `FABER_CLI_CODEX` absent, so the engine finds the program the guide just put in the data folder.

use super::{Result, Scenario, ask, ensure, eventually, pass_look, pause, same, skip_tour};
use crate::app::{Ctx, Opened};
use crate::page::{Loc, css};
use crate::sandbox::{CliSlot, Launch};
use std::time::Duration;

const BUDGET_MB: f64 = 400.0;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "C5.1: installed signed-in Claude connects without reinstalling and answers the first question", run: c5_1 },
        Scenario { name: "C5.2: missing ChatGPT installs from loopback, signs in and answers without reopening", run: c5_2 },
        Scenario { name: "C5.3: None persists, shows connection guidance and never reopens setup automatically", run: c5_3_none },
        Scenario { name: "C5.3: Skip persists, shows connection guidance and never reopens setup automatically", run: c5_3_skip },
        Scenario { name: "C5.4: the open guide and the look step, with no conversation, use at most 400 MB", run: c5_4 },
        Scenario { name: "C5.5: closing the guide leaves no setup child or external writes", run: c5_5 },
        Scenario {
            name: "first run: None to the end shows no tutorial, and connecting an account afterwards starts it",
            run: none_then_connect,
        },
        Scenario {
            name: "reconnecting an account saves the setup choice and keeps the finished tutorial and the first-run answers",
            run: reconnect_keeps_the_first_run,
        },
    ]
}

/// The state every setup scenario starts from: the guide is due, a loopback release server is up, and Claude is
/// signed in when `connected`.
fn start(ctx: &mut Ctx, connected: bool) -> Result {
    ctx.write_settings(&serde_json::json!({ "language": "en" }))?;
    ctx.serve_release()?;
    if connected {
        std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn recording(ctx: &Ctx, name: &str) -> Option<std::path::PathBuf> {
    Some(ctx.paths.fixtures.join("setup").join(name))
}

fn claude_installed(ctx: &Ctx) -> Launch {
    Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording: recording(ctx, "claude.setup.json"), ..Launch::default() }
}

/// ChatGPT is not installed: its program arrives from the loopback release.
fn codex_missing(ctx: &Ctx) -> Launch {
    Launch { claude: CliSlot::Off, codex: CliSlot::Absent, recording: recording(ctx, "codex.setup.json"), ..Launch::default() }
}

fn guide(app: &mut Opened) -> Result<Loc> {
    Ok(app.page.named("dialog", "setup.title")?.exact())
}

fn wait_for_guide(app: &mut Opened) -> Result {
    let guide = guide(app)?;
    app.page.wait_visible(&guide)
}

fn click_in_guide(app: &mut Opened, key: &str) -> Result {
    let button = guide(app)?.role("button", &app.page.text_of(key)?).exact();
    app.page.click(&button)
}

fn card(vendor: &str) -> Loc {
    css(&format!(".setup-card[data-vendor=\"{vendor}\"]"))
}

fn connected(app: &mut Opened, vendor: &str) -> Result {
    let shown = app.page.text_of("setup.connected")?;
    app.page.wait_visible(&card(vendor).text(&shown).exact())?;
    for key in ["setup.install", "setup.signIn"] {
        let button = card(vendor).role("button", &app.page.text_of(key)?).exact();
        same(app.page.count(&button)?, 0, &format!("{vendor} must not offer {key} once connected"))?;
    }
    Ok(())
}

/// Continue on step 1, which hands over to step 2.
fn finish(app: &mut Opened) -> Result {
    click_in_guide(app, "first.continue")?;
    let guide = guide(app)?;
    app.page.wait_gone(&guide)
}

fn ready_to_chat(app: &mut Opened) -> Result {
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    let composer = app.page.labelled("composer.message")?;
    same(app.page.is_disabled(&composer)?, false, "the composer is enabled right after setup")
}

fn answered(app: &mut Opened, word: &str) -> Result {
    ask(&mut app.page, "In one sentence: why is the sky blue?")?;
    let answer = app.page.named("article", "answer.label")?.text_re(word);
    app.page.wait_visible(&answer)
}

fn c5_1(ctx: &mut Ctx) -> Result {
    start(ctx, true)?;
    let launch = claude_installed(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.claude")?;
    connected(&mut app, "claude")?;
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.is_empty(), || format!("an existing CLI must not be downloaded again: {requests:?}"))?;
    let lines: Vec<String> = ctx.records()?.iter().map(|c| c.line()).collect();
    ensure(lines.iter().any(|l| l == "auth status"), || format!("the guide should check the sign-in: {lines:?}"))?;
    ensure(!lines.iter().any(|l| l == "auth login"), || format!("the guide must not sign in again: {lines:?}"))?;
    finish(&mut app)?;
    pass_look(&mut app.page)?;
    skip_tour(&mut app.page)?;
    ready_to_chat(&mut app)?;
    answered(&mut app, "Rayleigh")?;
    ctx.close(app.id)
}

fn c5_2(ctx: &mut Ctx) -> Result {
    start(ctx, false)?;
    let launch = codex_missing(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.chatgpt")?;
    install_and_connect_codex(ctx, &mut app)?;
    finish(&mut app)?;
    pass_look(&mut app.page)?;
    skip_tour(&mut app.page)?;
    ready_to_chat(&mut app)?;
    answered(&mut app, "scatter")?;
    same(ctx.exited(app.id), None, "the original app must still be running")?;
    ctx.close(app.id)
}

/// ChatGPT's card is open and its program is not installed: installs it from the loopback release, signs in, and waits until
/// the card says it is connected.
fn install_and_connect_codex(ctx: &mut Ctx, app: &mut Opened) -> Result {
    let absent = app.page.text_of("setup.notInstalled")?;
    app.page.wait_visible(&card("codex").text(&absent).exact())?;
    let install = card("codex").role("button", &app.page.text_of("setup.install")?).exact();
    app.page.click(&install)?;
    let sign_in = card("codex").role("button", &app.page.text_of("setup.signIn")?).exact();
    app.page.wait_visible(&sign_in)?;
    let installed = std::fs::read(ctx.sb.data.join("bin/codex.exe")).map_err(|e| format!("the installed program: {e}"))?;
    ensure(installed == ctx.payload, || "the served executable must be the installed CLI".to_string())?;
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.iter().any(|r| r == "/codex.zip"), || format!("the archive should have been downloaded: {requests:?}"))?;
    app.page.click(&sign_in)?;
    let marker = ctx.sb.state.join("codex.signed-in");
    eventually("the fake sign-in to complete", Duration::from_secs(15), || marker.exists())?;
    let again = card("codex").role("button", &app.page.text_of("setup.checkAgain")?).exact();
    if app.page.is_visible(&again)? {
        app.page.click(&again)?;
    }
    connected(app, "codex")?;
    let lines: Vec<String> = ctx.records()?.iter().map(|c| c.line()).collect();
    ensure(lines.iter().any(|l| l == "login"), || format!("the guide should sign in: {lines:?}"))?;
    ensure(lines.iter().any(|l| l == "login status"), || format!("the guide should check the sign-in: {lines:?}"))
}

fn none_then_connect(ctx: &mut Ctx) -> Result {
    start(ctx, false)?;
    let launch = codex_missing(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.none")?;
    finish(&mut app)?;
    pass_look(&mut app.page)?;
    // With no AI connected the new conversation gives way to the empty state, with no composer to type in, and the tutorial
    // does not start.
    let title = app.page.named("heading", "empty.noAiTitle")?;
    app.page.wait_visible(&title)?;
    same(app.page.count(&css(".composer"))?, 0, "no composer with no AI")?;
    pause(0.5);
    same(app.page.count(&css(".tour"))?, 0, "no tutorial with no AI")?;
    // The button of the empty state opens the account cards in their plain mode; connecting there brings the tutorial.
    let open = app.page.named("button", "empty.noAiButton")?;
    app.page.click(&open)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.chatgpt")?;
    install_and_connect_codex(ctx, &mut app)?;
    let close = css(".fr-stage").role("button", &app.page.text_of("setup.close")?).exact();
    app.page.click(&close)?;
    app.page.wait_visible(&css(".coach"))?;
    skip_tour(&mut app.page)?;
    let file = ctx.sb.data.join("settings.json");
    eventually("the end of the tutorial to be saved", Duration::from_secs(15), || {
        std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|settings| settings["tutorialDone"] == true)
    })?;
    // Asking Codex for its limits and models starts app servers. The checks that follow every focus and step ask again only
    // while the models are missing, one asking at a time: this run starts 3, against 4 to 10 when every check asked. A burst
    // of starts also made this check flaky: the runner ends the app at once, and a CLI caught between being created
    // (suspended) and joining its job was left behind, suspended, outside any job.
    let servers = ctx.records()?.iter().filter(|c| c.args.first().is_some_and(|a| a == "app-server")).count();
    ensure(servers <= 3, || format!("Codex was asked for its models too often: {servers} app servers"))?;
    ctx.close(app.id)
}

fn reconnect_keeps_the_first_run(ctx: &mut Ctx) -> Result {
    ctx.serve_release()?;
    // A person who skipped the accounts long ago, finished the tutorial, and answered the first-run question.
    let kept = serde_json::json!({ "base": "oficina", "textSize": 16 });
    ctx.write_settings(
        &serde_json::json!({ "language": "en", "setup": "skipped", "appearance": kept, "experience": "experienced", "tutorialDone": true }),
    )?;
    let launch = codex_missing(ctx);
    let mut app = ctx.launch(launch)?;
    let open = app.page.named("button", "empty.noAiButton")?;
    app.page.wait_visible(&open)?;
    app.page.click(&open)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.chatgpt")?;
    install_and_connect_codex(ctx, &mut app)?;
    let close = css(".fr-stage").role("button", &app.page.text_of("setup.close")?).exact();
    app.page.click(&close)?;
    let guide = guide(&mut app)?;
    app.page.wait_gone(&guide)?;
    // The decision is saved, and what the first run stored is as it was: a finished tutorial is not taught again.
    let file = ctx.sb.data.join("settings.json");
    let read = || std::fs::read_to_string(&file).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    eventually("the setup choice to be saved", Duration::from_secs(15), || read().is_some_and(|s| s["setup"] == "done"))?;
    let settings = read().unwrap_or_default();
    same(&settings["appearance"], &kept, "the saved look")?;
    same(settings["experience"].as_str(), Some("experienced"), "the answer to the question about AI")?;
    same(settings["tutorialDone"].as_bool(), Some(true), "the finished tutorial")?;
    pause(1.0);
    same(app.page.count(&css(".tour"))?, 0, "a finished tutorial never starts again")?;
    ctx.close(app.id)
}

fn c5_3_none(ctx: &mut Ctx) -> Result {
    c5_3(ctx, true)
}

fn c5_3_skip(ctx: &mut Ctx) -> Result {
    c5_3(ctx, false)
}

fn c5_3(ctx: &mut Ctx, none: bool) -> Result {
    start(ctx, false)?;
    let mut app = ctx.launch(Launch::default())?;
    wait_for_guide(&mut app)?;
    if none {
        click_in_guide(&mut app, "setup.none")?;
        finish(&mut app)?;
    } else {
        click_in_guide(&mut app, "setup.skip")?;
    }
    // Neither choice skips step 2, and with no AI connected there is no tutorial to follow it.
    pass_look(&mut app.page)?;
    let wanted = if none { "done" } else { "skipped" };
    for launch in 0..2 {
        let title = app.page.named("heading", "empty.noAiTitle")?;
        app.page.wait_visible(&title)?;
        let body = app.page.inner_text(&css(".empty p"))?;
        same(body.clone(), app.page.text_of("empty.noAiBody")?, "no-subscription guidance")?;
        let section = app.page.text_of("set.subscriptions")?;
        ensure(!body.contains("Settings") && !body.contains(&section), || format!("it should not name a Settings path: {body}"))?;
        let lower = body.to_lowercase();
        ensure(!["restart", "reopen", "relaunch"].iter().any(|w| lower.contains(w)), || format!("it should not ask to restart: {body}"))?;
        let guide = guide(&mut app)?;
        same(app.page.count(&guide)?, 0, "the saved choice must suppress the guide")?;
        same(app.page.count(&css(".fr-stage"))?, 0, "nor any other step of the first run")?;
        same(app.page.count(&css(".tour"))?, 0, "with no AI there is no tutorial")?;
        let file = ctx.sb.data.join("settings.json");
        eventually("the setup decision to be durable before closing", Duration::from_secs(15), || {
            std::fs::read_to_string(&file)
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                .is_some_and(|settings| settings["setup"] == wanted)
        })?;
        ctx.close(app.id)?;
        if launch == 0 {
            app = ctx.launch(Launch::default())?;
        }
    }
    let requests = ctx.server.as_ref().map(|s| s.requests()).unwrap_or_default();
    ensure(requests.is_empty(), || format!("nothing may be downloaded: {requests:?}"))?;
    let lines: Vec<String> = ctx.records()?.iter().map(|c| c.line()).collect();
    ensure(lines.is_empty(), || format!("None and Skip must not start a CLI: {lines:?}"))
}

/// Three samples of the app and its WebView2 children together, noted by name and held to the budget.
fn measure(ctx: &mut Ctx, app: &Opened, what: &str) -> Result {
    let mut samples = Vec::new();
    for _ in 0..3 {
        let (mb, webview) = ctx.memory_mb(app.id)?;
        ensure(webview, || "measure the app and its WebView2 children together".to_string())?;
        ensure(mb.is_finite() && mb > 0.0, || "the private working set must be a real positive measurement".to_string())?;
        samples.push(mb);
    }
    let most = samples.iter().copied().fold(0.0, f64::max);
    let shown: Vec<String> = samples.iter().map(|mb| format!("{mb:.1}")).collect();
    ctx.notes.push(format!("{what}: {} MB", shown.join(", ")));
    ensure(most <= BUDGET_MB, || format!("{what}: {} MB; limit {BUDGET_MB} MB", shown.join(", ")))
}

fn c5_4(ctx: &mut Ctx) -> Result {
    start(ctx, false)?;
    let mut app = ctx.launch(Launch::default())?;
    wait_for_guide(&mut app)?;
    measure(ctx, &app, "open guide")?;
    // Step 2 draws three small copies of the screen, each with its own fonts: the measurement of the first run's heaviest page.
    click_in_guide(&mut app, "setup.skip")?;
    let heading = app.page.named("heading", "first.look.title")?;
    app.page.wait_visible(&heading)?;
    app.page.wait_for(
        "the three pictures to be drawn",
        "[...document.querySelectorAll('.tcard .mini')].map((m) => m.dataset.base).join() === 'sala,oficina,bancada'",
    )?;
    pause(1.0);
    measure(ctx, &app, "open look step")?;
    let lines: Vec<String> = ctx.records()?.iter().map(|c| c.line()).collect();
    ensure(lines.is_empty(), || format!("opening the guide must not keep an idle CLI alive: {lines:?}"))?;
    ctx.close(app.id)
}

fn c5_5(ctx: &mut Ctx) -> Result {
    start(ctx, true)?;
    let launch = claude_installed(ctx);
    let mut app = ctx.launch(launch)?;
    wait_for_guide(&mut app)?;
    click_in_guide(&mut app, "setup.claude")?;
    connected(&mut app, "claude")?;
    // Every other scenario audits the same boundaries when it ends, including after a failed flow. This one also closes
    // while the guide itself is still open, and waits for the status and login children to stop.
    ctx.close(app.id)?;
    ctx.audit_within(Duration::from_secs(5))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port completeness: C5.1 to C5.5 of `tests/e2e/setup.e2e.ts` (C5.3 covers None and Skip, one scenario each).
    // R5.5 and the replay check are `rust` suite tests (`sandbox`, `app` and `fake`), because they need no app.
    #[test]
    fn the_setup_scenarios_of_the_node_suite_are_all_ported() {
        let names: Vec<&str> = scenarios().iter().map(|s| s.name).filter(|n| n.starts_with("C5.")).collect();
        let starts: Vec<&str> = names.iter().map(|n| n.split(':').next().unwrap()).collect();
        assert_eq!(starts, ["C5.1", "C5.2", "C5.3", "C5.3", "C5.4", "C5.5"]);
        assert!(names[2].contains("None") && names[3].contains("Skip"));
    }
}
