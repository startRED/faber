//! A real process restart keeps an old reading point; no AI is launched or sent a message.

use super::{Result, Scenario, ensure, eventually, no_cli, pause, same};
use crate::app::Ctx;
use crate::page::{Page, css};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const NAME: &str = "reading: a real restart preserves an old reading point within the memory budget";
pub(super) const PROJECT_NAME: &str = "reading: a real Projects restart preserves an old reading point within the memory budget";
const OFFSET: &str = "(() => { const e=document.querySelector('[data-message-index=\"7\"]'); const t=document.querySelector('.thread'); return e ? e.getBoundingClientRect().top-t.getBoundingClientRect().top : null; })()";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: restart }, Scenario { name: PROJECT_NAME, run: restart_project }]
}

fn memory(ctx: &mut Ctx, id: usize, peak: &mut f64) -> Result {
    let (mb, webview) = ctx.memory_mb(id)?;
    ensure(webview, || "the reading fixture's memory sample must include WebView2".into())?;
    *peak = peak.max(mb);
    ensure(*peak <= 400.0, || format!("160-message reading fixture sampled peak {peak:.1} MB exceeds 400 MB"))
}

fn at_point(page: &mut Page, offset: f64) -> Result {
    page.wait_for("message 7 at the saved viewport offset", &format!("Math.abs(({OFFSET})-({offset})) < 3 && ({OFFSET}) !== null"))
}

fn restart(ctx: &mut Ctx) -> Result {
    restart_area(ctx, false)
}

fn restart_project(ctx: &mut Ctx) -> Result {
    restart_area(ctx, true)
}

