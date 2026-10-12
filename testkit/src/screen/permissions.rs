//! Permission cards: the screen answers a question by the ids of its turn and its request, and the card goes only once
//! the engine says the CLI has the answer.

use super::{Result, Scenario, ask, ensure, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};

/// The request of the recording `claude/tools.jsonl`, which allows the first Write and denies the second.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "a permission card is answered from the screen and the answer reaches the CLI", run: answers_a_card }]
}

fn answers_a_card(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    super::shell::seed_project(ctx, "Demo")?;
    let launch = Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    };
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let demo = css(".prow .item").has_text("Demo");
    page.wait_visible(&demo)?;
    page.click(&demo)?;
    let notice = page.text_of("project.permissionsDontAsk")?;
    page.wait_visible(&css("[role=status]").has_text(&notice))?;
    let settings = page.named("button", "settings.open")?;
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
    // Reload returns to the project already opened; the new mode is disclosed before the first turn.
    let notice = page.text_of("project.permissionsAsk")?;
    page.wait_visible(&css("[role=status]").has_text(&notice))?;
    page.wait_visible(&css(".empty h1").has_text("Demo"))?;
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;

    // The CLI asks to write a.txt. Allow it ("Always allow" holds the same word, so the name must match exactly); the card for b.txt follows, and the recording denies that one.
    let first = css(".ask").has_text("a.txt");
    page.wait_visible(&first)?;
    let allow = page.named("button", "ask.allow")?.exact();
    page.click(&allow)?;
    page.wait_gone(&first)?;
    let second = css(".ask").has_text("b.txt");
    page.wait_visible(&second)?;
    let deny = page.named("button", "ask.deny")?.exact();
    page.click(&deny)?;

    // The recording ends the turn only if the CLI got both answers as recorded.
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "no card should be left".to_string())
}
