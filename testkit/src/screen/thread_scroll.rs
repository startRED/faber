//! Opening a step pauses following until the person returns to the end.
use super::{Result, Scenario, ask, ensure, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "thread: opening a step pauses following until the end", run: scroll }]
}

fn scroll(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        recording: Some(ctx.paths.fixtures.join("claude/thread-scroll.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    super::shell::set_size(page, 1280, 720)?;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "Write a long reply.")?;
    page.wait_visible(&css("article.ai .step-line"))?;
    page.click(&css("article.ai .step-line"))?;
    page.wait_for("the step to open", "document.querySelector('article.ai .step-line')?.getAttribute('aria-expanded') === 'true'")?;
    page.eval("window.__scrollProof = { top: document.querySelector('article.ai .step-line').getBoundingClientRect().top, height: document.querySelector('article.ai').getBoundingClientRect().height }; true")?;
    page.wait_for("the streaming reply to grow by 600 pixels", "document.querySelector('article.ai').getBoundingClientRect().height >= window.__scrollProof.height + 600 && !document.querySelector('article.ai .actions')")?;
    let state = page.eval(r#"(() => {
        const row = document.querySelector('article.ai .step-line'), thread = document.querySelector('.thread');
        return { moved: Math.abs(row.getBoundingClientRect().top - window.__scrollProof.top), below: thread.scrollHeight - thread.scrollTop - thread.clientHeight };
    })()"#)?;
    ensure(state["moved"].as_f64().is_some_and(|n| n < 2.0), || format!("the opened step moved while the reply grew: {state}"))?;
    ensure(state["below"].as_f64().is_some_and(|n| n >= 40.0), || format!("the thread jumped to the end: {state}"))?;
    page.eval("(() => { const t = document.querySelector('.thread'); t.scrollTop = t.scrollHeight; t.dispatchEvent(new Event('scroll')); window.__followHeight = document.querySelector('article.ai').getBoundingClientRect().height; return true; })()")?;
    page.wait_for("more words after returning to the end", "document.querySelector('article.ai').getBoundingClientRect().height >= window.__followHeight + 400 && !document.querySelector('article.ai .actions')")?;
    page.wait_for(
        "the thread to follow the new words",
        "(() => { const t = document.querySelector('.thread'); return t.scrollHeight - t.scrollTop - t.clientHeight < 2; })()",
    )?;
    page.wait_visible(&css("article.ai .actions"))?;
    Ok(())
}
