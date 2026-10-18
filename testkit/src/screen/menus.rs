//! Right-click (R123): the browser's own menu never shows, and Faber's short menus open where they help.

use super::{Result, Scenario, ask, claude_chat, ensure, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css};
use std::time::Duration;

const QUESTION: &str = "In one sentence: why is the sky blue?";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "right-click opens Faber's short menus on a chat and a text field, and nothing anywhere else", run: right_click }]
}

/// The words of the open menu, in order.
fn menu_words(page: &mut Page) -> Result<Vec<String>> {
    let found = page.eval("[...document.querySelectorAll('.cmenu:not(.leaving) [role=menuitem]')].map((b) => b.textContent)")?;
    Ok(found.as_array().map(|all| all.iter().filter_map(|w| w.as_str().map(str::to_string)).collect()).unwrap_or_default())
}

fn wait_menu_gone(page: &mut Page) -> Result {
    page.wait_for("the menu to leave", "!document.querySelector('.cmenu')")
}

fn right_click(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    page.wait_visible(&css(".list .crow .item").has_text(QUESTION))?;
    // What the page decided about every right-click after Faber's own handlers: the browser's menu is stopped in all of them.
    page.eval("window.__rc = []; window.addEventListener('contextmenu', (e) => window.__rc.push(e.defaultPrevented)); true")?;

    // A chat in the sidebar: Rename, Archive, Delete.
    page.right_click(&css(".list .crow"))?;
    page.wait_visible(&css(".cmenu"))?;
    let (rename, archive, delete) = (page.text_of("menu.rename")?, page.text_of("menu.archive")?, page.text_of("menu.delete")?);
    same(menu_words(page)?, vec![rename.clone(), archive.clone(), delete], "the menu of a chat")?;
    // The animation has settled when the shot is taken.
    std::thread::sleep(Duration::from_millis(400));
    ctx.shot(&mut app, "chat-menu")?;
    let page = &mut app.page;
    page.press("Escape")?;
    wait_menu_gone(page)?;

    // Rename turns the row into a text box that keeps what is typed.
    page.right_click(&css(".list .crow"))?;
    page.wait_visible(&css(".cmenu"))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&rename))?;
    let box_ = css(".list .crow input.rename");
    page.wait_visible(&box_)?;
    page.fill(&box_, "The blue sky")?;
    page.press("Enter")?;
    page.wait_visible(&css(".list .crow .item").has_text("The blue sky"))?;

    // Archive takes the open chat off the list, and one line with Unarchive takes the composer's place until it comes back.
    page.right_click(&css(".list .crow"))?;
    page.wait_visible(&css(".cmenu"))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&archive))?;
    page.wait_visible(&css(".archived-bar"))?;
    page.wait_gone(&css(".list .crow"))?;
    same(page.count(&css(".dock .composer"))?, 0, "composers of an archived chat")?;
    wait_menu_gone(page)?;
    ctx.shot(&mut app, "archived")?;
    let page = &mut app.page;
    let unarchive = page.text_of("menu.unarchive")?;
    page.click(&css(".archived-bar button").has_text(&unarchive))?;
    page.wait_visible(&css(".list .crow .item").has_text("The blue sky"))?;
    page.wait_visible(&css(".dock .composer"))?;

    // The title bar, the sidebar footer and the greeting area show nothing at all.
    for place in ["header.top", ".side-foot", ".main"] {
        page.right_click(&css(place))?;
        std::thread::sleep(Duration::from_millis(150));
        same(page.count(&css(".cmenu"))?, 0, &format!("menus after a right-click on {place}"))?;
    }

    // A text field: Cut, Copy, Paste, Select all, working through the real clipboard.
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, "hello world")?;
    page.right_click(&composer)?;
    page.wait_visible(&css(".cmenu"))?;
    let words = [page.text_of("menu.cut")?, page.text_of("menu.copy")?, page.text_of("menu.paste")?, page.text_of("menu.selectAll")?];
    same(menu_words(page)?, words.to_vec(), "the menu of a text field")?;
    std::thread::sleep(Duration::from_millis(400));
    ctx.shot(&mut app, "text-menu")?;
    let page = &mut app.page;
    page.click(&css(".cmenu [role=menuitem]").has_text(&words[3]))?;
    page.right_click(&composer)?;
    page.wait_visible(&css(".cmenu"))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&words[1]))?;
    // The cursor goes after the copied text, so Paste adds to it instead of replacing it.
    page.eval("(() => { const t = document.querySelector('.composer textarea'); t.focus(); t.setSelectionRange(t.value.length, t.value.length); })()")?;
    page.right_click(&composer)?;
    page.wait_visible(&css(".cmenu"))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&words[2]))?;
    page.wait_for("the pasted text", "document.querySelector('.composer textarea').value === 'hello worldhello world'")?;

    let seen = page.eval("window.__rc")?;
    let all = seen.as_array().cloned().unwrap_or_default();
    ensure(!all.is_empty() && all.iter().all(|p| p.as_bool() == Some(true)), || format!("every right-click must be stopped, saw {all:?}"))
}
