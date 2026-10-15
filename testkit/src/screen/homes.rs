//! The two homes (the new conversation of Chat and the first screen of a project): how they look, what moves while the
//! person does nothing, and that nothing moves with reduced motion or while the window is hidden.

use super::{Result, Scenario, claude_chat, ensure, pause, system_motion};
use crate::app::{Ctx, Opened};
use crate::page::css;
use serde_json::{Value, json};

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "homes: the Chat home and the Projects home look alive, and stand still with reduced motion", run: looks },
        Scenario { name: "homes: the safety rows read in plain words and few lines at 1280x800", run: plain_rows_at_1280 },
        Scenario {
            name: "homes: idle life costs the processor little, and nothing while the window is hidden",
            run: costs_nothing_while_hidden,
        },
    ]
}

/// The owner's own first screens: a name, two projects and some conversations to pick up.
fn start(ctx: &mut Ctx) -> Result<Opened> {
    ctx.write_settings(&json!({
        "language": "pt-BR",
        "setup": "done",
        "appearance": { "base": "sala" },
        "tutorialDone": true,
        "userName": "Erick"
    }))?;
    let git = super::projects_ui::fixture(ctx)?;
    let projects = ctx.sb.data.join("projects.json");
    let mut list: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&projects).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let folder = ctx.sb.data.join("iridium");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    list.push(json!({ "id": "iridium", "name": "Iridium", "path": folder.to_string_lossy() }));
    std::fs::write(&projects, serde_json::to_vec(&list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis() as u64;
    for (i, (title, area, project)) in [
        ("Quero que você faça uma reformulação completa do Iridium", "project", Some("iridium")),
        ("Configure o projeto para atualizar no momento certo", "project", Some("iridium")),
        ("Olá", "project", Some("demo")),
        ("Explique a cota do Codex em palavras simples", "chat", None),
    ]
    .into_iter()
    .enumerate()
    {
        let at = now - 3_600_000 * (i as u64 + 1);
        let mut conversation = json!({ "id": format!("c{i}"), "title": title, "area": area, "sessions": {}, "createdAt": at, "updatedAt": at,
            "messages": [{ "id": format!("m{i}"), "role": "user", "text": title, "at": at }] });
        if let Some(project) = project {
            conversation["projectId"] = json!(project);
        }
        std::fs::write(folder.join(format!("c{i}.json")), conversation.to_string()).map_err(|e| e.to_string())?;
    }
    let mut launch = claude_chat(ctx);
    launch.git_bin = git.git_bin;
    ctx.launch(launch)
}

/// What is alive on the screen now: the idle composer, how many animations run, the motion level and whether the window counts
/// as hidden.
const LIFE: &str = r#"(() => ({
  ghost: !!document.querySelector('.composer .idle-box'),
  hints: document.querySelectorAll('.composer .g-hint').length,
  carets: document.querySelectorAll('.composer .g-caret').length,
  running: document.getAnimations().filter((a) => a.playState === 'running').length,
  level: document.documentElement.dataset.motion,
  hidden: document.hidden,
  paused: document.documentElement.classList.contains('hidden-win'),
  projectLight: !!document.querySelector('.ambient.project'),
  chatLight: !!document.querySelector('.ambient:not(.project)'),
}))()"#;

fn life(app: &mut Opened) -> Result<Value> {
    app.page.eval(LIFE)
}

/// The size of the owner's own screenshots.
fn big(app: &mut Opened) -> Result {
    app.page
        .cdp
        .call("Emulation.setDeviceMetricsOverride", json!({ "width": 1920, "height": 1080, "deviceScaleFactor": 1, "mobile": false }))
        .map(|_| ())
}

fn open_project(app: &mut Opened) -> Result {
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("Demo"))?;
    app.page.wait_visible(&css(".start-card"))
}

fn still(state: &Value) -> bool {
    state["ghost"] == false && state["carets"] == 0 && state["running"] == 0
}

fn looks(ctx: &mut Ctx) -> Result {
    let mut app = start(ctx)?;
    big(&mut app)?;
    system_motion(&mut app.page, false)?;
    app.page.wait_visible(&css(".hello"))?;
    pause(1.5);
    let chat = life(&mut app)?;
    ensure(chat["hidden"] == false, || format!("the test window counts as hidden: {chat}"))?;
    ensure(chat["ghost"] == true && chat["hints"] == 3, || format!("the Chat home's composer idles: {chat}"))?;
    ensure(chat["chatLight"] == true && chat["projectLight"] == false, || format!("the Chat home has the chat light: {chat}"))?;
    ensure(chat["running"].as_u64().unwrap_or(0) > 0, || format!("something moves on the Chat home: {chat}"))?;
    ctx.shot(&mut app, "homes-chat")?;
    open_project(&mut app)?;
    pause(1.5);
    let project = life(&mut app)?;
    ensure(project["ghost"] == true && project["hints"] == 3, || format!("the Projects home's composer idles: {project}"))?;
    ensure(project["projectLight"] == true && project["chatLight"] == false, || format!("the Projects home has its own light: {project}"))?;
    ensure(project["running"].as_u64().unwrap_or(0) > 0, || format!("something moves on the Projects home: {project}"))?;
    ensure(app.page.count(&css(".start-card.welcome-grid .hello"))? == 1, || "the Projects home has the greeting of a home".into())?;
    ctx.shot(&mut app, "homes-project")?;
    // With reduced motion both homes stand completely still and the fields show their plain placeholders.
    system_motion(&mut app.page, true)?;
    pause(0.6);
    let quiet = life(&mut app)?;
    ensure(still(&quiet), || format!("the Projects home moves with reduced motion: {quiet}"))?;
    ctx.shot(&mut app, "homes-project-reduced")?;
    let chat = app.page.named("button", "area.chat")?;
    app.page.click(&chat)?;
    app.page.wait_visible(&css(".hello"))?;
    pause(0.6);
    let quiet = life(&mut app)?;
    ensure(still(&quiet), || format!("the Chat home moves with reduced motion: {quiet}"))?;
    ctx.shot(&mut app, "homes-chat-reduced")?;
    // Motion comes back when the preference does.
    system_motion(&mut app.page, false)?;
    pause(0.6);
    let again = life(&mut app)?;
    ensure(again["ghost"] == true && again["running"].as_u64().unwrap_or(0) > 0, || format!("motion did not come back: {again}"))
}

