//! Retained helper history and controls through synthetic fixtures and fake CLIs; no real account or model request.

use super::{Result, Scenario, ask, ensure, no_cli, same};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::json;
use std::time::{Duration, Instant};

pub const NAME: &str = "subagent history: nested sessions navigate, reopen and stay within the memory budget";
const CONVERSATION: &str = "5f1c0000-0000-4000-8000-0000000000e2";
const PAGES: u64 = 10;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "subagent history: baseline-compatible helper preview screenshot", run: baseline_preview },
        Scenario { name: NAME, run: paged_nested_history },
        Scenario { name: "subagent history: project conversation opens parent and nested retained pages", run: project_history },
        Scenario {
            name: "subagent history: a corrupt retained page recovers through keyboard Retry without reopening",
            run: retry_history,
        },
        Scenario {
            name: "helper controls: Send reaches its recorded session and Continue preserves the conversation draft at 720 px",
            run: helper_720,
        },
        Scenario {
            name: "helper controls: Send reaches its recorded session and Continue preserves the conversation draft at 1280 px",
            run: helper_1280,
        },
    ]
}

fn helper_720(ctx: &mut Ctx) -> Result {
    helper_controls(ctx, 720)
}

fn helper_1280(ctx: &mut Ctx) -> Result {
    helper_controls(ctx, 1280)
}

fn helper_controls(ctx: &mut Ctx, width: u32) -> Result {
    ctx.write_settings(&json!({"language":"en", "setup":"done", "tutorialDone":true,
        "permissions":"dontAsk", "appearance":{"base":"sala", "motion":"none"}}))?;
    super::shell::seed_project(ctx, "Helper control proof")?;
    let setup = ctx.sb.sandbox.join("helper.setup.json");
    std::fs::write(
        &setup,
        json!({"vendor":"codex", "version":"0.162.1",
        "chat":ctx.paths.fixtures.join("codex/helper-told.jsonl"), "models":ctx.paths.fixtures.join("codex/models.jsonl")})
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    // This marker belongs to the fake setup protocol, contains no account information and is confined to the sandbox.
    std::fs::write(ctx.sb.state.join("codex.signed-in"), "true").map_err(|e| e.to_string())?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Fake,
        claude_recording: Some(ctx.paths.fixtures.join("claude/helper-call.jsonl")),
        codex_recording: Some(setup),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Helper control proof"))?;
    page.wait_for("the synthetic helper subscription and models", "window.__TAURI_INTERNALS__.invoke('state').then(s=>s.models.some(m=>m.id==='gpt-6.1-sol') && s.usage?.some(u=>u.vendor==='codex' && !u.billingUnknown && !u.billedPerUse))")?;
    super::select_model(page, "Claude Haiku")?;
    ask(page, "Call the helper tool once with title \"Check\" and brief \"What is the answer?\". Then tell me in one line what it said.")?;
    let helper = css("article .agents .agent").has_text("Check config");
    page.wait_visible(&helper)?;
    let crumb = page.inner_text(&css(".crumb b"))?;
    let draft = "Keep this conversation's draft.";
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, draft)?;
    page.click(&helper)?;
    let helper_box = css("#running .side-write textarea");
    page.wait_visible(&helper_box)?;
    super::shell::set_size(page, width, 800)?;
    const TOLD: &str = "Also say whether retries above 5 would be risky.";
    page.fill(&helper_box, TOLD)?;
    page.press_on(&helper_box, "Tab")?;
    page.wait_for(
        "Send to have visible keyboard focus",
        "document.activeElement?.matches('#running .side-write button[type=submit]:focus-visible')",
    )?;
    ctx.shot(&mut app, &format!("helper-send-focused-{width}"))?;
    app.page.press("Enter")?;
    let taken = app.page.text_of("agent.helper.taken")?;
    app.page.wait_visible(&css("#running .side-told .you").has_text(TOLD))?;
    app.page.wait_visible(&css("#running .side-told p").has_text(&taken))?;
    same(app.page.count(&css("#running .side-told"))?, 1, "the helper took the person's message once")?;
    // This distinct recorded result is emitted only after the helper's turn/steer input matches the person's message.
    app.page.wait_visible(&css("#running .side-body").has_text("Retries acima de 5"))?;
    app.page.wait_gone(&css("#running .side-write"))?;
    same(app.page.count(&css("#running .agent-pages"))?, 0, "a short streamed report fits one reader window instead of one page per word")?;
    let next = app.page.named("button", "agent.continue")?.exact();
    app.page.wait_visible(&next)?;
    app.page.press_on(&next, "Tab")?;
    app.page.press("Shift+Tab")?;
    let continue_label = app.page.text_of("agent.continue")?;
    app.page.wait_for(
        "Continue to have visible keyboard focus",
        &format!(
            "document.activeElement?.matches('#running .side-tell button:focus-visible') && document.activeElement.textContent==={}",
            json!(continue_label)
        ),
    )?;
    ctx.shot(&mut app, &format!("helper-continue-focused-{width}"))?;
    app.page.press("Enter")?;
    let expected = app.page.text_with("agent.continueAbout", &json!({"title":"Check config"}), None)? + draft;
    app.page.wait_for(
        "Continue to preserve and focus the same conversation's draft",
        &format!("document.activeElement?.matches('.composer textarea') && document.activeElement.value==={}", json!(expected)),
    )?;
    same(app.page.inner_text(&css(".crumb b"))?, crumb, "Continue stays in the helper's own conversation")?;
    let records = ctx.records()?;
    ensure(records.iter().any(|r| r.vendor == "codex" && r.args.iter().any(|a| a == "app-server")), || {
        "the helper replay never started its own Codex process".into()
    })?;
    ctx.notes.push(format!("{width}px emulated viewport: keyboard Send reached the independent recorded helper session and its distinct result; Continue preserved the unsent draft in the same conversation. No follow-up message was sent."));
    ctx.close(app.id)
}

