//! Permission cards: the screen answers a question by the ids of its turn and its request, and the card goes only once
//! the engine says the CLI has the answer.

use super::{Result, Scenario, ask, ensure, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};

/// The request of the recording `claude/tools.jsonl`, which allows the first Write and denies the second.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "a permission card is answered from the screen and the answer reaches the CLI", run: answers_a_card },
        Scenario {
            name: "R121: default Chat answers every request the CLI still raises, with no permission card",
            run: chat_without_cards,
        },
        Scenario { name: "R121: Do not ask is one calm line in the thread and its details are in Settings", run: do_not_ask_text },
    ]
}

/// The default mode, read like a person would: the line over the thread, its link to Settings, the details there, and the
/// details changing with the choice.
fn do_not_ask_text(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    };
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    let line = page.text_of("project.permissionsDontAsk")?;
    let status = css("[role=status]").has_text(&line);
    page.wait_visible(&status)?;
    // Calm and short: no error vocabulary, no block of technical text over every thread.
    ensure(line.chars().count() < 60, || format!("the thread line is too long: {line}"))?;
    let shown = page.inner_text(&status)?;
    ensure(!shown.to_lowercase().contains("error") && !shown.contains("sandbox"), || format!("the line reads like an error: {shown}"))?;
    // The link opens Settings, where the details are.
    let details = page.named("button", "project.permissionsDetails")?;
    page.click(&details)?;
    let dont_ask_body = page.text_of("set.permissionsDontAskBody")?;
    page.wait_visible(&css(".hint").has_text(&dont_ask_body))?;
    let dont_ask = page.named("button", "set.permissions.dontAsk")?;
    same(page.attr(&dont_ask, "aria-pressed")?, Some("true".into()), "Do not ask is the default")?;
    // Choosing the other mode swaps the details; choosing this one again brings them back.
    let ask_button = page.named("button", "set.permissions.ask")?;
    page.click(&ask_button)?;
    let ask_body = page.text_of("set.permissionsAskBody")?;
    page.wait_visible(&css(".hint").has_text(&ask_body))?;
    ensure(page.count(&css(".hint").has_text(&dont_ask_body))? == 0, || "the old details are still on screen".into())?;
    page.click(&dont_ask)?;
    page.wait_visible(&css(".hint").has_text(&dont_ask_body))?;
    ensure(page.count(&css(".hint").has_text(&ask_body))? == 0, || "the details of the other mode are still on screen".into())
}

fn chat_without_cards(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/chat-full-access.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "check policy")?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "full access must not leave a permission card".into())?;
    let settings = page.named("button", "settings.open")?.exact();
    page.click(&settings)?;
    let hint = page.text_of("set.chatFolderHint")?;
    page.wait_visible(&css(".hint").has_text(&hint))?;
    let folder = page.named("button", "set.chooseFolder")?;
    same(page.attr(&folder, "disabled")?, Some("".into()), "the folder is fixed once messages exist")
}

fn answers_a_card(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    };
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let settings = page.named("button", "settings.open")?.exact();
    page.click(&settings)?;
    let dont_ask = page.named("button", "set.permissions.dontAsk")?;
    page.wait_visible(&dont_ask)?;
    same(page.attr(&dont_ask, "aria-pressed")?, Some("true".into()), "older settings default to Don't ask")?;
    let ask_first = page.named("button", "set.permissions.ask")?;
    page.click(&ask_first)?;
    page.wait_for(
        "the permissions choice to be saved",
        "document.querySelector('#set-permissions + .seg-ctl button[aria-pressed=true]')?.textContent === 'Ask before commands'",
    )?;
    page.cdp.call("Page.reload", serde_json::json!({}))?;
    page.wait_visible(&settings)?;
    page.click(&settings)?;
    page.wait_visible(&ask_first)?;
    same(page.attr(&ask_first, "aria-pressed")?, Some("true".into()), "the permissions choice survives reload")?;
    let back = page.named("button", "set.back")?;
    page.click(&back)?;
    // Projects run only missions, so the mode is a Chat matter: a chat says it over its thread.
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    let notice = page.text_of("project.permissionsAsk")?;
    page.wait_visible(&css("[role=status]").has_text(&notice))?;

    // The CLI asks to write a.txt. Allow it ("Always allow" holds the same word, so the name must match exactly); the card for b.txt follows, and the recording denies that one.
    let first = css("article .ask").has_text("a.txt");
    page.wait_visible(&first)?;
    let allow = first.clone().role("button", &page.text_of("ask.allow")?).exact();
    page.click(&allow)?;
    page.wait_gone(&first)?;
    let second = css("article .ask").has_text("b.txt");
    page.wait_visible(&second)?;
    page.wait_visible(&css(".runcol .ask").has_text("b.txt"))?;
    let deny = second.clone().role("button", &page.text_of("ask.deny")?).exact();
    ensure(page.count(&deny)? == 1, || "the answer locator must name one button while both question copies are visible".to_string())?;
    page.click(&deny)?;

    // The recording ends the turn only if the CLI got both answers as recorded.
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "no card should be left".to_string())
}