/// The cost of standing still, in percent of one processor core, over `seconds` with nobody touching the window.
fn cpu(ctx: &mut Ctx, app: &Opened, seconds: f64) -> Result<f64> {
    let before = ctx.cpu_seconds(app.id)?;
    pause(seconds);
    Ok((ctx.cpu_seconds(app.id)? - before) / seconds * 100.0)
}

fn costs_nothing_while_hidden(ctx: &mut Ctx) -> Result {
    let mut app = start(ctx)?;
    big(&mut app)?;
    system_motion(&mut app.page, false)?;
    app.page.wait_visible(&css(".hello"))?;
    pause(3.0);
    let chat = cpu(ctx, &app, 8.0)?;
    open_project(&mut app)?;
    pause(3.0);
    let project = cpu(ctx, &app, 8.0)?;
    // The window goes to the tray or is minimized: the page learns it through `visibilitychange`.
    app.page.eval("Object.defineProperty(document, 'hidden', { configurable: true, get: () => true }); document.dispatchEvent(new Event('visibilitychange')); 1")?;
    pause(2.0);
    let state = life(&mut app)?;
    ensure(state["paused"] == true && still(&state), || format!("a hidden window still moves: {state}"))?;
    let hidden = cpu(ctx, &app, 8.0)?;
    app.page.eval("Reflect.deleteProperty(document, 'hidden'); document.dispatchEvent(new Event('visibilitychange')); 1")?;
    pause(1.0);
    let back = life(&mut app)?;
    ensure(back["paused"] == false && back["ghost"] == true && back["running"].as_u64().unwrap_or(0) > 0, || {
        format!("a shown window stays still: {back}")
    })?;
    system_motion(&mut app.page, true)?;
    pause(2.0);
    let reduced = cpu(ctx, &app, 8.0)?;
    ctx.notes.push(format!(
        "idle CPU, percent of one core: Chat home {chat:.1}, Projects home {project:.1}, hidden {hidden:.1}, reduced motion {reduced:.1}"
    ));
    // A hidden window must not cost more than a still one, give or take the noise of the measurement.
    ensure(hidden <= reduced + 3.0, || format!("hidden {hidden:.1}% against still {reduced:.1}%"))
}

/// What the safety rows look like at the owner's common window size: every choice and sentence reads in a few clean lines.
const ROWS: &str = r#"(() => {
  const lines = (el) => Math.round(el.getBoundingClientRect().height / parseFloat(getComputedStyle(el).lineHeight || '20'));
  const rows = [...document.querySelectorAll('.setup .seg-ctl button, .setup .note, .setup .tg')].map((el) => ({ text: el.textContent.trim().slice(0, 40), lines: lines(el) }));
  const meta = document.querySelector('.p-meta');
  return { rows, meta: meta ? meta.textContent.trim() : '', title: meta?.querySelector('span')?.title ?? '', scrolls: document.querySelector('.projects-stage')?.scrollHeight > document.querySelector('.projects-stage')?.clientHeight };
})()"#;

fn plain_rows_at_1280(ctx: &mut Ctx) -> Result {
    let mut app = start(ctx)?;
    app.page
        .cdp
        .call("Emulation.setDeviceMetricsOverride", json!({ "width": 1280, "height": 800, "deviceScaleFactor": 1, "mobile": false }))?;
    system_motion(&mut app.page, false)?;
    app.page.wait_visible(&css(".hello"))?;
    pause(1.0);
    ctx.shot(&mut app, "homes-chat-1280")?;
    open_project(&mut app)?;
    pause(1.0);
    let state = app.page.eval(ROWS)?;
    for row in state["rows"].as_array().cloned().unwrap_or_default() {
        ensure(row["lines"].as_u64().unwrap_or(9) <= 2, || format!("a row wraps awkwardly at 1280x800: {row}"))?;
    }
    let meta = state["meta"].as_str().unwrap_or_default();
    ensure(!meta.contains("Fixture") && !meta.contains('<') && meta.split_whitespace().count() <= 6, || {
        format!("the home shows raw repository facts: {meta}")
    })?;
    ensure(state["title"].as_str().unwrap_or_default().len() > meta.len(), || format!("the full path is on hover: {state}"))?;
    ctx.shot(&mut app, "homes-project-1280")?;
    app.page.click(&css(".setup-more summary"))?;
    pause(0.4);
    ctx.shot(&mut app, "homes-project-1280-details")
}
