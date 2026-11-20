//! A saved wait expires with the native shell closed and resumes only after account confirmation.

use super::{Result, Scenario, ensure, eventually, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(super) const NAME: &str = "projects waits: a closed deadline is restored and account confirmation resumes exactly once";
const ID: &str = "5f1d0000-0000-4000-8000-000000000430";
const SESSION: &str = "5f1c0000-0000-4000-8000-000000000430";
const NOTE: &str = "Synthetic project work to check";
const REPLY: &str = "The saved project work was checked once.";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: NAME, run: restart },
        Scenario { name: "projects waits: an unconfirmed return leads to accounts or the existing draft", run: recover_visible_wait },
        Scenario { name: "projects waits: a vanished member and a recovered report have visible next steps", run: followed_members },
    ]
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

fn followed_members(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "pt-BR")?;
    let project = "Projeto com equipe";
    super::shell::seed_project(ctx, project)?;
    let leader = "5f1d0000-0000-4000-8000-000000000422";
    let member = "5f1d0000-0000-4000-8000-000000000423";
    let missing = "5f1d0000-0000-4000-8000-000000000424";
    let finished = "5f1d0000-0000-4000-8000-000000000425";
    let since = now() - 60_000;
    let folder = ctx.sb.data.join("conversations");
    let read = |id: &str| -> Result<Value> {
        let data = std::fs::read(folder.join(format!("{id}.json"))).map_err(|e| e.to_string())?;
        serde_json::from_slice(&data).map_err(|e| e.to_string())
    };
    let write = |id: &str, value: Value| -> Result {
        std::fs::write(folder.join(format!("{id}.json")), value.to_string()).map_err(|e| e.to_string())
    };
    for (id, title, child, until) in [
        (leader, "Equipe que terminou", finished, now() + 300_000),
        (ID, "Equipe que sumiu", missing, since + 1000),
        (member, "Equipe que ainda espera", "", now() + 300_000),
    ] {
        super::shell::seed(ctx, id, title, Some("demo"), since)?;
        let mut conversation = read(id)?;
        conversation["waitingOn"] = json!({"since":since, "until":until});
        if !child.is_empty() {
            conversation["waitingOn"]["ids"] = json!([child]);
        }
        write(id, conversation)?;
    }
    super::shell::seed(ctx, finished, "Conferência terminada", Some("demo"), since)?;
    let mut child = read(finished)?;
    child["startedBy"] = json!(leader);
    write(finished, child)?;

    let mut app = ctx.launch(Launch::default())?;
    open_wait_project(&mut app.page, project)?;
    let open = |page: &mut crate::page::Page, title| page.click(&css(".proj-chats .item").has_text(title));
    let healthy = css("[role=status].conversation-wait:not(.wait-attention)");
    let warning = css("[role=status].conversation-wait.wait-attention");
    let style = "return getComputedStyle(el).color;";
    open(&mut app.page, "Equipe que ainda espera")?;
    app.page.wait_visible(&healthy)?;
    let calm_color = app.page.eval_on(&healthy, style)?;
    open(&mut app.page, "Equipe que sumiu")?;
    let why = app.page.text_of("error.noSubscription")?;
    let refusal = app.page.text_with("wait.blocked", &json!({"why":why}), None)?;
    app.page.wait_visible(&warning.clone().has_text(&refusal))?;
    same(
        app.page.eval_on(&warning, "return el.textContent.trim();")?,
        json!(refusal),
        "the blocked return must not claim to be waiting for a vanished conversation",
    )?;
    ensure(app.page.eval_on(&warning, style)? != calm_color, || "a blocked wait must be styled distinctly from a healthy wait".into())?;
    let accounts = css("button.wait-message");
    let setup = app.page.named("heading", "setup.title")?;
    for (width, height) in [(1280, 800), (720, 800), (390, 844)] {
        super::shell::set_size(&mut app.page, width, height)?;
        app.page.wait_visible(&accounts)?;
        ctx.shot(&mut app, &format!("wait-vanished-{width}"))?;
    }
    app.page.press_on(&accounts, "Enter")?;
    app.page.wait_visible(&setup)?;
    let close = css(".fr-stage").role("button", &app.page.text_of("setup.close")?).exact();
    app.page.click(&close)?;
    app.page.wait_visible(&css("button.wait-message:focus"))?;

    super::shell::set_size(&mut app.page, 1280, 800)?;
    open(&mut app.page, "Equipe que terminou")?;
    let queued = css("p.woke.wait-attention").has_text(&app.page.text_of("wake.childQueued")?);
    let reconnect = queued.clone().role("button", &app.page.text_of("empty.noAiButton")?).exact();
    let remove = app.page.named("button", "queue.removeLabel")?;
    let send = app.page.named("button", "queue.sendNowLabel")?;
    app.page.wait_gone(&css("[role=status].conversation-wait"))?;
    for (width, height) in [(1280, 800), (720, 800), (390, 844)] {
        super::shell::set_size(&mut app.page, width, height)?;
        app.page.wait_visible(&reconnect)?;
        app.page.wait_visible(&remove)?;
        app.page.wait_gone(&send)?;
        ctx.shot(&mut app, &format!("wait-finished-member-{width}"))?;
        if width == 1280 {
            app.page.press_on(&reconnect, "Enter")?;
        } else {
            app.page.click(&reconnect)?;
        }
        app.page.wait_visible(&setup)?;
        ctx.shot(&mut app, &format!("wait-finished-accounts-{width}"))?;
        app.page.click(&close)?;
        app.page.wait_visible(&css("p.woke.wait-attention [data-restore]:focus"))?;
    }
    let blocked = read(ID)?;
    same(blocked["waitingOn"]["ids"].clone(), json!([missing]), "the lost child reference survives the admission refusal")?;
    same(blocked["waitingOn"]["problem"].clone(), json!("error.noSubscription"), "the lost child leaves a durable account blocker")?;
    let done = read(leader)?;
    same(done["waitingOn"].clone(), Value::Null, "the finished child cleared the stale wait")?;
    same(done["queued"].as_array().map(Vec::len), Some(1), "the recovered child report is held once")?;
    same(done["queued"][0]["id"].clone(), json!(format!("report-{finished}-{finished}-a")), "the same saved reply will resume the leader")?;
    for c in [blocked, done] {
        same(c["messages"].as_array().map(Vec::len), Some(2), "the disconnected app starts no extra turn")?;
    }
    ctx.close(app.id)?;
    ctx.audit_within(Duration::from_secs(15))?;
    Ok(())
}

