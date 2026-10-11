//! The app scenarios: what a person sees and does in a chat, in settings, and how much memory the app uses.

use super::{Result, Scenario, ask, claude_chat, ensure, eventually, no_cli, pause, same, start_settings};
use crate::app::Ctx;
use crate::page::{css, role_any};
use serde_json::json;
use std::time::{Duration, Instant};

/// What Faber may use with no CLI running, as Task Manager counts it (private working set of the app and its webview).
/// The product limit is 500 MB when Faber is complete; the test keeps half of it as room for what is still to come.
const BUDGET_MB: f64 = 250.0;

const QUESTION: &str = "In one sentence: why is the sky blue?";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "the app opens on an empty chat and fits the window", run: opens_empty },
        Scenario { name: "a chat streams an answer with the model and the reason, and lands in the sidebar", run: streams_an_answer },
        Scenario { name: "editing a message sends it as a branch and keeps the original conversation", run: edits_as_a_branch },
        Scenario { name: "deleting a conversation asks once, then removes it", run: deletes_after_asking },
        Scenario { name: "with no AI installed, the app says what to do", run: says_what_to_do },
        Scenario { name: "the screen speaks the chosen language and switches at once", run: switches_language },
        Scenario { name: "appearance changes apply at once and are kept after reopening", run: keeps_appearance },
        Scenario { name: "with no CLI running, Faber stays within its memory budget", run: idle_memory },
        Scenario { name: "a conversation with a thousand messages opens quickly and stays within the budget", run: thousand_messages },
    ]
}

fn opens_empty(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&heading)?;
    let foot = page.inner_text(&css(".foot"))?;
    ensure(super::has_version_after(&foot, "Claude · v"), || format!("the footer should name the CLI version, it says {foot:?}"))?;
    let scrolls = page.eval("document.documentElement.scrollHeight > innerHeight")?;
    same(scrolls.as_bool(), Some(false), "the window should not scroll as a whole")?;
    // No system title bar: the top bar carries the window buttons and drags the window.
    for key in ["window.minimize", "window.maximize", "window.close"] {
        let button = page.named("button", key)?.exact();
        page.wait_visible(&button)?;
    }
    same(page.attr(&css("header.top"), "data-tauri-drag-region")?, Some(String::new()), "the top bar drags the window")?;
    ctx.shot(&mut app, "home")
}

fn streams_an_answer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let answer = page.named("article", "answer.label")?;
    page.wait_visible(&answer.clone().css(".actions"))?;
    let text = page.inner_text(&answer)?;
    ensure(text.contains("Rayleigh"), || format!("the answer should explain the sky: {text}"))?;
    ensure(text.contains("Haiku 5.5"), || format!("the answer should name its model: {text}"))?;
    let reason = page.text_of("reason.autoDefault")?;
    ensure(text.contains(&reason), || format!("the answer should say why Auto chose it: {text}"))?;
    ensure(text.contains("at API prices"), || format!("the answer should show the price note: {text}"))?;
    page.wait_visible(&css(".list .item").has_text(QUESTION))?;
    ctx.shot(&mut app, "chat")
}

fn edits_as_a_branch(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.hover(&css(".you"))?;
    let edit = page.named("button", "mine.editLabel")?;
    page.click(&edit)?;
    let send = page.named("button", "mine.sendBranch")?;
    page.click(&send)?;
    page.wait_visible(&actions)?;
    page.wait_for("two conversations in the list", "document.querySelectorAll('.list .item').length === 2")?;
    ctx.shot(&mut app, "branch")
}

fn deletes_after_asking(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    page.hover(&css(".list .item"))?;
    let x = css(".list .item .x");
    page.click(&x)?;
    let confirm = page.text_of("side.confirmDelete")?;
    same(page.inner_text(&x)?, confirm, "the first click asks")?;
    page.click(&x)?;
    page.wait_for("the list to be empty", "document.querySelectorAll('.list .item').length === 0")
}

fn says_what_to_do(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.noAiTitle")?;
    page.wait_visible(&title)?;
    let body = page.inner_text(&css(".empty p"))?;
    same(body.clone(), page.text_of("empty.noAiBody")?, "the empty state says what to do")?;
    ensure(body.contains("Settings"), || format!("it should point to Settings: {body}"))?;
    ensure(body.to_lowercase().contains("subscriptions"), || format!("it should mention Subscriptions: {body}"))?;
    let lower = body.to_lowercase();
    ensure(!["reopen", "restart", "relaunch"].iter().any(|w| lower.contains(w)), || format!("it should not ask to restart: {body}"))?;
    let send = page.named("button", "composer.send")?;
    same(page.is_disabled(&send)?, true, "Send is disabled with no AI")
}

fn switches_language(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "pt-BR")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named_in("heading", "empty.chatTitle", "pt-BR")?;
    page.wait_visible(&title)?;
    same(page.eval("document.documentElement.lang")?.as_str(), Some("pt-BR"), "the page language")?;
    ctx.shot(&mut app, "home-pt-BR")?;
    let page = &mut app.page;
    let open = page.named_in("button", "settings.open", "pt-BR")?;
    page.click(&open)?;
    let language = crate::page::label(&page.text_in("set.language", "pt-BR")?);
    page.select_option(&language, "es")?;
    let general = page.named_in("heading", "set.general", "es")?;
    page.wait_visible(&general)?;
    same(page.eval("document.documentElement.lang")?.as_str(), Some("es"), "the page language after the switch")
}

