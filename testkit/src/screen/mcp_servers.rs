use super::{Result, Scenario, no_cli, same, start_settings};
use crate::app::Ctx;
use serde_json::json;

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "mcp-servers: Tab and Space keep the keyboard on the same switch across saves", run: keyboard_switch }]
}

fn keyboard_switch(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    page.wait_for("the keyboard fixture to load", "!!window.__faberMcpKeyboard")?;
    page.eval("window.__faberMcpKeyboard.mount()")?;
    let run: Result = (|| {
        page.press("Tab")?;
        same(page.eval("document.activeElement.id")?, json!("set-mcp-claude-0-switch"), "Tab reaches the catalog switch")?;
        page.eval("document.activeElement.dataset.keyboardOriginal = 'true'")?;
        for (count, enabled) in [(1, false), (2, true)] {
            page.press("Space")?;
            page.wait_for(
                "Space to send the saved choice",
                &format!("document.querySelector('[data-mcp-keyboard]').dataset.saves === '{count}'"),
            )?;
            same(page.eval("document.activeElement.dataset.keyboardOriginal")?, json!("true"), "focus stays during saving")?;
            same(
                page.eval("document.querySelector('[data-mcp-keyboard]').dataset.enabled")?,
                json!(enabled.to_string()),
                "Space reverses the choice",
            )?;
            page.eval("window.__faberMcpKeyboard.finishSave()")?;
            page.wait_for("the save to finish", "!document.querySelector('[data-mcp-keyboard] [aria-busy=true]')")?;
            same(
                page.eval("document.activeElement.dataset.keyboardOriginal")?,
                json!("true"),
                "the original switch keeps focus after saving",
            )?;
            same(page.eval("document.activeElement.checked")?, json!(enabled), "the saved switch stays at the new value")?;
        }
        Ok(())
    })();
    let cleanup = page.eval("window.__faberMcpKeyboard.close()");
    run?;
    cleanup?;
    Ok(())
}
