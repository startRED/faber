//! Supplemental controls exercised through the real hidden application, never through component callbacks.

use super::{Result, Scenario, ensure, eventually, no_cli, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use serde_json::{Value, json};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const WINDOW: &str = "control walk: the native window maximizes, restores and closes through its caption buttons";
const ACTIVE: &str = "5f1c0000-0000-4000-8000-0000000000c1";
const DISMISS: &str = "5f1c0000-0000-4000-8000-0000000000c2";
const OPEN: &str = "5f1c0000-0000-4000-8000-0000000000c3";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "control walk: a running project row opens its conversation and its details", run: running },
        Scenario { name: "control walk: finished cards dismiss persistently and open their own conversation", run: finished },
        Scenario { name: WINDOW, run: window },
    ]
}

fn running(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    super::shell::seed_project(ctx, "Control proof")?;
    // The recording expects its first request alone; seeded replies would prepend a history transcript to that input.
    let conversations = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&conversations).map_err(|e| e.to_string())?;
    let saved = json!({"id":ACTIVE, "title":"Active control proof", "area":"project", "projectId":"demo",
        "sessions":{}, "createdAt":1_790_000_000_000u64, "updatedAt":1_790_000_000_000u64, "messages":[]});
    std::fs::write(conversations.join(format!("{ACTIVE}.json")), saved.to_string()).map_err(|e| e.to_string())?;
    let launch = super::shell::two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Control proof"))?;
    page.click(&css(".proj-chats .item").has_text("Active control proof"))?;
    super::shell::ask_two_writes(page).map_err(|e| format!("before running-row controls: initial fake permission did not arrive: {e}"))?;
    let row = css("#running .ri:not(.done)").has_text("Active control proof");
    page.wait_visible(&row)?;
    let chat = page.named("button", "area.chat")?;
    page.click(&chat)?;
    page.wait_gone(&css("article .ask"))?;
    page.click(&row.clone().css("button.name"))?;
    page.wait_visible(&css("article .ask").has_text("a.txt"))?;
    page.wait_visible(&css(".crumb").has_text("Active control proof"))?;
    // Open Details from another area too: a stale details view would otherwise look like success.
    page.click(&chat)?;
    page.wait_gone(&css("article .ask"))?;
    let details = page.text_of("details.title")?;
    page.press_on(&row.clone().role("button", &details).exact(), "Enter")?;
    page.wait_visible(&css("article .ask").has_text("a.txt"))?;
    page.wait_visible(&css("#running .details-view"))?;
    page.wait_visible(&css(".crumb").has_text("Active control proof"))?;
    let back = page.named("button", "agent.back")?;
    page.click(&back)?;
    page.wait_gone(&css("#running .details-view"))?;
    page.wait_visible(&row)?;
    ctx.shot(&mut app, "control-running-row-returned")?;
    ctx.close(app.id)
}

fn finished(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis() as u64;
    for (id, title) in [(DISMISS, "Dismiss control proof"), (OPEN, "Open control proof")] {
        super::shell::seed(ctx, id, title, None, now)?;
        let path = ctx.sb.data.join("conversations").join(format!("{id}.json"));
        let mut saved: Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        saved["ended"] = json!({"at":now, "kind":"done", "workMs":1250, "line":"Synthetic completed work"});
        std::fs::write(path, saved.to_string()).map_err(|e| e.to_string())?;
    }
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    if !page.is_visible(&css("#running"))? {
        page.click(&css(".run-btn"))?;
    }
    let dismissed = css("#running .ri.done").has_text("Dismiss control proof");
    let opened = css("#running .ri.done").has_text("Open control proof");
    page.wait_visible(&dismissed)?;
    page.wait_visible(&opened)?;
    page.click(&dismissed.clone().css("button.dismiss"))?;
    page.wait_gone(&dismissed)?;
    page.wait_visible(&opened)?;
    same(page.count(&css("article"))?, 0, "dismiss does not open any conversation")?;
    let dismissed_path = ctx.sb.data.join("conversations").join(format!("{DISMISS}.json"));
    eventually("the dismissed fixture's completed card to clear on disk", Duration::from_secs(5), || {
        std::fs::read(&dismissed_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|saved| saved.get("ended").is_none_or(Value::is_null))
    })?;
    page.reload()?;
    page.wait_visible(&css(".run-btn"))?;
    if !page.is_visible(&css("#running"))? {
        page.click(&css(".run-btn"))?;
    }
    page.wait_visible(&opened)?;
    same(page.count(&dismissed)?, 0, "dismissed card stays gone after reloading real engine state")?;
    page.press_on(&opened.css("button.name"), "Enter")?;
    page.wait_visible(&css("article").has_text("Open control proof: the answer"))?;
    let new = page.named("button", "side.new")?;
    page.click(&new)?;
    if !page.is_visible(&css("#running"))? {
        page.click(&css(".run-btn"))?;
    }
    page.wait_visible(&css("#running .rc-empty"))?;
    ensure(ctx.records()?.is_empty(), || "finished-card navigation started a CLI".into())?;
    ctx.shot(&mut app, "control-finished-cards-cleared")?;
    ctx.close(app.id)
}

fn window(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    const MAXIMIZED: &str = "window.__TAURI_INTERNALS__.invoke('plugin:window|is_maximized',{label:'main'})";
    same(page.eval(MAXIMIZED)?.as_bool(), Some(false), "native initial maximize state")?;
    let maximize = page.named("button", "window.maximize")?.exact();
    page.click(&maximize)?;
    page.wait_for("the operating system to maximize the window", MAXIMIZED)?;
    let restore = page.named("button", "window.restore")?.exact();
    page.wait_visible(&restore)?;
    page.press_on(&restore, "Enter")?;
    page.wait_for("the operating system to restore the window", &format!("{MAXIMIZED}.then(value=>value===false)"))?;
    page.wait_visible(&maximize)?;
    ctx.guard_desktop(app.id)?;
    ctx.shot(&mut app, "control-native-window-restored")?;
    ensure(ctx.exited(app.id).is_none(), || "the app exited before Close was pressed".into())?;
    let close = app.page.named("button", "window.close")?.exact();
    app.page.click(&close)?;
    // Production frees the screen and keeps the tray; hidden test runs intentionally exit instead (main.rs).
    eventually("Close to end the hidden test application", Duration::from_secs(10), || ctx.exited(app.id).is_some())?;
    same(ctx.exited(app.id), Some(std::process::ExitStatus::from_raw(0).to_string()), "Close exits successfully instead of crashing")?;
    ctx.notes.push("Caption Close ended the real hidden test process; production tray retention was not exercised.".into());
    ctx.close(app.id)
}