fn restart_area(ctx: &mut Ctx, project: bool) -> Result {
    let shot_prefix = if project { "project-reading" } else { "reading" };
    ctx.write_settings(&json!({ "language":"en", "setup":"done", "tutorialDone":true,
        "appearance": {"base":"sala", "motion":"none", "effects":{"ambient":false,"texture":false,"glow":false}} }))?;
    let mut conversation = json!({ "id":"reading-native", "title":"Synthetic reading fixture", "area":"chat",
        "createdAt":1, "updatedAt":2, "sessions":{}, "queued":[],
        "messages": (0..160).map(|i| json!({"id":format!("reading-{i}"), "role":if i%2==0 {"user"} else {"assistant"},
            "at":i, "status":"done", "text":format!("{} Synthetic message {i}. {}",
                if i==7 {"Albatrossanchor"} else {"Reading"}, "A calm paragraph for this synthetic reading fixture. ".repeat(12))})).collect::<Vec<_>>() });
    if project {
        super::shell::seed_project(ctx, "Reading project")?;
        conversation["area"] = json!("project");
        conversation["projectId"] = json!("demo");
    }
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join("reading-native.json");
    std::fs::write(&file, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    super::shell::set_size(&mut app.page, 1280, 800)?;
    let start_cpu = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    let mut peak = 0.0;
    memory(ctx, app.id, &mut peak)?;
    if project {
        let projects = app.page.named("button", "area.projects")?;
        app.page.click(&projects)?;
        app.page.click(&css(".prow .item").has_text("Reading project"))?;
        app.page.click(&css(".proj-chats .item").has_text("Synthetic reading fixture"))?;
    } else {
        app.page.click(&css(".crow .item").has_text("Synthetic reading fixture"))?;
    }
    app.page.wait_visible(&css("[data-message-index=\"159\"]"))?;
    app.page.fill(&css(".search input"), "Albatrossanchor")?;
    app.page.wait_visible(&css(".message-hit"))?;
    app.page.click(&css(".message-hit"))?;
    app.page.wait_visible(&css("[data-message-index=\"7\"]"))?;
    app.page.eval(&format!(r#"(() => {{const t=document.querySelector('.thread');t.scrollTop+=({OFFSET})+24;t.dispatchEvent(new Event('scroll'));return true;}})()"#))?;
    at_point(&mut app.page, -24.0)?;
    app.page.wait_for(
        "the old real message id and offset to reach browser storage",
        r#"(() => {
      const p=JSON.parse(localStorage.getItem('faber.reading')||'[]').find(([id])=>id==='reading-native')?.[1];
      return p?.id==='reading-7' && Math.abs(p.offset+24)<3;
    })()"#,
    )?;
    ctx.shot(&mut app, &format!("{shot_prefix}-before-restart-1280"))?;
    memory(ctx, app.id, &mut peak)?;
    let first_cpu = ctx.cpu_ms(app.id)? - start_cpu;
    let first_ms = started.elapsed().as_secs_f64() * 1000.0;
    let first_pid = app.pid;
    ctx.close(app.id)?;
    eventually("the first reading app process to exit", Duration::from_secs(10), || ctx.exited(app.id).is_some())?;
    // Include WebView2: the next launch must read the closed profile, not reuse a warm browser process.
    ctx.audit_within(Duration::from_secs(15))?;

    conversation["messages"][5]["text"] = json!("Earlier content grew while the app was closed.\n\n".repeat(50));
    std::fs::write(&file, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    ensure(app.pid != first_pid, || "the second launch must be a different native process".into())?;
    super::shell::set_size(&mut app.page, 1280, 800)?;
    let second_cpu = ctx.cpu_ms(app.id)?;
    let second = Instant::now();
    if project {
        app.page.wait_for(
            "the same project conversation to reopen in Projects",
            "(() => {const p=JSON.parse(localStorage.getItem('faber.place')||'{}');return p.area==='project'&&p.projectId==='demo'&&p.id==='reading-native';})()",
        )?;
    }
    at_point(&mut app.page, -24.0)?;
    app.page.wait_visible(&css(".toend"))?;
    ctx.shot(&mut app, &format!("{shot_prefix}-after-restart-1280"))?;
    memory(ctx, app.id, &mut peak)?;
    super::shell::set_size(&mut app.page, 720, 800)?;
    at_point(&mut app.page, -24.0)?;
    ctx.shot(&mut app, &format!("{shot_prefix}-after-restart-720"))?;
    memory(ctx, app.id, &mut peak)?;

    let before = app.page.eval(OFFSET)?.as_f64().ok_or("the restored point is missing")?;
    app.page.eval("(() => {window.readingWheelDone=false;document.querySelector('.thread').addEventListener('scrollend',()=>{window.readingWheelDone=true},{once:true});return true;})()")?;
    let pointer = app.page.eval(
        "(() => {const r=document.querySelector('.thread').getBoundingClientRect();return {x:r.left+r.width/2,y:r.top+r.height/2};})()",
    )?;
    app.page
        .cdp
        .call("Input.dispatchMouseEvent", json!({"type":"mouseWheel", "x":pointer["x"], "y":pointer["y"], "deltaX":0,"deltaY":160}))?;
    app.page.wait_for("a real wheel gesture to move the reading point", &format!("({OFFSET}) < ({before})-40"))?;
    app.page.wait_for("the native wheel gesture to finish", "window.readingWheelDone===true")?;
    let moved = app.page.eval(OFFSET)?.as_f64().ok_or("message 7 disappeared during the wheel gesture")?;
    pause(0.3);
    same(
        app.page.eval(OFFSET)?.as_f64().map(|n| (n - moved).abs() < 3.0),
        Some(true),
        "the resize observer does not undo a manual scroll",
    )?;
    app.page.click(&css(".toend"))?;
    app.page.wait_for(
        "Jump to latest to reach the actual final message",
        r#"(() => {
      const e=document.querySelector('[data-message-index="159"]'),t=document.querySelector('.thread');
      return !!e && e.getBoundingClientRect().bottom<=t.getBoundingClientRect().bottom && t.scrollHeight-t.scrollTop-t.clientHeight<4;
    })()"#,
    )?;
    app.page.wait_for(
        "reaching the tail to forget the old reading point",
        "!JSON.parse(localStorage.getItem('faber.reading')||'[]').some(([id])=>id==='reading-native')",
    )?;
    memory(ctx, app.id, &mut peak)?;
    let cpu = first_cpu + ctx.cpu_ms(app.id)? - second_cpu;
    let wall = first_ms + second.elapsed().as_secs_f64() * 1000.0;
    let disk: Value = serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    same(disk, conversation, "reading, relaunching and scrolling never write the seeded conversation")?;
    same(ctx.records()?.len(), 0, "the reading scenario never launches or sends to an AI")?;
    let area = if project { "Projects" } else { "Chat" };
    ctx.notes.push(format!("{area} reading fixture160 only, two real app processes {first_pid}/{}: sampled tree peak {peak:.1} MB <=400 MB; post-launch scripted flow {cpu:.0} ms CPU over {wall:.0} ms ({:.1}% of one core), including navigation, scrolling and screenshots; not isolated anchor overhead or a latency benchmark", app.pid, cpu/wall*100.0));
    ctx.close(app.id)
}