fn keeps_appearance(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let background = "getComputedStyle(document.body).backgroundColor";
    let settings = page.named("button", "settings.open")?;
    page.click(&settings)?;
    let appearance = page.named("button", "set.appearance")?;
    page.click(&appearance)?;
    let heading = page.named("heading", "set.appearance")?;
    page.wait_visible(&heading)?;
    let light = page.named("button", "set.modeLight")?;
    page.click(&light)?;
    same(page.eval(background)?.as_str(), Some("rgb(246, 247, 249)"), "light mode background")?;
    page.click(&crate::page::role("button", "Cockpit"))?;
    same(page.eval(background)?.as_str(), Some("rgb(0, 0, 0)"), "Cockpit background")?;
    let compact = page.named("button", "set.densityCompact")?;
    same(page.attr(&compact, "aria-pressed")?, Some("true".to_string()), "Cockpit is compact")?;
    ctx.shot(&mut app, "settings-cockpit")?;
    let page = &mut app.page;
    let projects = page.named("button", "set.projects")?.last();
    page.click(&projects)?;
    page.check(&role_any("checkbox"))?;
    // The file is written in the background; closing earlier could lose the last choice.
    let file = ctx.sb.data.join("settings.json");
    eventually("the choice to reach settings.json", Duration::from_secs(15), || {
        std::fs::read_to_string(&file).is_ok_and(|text| text.contains("\"userHooks\": true"))
    })?;
    ctx.close(app.id)?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    page.wait_for("the saved background", &format!("{background} === 'rgb(0, 0, 0)'"))?;
    // An unfocused offscreen window reopened on the same profile only takes keys after a click, like a real one.
    let composer = page.labelled("composer.message")?;
    page.click(&composer)?;
    page.press("Control+,")?;
    let general = page.named("heading", "set.general")?;
    page.wait_visible(&general)?;
    let projects = page.named("button", "set.projects")?.last();
    page.click(&projects)?;
    same(page.is_checked(&role_any("checkbox"))?, true, "the saved choice")
}

fn idle_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let title = app.page.named("heading", "empty.noAiTitle")?;
    app.page.wait_visible(&title)?;
    pause(2.0);
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("idle: {mb:.0} MB"));
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}

fn thousand_messages(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let at: u64 = 1_790_000_000_000;
    let paragraph = "Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ";
    let fence = "`".repeat(3);
    let id = "5f1c0000-0000-4000-8000-000000000001";
    let messages: Vec<_> = (0..1000u64)
        .map(|i| {
            let message_id = format!("5f1c{i:04x}-0000-4000-8000-0000000000{:02x}", i % 256);
            if i % 2 == 0 {
                json!({ "id": message_id, "role": "user", "text": format!("Question {}: why is the sky blue?", i / 2), "at": at + i })
            } else {
                let text = [paragraph.repeat(6), "- one\n- two".to_string(), format!("{fence}js\nconsole.log({i});\n{fence}")].join("\n\n");
                json!({ "id": message_id, "role": "assistant", "text": text, "at": at + i, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" })
            }
        })
        .collect();
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let conversation =
        json!({ "id": id, "title": "Long one", "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let started = Instant::now();
    page.click(&css(".list .item").has_text("Long one"))?;
    page.wait_visible(&css("article.ai").last())?;
    let open_ms = started.elapsed().as_millis();
    let drawn = page.count(&css("article.ai"))?;
    ensure(drawn <= 25, || format!("{drawn} answers drawn at once"))?;
    // Scrolling up draws the earlier messages, keeping the reader's place.
    page.eval_on(&css(".thread"), "el.scrollTop = 0; return {};")?;
    page.wait_for("earlier answers to be drawn", &format!("document.querySelectorAll('article.ai').length > {drawn}"))?;
    pause(2.0);
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("1000 messages: opened in {open_ms} ms, {mb:.0} MB"));
    ensure(open_ms < 3000, || format!("opening took {open_ms} ms"))?;
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port completeness: the nine tests of `tests/e2e/app.e2e.ts`, one to one, the appearance one unchanged.
    #[test]
    fn the_nine_scenarios_of_the_node_suite_are_all_ported() {
        let names: Vec<&str> = scenarios().iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "the app opens on an empty chat and fits the window",
                "a chat streams an answer with the model and the reason, and lands in the sidebar",
                "editing a message sends it as a branch and keeps the original conversation",
                "deleting a conversation asks once, then removes it",
                "with no AI installed, the app says what to do",
                "the screen speaks the chosen language and switches at once",
                "appearance changes apply at once and are kept after reopening",
                "with no CLI running, Faber stays within its memory budget",
                "a conversation with a thousand messages opens quickly and stays within the budget",
            ]
        );
    }
}
