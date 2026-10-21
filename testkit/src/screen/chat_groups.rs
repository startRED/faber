//! Chat folders are made through the screen; their copied knowledge stays bounded and survives a reload.

use super::{Result, Scenario, ask, claude_chat, ensure, eventually, no_cli, pause, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const NAME: &str = "Sky notes";
const INSTRUCTIONS: &str = "Explain the sky in short, plain sentences.";
const QUESTION: &str = "In one sentence: why is the sky blue?";
const BUDGET_MB: f64 = 400.0;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "chat groups: make a folder, write its instructions and start a chat inside it", run: create_and_chat },
        Scenario { name: "chat groups: ten knowledge files survive reopening within the memory budget", run: files_memory },
        Scenario { name: "chat groups: narrow page shares the modal running panel's grid area and becomes inert", run: narrow_modal },
    ]
}

fn create(page: &mut Page) -> Result {
    let add = css(".chat-groups [data-action=new-group]");
    page.wait_visible(&add)?;
    page.click(&add)?;
    let name = css(".chat-groups input.rename");
    page.wait_visible(&name)?;
    page.fill(&name, NAME)?;
    page.press_on(&name, "Enter")?;
    page.wait_visible(&css(".chat-group-page"))?;
    page.fill(&css("[data-testid=group-instructions]"), INSTRUCTIONS)?;
    page.wait_enabled(&css("[data-action=new-group-chat]"))
}

fn saved(ctx: &Ctx) -> Result<Value> {
    let bytes = std::fs::read(ctx.sb.data.join("chat-groups.json")).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn narrow_modal(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    create(&mut app.page)?;
    app.page.eval(r#"(() => {
      const data = new DataTransfer();
      data.items.add(new File(['Group notes'], 'notes.txt', {type: 'text/plain'}));
      document.querySelector('.chat-group-page').dispatchEvent(new DragEvent('drop', {dataTransfer: data, bubbles: true, cancelable: true}));
      return true;
    })()"#)?;
    app.page.wait_count(&css(".group-files li"), 1)?;
    app.page.wait_enabled(&css("[data-action=new-group-chat]"))?;
    super::shell::set_size(&mut app.page, 720, 600)?;
    app.page.click(&css(".run-btn"))?;
    app.page.wait_visible(&css(".work > .runcol[aria-modal=true]"))?;
    let layout = app.page.eval(
        r#"(() => {
      const page = document.querySelector('.chat-group-page');
      const panel = document.querySelector('.work > .runcol');
      const work = document.querySelector('.work').getBoundingClientRect();
      const box = page.getBoundingClientRect(), overlay = panel.getBoundingClientRect();
      const grid = getComputedStyle(page);
      return {width: innerWidth, row: grid.gridRowStart, column: grid.gridColumnStart,
        sameTop: Math.abs(box.top - overlay.top) < 1, fillsWork: overlay.height >= work.height - 1};
    })()"#,
    )?;
    same(layout["width"].as_u64(), Some(720), "the real viewport is 720 px wide")?;
    same(layout["row"].as_str(), Some("1"), "the group page stays in the stage's first grid row")?;
    same(layout["column"].as_str(), Some("1"), "the group page stays in the stage's first grid column")?;
    ensure(layout["sameTop"] == true && layout["fillsWork"] == true, || {
        format!("the overlay and page must share the full work area: {layout}")
    })?;
    let blocked = app.page.eval(r#"(() => {
      const page = document.querySelector('.chat-group-page');
      const panel = document.querySelector('.work > .runcol');
      const controls = [page.querySelector('textarea'), page.querySelector('.group-files button'), page.querySelector('[data-action=new-group-chat]')];
      return page.inert && controls.every(el => {
        el.focus();
        return el.closest('[inert]') === page && document.activeElement !== el;
      }) && panel.contains(document.activeElement);
    })()"#)?;
    ensure(blocked == true, || "instructions, file removal and New chat here must be inert behind the modal panel".into())?;
    ctx.shot(&mut app, "chat-group-narrow-modal")?;
    app.page.press("Escape")?;
    app.page.wait_gone(&css(".work > .runcol"))?;
    let restored = app.page.eval(
        r#"(() => {
      const page = document.querySelector('.chat-group-page');
      const field = page.querySelector('textarea');
      field.focus();
      return !page.inert && document.activeElement === field;
    })()"#,
    )?;
    ensure(restored == true, || "closing the panel must restore the group page's controls".into())
}