fn seed(ctx: &Ctx) -> Result {
    ctx.write_settings(&json!({
        "language": "en",
        "setup": "done",
        "tutorialDone": true,
        "appearance": { "base": "sala", "motion": "none", "effects": {"ambient": false, "texture": false, "glow": false} }
    }))?;
    let tools = json!([
        {
            "id": "parent-call", "name": "Agent", "ok": true,
            "input": { "description": "Review the synthetic history", "subagent_type": "Explore", "prompt": "Read the synthetic fixture only." },
            "output": "Subagent hand-back: Parent final report.",
            "started": 1_790_000_000_010u64, "ended": 1_790_000_000_020u64,
            "words": [{"at":0,"text":"A short legacy preview before the paged transcript is opened."}],
            "transcript": {"pages": PAGES, "failed": false},
            "facts": {"checks":0,"failed":0}
        },
        {
            "id": "child-call", "name": "Agent", "parent": "parent-call", "ok": true,
            "input": { "description": "Inspect the nested history", "subagent_type": "Explore", "prompt": "Read the child history fixture only." },
            "output": "Subagent hand-back: Child final report.",
            "started": 1_790_000_000_030u64, "ended": 1_790_000_000_040u64,
            "words": [{"at":0,"text":"A short child preview."}],
            "transcript": {"pages": PAGES, "failed": false},
            "facts": {"checks":0,"failed":0}
        }
    ]);
    let conversation = json!({
        "id": CONVERSATION, "title": "Synthetic nested history", "area": "chat",
        "createdAt": 1_790_000_000_000u64, "updatedAt": 1_790_000_000_050u64,
        "sessions": {}, "queued": [],
        "messages": [
            {"id":"history-question","role":"user","text":"Inspect the synthetic nested history.","at":1_790_000_000_000u64},
            {"id":"history-answer","role":"assistant","text":"The fixture has a parent helper and a nested helper.","at":1_790_000_000_050u64,
             "vendor":"claude","model":"synthetic","status":"done","tools":tools}
        ]
    });
    let conversations = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&conversations).map_err(|e| e.to_string())?;
    std::fs::write(conversations.join(format!("{CONVERSATION}.json")), conversation.to_string()).map_err(|e| e.to_string())?;

    for (call, directory) in [
        ("parent-call", "01a6c0e4a07bb987aa9be7151cef03163104788687b1ce5a5c05f4d6345ff87a"),
        ("child-call", "eb801554942f00daa2f1a3d2888c6cbbab86d35ce27c86be3e38d1fe5b9f5a97"),
    ] {
        let call_dir = ctx.sb.data.join("child-transcripts").join(CONVERSATION).join(directory);
        std::fs::create_dir_all(&call_dir).map_err(|e| e.to_string())?;
        for page in 0..PAGES {
            let mut entries = vec![json!({
                "at": page, "kind": "text",
                "text": format!("{} history page {:02}: synthetic archived detail for pagination and restart checks.", if call == "parent-call" {"Parent"} else {"Child"}, page + 1)
            })];
            // Three ~6.8 KiB paragraphs make each page about 20 KiB; eight visible pages exercise a realistic long reader
            // without turning the fixture into an unbounded DOM or claiming a measured memory cost.
            let paragraph = "A synthetic archived paragraph stays readable when a person moves between helper sessions. ";
            for _ in 0..3 {
                entries.push(json!({ "at": page, "kind": "text", "text": paragraph.repeat(76) }));
            }
            if call == "parent-call" && page == 0 {
                entries.push(json!({"at":0,"kind":"tool","tool":"child-call","text":""}));
            }
            if page == PAGES - 1 {
                entries.push(json!({"at":page,"kind":"result","tool":call,"text":format!("Subagent hand-back: {} final report.", if call == "parent-call" {"Parent"} else {"Child"})}));
            }
            let path = call_dir.join(format!("{page:020}.json"));
            std::fs::write(path, json!({"version":1,"entries":entries}).to_string()).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn open_parent(ctx: &mut Ctx, wait_for_history: bool) -> Result<crate::app::Opened> {
    seed(ctx)?;
    let mut app = ctx.launch(no_cli())?;
    super::shell::set_size(&mut app.page, 1280, 800)?;
    app.page.click(&css(".side .crow .item"))?;
    app.page.wait_visible(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    app.page.click(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    app.page.wait_visible(&css("#running .side-body"))?;
    if wait_for_history {
        app.page.wait_visible(&css("#running .agent-pages"))?;
        app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 10"))?;
    } else {
        app.page.wait_for(
            "the same seeded helper preview to render on old and new builds",
            "(() => {const s=document.querySelector('#running .side-body')?.innerText||'';return s.includes('Parent history page 10')||s.includes('A short legacy preview');})()",
        )?;
    }
    Ok(app)
}

fn project_history(ctx: &mut Ctx) -> Result {
    seed(ctx)?;
    super::shell::seed_project(ctx, "History project")?;
    let path = ctx.sb.data.join("conversations").join(format!("{CONVERSATION}.json"));
    let mut conversation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    conversation["area"] = json!("project");
    conversation["projectId"] = json!("demo");
    std::fs::write(path, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("History project"))?;
    app.page.click(&css(".proj-chats .item").has_text("Synthetic nested history"))?;
    app.page.click(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 10"))?;
    app.page.click(&css("#running [data-history=older]"))?;
    app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 01"))?;
    app.page.click(&css("#running .agent-handoff").has_text("Inspect the nested history"))?;
    app.page.wait_visible(&css("#running .side-said").has_text("Child history page 10"))?;
    app.page.press_on(&css("#running [data-history=older]"), "Enter")?;
    app.page.wait_visible(&css("#running .side-said").has_text("Child history page 01"))?;
    ctx.shot(&mut app, "agent-history-project-child-1280")?;
    app.page.press_on(&css("#running .back"), "Enter")?;
    app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 10"))?;
    ensure(ctx.records()?.is_empty(), || "the project history fixture started a CLI".into())?;
    ctx.close(app.id)
}

fn retry_history(ctx: &mut Ctx) -> Result {
    seed(ctx)?;
    let page_path = ctx
        .sb
        .data
        .join("child-transcripts")
        .join(CONVERSATION)
        .join("01a6c0e4a07bb987aa9be7151cef03163104788687b1ce5a5c05f4d6345ff87a")
        .join("00000000000000000009.json");
    let original = std::fs::read(&page_path).map_err(|e| e.to_string())?;
    // Corrupt only this scenario's owned fixture; the real reader must surface the store's decoding failure.
    std::fs::write(&page_path, b"{").map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    super::shell::set_size(&mut app.page, 1280, 800)?;
    app.page.click(&css(".side .crow .item"))?;
    app.page.click(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    let error = app.page.text_of("error.store.corrupt")?;
    let alert = css("#running .side-body [role=alert]").has_text(&error);
    app.page.wait_visible(&alert)?;
    let retry = css("#running [data-history=retry]");
    app.page.wait_visible(&retry)?;
    let recovered = css("#running .side-said").has_text("Parent history page 10");
    ensure(!app.page.is_visible(&recovered)?, || "the corrupt page was shown before recovery".into())?;
    ctx.shot(&mut app, "agent-history-corrupt-page-error")?;
    std::fs::write(&page_path, &original).map_err(|e| e.to_string())?;
    app.page.press_on(&retry, "Enter")?;
    app.page.wait_visible(&recovered)?;
    app.page.wait_gone(&alert)?;
    app.page.wait_gone(&retry)?;
    app.page.wait_visible(&css("#running .side-head h2").has_text("Review the synthetic history"))?;
    ensure(ctx.records()?.is_empty(), || "retrying retained history started a CLI".into())?;
    ctx.shot(&mut app, "agent-history-corrupt-page-recovered")?;
    ctx.notes.push(
        "The real store rejected one corrupt synthetic page; keyboard Retry read the restored bytes in the same open parent reader.".into(),
    );
    ctx.close(app.id)
}

fn baseline_preview(ctx: &mut Ctx) -> Result {
    let mut app = open_parent(ctx, false)?;
    // This checkpoint intentionally makes no assertion about paged-history controls.
    ctx.shot(&mut app, "agent-history-baseline-preview-1280")
}

fn memory(ctx: &mut Ctx, id: usize, peak: &mut f64) -> Result {
    let (mb, webview) = ctx.memory_mb(id)?;
    ensure(webview, || "the helper-history sample must include WebView2".into())?;
    *peak = peak.max(mb);
    ensure(*peak <= 400.0, || format!("synthetic helper history sampled peak {peak:.1} MB exceeds 400 MB"))
}

fn paged_nested_history(ctx: &mut Ctx) -> Result {
    let mut app = open_parent(ctx, true)?;
    let started = Instant::now();
    let first_cpu_start = ctx.cpu_ms(app.id)?;
    let mut peak = 0.0;
    memory(ctx, app.id, &mut peak)?;
    let older = css("#running [data-history=older]");
    app.page.wait_enabled(&older)?;
    app.page.click(&older)?;
    app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 01"))?;
    app.page.wait_visible(&css("#running .agent-handoff").has_text("Inspect the nested history"))?;
    ensure(app.page.is_disabled(&older)?, || "the oldest page should disable Older".into())?;
    ctx.shot(&mut app, "agent-history-parent-oldest-1280")?;

    // The child session is independently clickable from its parent's transcript.
    app.page.click(&css("#running .agent-handoff").has_text("Inspect the nested history"))?;
    app.page.wait_visible(&css("#running .agent-pages"))?;
    app.page.wait_visible(&css("#running .side-said").has_text("Child history page 10"))?;
    let child_older = css("#running [data-history=older]");
    app.page.press_on(&child_older, "Enter")?;
    app.page.wait_visible(&css("#running .side-said").has_text("Child history page 01"))?;
    let later = css("#running [data-history=newer]");
    app.page.click(&later)?;
    app.page.wait_visible(&css("#running .side-said").has_text("Child history page 10"))?;
    let latest = css("#running [data-history=latest]");
    app.page.click(&latest)?;
    app.page.wait_gone(&latest)?;
    ctx.shot(&mut app, "agent-history-child-latest-1280")?;

    // Full-stage expand/collapse works from the keyboard too.
    let widen = css("#running .agent-wide");
    app.page.press_on(&widen, "Enter")?;
    app.page.wait_visible(&css(".runcol.wide"))?;
    ctx.shot(&mut app, "agent-history-child-full-1280")?;
    app.page.press_on(&widen, "Space")?;
    app.page.wait_gone(&css(".runcol.wide"))?;

    for (width, height) in [(720, 800), (1280, 800), (1920, 1080)] {
        super::shell::set_size(&mut app.page, width, height)?;
        app.page.wait_visible(&css("#running .side-said").has_text("Child history page 10"))?;
        ctx.shot(&mut app, &format!("agent-history-child-{width}"))?;
        memory(ctx, app.id, &mut peak)?;
    }

    // The back button follows the helper tree: child -> parent, which opens its latest page again.
    app.page.press_on(&css("#running .back"), "Enter")?;
    app.page.wait_visible(&css("#running .side-said").has_text("Parent history page 10"))?;
    app.page.wait_visible(&css("#running .agent-pages"))?;
    ctx.shot(&mut app, "agent-history-parent-after-child-1920")?;
    let first_cpu = ctx.cpu_ms(app.id)? - first_cpu_start;
    let first_wall = started.elapsed().as_secs_f64() * 1000.0;
    memory(ctx, app.id, &mut peak)?;
    ctx.close(app.id)?;
    super::eventually("the first history app process to exit", Duration::from_secs(10), || ctx.exited(app.id).is_some())?;
    ctx.audit_within(Duration::from_secs(15))?;

    // Restart from the same disk fixture: history pages remain available after the app and its WebView close.
    let mut reopened = ctx.launch(no_cli())?;
    let second_started = Instant::now();
    super::shell::set_size(&mut reopened.page, 1280, 800)?;
    let second_cpu_start = ctx.cpu_ms(reopened.id)?;
    reopened.page.click(&css(".side .crow .item"))?;
    reopened.page.wait_visible(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    reopened.page.click(&css("article .agents .agent").has_text("Review the synthetic history"))?;
    reopened.page.wait_visible(&css("#running .agent-pages"))?;
    reopened.page.wait_visible(&css("#running .side-said").has_text("Parent history page 10"))?;
    reopened.page.click(&css("#running [data-history=older]"))?;
    reopened.page.wait_visible(&css("#running .agent-handoff").has_text("Inspect the nested history"))?;
    reopened.page.click(&css("#running .agent-handoff").has_text("Inspect the nested history"))?;
    reopened.page.wait_visible(&css("#running .side-said").has_text("Child history page 10"))?;
    ctx.shot(&mut reopened, "agent-history-after-restart-1280")?;
    memory(ctx, reopened.id, &mut peak)?;
    let second_cpu = ctx.cpu_ms(reopened.id)? - second_cpu_start;
    let second_wall = second_started.elapsed().as_secs_f64() * 1000.0;
    ctx.notes.push(format!(
        "two hidden app processes reopened 20 synthetic immutable pages; tree peak {peak:.1} MB <=400 MB; first process UI flow {first_cpu:.0} ms CPU over {first_wall:.0} ms; restart check {second_cpu:.0} ms CPU over {second_wall:.0} ms; not isolated history-render cost or a latency benchmark"
    ));
    ctx.close(reopened.id)
}
