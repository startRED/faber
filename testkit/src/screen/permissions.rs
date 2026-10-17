//! Permission cards: the screen answers a question by the ids of its turn and its request, and the card goes only once
//! the engine says the CLI has the answer.

use super::{Result, Scenario, ask, ensure, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css};
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

/// The default mode, read like a person would: no sentence over the thread (the mode is a Settings matter), the details in
/// Settings, and the details changing with the choice.
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
    // Calm: no permission sentence over the conversation, whatever the mode.
    page.wait_visible(&css("article .ai, article.ai"))?;
    ensure(page.count(&css(".col .hint[role=status]"))? == 0, || "a permission sentence sits over the thread".into())?;
    // The details are in Settings.
    let settings = page.named("button", "settings.open")?.exact();
    page.click(&settings)?;
    let dont_ask_body = page.text_of("set.permissionsDontAskBody")?;
    // The long text sits behind the "?" of the row: it is not on the page until that is opened (R140).
    ensure(page.count(&css(".hint").has_text(&dont_ask_body))? == 0, || "the long risk text is on the page while closed".into())?;
    page.click(&css("#set-permissions-q"))?;
    page.wait_visible(&css(".hint").has_text(&dont_ask_body))?;
    let full_access = page.text_of("set.permissions.dontAsk")?;
    same(page.eval(PICKED)?.as_str(), Some(full_access.as_str()), "Do not ask is the default")?;
    // Choosing the other mode swaps the details; choosing this one again brings them back.
    pick_mode(page, 1)?;
    let ask_body = page.text_of("set.permissionsAskBody")?;
    page.wait_visible(&css(".hint").has_text(&ask_body))?;
    ensure(page.count(&css(".hint").has_text(&dont_ask_body))? == 0, || "the old details are still on screen".into())?;
    pick_mode(page, 5)?;
    page.wait_visible(&css(".hint").has_text(&dont_ask_body))?;
    ensure(page.count(&css(".hint").has_text(&ask_body))? == 0, || "the details of the other mode are still on screen".into())
}

/// The mode the permissions menu shows as picked.
const PICKED: &str = "document.querySelector('#set-permissions-pick').textContent.trim()";

/// Opens the permissions menu and picks the mode at that place in its list (1 = Ask, 5 = Full access).
fn pick_mode(page: &mut Page, place: usize) -> Result {
    page.click(&css("#set-permissions-pick"))?;
    page.click(&css(&format!("#set-permissions-pick-list [role=option]:nth-child({place})")))
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
    // Every request is answered for the person; only the agent's own question waits, as a card to tap (R160).
    let question = css("article [data-testid=question-card]");
    page.wait_visible(&question)?;
    ensure(page.count(&css("div.ask:not(.question)"))? == 0, || "full access must not leave a permission card".into())?;
    page.click(&question.clone().css(".q-option").has_text("SQLite"))?;
    let send = page.text_of("ask.question.send")?;
    page.click(&question.clone().role("button", &send).exact())?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css("div.ask"))? == 0, || "the answered question must not stay on screen".into())?;
    let settings = page.named("button", "settings.open")?.exact();
    page.click(&settings)?;
    let folder = page.named("button", "set.chooseFolder")?;
    page.wait_visible(&folder)?;
    let locked = page.text_of("error.chatFolderLocked")?;
    page.wait_visible(&css("#set-folder-hint").has_text(&locked))?;
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
    let pick = css("#set-permissions-pick");
    page.wait_visible(&pick)?;
    let (full_access, ask_mode) = (page.text_of("set.permissions.dontAsk")?, page.text_of("set.permissions.ask")?);
    same(page.eval(PICKED)?.as_str(), Some(full_access.as_str()), "older settings default to Don't ask")?;
    pick_mode(page, 1)?;
    page.wait_for("the permissions choice to be saved", &format!("({PICKED}) === {}", serde_json::json!(ask_mode)))?;
    page.cdp.call("Page.reload", serde_json::json!({}))?;
    page.wait_visible(&settings)?;
    page.click(&settings)?;
    page.wait_visible(&pick)?;
    same(page.eval(PICKED)?.as_str(), Some(ask_mode.as_str()), "the permissions choice survives reload")?;
    let back = page.named("button", "set.back")?;
    page.click(&back)?;
    // Projects run only missions, so the mode is a Chat matter: a chat says it over its thread.
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    ensure(page.count(&css(".col .hint[role=status]"))? == 0, || "a permission sentence sits over the thread".into())?;

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