fn create_and_chat(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    create(&mut app.page)?;
    eventually("the group's instructions to be saved", Duration::from_secs(5), || {
        saved(ctx).is_ok_and(|groups| groups[0]["instructions"] == INSTRUCTIONS)
    })?;
    app.page.click(&css("[data-action=new-group-chat]"))?;
    app.page.wait_gone(&css(".chat-group-page"))?;
    ask(&mut app.page, QUESTION)?;
    app.page.wait_visible(&css(".chat-groups .crow .item").has_text(QUESTION))?;
    let answer = app.page.named("article", "answer.label")?;
    app.page.wait_visible(&answer.clone().css(".actions"))?;
    app.page.wait_for("the group's answer", "document.querySelector('article.ai .md')?.innerText.includes('Rayleigh')")?;
    let groups = saved(ctx)?;
    same(groups[0]["conversationIds"].as_array().map(Vec::len), Some(1), "the chat belongs to its folder")?;
    app.page.click(&css(".chat-groups .prow .item").has_text(NAME))?;
    app.page.wait_visible(&css(".chat-group-page"))?;
    same(
        app.page.eval("document.querySelector('[data-testid=group-instructions]').value")?,
        json!(INSTRUCTIONS),
        "the group's own words after starting a chat",
    )?;
    ctx.shot(&mut app, "chat-group-instructions")
}

fn files_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    create(&mut app.page)?;
    let (baseline, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the memory reading must include WebView2".into())?;
    // Native file selection uses ten real sandbox files, exactly the group's 5 MB total limit.
    let contents = "k".repeat(500_000);
    let mut sources = Vec::new();
    for n in 0..10 {
        let path = ctx.sb.sandbox.join(format!("knowledge-{n:02}.txt"));
        std::fs::write(&path, &contents).map_err(|e| e.to_string())?;
        sources.push(path);
    }
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    let dom = app.page.cdp.call("DOM.getDocument", json!({}))?;
    let input = app
        .page
        .cdp
        .call("DOM.querySelector", json!({ "nodeId": dom["root"]["nodeId"], "selector": ".chat-group-page input[type=file]" }))?;
    app.page.cdp.call("DOM.setFileInputFiles", json!({ "nodeId": input["nodeId"], "files": sources }))?;
    app.page.wait_count(&css(".group-files li"), 10)?;
    app.page.wait_enabled(&css("[data-action=new-group-chat]"))?;
    let added = ctx.memory_mb(app.id)?.0;
    let add_cpu = ctx.cpu_ms(app.id)? - cpu_before;
    let add_ms = started.elapsed().as_secs_f64() * 1000.0;

    let groups = saved(ctx)?;
    let group = &groups[0];
    let files = group["files"].as_array().ok_or("the group has no file list")?;
    same(files.len(), 10, "all ten files are saved")?;
    same(files.iter().filter_map(|f| f["size"].as_u64()).sum::<u64>(), 5_000_000, "the saved files' total bytes")?;
    for source in sources {
        std::fs::remove_file(source).map_err(|e| e.to_string())?;
    }
    let group_dir = ctx.sb.data.join("chat-groups").join(group["id"].as_str().ok_or("the group has no id")?);
    for file in files {
        let path = group_dir.join(file["id"].as_str().ok_or("a knowledge file has no id")?);
        let copy = std::fs::read(path).map_err(|e| e.to_string())?;
        ensure(copy == contents.as_bytes(), || "a knowledge copy changed after its source was removed".into())?;
    }
    app.page.reload()?;
    app.page.click(&css(".chat-groups .prow .item").has_text(NAME))?;
    app.page.wait_count(&css(".group-files li"), 10)?;
    same(
        app.page.eval("document.querySelector('[data-testid=group-instructions]').value")?,
        json!(INSTRUCTIONS),
        "instructions survive reloading with the files",
    )?;
    pause(2.0);
    let reopened = ctx.memory_mb(app.id)?.0;
    let idle_before = ctx.cpu_seconds(app.id)?;
    let idle = Instant::now();
    pause(5.0);
    let idle_cpu = (ctx.cpu_seconds(app.id)? - idle_before) / idle.elapsed().as_secs_f64() * 100.0;
    let notes = format!(
        "group 10 files / 5 MB: before {baseline:.1} MB, after add {added:.1} MB, reopened {reopened:.1} MB; add {add_cpu:.0} ms CPU over {add_ms:.0} ms; idle {idle_cpu:.1}% of one core"
    );
    ctx.notes.push(notes.clone());
    ensure(added.max(reopened) <= BUDGET_MB, || format!("{notes}; over the {BUDGET_MB} MB budget"))
}
