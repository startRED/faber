//! The reply keeps words and tool steps in the order the agent produced them.
use super::{Result, Scenario, ask, ensure, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "thread: words and tools stay in order while streaming and when done", run: order }]
}

fn order(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        recording: Some(ctx.paths.fixtures.join("claude/thread-order.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "Show the work in order.")?;
    page.wait_for(
        "the final words while streaming",
        "document.querySelector('article.ai')?.textContent.includes('Final answer.') && !document.querySelector('article.ai .actions')",
    )?;
    check(page)?;
    page.click(&css("article.ai .steps-head"))?;
    page.wait_for("the two tool rows", "document.querySelectorAll('article.ai .steps-list .step').length === 2")?;
    let tools = page.eval("[...document.querySelectorAll('article.ai .steps-list .step')].map(e => e.textContent)")?;
    ensure(
        tools[0].as_str().is_some_and(|s| s.contains("file1.txt")) && tools[1].as_str().is_some_and(|s| s.contains("file2.txt")),
        || format!("the grouped tools are out of order: {tools}"),
    )?;
    page.wait_visible(&css("article.ai .actions"))?;
    check(page)?;
    Ok(())
}

fn check(page: &mut crate::page::Page) -> Result {
    let result = page.eval(r#"(() => {
      const a = document.querySelector('article.ai');
      const parts = [...a.children].filter(e => e.matches('.md, .steps'));
      const words = (e, text) => e?.matches('.md') && e.innerText.includes(text) && e.checkVisibility() && !e.closest('details:not([open])');
      return parts.length === 5 && words(parts[0], 'First sentence.') && parts[1].matches('.steps:not(.one)') &&
        words(parts[2], 'Second sentence.') && parts[3].matches('.steps.one') && parts[3].innerText.includes('file3.txt') &&
        words(parts[4], 'Final answer.') && !a.querySelector('details.activity');
    })()"#)?;
    ensure(result == true, || "words and steps are missing, hidden, or out of order".into())
}