fn recover_visible_wait(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "pt-BR")?;
    super::shell::seed_project(ctx, "Projeto de recuperação")?;
    let since = now() - 60_000;
    let active = "5f1d0000-0000-4000-8000-000000000421";
    for (id, title, wait) in [
        (active, "Esperando conferência", json!({"since":since,"until":now()+300_000,"note":"Conferência em andamento"})),
        (
            ID,
            "Retorno interrompido",
            json!({"since":since,"until":since+1000,"note":"Conferência da cópia","problem":"reason.wait.delivery"}),
        ),
    ] {
        super::shell::seed(ctx, id, title, Some("demo"), since)?;
        let file = ctx.sb.data.join("conversations").join(format!("{id}.json"));
        let mut conversation: Value =
            serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        conversation["waitingOn"] = wait;
        std::fs::write(file, conversation.to_string()).map_err(|e| e.to_string())?;
    }
    let mut disconnected = ctx.launch(Launch::default())?;
    open_wait_project(&mut disconnected.page, "Projeto de recuperação")?;
    disconnected.page.click(&css(".proj-chats .item").has_text("Retorno interrompido"))?;
    let connect = css("button.wait-message").has_text(&disconnected.page.text_of("empty.noAiButton")?);
    let setup = disconnected.page.named("heading", "setup.title")?;
    let close = css(".fr-stage").role("button", &disconnected.page.text_of("setup.close")?).exact();
    for (width, height) in [(1280, 800), (390, 844)] {
        super::shell::set_size(&mut disconnected.page, width, height)?;
        disconnected.page.wait_visible(&connect)?;
        ctx.shot(&mut disconnected, &format!("wait-disconnected-{width}"))?;
        if width == 1280 {
            disconnected.page.press_on(&connect, "Enter")?;
        } else {
            disconnected.page.click(&connect)?;
        }
        disconnected.page.wait_visible(&setup)?;
        disconnected.page.click(&close)?;
        disconnected.page.wait_visible(&css("button.wait-message:focus"))?;
    }
    ctx.close(disconnected.id)?;

    let launch = Launch { claude: CliSlot::Fake, recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")), ..Launch::default() };
    let mut app = ctx.launch(launch)?;
    open_wait_project(&mut app.page, "Projeto de recuperação")?;
    app.page.click(&css(".proj-chats .item").has_text("Esperando conferência"))?;
    app.page.wait_visible(&css("[role=status].conversation-wait:not(.wait-attention)"))?;
    ctx.shot(&mut app, "wait-healthy-desktop")?;
    app.page.click(&css(".proj-chats .item").has_text("Retorno interrompido"))?;
    app.page.wait_visible(&css("[role=status].conversation-wait.wait-attention"))?;
    let action = app.page.named("button", "wait.write")?;
    let draft = css(".composer textarea");
    app.page.fill(&draft, "Rascunho já escrito")?;
    for (width, height) in [(1280, 800), (390, 844)] {
        super::shell::set_size(&mut app.page, width, height)?;
        app.page.wait_visible(&action)?;
        ctx.shot(&mut app, &format!("wait-blocked-{width}"))?;
        app.page.press_on(&action, "Enter")?;
        same(
            app.page.eval("document.activeElement?.matches('.composer textarea') && document.activeElement.value")?,
            json!("Rascunho já escrito"),
            "the recovery action focuses the existing draft intact",
        )?;
    }
    super::shell::set_size(&mut app.page, 1280, 800)?;
    app.page.right_click(&css(&format!(".proj-chats .crow[data-conversation-id='{ID}']")))?;
    let archive = app.page.named("menuitem", "menu.archive")?;
    app.page.click(&archive)?;
    app.page.wait_visible(&css(".archived-bar"))?;
    app.page.wait_gone(&action)?;
    same(app.page.count(&css("[role=status].conversation-wait.wait-attention"))?, 0, "archived wait has no active warning")?;
    let saved = ctx.sb.data.join("conversations").join(format!("{ID}.json"));
    let conversation: Value = serde_json::from_slice(&std::fs::read(saved).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    same(conversation["messages"].as_array().map(Vec::len), Some(2), "focusing the draft did not start another turn")?;
    Ok(())
}

fn open_wait_project(page: &mut crate::page::Page, name: &str) -> Result {
    super::shell::set_size(page, 1280, 800)?;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    let project = css(".prow .item").has_text(name);
    page.wait_visible(&project)?;
    if page.eval_on(&project, "return { expanded: el.getAttribute('aria-expanded') === 'true' };")?["expanded"] != true {
        page.click(&project)?;
    }
    Ok(())
}

fn restart(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    super::shell::seed_project(ctx, "Wait recovery project")?;
    let setup = ctx.sb.sandbox.join("wait-restart.setup.json");
    let recording = "wait-restart.jsonl";
    std::fs::write(ctx.sb.sandbox.join(recording), include_str!("../../../tests/fixtures/claude/wait-restart.jsonl"))
        .map_err(|e| e.to_string())?;
    std::fs::write(&setup, json!({"vendor":"claude", "version":"2.1.0", "chat":recording}).to_string()).map_err(|e| e.to_string())?;
    let since = now();
    let until = since + 20_000;
    super::shell::seed(ctx, ID, "Wait recovery thread", Some("demo"), since - 10)?;
    let file = ctx.sb.data.join("conversations").join(format!("{ID}.json"));
    let saved =
        || -> Result<Value> { serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string()) };
    let wait = json!({"note":NOTE, "since":since, "until":until});
    let mut conversation = saved()?;
    let original = conversation["messages"].clone();
    conversation["waitingOn"] = wait.clone();
    conversation["sessions"] = json!({"claude":SESSION});
    conversation["choice"] = json!({"vendor":"claude", "model":"claude-haiku-5-5", "effort":"low"});
    std::fs::write(&file, conversation.to_string()).map_err(|e| e.to_string())?;

    let wait_state = format!(
        "(window.faberShell ?? window.__TAURI_INTERNALS__).invoke('state').then(s=>s.conversations.find(c=>c.id==='{ID}')?.waitingOn)"
    );
    let mut first = ctx.launch(Launch::default())?;
    same(first.page.eval(&wait_state)?, wait.clone(), "the original deadline loaded from disk")?;
    ctx.close(first.id)?;
    ensure(ctx.exited(first.id).is_some() && now() < until, || "the first native process must exit before the saved deadline".into())?;
    eventually("the deadline to pass while no app process exists", Duration::from_secs(20), || now() > until)?;
    same(saved()?["waitingOn"].clone(), wait, "the closed app made no change to the wait")?;

    let launch = Launch { claude: CliSlot::Fake, recording: Some(setup), ..Launch::default() };
    let mut second = ctx.launch(launch.clone())?;
    ensure(second.pid != first.pid, || "the restored wait must run in a new native process".into())?;
    let refusal = second.page.text_of("error.noSubscription")?;
    open_wait_project(&mut second.page, "Wait recovery project")?;
    second.page.click(&css(".proj-chats .item").has_text("Wait recovery thread"))?;
    second.page.wait_visible(&css("[role=status].conversation-wait").has_text(&refusal))?;
    let manage = css("button.wait-message").has_text(&second.page.text_of("set.openSetup")?);
    let setup = second.page.named("heading", "setup.title")?;
    second.page.click(&manage)?;
    second.page.wait_visible(&setup)?;
    let close = css(".fr-stage").role("button", &second.page.text_of("setup.close")?).exact();
    second.page.click(&close)?;
    second.page.wait_visible(&css("button.wait-message:focus"))?;
    let blocked = json!({"note":NOTE,"since":since,"until":until,"problem":"error.noSubscription"});
    same(second.page.eval(&wait_state)?, blocked.clone(), "the overdue deadline was retained in the visible refusal")?;
    same(saved()?["waitingOn"].clone(), blocked, "the refusal was durably saved before sign-in")?;
    same(saved()?["messages"].clone(), original.clone(), "the app kept the original messages and made no turn while refused")?;
    let turns = |ctx: &Ctx| -> Result<usize> {
        Ok(ctx
            .records()?
            .iter()
            .filter(|r| r.args.iter().any(|a| a == "-p") && !r.args.iter().any(|a| a == "--no-session-persistence"))
            .count())
    };
    same(turns(ctx)?, 0, "no persistent model invocation while disconnected")?;
    let (mb, webview) = ctx.memory_mb(second.id)?;
    ensure(webview && mb < 400.0, || format!("the native wait screen used {mb:.1} MB and renderer present was {webview}"))?;

    std::fs::write(ctx.sb.state.join("claude.signed-in"), "true").map_err(|e| e.to_string())?;
    let check = "(window.faberShell ?? window.__TAURI_INTERNALS__).invoke('setup_check',{vendor:'claude'}).then(r=>r.account)";
    same(second.page.eval(check)?, json!("signedIn"), "the fake CLI confirmed the separate test account")?;
    eventually("the fake model process to receive the resumed turn", Duration::from_secs(8), || turns(ctx) == Ok(1))?;
    let expected: Value = serde_json::from_str(include_str!("../../../tests/fixtures/claude/wait-restart.jsonl").lines().next().unwrap())
        .map_err(|e| e.to_string())?;
    let (_, prompt) = expected["msg"]["message"]["content"]
        .as_str()
        .and_then(|text| text.rsplit_once("\nNow the user says:\n"))
        .ok_or("the replay must include the original history")?;
    same(saved()?["messages"][2]["text"].clone(), json!(prompt), "the saved wake matches the replay")?;
    second.page.wait_visible(&css("article.ai").has_text(REPLY).css(".actions"))?;
    let complete = saved()?;
    same(complete["waitingOn"].clone(), Value::Null, "the completed wake cleared the saved wait")?;
    let after = complete["messages"].as_array().ok_or("the saved thread has no messages")?;
    same(after.len(), 4, "one wake and one answer were appended")?;
    same(after[..2].to_vec(), original.as_array().unwrap().clone(), "the original messages survived both launches")?;
    same(after[2]["id"].clone(), json!(format!("wait-{since}-{until}")), "the wake has the original wait receipt")?;
    same(after[2]["wake"]["why"].clone(), json!("wake.wait"), "the follow-up retains the source of the wake")?;
    same(turns(ctx)?, 1, "the connected account dispatched one model turn")?;
    ctx.close(second.id)?;
    ctx.audit_within(Duration::from_secs(15))?;

    let mut third = ctx.launch(launch)?;
    ensure(third.pid != second.pid, || "the receipt must be read in a new native process".into())?;
    third.page.wait_visible(&css("article.ai").has_text(REPLY).css(".actions"))?;
    same(third.page.eval(check)?, json!("signedIn"), "the same account confirmation still succeeds after reopening")?;
    same(third.page.eval(&wait_state)?, Value::Null, "an accounted wait never reappears")?;
    ctx.close(third.id)?;
    ctx.audit_within(Duration::from_secs(15))?;
    same(saved()?["messages"].clone(), complete["messages"].clone(), "reopening did not append a turn")?;
    same(turns(ctx)?, 1, "repeated confirmation did not dispatch a second model turn")?;
    ctx.notes.push(format!(
        "{:?}: saved deadline expired between native processes, one synthetic turn across three launches, disconnected tree {mb:.1} MB",
        crate::app::Shell::chosen()
    ));
    Ok(())
}
