//! The shell of the screen: the layout at each width, the running column and when it opens, the new conversation with
//! its greeting, and the quota in the sidebar's footer.

use super::{Result, Scenario, ask, claude_chat, ensure, no_cli, pause, same, start_settings, system_motion};
use crate::app::Ctx;
use crate::page::{Page, css};
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Duration;

const QUESTION: &str = "In one sentence: why is the sky blue?";

/// The request of the recording `claude/tools.jsonl`, which allows the first Write and denies the second.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "the context meter opens what fills the context and compacts it now (task 190)", run: compacts_now },
        Scenario {
            name: "R161: a subagent the agent starts is one step in its reply, with what it ran and its report",
            run: subagent_step,
        },
        Scenario {
            name: "an opened subagent's side column takes the room beside the thread, cuts no line, and can fill the stage (English)",
            run: subagent_column_en,
        },
        Scenario {
            name: "an opened subagent's side column takes the room beside the thread, cuts no line, and can fill the stage (German)",
            run: subagent_column_de,
        },
        Scenario { name: "the layout follows the width, from 720 to 1440, and the running column fits each one", run: follows_the_width },
        Scenario { name: "a larger text size still fits every width", run: fits_large_text },
        Scenario {
            name: "the running column opens by itself once for each question, and closing it leaves the button amber",
            run: opens_by_itself,
        },
        Scenario {
            name: "a question that waits when the screen loads shows the amber button at once and opens the column once",
            run: opens_at_load,
        },
        Scenario {
            name: "over the stage the column opens above the composer without taking focus, and the user's own opening is modal",
            run: overlays_the_stage,
        },
        Scenario { name: "a click on a running conversation opens it", run: opens_a_running_conversation },
        Scenario {
            name: "a project conversation opens as a chat with its composer, and opening it starts no CLI (R160)",
            run: opens_a_project_conversation,
        },
        Scenario {
            name: "the new conversation greets, suggests without sending, and offers the last conversations",
            run: greets_and_suggests,
        },
        Scenario { name: "with no AI connected the new conversation gives way to the empty state with a button", run: gives_way_to_no_ai },
        Scenario { name: "the quota of each vendor shows in the footer, and comes back after the screen reloads", run: shows_the_quota },
        Scenario {
            name: "a conversation that is still opening takes no message, and a draft written meanwhile stays",
            run: waits_for_the_conversation,
        },
        Scenario { name: "the column over the stage leaves the greeting's composer in reach", run: leaves_the_greeting_composer },
        Scenario {
            name: "a modal dialog keeps Tab inside it, Ctrl+N leaves it, a resize moves focus with it, and two dialogs never stack",
            run: keeps_focus_in_dialogs,
        },
        Scenario {
            name: "the column waits for every key, also those that change no text, and opens with the caret where it was",
            run: waits_for_every_key,
        },
        Scenario { name: "a column opened and closed while it waits for the typing stays closed", run: dismissal_sticks },
        Scenario {
            name: "with reduced motion on, a column opened and closed while it waits for the typing stays closed",
            run: dismissal_sticks_reduced,
        },
        Scenario {
            name: "in a window too narrow to dock it, a column closed with Esc while it waits for the typing stays closed",
            run: dismissal_sticks_narrow,
        },
        Scenario { name: "long titles stay inside the sidebar, cut before the delete button", run: keeps_long_titles_inside },
    ]
}

pub(super) fn set_size(page: &mut Page, width: u32, height: u32) -> Result {
    let size = json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false });
    page.cdp.call("Emulation.setDeviceMetricsOverride", size)?;
    // The screen follows `resize`, which comes a moment after the override.
    let narrow = width < 900;
    page.wait_for("the screen to follow the new width", &format!("(!!document.querySelector('.topbar .menu')) === {narrow}"))
}

/// Where the parts of the shell are: each one's box and whether it is drawn, and what sticks out past the right edge.
const SHELL: &str = r#"(() => {
  const box = (selector) => {
    const e = document.querySelector(selector);
    if (!e) return null;
    const b = e.getBoundingClientRect();
    const s = getComputedStyle(e);
    return { x: b.x, y: b.y, w: b.width, h: b.height, r: b.right, b: b.bottom, shown: s.display !== 'none' && s.visibility !== 'hidden' };
  };
  // Whether a control can be used: the element on top at its centre is the control itself, nothing clips it away, and no
  // dialog has made it inert.
  const reach = (selector, scroll) => {
    const e = document.querySelector(selector);
    if (!e) return null;
    if (scroll) e.scrollIntoView({ block: 'nearest' });
    const b = e.getBoundingClientRect();
    const top = document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2);
    let clipped = false;
    for (let p = e.parentElement; p && p !== document.documentElement; p = p.parentElement) {
      const s = getComputedStyle(p);
      if (s.overflowX === 'visible' && s.overflowY === 'visible') continue;
      const r = p.getBoundingClientRect();
      if (b.x < r.x - 0.5 || b.right > r.right + 0.5 || b.y < r.y - 0.5 || b.bottom > r.bottom + 0.5) clipped = true;
    }
    return { hit: !!top && e.contains(top), inert: !!e.closest('[inert]'), clipped };
  };
  const past = [...document.querySelectorAll('header.top *, .shell *')]
    .filter((e) => { const s = getComputedStyle(e); return !e.closest('.ambient, .scrim') && s.display !== 'none' && s.position !== 'fixed' && e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().right > innerWidth + 0.5; })
    .map((e) => String(e.className && e.className.baseVal !== undefined ? e.className.baseVal : e.className || e.tagName));
  return {
    page: document.documentElement.scrollWidth <= innerWidth,
    past,
    title: box('header.top'),
    side: box('.side'),
    topbar: box('.topbar'),
    run: box('.run-btn'),
    view: { w: innerWidth, h: innerHeight },
    main: box('.main'),
    col: box('.thread .col'),
    docked: box('.shell > .runcol'),
    over: box('.work > .runcol'),
    composer: box('.dock .composer') || box('.composer'),
    reach: {
      composer: reach(document.querySelector('.dock .composer') ? '.dock .composer' : '.composer', true),
      run: reach('.run-btn'),
      close: reach('.runcol .rc-head button'),
    },
    modal: (document.querySelector('.runcol') || {}).getAttribute ? document.querySelector('.runcol').getAttribute('aria-modal') : null,
    focusIn: !!document.activeElement && !!document.activeElement.closest('.runcol'),
  };
})()"#;

fn num(v: &Value, path: &[&str]) -> f64 {
    path.iter().fold(v, |v, key| &v[*key]).as_f64().unwrap_or(f64::NAN)
}

fn shown(v: &Value, part: &str) -> bool {
    v[part]["shown"].as_bool().unwrap_or(false)
}

/// The window width from which the column sits beside the stage: the usual 288 px sidebar, the column's 300 px minimum and the
/// stage's 440 px (`dockable` in ui/layout.ts).
const DOCKS_FROM: u32 = 1028;

/// Closes the column the way the width allows: the button while the column sits beside the stage, its own close button
/// while it is a modal dialog over the stage (the top bar is out of reach then).
fn close_column(page: &mut Page, width: u32) -> Result {
    if width >= DOCKS_FROM {
        run_button(page)?;
    } else {
        let close = page.named("button", "run.close")?;
        page.click(&close)?;
    }
    // A retained exit is inert before its content is released; the next layout starts after removal.
    page.wait_count(&css(".runcol"), 0)
}

fn run_button(page: &mut Page) -> Result {
    let button = css(".run-btn");
    page.click(&button)
}

/// The same checks at every width: nothing sticks out, the bars have their heights, the sidebar sits where the width says,
/// and the composer and the run button are there and can be used. `modal`: a dialog is open over them, so they are out of
/// reach on purpose (inert) and the dialog's own close button must be the one that works.
fn check_shell(at: &str, seen: &Value, width: u32, modal: bool) -> Result {
    ensure(seen["page"] == true, || format!("{at}: the page scrolls sideways"))?;
    ensure(seen["past"].as_array().is_some_and(Vec::is_empty), || format!("{at}: these stick out past the right edge: {} ", seen["past"]))?;
    same(num(seen, &["title", "h"]), 36.0, &format!("{at}: the title bar"))?;
    same(num(seen, &["topbar", "h"]), 52.0, &format!("{at}: the top bar"))?;
    if width >= 900 {
        same(num(seen, &["side", "w"]), 288.0, &format!("{at}: the sidebar"))?;
    } else {
        ensure(!shown(seen, "side"), || format!("{at}: the sidebar should be behind its button"))?;
    }
    for part in ["composer", "run"] {
        ensure(!seen[part].is_null(), || format!("{at}: the {part} is missing"))?;
        let whole = num(seen, &[part, "x"]) >= -0.5
            && num(seen, &[part, "r"]) <= num(seen, &["view", "w"]) + 0.5
            && num(seen, &[part, "b"]) <= num(seen, &["view", "h"]) + 0.5;
        ensure(whole, || format!("{at}: the {part} is cut off: {}", seen[part]))?;
        let reach = &seen["reach"][part];
        if modal {
            ensure(reach["inert"] == true, || format!("{at}: a modal dialog is open, so the {part} should be inert: {reach}"))?;
        } else {
            ensure(reach["inert"] == false && reach["hit"] == true && reach["clipped"] == false, || {
                format!("{at}: the {part} cannot be used (inert, covered or clipped): {reach}")
            })?;
        }
    }
    let close = &seen["reach"]["close"];
    ensure(close.is_null() || (close["hit"] == true && close["clipped"] == false && close["inert"] == false), || {
        format!("{at}: the column's close button cannot be used: {close}")
    })?;
    ensure(num(seen, &["col", "w"]) <= 760.5 || seen["col"].is_null(), || format!("{at}: the reading column is {}", seen["col"]["w"]))
}

fn follows_the_width(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    for (width, height) in [(1440, 900), (1100, 700), (DOCKS_FROM, 700), (DOCKS_FROM - 1, 700), (900, 700), (899, 700), (720, 480)] {
        set_size(page, width, height)?;
        let at = format!("{width}x{height}");
        let closed = page.eval(SHELL)?;
        check_shell(&at, &closed, width, false)?;
        ensure(closed["docked"].is_null() && closed["over"].is_null(), || format!("{at}: the column starts closed"))?;
        // Opened by the user.
        run_button(page)?;
        page.wait_visible(&css(".runcol"))?;
        pause(0.45);
        let open = page.eval(SHELL)?;
        check_shell(&format!("{at}, column open"), &open, width, width < DOCKS_FROM)?;
        if width == 1440 {
            // Nothing runs: the column shows Curioso, 64 px, and his drawing does not move.
            let peek = page.eval(
                "(() => { const s = document.querySelector('.rc-empty svg'); const r = s.getBoundingClientRect(); return { w: r.width, h: r.height, still: getComputedStyle(s).animationName === 'none' && document.getAnimations().every((a) => !s.contains(a.effect.target)) }; })()",
            )?;
            same((peek["w"].as_f64(), peek["h"].as_f64()), (Some(64.0), Some(64.0)), "Curioso in the empty column")?;
            same(peek["still"].as_bool(), Some(true), "Curioso is still")?;
        }
        if width >= DOCKS_FROM {
            // The room beside the 760 px reading column (the window less the sidebar and 800 px), 360 to 560 px, with the 8 px of
            // frame on each side of the column, its own card beside the stage.
            same(
                num(&open, &["docked", "w"]) + 16.0,
                (f64::from(width) - 288.0 - 800.0).clamp(360.0, 560.0).min(f64::from(width) - 288.0 - 440.0),
                &format!("{at}: the docked column"),
            )?;
            ensure(num(&open, &["main", "w"]) >= 439.5, || format!("{at}: the stage is {} px", open["main"]["w"]))?;
            ensure(open["modal"].is_null(), || format!("{at}: a docked column is not a dialog"))?;
        } else {
            ensure(!open["over"].is_null(), || format!("{at}: the column should lie over the stage"))?;
            ensure(num(&open, &["over", "b"]) <= num(&open, &["composer", "y"]) + 0.5, || {
                format!("{at}: the column reaches the composer: {}", open["over"])
            })?;
            same(open["modal"].as_str(), Some("true"), &format!("{at}: opened by the user, the column is modal"))?;
            ensure(open["focusIn"] == true, || format!("{at}: focus should move into the column"))?;
        }
        page.screenshot(&ctx.paths.shots.join(format!("shell-{width}-column.png")))?;
        close_column(page, width)?;
        if width < 900 {
            // The sidebar is a modal dialog behind its button; Esc closes it and focus goes back to the button.
            let menu = page.named("button", "side.show")?;
            page.click(&menu)?;
            page.wait_visible(&css(".side.shown"))?;
            pause(0.3);
            page.screenshot(&ctx.paths.shots.join(format!("shell-{width}-sidebar.png")))?;
            same(page.attr(&css(".side.shown"), "aria-modal")?, Some("true".to_string()), &format!("{at}: the sidebar overlay is modal"))?;
            page.press("Escape")?;
            page.wait_gone(&css(".side.shown"))?;
            same(
                page.eval("document.activeElement === document.querySelector('.topbar .menu')")?.as_bool(),
                Some(true),
                "focus goes back to the menu button",
            )?;
        }
    }
    // The window narrows and widens while the column and the sidebar are open: each follows the width, nothing sticks out,
    // and the controls work again once the width allows.
    set_size(page, 1440, 900)?;
    run_button(page)?;
    page.wait_visible(&css(".shell > .runcol"))?;
    set_size(page, 1000, 700)?;
    page.wait_visible(&css(".work > .runcol"))?;
    check_shell("1440 narrowed to 1000, column open", &page.eval(SHELL)?, 1000, true)?;
    set_size(page, 1440, 900)?;
    page.wait_visible(&css(".shell > .runcol"))?;
    close_column(page, 1440)?;
    set_size(page, 720, 480)?;
    let menu = page.named("button", "side.show")?;
    page.click(&menu)?;
    page.wait_visible(&css(".side.shown"))?;
    set_size(page, 1000, 700)?;
    page.wait_gone(&css(".side.shown"))?;
    check_shell("720 widened to 1000, sidebar was open", &page.eval(SHELL)?, 1000, false)?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn fits_large_text(ctx: &mut Ctx) -> Result {
    ctx.write_settings(
        &json!({ "language": "en", "setup": "done", "appearance": { "base": "sala", "textSize": 18 }, "tutorialDone": true }),
    )?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&heading)?;
    same(page.eval("parseFloat(getComputedStyle(document.body).fontSize)")?.as_f64(), Some(18.0), "the text size")?;
    for (width, height) in [(1440, 900), (1100, 700), (DOCKS_FROM, 700), (DOCKS_FROM - 1, 700), (900, 700), (899, 700), (720, 480)] {
        set_size(page, width, height)?;
        let at = format!("{width}x{height} at 18 px");
        check_shell(&at, &page.eval(SHELL)?, width, false)?;
        run_button(page)?;
        page.wait_visible(&css(".runcol"))?;
        pause(0.45);
        let open = page.eval(SHELL)?;
        check_shell(&format!("{at}, column open"), &open, width, width < DOCKS_FROM)?;
        // The run button and the model chip stay whole in the top bar, and the column's own text fits its width.
        let clipped = page.eval(
            "[...document.querySelectorAll('.topbar .right *, .runcol *')].filter((e) => e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === 'visible' && e.children.length === 0).map((e) => e.className || e.tagName)",
        )?;
        ensure(clipped.as_array().is_some_and(Vec::is_empty), || format!("{at}: text spills out of its box: {clipped}"))?;
        close_column(page, width)?;
    }
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

/// A launch that stops at a question: the recording asks to write two files and waits for the answers.
pub(super) fn two_writes(ctx: &Ctx) -> Result<Launch> {
    // Card scenarios explicitly choose Ask; the app's default now applies to Chat too.
    let path = ctx.sb.data.join("settings.json");
    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    settings["permissions"] = "ask".into();
    std::fs::write(path, settings.to_string()).map_err(|e| e.to_string())?;
    Ok(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    })
}

/// Sends the request and returns once the first question has been asked.
pub(super) fn ask_two_writes(page: &mut Page) -> Result {
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    page.wait_visible(&css("article .ask").has_text("a.txt"))
}

fn opens_by_itself(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    // Not at once: the user has just typed. It comes 1.5 s after the last key.
    let column = css(".runcol");
    page.wait_visible(&column)?;
    let seen = page.eval(SHELL)?;
    ensure(seen["modal"].is_null(), || "a column that opens by itself is not a dialog".to_string())?;
    ensure(seen["focusIn"] == false, || "a column that opens by itself does not take focus".to_string())?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    page.wait_visible(&css(".run-btn.need"))?;
    // The row that waits names its vendor, like every other row.
    page.wait_visible(&css(".runcol .ri.need .vm").has_text("Claude"))?;
    let words = page.text_with("run.needs", &json!({ "n": 1 }), None)?;
    let button = page.inner_text(&css(".run-btn"))?;
    ensure(button.contains(&words), || format!("the amber button should say {words:?}: {button:?}"))?;
    // A question with a long path stays inside the reading column and the stage, beside the docked column.
    let inside = page.eval("(() => { const stage = document.querySelector('.main').getBoundingClientRect().right; return [...document.querySelectorAll('article .ask, article .activity summary')].every((e) => e.getBoundingClientRect().right <= stage + 0.5); })()")?;
    same(inside.as_bool(), Some(true), "the card and the activity row stay inside the stage")?;
    ctx.shot(&mut app, "running-column-asks")?;
    let page = &mut app.page;
    // Closing it leaves only the button amber, and the same question does not open it again.
    let close = page.named("button", "run.close")?;
    page.click(&close)?;
    page.wait_gone(&column)?;
    pause(2.0);
    ensure(!page.is_visible(&column)?, || "the column opened again for the same question".to_string())?;
    ensure(page.is_visible(&css(".run-btn.need"))?, || "the button should stay amber while a question waits".to_string())?;
    // The button brings it back, and the card there answers the question; the card in the thread goes with it.
    run_button(page)?;
    page.wait_visible(&column)?;
    let allow = page.text_of("ask.allow")?;
    let in_column = css(".runcol .ask").has_text("a.txt");
    page.click(&in_column.clone().role("button", &allow).exact())?;
    page.wait_gone(&css(".ask").has_text("a.txt"))?;
    // The next question is a new one: the column comes by itself again (it was open already, so it stays).
    page.wait_visible(&css(".runcol .ask").has_text("b.txt"))?;
    let deny = page.text_of("ask.deny")?;
    page.click(&css(".runcol .ask").has_text("b.txt").role("button", &deny).exact())?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "no card should be left".to_string())?;
    // Answered, nothing waits any more: the button is no longer amber.
    page.wait_gone(&css(".run-btn.need"))
}

/// A question that waits when the screen loads (a window rebuilt after closing to the tray, seen from the engine): the
/// button is amber on first paint and the column opens once.
fn opens_at_load(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    page.reload()?;
    page.wait_visible(&css(".run-btn.need"))?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    // Closed by the user, it stays closed for the same question.
    let close = page.named("button", "run.close")?;
    page.click(&close)?;
    pause(2.0);
    ensure(!page.is_visible(&css(".runcol"))?, || "the column opened again for the same question".to_string())
}

fn overlays_the_stage(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    set_size(page, 1000, 700)?;
    ask_two_writes(page)?;
    let column = css(".work > .runcol");
    page.wait_visible(&column)?;
    pause(0.45);
    let seen = page.eval(SHELL)?;
    ensure(seen["modal"].is_null(), || "a column that opens by itself is not a dialog at any width".to_string())?;
    ensure(seen["focusIn"] == false, || "it does not take focus".to_string())?;
    ensure(num(&seen, &["over", "b"]) <= num(&seen, &["composer", "y"]) + 0.5, || format!("it lies over the composer: {}", seen["over"]))?;
    // Esc closes it only while focus is inside it: the composer has focus, so the column stays.
    let composer = page.labelled("composer.message")?;
    page.click(&composer)?;
    page.press("Escape")?;
    pause(0.3);
    ensure(page.is_visible(&column)?, || "Esc closed a column that did not have focus".to_string())?;
    ctx.shot(&mut app, "running-column-over")?;
    let page = &mut app.page;
    // Opened by the user it is a modal dialog: focus moves in, Esc closes it, and focus goes back to the button.
    run_button(page)?;
    page.wait_gone(&column)?;
    run_button(page)?;
    page.wait_visible(&column)?;
    let seen = page.eval(SHELL)?;
    same(seen["modal"].as_str(), Some("true"), "opened by the user")?;
    ensure(seen["focusIn"] == true, || "focus should be in the column".to_string())?;
    page.press("Escape")?;
    page.wait_gone(&column)?;
    same(
        page.eval("document.activeElement === document.querySelector('.run-btn')")?.as_bool(),
        Some(true),
        "focus goes back to the button",
    )?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn opens_a_running_conversation(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    // Leave the conversation for a new one; it keeps running, and the column still lists it.
    let new = page.named("button", "side.new")?;
    page.click(&new)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    page.wait_visible(&css(".runcol .ri .name"))?;
    page.click(&css(".runcol .ri .name"))?;
    // The conversation is back, with its question, and the crumb names it.
    page.wait_visible(&css("article .ask").has_text("a.txt"))?;
    page.wait_for(
        "the crumb to name the conversation",
        "document.querySelector('.crumb b')?.textContent.length > 0 && !document.querySelector('.crumb b').textContent.includes('New')",
    )?;
    let allow = page.text_of("ask.allow")?;
    page.click(&css("article .ask").has_text("a.txt").role("button", &allow).exact())?;
    page.wait_visible(&css("article .ask").has_text("b.txt"))
}

fn opens_a_project_conversation(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut settings: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(ctx.sb.data.join("settings.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    settings["permissions"] = "ask".into();
    std::fs::write(ctx.sb.data.join("settings.json"), settings.to_string()).map_err(|e| e.to_string())?;
    // One project, so the screen can send into it.
    seed_project(ctx, "Demo")?;
    let conversations = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&conversations).map_err(|e| e.to_string())?;
    let saved = json!({"id":"legacy", "title":"Legacy history", "area":"project", "projectId":"demo",
        "sessions":{}, "createdAt":1, "updatedAt":1,
        "messages":[{"id":"answer", "role":"assistant", "text":"An old project answer", "at":1, "status":"done"}]});
    std::fs::write(conversations.join("legacy.json"), saved.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Demo"))?;
    page.wait_visible(&css(".welcome-grid .p-header h2").has_text("Demo"))?;
    page.click(&css(".proj-chats .item").has_text("Legacy history"))?;
    page.wait_visible(&css("article").has_text("An old project answer"))?;
    page.wait_visible(&css(".work .composer"))?;
    ensure(ctx.records()?.is_empty(), || "opening a project conversation started a CLI".into())
}

fn greets_and_suggests(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    same(page.inner_text(&title)?, page.text_of("empty.chatTitle")?, "the greeting says no name")?;
    // The heading is at most 9vw in its font size, at the widths the spec names.
    for (width, height) in [(1100, 700), (1440, 900), (720, 480)] {
        set_size(page, width, height)?;
        let found = page.eval("(() => { const h = document.querySelector('.hello'); return { size: parseFloat(getComputedStyle(h).fontSize), wide: innerWidth, past: h.getBoundingClientRect().right > innerWidth + 0.5, page: document.documentElement.scrollWidth <= innerWidth }; })()")?;
        ensure(found["size"].as_f64().unwrap_or(f64::MAX) <= width as f64 * 0.09 + 0.5, || {
            format!("{width}: the heading is {} px", found["size"])
        })?;
        ensure(found["past"] == false && found["page"] == true, || format!("{width}: the greeting sticks out: {found}"))?;
    }
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    // Three suggestions; one fills the composer and sends nothing.
    same(page.count(&css(".try button"))?, 3, "suggestions")?;
    // One column: no line under the greeting, and the suggestions are short chips in one row under the composer.
    let chips = page.eval(
        "(() => { const c = document.querySelector('.welcome-grid .composer').getBoundingClientRect(); const tops = [...document.querySelectorAll('.try button')].map((b) => Math.round(b.getBoundingClientRect().top)); return { row: new Set(tops).size === 1, under: tops[0] >= c.bottom, lead: document.querySelectorAll('.welcome-grid .lead').length }; })()",
    )?;
    ensure(chips["row"] == true && chips["under"] == true, || format!("the suggestions are not one row under the composer: {chips}"))?;
    same(chips["lead"].as_u64(), Some(0), "lines under the Chat greeting")?;
    // A hover changes only the chip's colour, never its box.
    let first = css(".try button").has_text(&page.text_of("suggest.1")?);
    let size = "(() => { const r = document.querySelector('.try button').getBoundingClientRect(); return [r.width, r.height]; })()";
    let before = page.eval(size)?;
    page.hover(&first)?;
    pause(0.4);
    same(page.eval(size)?, before, "the chip's box on hover")?;
    ensure(page.count(&css(".resume"))? == 0, || "there is nothing to pick up yet".to_string())?;
    let wanted = page.text_of("suggest.2")?;
    page.click(&css(".try button").has_text(&wanted))?;
    let composer = page.labelled("composer.message")?;
    let value = page.eval_on(&composer, "return { value: el.value, focused: document.activeElement === el };")?;
    same(value["value"].as_str(), Some(wanted.as_str()), "the composer holds the suggestion")?;
    same(value["focused"].as_bool(), Some(true), "the composer has focus")?;
    pause(0.4);
    let articles = page.named("article", "answer.label")?;
    same(page.count(&articles)?, 0, "the suggestion was sent")?;
    ctx.shot(&mut app, "greeting")?;
    // After a conversation exists, "pick up" lists it.
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    let new = page.named("button", "side.new")?;
    page.click(&new)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    let row = css(".resume .rrow").has_text(QUESTION);
    page.wait_visible(&row)?;
    let placed = page.eval(
        "(() => { const r = document.querySelector('.welcome-grid .w-main .resume'); const t = document.querySelector('.try'); return !!r && r.getBoundingClientRect().top >= t.getBoundingClientRect().bottom; })()",
    )?;
    same(placed.as_bool(), Some(true), "what to pick up sits in the one column, under the suggestions")?;
    ctx.shot(&mut app, "greeting-pick-up")?;
    let page = &mut app.page;
    page.click(&row)?;
    page.wait_visible(&actions)
}

fn gives_way_to_no_ai(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.noAiTitle")?;
    page.wait_visible(&title)?;
    same(page.count(&css(".hello"))?, 0, "a greeting with no AI")?;
    same(page.count(&css(".try"))?, 0, "suggestions with no AI")?;
    same(page.count(&css(".empty svg"))?, 1, "Curioso")?;
    let size = page.eval(
        "(() => { const r = document.querySelector('.empty svg').getBoundingClientRect(); return { w: r.width, h: r.height }; })()",
    )?;
    same((size["w"].as_f64(), size["h"].as_f64()), (Some(64.0), Some(64.0)), "Curioso's size")?;
    let connect = page.named("button", "empty.noAiButton")?;
    page.click(&connect)?;
    let setup = page.named("heading", "setup.title")?;
    page.wait_visible(&setup)
}

/// The recording `claude/chat.jsonl` with its reset times moved to the future: a reading past its reset says nothing.
pub(super) fn chat_with_future_resets(ctx: &Ctx) -> std::result::Result<Launch, String> {
    let source = ctx.paths.fixtures.join("claude/chat.jsonl");
    let text = std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    let text = text.replace("1791627000", &(now + 3 * 3600).to_string()).replace("1791878400", &(now + 3 * 86400).to_string());
    let copy = ctx.sb.data.join("chat-future.jsonl");
    std::fs::write(&copy, text).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok(Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording: Some(copy), ..Launch::default() })
}

fn shows_the_quota(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_future_resets(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let share = css(".usage .vendor").has_text("Claude").css(".share");
    let unknown = page.text_of("quota.unknown")?;
    page.wait_visible(&share)?;
    same(page.inner_text(&share)?, unknown, "before the CLI has reported")?;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    // The tightest window of the vendor: 47 % of the 7-day one, not 7 % of the 5-hour one.
    page.wait_for("the quota to show", "/^(≥ )?47%$/.test(document.querySelector('.usage .share')?.textContent ?? '')")?;
    ctx.shot(&mut app, "quota")?;
    let page = &mut app.page;
    // A screen that loads again asks the engine and shows the same reading.
    page.reload()?;
    page.wait_for("the quota after a reload", "/^(≥ )?47%$/.test(document.querySelector('.usage .share')?.textContent ?? '')")
}

// ---- Seeded conversations and the checks of the second pass of the review.

/// Writes a finished conversation of two messages to the data folder, newest by `at`.
pub(super) fn seed(ctx: &Ctx, id: &str, title: &str, project: Option<&str>, at: u64) -> Result {
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let messages = json!([
        { "id": format!("{id}-u"), "role": "user", "text": format!("{title}: the question"), "at": at },
        { "id": format!("{id}-a"), "role": "assistant", "text": format!("{title}: the answer"), "at": at + 1, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" },
    ]);
    let mut conversation =
        json!({ "id": id, "title": title, "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at, "messages": messages });
    if let Some(project) = project {
        conversation["area"] = json!("project");
        conversation["projectId"] = json!(project);
    }
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())
}

/// Writes `projects.json` with one project, `demo`, and its folder.
pub(super) fn seed_project(ctx: &Ctx, name: &str) -> Result {
    let folder = ctx.sb.data.join("demo");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let projects = json!([{ "id": "demo", "name": name, "path": folder.to_string_lossy() }]);
    std::fs::write(ctx.sb.data.join("projects.json"), projects.to_string()).map_err(|e| e.to_string())
}

/// What has focus: where it sits and what it is, in a few words.
fn focus_of(page: &mut Page) -> Result<Value> {
    page.eval(
        "(() => { const a = document.activeElement; return { column: !!a && !!a.closest('.runcol'), side: !!a && !!a.closest('.side'), run: !!a && a.classList.contains('run-btn'), composer: !!a && a.matches('.composer textarea'), what: a ? a.tagName + ':' + (a.getAttribute('aria-label') || a.textContent || '').slice(0, 24) : 'none' }; })()",
    )
}

/// Presses `chord` many times and checks that focus stays inside `place` ("column" or "side") and that it did move: a
/// dialog that swallowed every Tab would pass the first half alone.
fn tab_stays_in(page: &mut Page, place: &str, at: &str) -> Result {
    let mut visited = std::collections::BTreeSet::new();
    for chord in ["Tab", "Shift+Tab"] {
        for _ in 0..14 {
            page.press(chord)?;
            let focus = focus_of(page)?;
            ensure(focus[place] == true, || format!("{at}: {chord} took focus out of the {place}: {}", focus["what"]))?;
            visited.insert(focus["what"].to_string());
        }
    }
    ensure(visited.len() >= 3, || format!("{at}: focus did not move through the dialog: {visited:?}"))
}

/// A delay for the `open` command of the engine, so that a conversation takes `ms` to arrive. The page's `invoke` cannot
/// be replaced, but its requests go through `fetch`, which can. `window.__slowed` counts the requests that were held.
fn delay_open(page: &mut Page, ms: u32) -> Result {
    page.eval(&format!(
        "(() => {{ const real = window.fetch.bind(window); window.__slowed = 0;          window.fetch = (url, init) => String(url).endsWith('/open') ? (window.__slowed++, new Promise((r) => setTimeout(r, {ms})).then(() => real(url, init))) : real(url, init); return true; }})()"
    ))
    .map(drop)
}

fn waits_for_the_conversation(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    seed_project(ctx, "Demo")?;
    seed(ctx, "5f1c0000-0000-4000-8000-0000000000a1", "Alpha", None, 1_790_000_000_000)?;
    seed(ctx, "5f1c0000-0000-4000-8000-0000000000b1", "Beta", None, 1_790_000_100_000)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    page.click(&css(".list .item").has_text("Alpha"))?;
    page.wait_visible(&css("article").has_text("Alpha: the answer"))?;
    // Beta takes 2.5 s to arrive. The screen still shows Alpha, so nothing may go out for either.
    delay_open(page, 2500)?;
    page.click(&css(".list .item").has_text("Beta"))?;
    same(page.eval("window.__slowed")?.as_u64(), Some(1), "the open of Beta was held back by the test")?;
    page.wait_for("the crumb to name Beta", "document.querySelector('.crumb b')?.textContent === 'Beta'")?;
    ensure(page.is_visible(&css("article").has_text("Alpha: the answer"))?, || {
        "the old conversation should still be on screen".to_string()
    })?;
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, "a message for Beta")?;
    let send = page.named("button", "composer.send")?;
    ensure(page.is_disabled(&send)?, || "a message could be sent while the conversation was still opening".to_string())?;
    page.press_on(&composer, "Enter")?;
    pause(0.3);
    let value = page.eval_on(&composer, "return { value: el.value };")?;
    same(value["value"].as_str(), Some("a message for Beta"), "Enter does not send while the conversation opens")?;
    // Once Beta is in, the draft is still there, belongs to Beta and can go.
    page.wait_visible(&css("article").has_text("Beta: the answer"))?;
    page.wait_enabled(&send)?;
    let value = page.eval_on(&composer, "return { value: el.value };")?;
    same(value["value"].as_str(), Some("a message for Beta"), "the draft written while Beta opened")
}

fn leaves_the_greeting_composer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    set_size(page, DOCKS_FROM - 1, 700)?;
    ask_two_writes(page)?;
    // The question is shown by itself, over the stage; then the user starts a new conversation without closing it.
    page.wait_visible(&css(".work > .runcol .ask").has_text("a.txt"))?;
    page.press("Control+n")?;
    let greeting = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&greeting)?;
    for (width, height) in [(DOCKS_FROM - 1, 700), (899, 700), (720, 480)] {
        set_size(page, width, height)?;
        page.wait_visible(&css(".work.under .thread.welcome"))?;
        let at = format!("{width}x{height}, the greeting with the column over the stage");
        pause(0.3);
        check_shell(&at, &page.eval(SHELL)?, width, false)?;
        let found = page.eval(
            "(() => { const c = document.querySelector('.composer').getBoundingClientRect(); const k = document.querySelector('.work > .runcol').getBoundingClientRect(); return { apart: c.right <= k.left + 0.5, wide: c.width }; })()",
        )?;
        ensure(found["apart"] == true, || format!("{at}: the column covers the composer: {found}"))?;
        ensure(num(&found, &["wide"]) >= 240.0, || format!("{at}: the composer is only {} px wide", found["wide"]))?;
        // The user can write in it: a click lands in the box, and the column keeps out of the way.
        let composer = page.labelled("composer.message")?;
        page.click(&composer)?;
        ensure(focus_of(page)?["composer"] == true, || format!("{at}: a click on the composer did not focus it"))?;
    }
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn keeps_focus_in_dialogs(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    set_size(page, 1000, 700)?;
    ask_two_writes(page)?;
    page.wait_visible(&css(".work > .runcol .ask").has_text("a.txt"))?;
    // From the keyboard: Ctrl+. closes the column that opened itself, and opens it again as a modal dialog.
    page.press("Control+.")?;
    page.wait_gone(&css(".runcol"))?;
    page.press("Control+.")?;
    page.wait_visible(&css(".work > .runcol"))?;
    page.wait_for("focus to move into the column", "!!document.activeElement.closest('.runcol')")?;
    same(page.attr(&css(".runcol"), "aria-modal")?, Some("true".to_string()), "the column opened from the keyboard is modal")?;
    tab_stays_in(page, "column", "1000, the running column")?;
    // The window widens: the column sits beside the stage and is no longer a dialog, and focus is on the run button
    // (the top bar works again). Narrowing makes it a dialog again and focus goes in.
    set_size(page, 1440, 900)?;
    page.wait_visible(&css(".shell > .runcol"))?;
    page.wait_for("focus to go to the run button", "document.activeElement.classList.contains('run-btn')")?;
    set_size(page, 1000, 700)?;
    page.wait_visible(&css(".work > .runcol"))?;
    page.wait_for("focus to go into the column", "!!document.activeElement.closest('.runcol')")?;
    // Ctrl+N from inside the dialog: the column goes, and the new conversation's composer has focus.
    page.press("Control+n")?;
    page.wait_gone(&css(".runcol"))?;
    page.wait_for("the composer to have focus", "document.activeElement === document.querySelector('.composer textarea')")?;
    // The sidebar overlay of a narrow window is a dialog too.
    set_size(page, 720, 480)?;
    let menu = page.named("button", "side.show")?;
    page.click(&menu)?;
    page.wait_visible(&css(".side.shown"))?;
    page.wait_for("focus to move into the sidebar", "!!document.activeElement.closest('.side')")?;
    tab_stays_in(page, "side", "720, the sidebar")?;
    // Two dialogs never stack: Ctrl+. with the sidebar open swaps it for the column, which is whole and not inert.
    page.press("Control+.")?;
    page.wait_gone(&css(".side.shown"))?;
    page.wait_visible(&css(".work > .runcol"))?;
    page.wait_for("focus to move into the column", "!!document.activeElement.closest('.runcol')")?;
    same(
        page.eval("!document.querySelector('.runcol').closest('[inert]')")?.as_bool(),
        Some(true),
        "the column is not inert under the sidebar",
    )?;
    tab_stays_in(page, "column", "720, the column that replaced the sidebar")?;
    page.press("Escape")?;
    page.wait_gone(&css(".runcol"))?;
    // The same chord does nothing while Settings is open: it would change a column nobody can see.
    page.press("Control+,")?;
    page.wait_gone(&css(".shell"))?;
    page.press("Control+.")?;
    page.press("Escape")?;
    page.wait_visible(&css(".shell"))?;
    pause(0.3);
    ensure(!page.is_visible(&css(".runcol"))?, || "Ctrl+. in Settings opened the column behind it".to_string())?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

/// Presses `chord` in the composer every 0.3 s until the question of the recording is on screen, then a little longer.
fn keys_until_the_question(page: &mut Page, chords: &[&str]) -> Result<std::time::Instant> {
    let card = css("article .ask").has_text("a.txt");
    let mut after = 0;
    for turn in 0..60 {
        page.press(chords[turn % chords.len()])?;
        let last = std::time::Instant::now();
        ensure(!page.is_visible(&css(".runcol"))?, || "the column opened while keys were still coming".to_string())?;
        pause(0.3);
        if page.is_visible(&card)? {
            after += 1;
            // Finish the whole key cycle: its net caret movement must not depend on when the question arrived.
            if after >= 4 && (turn + 1) % chords.len() == 0 {
                return Ok(last);
            }
        }
    }
    Err("the question never arrived".to_string())
}

fn waits_for_every_key(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut launch = two_writes(ctx)?;
    let mut recording =
        std::fs::read_to_string(launch.recording.as_ref().ok_or("the tool recording is missing")?).map_err(|e| e.to_string())?;
    let permission =
        recording.find("{\"dir\":\"out\",\"msg\":{\"type\":\"control_request\"").ok_or("the tool recording has no permission request")?;
    recording.insert_str(permission, "{\"dir\":\"out\",\"msg\":{\"faber_hold\":true}}\n");
    let held = ctx.sb.sandbox.join("typing-question.jsonl");
    std::fs::write(&held, recording).map_err(|e| e.to_string())?;
    launch.recording = Some(held);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    // The title can arrive before the snapshot replaces the welcome composer with the conversation's composer.
    page.wait_visible(&css(".dock .composer textarea"))?;
    page.wait_enabled(&model)?;
    ensure(!page.is_visible(&css("article .ask"))?, || "the question arrived before typing setup was ready".to_string())?;
    ensure(!page.is_visible(&css(".runcol"))?, || "the column opened before typing setup was ready".to_string())?;
    let composer = page.labelled("composer.message")?;
    // As in `ask`, establish native input focus on the replacement field before typing through DevTools.
    page.click(&composer)?;
    page.fill(&composer, "hello")?;
    page.wait_for(
        "the typed text and its caret",
        "(() => { const f = document.querySelector('.dock .composer textarea'); return document.activeElement === f && f.value === 'hello' && f.selectionStart === 5 && f.selectionEnd === 5; })()",
    )?;
    page.press("ArrowLeft")?;
    page.press("ArrowLeft")?;
    let typed_at = std::time::Instant::now();
    let initial = page.eval_on(&composer, "return { text: el.value, at: el.selectionStart, end: el.selectionEnd };")?;
    same(initial["text"].as_str(), Some("hello"), "the draft before the snapshot arrives")?;
    same((initial["at"].as_u64(), initial["end"].as_u64()), (Some(3), Some(3)), "the initial caret")?;
    ensure(typed_at.elapsed() < Duration::from_millis(1400), || {
        "typing setup exceeded the quiet window before releasing the question".to_string()
    })?;
    // Hold the question, rather than the open request, until the replacement composer is ready and has received real keys.
    std::fs::write(ctx.sb.state.join("release"), "").map_err(|e| e.to_string())?;
    // Only arrow keys from here on: they move the caret and change no text, so no `input` event comes. The question
    // arrives meanwhile, and the column must keep out of the way for as long as the keys come.
    let last = keys_until_the_question(page, &["ArrowLeft", "ArrowRight"])?;
    // Not 1.5 s after the last key yet: still closed.
    let closed_for = last.elapsed();
    ensure(closed_for < Duration::from_millis(1400), || format!("too slow to look: {closed_for:?}"))?;
    ensure(!page.is_visible(&css(".runcol"))?, || format!("the column opened {closed_for:?} after the last key"))?;
    same(
        page.eval_on(&composer, "return [el.selectionStart, el.selectionEnd];")?,
        json!([3, 3]),
        "the keys returned the caret to 3 before the column opens",
    )?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    let waited = last.elapsed();
    ensure(waited >= Duration::from_millis(1450), || format!("the column opened {waited:?} after the last key"))?;
    let now = page.eval_on(
        &composer,
        "return { focused: document.activeElement === el, at: el.selectionStart, end: el.selectionEnd, value: el.value };",
    )?;
    same(now["focused"].as_bool(), Some(true), "focus is still in the composer")?;
    same((now["at"].as_u64(), now["end"].as_u64()), (Some(3), Some(3)), "the caret has not moved")?;
    same(now["value"].as_str(), Some("hello"), "the text")?;
    let seen = page.eval(SHELL)?;
    ensure(seen["modal"].is_null() && seen["focusIn"] == false, || {
        "the column that opened by itself is not a dialog and has no focus".to_string()
    })
}

fn dismissal_sticks(ctx: &mut Ctx) -> Result {
    dismissal(ctx, false, 1440)
}

fn dismissal_sticks_reduced(ctx: &mut Ctx) -> Result {
    dismissal(ctx, true, 1440)
}

fn dismissal_sticks_narrow(ctx: &mut Ctx) -> Result {
    dismissal(ctx, false, 1000)
}

/// The column the user opens and closes again before it would have opened itself. Where it does not dock (under `DOCKS_FROM`) it is a dialog over the
/// stage and the run button behind it is inert, so Esc closes it there; the window is sized here because a CI runner's
/// screen may open it narrower than a desk does.
fn dismissal(ctx: &mut Ctx, reduce: bool, width: u32) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    system_motion(page, reduce)?;
    set_size(page, width, 900)?;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    page.wait_for(
        "the conversation to open",
        "document.querySelector('.crumb b')?.textContent.length > 0 && !document.querySelector('.crumb b').textContent.includes('New')",
    )?;
    let composer = page.labelled("composer.message")?;
    page.fill(&composer, "x")?;
    // Keys keep coming until the question is on screen; the next moment the user opens the column and closes it again,
    // well inside the 1.5 s the column would have waited.
    let card = css("article .ask").has_text("a.txt");
    for _ in 0..60 {
        page.press("ArrowLeft")?;
        if page.is_visible(&card)? {
            break;
        }
        pause(0.25);
    }
    page.wait_visible(&card)?;
    ensure(!page.is_visible(&css(".runcol"))?, || "the column opened before the user did".to_string())?;
    // One more key starts the 1.5 s wait afresh, so a slow moment of the machine between the last key and the click cannot
    // let the column open by itself first.
    page.press("ArrowLeft")?;
    run_button(page)?;
    page.wait_visible(&css(".runcol"))?;
    if width >= DOCKS_FROM {
        run_button(page)?;
    } else {
        page.press("Escape")?;
    }
    page.wait_gone(&css(".runcol"))?;
    // The wait ends here; the question is the same one the user has just seen, so the column does not come back.
    pause(3.0);
    ensure(!page.is_visible(&css(".runcol"))?, || "the column opened again for a question the user had dismissed".to_string())?;
    ensure(page.is_visible(&css(".run-btn.need"))?, || "the button should stay amber".to_string())
}

fn keeps_long_titles_inside(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let long = "A_title_that_never_breaks_because_it_has_no_spaces_in_it_at_all_and_goes_on_and_on";
    seed_project(ctx, "A_project_name_that_never_breaks_because_it_has_no_spaces_in_it_either")?;
    seed(ctx, "5f1c0000-0000-4000-8000-0000000000c1", long, None, 1_790_000_000_000)?;
    seed(ctx, "5f1c0000-0000-4000-8000-0000000000d1", long, Some("demo"), 1_790_000_100_000)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    pause(0.3);
    // Every row ends inside the sidebar, and its title is cut with an ellipsis. The button on the right shows when the row is
    // hovered or holds the focus, and the title then makes room for it, so each row takes the focus before it is measured.
    let measure = "(() => { const sc = document.querySelector('.side-scroll'); const edge = sc.getBoundingClientRect().right; const bad = []; \
         for (const row of document.querySelectorAll('.side .crow, .side .prow')) { \
           const item = row.querySelector('.item'); item.focus(); const title = item.querySelector(':scope > span'); const button = row.querySelector('.x, .plus'); \
           const r = row.getBoundingClientRect(), t = title.getBoundingClientRect(), b = button.getBoundingClientRect(); \
           if (r.right > edge + 0.5) bad.push('row ends at ' + r.right); \
           if (getComputedStyle(title).textOverflow !== 'ellipsis' || title.scrollWidth <= title.clientWidth) bad.push('no ellipsis: ' + title.textContent.slice(0, 12)); \
           if (t.right > b.left + 0.5) bad.push('title runs under the button: ' + t.right + ' > ' + b.left); } \
         return { bad, rows: document.querySelectorAll('.side .crow, .side .prow').length, still: sc.scrollWidth <= sc.clientWidth + 1 }; })()";
    // Each area lists only its own: the chat in Chat, then the project with its conversation in Projects.
    let in_chat = page.eval(measure)?;
    ensure(num(&in_chat, &["rows"]) >= 1.0 && page.count(&css(".side .prow"))? == 0, || format!("Chat lists only chats: {in_chat}"))?;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item"))?;
    page.wait_visible(&css(".proj-chats .item"))?;
    pause(0.3);
    let in_projects = page.eval(measure)?;
    ensure(num(&in_projects, &["rows"]) >= 2.0 && page.count(&css(".side .list .crow"))? == 0, || {
        format!("Projects lists the project and its conversation: {in_projects}")
    })?;
    for found in [in_chat, in_projects] {
        ensure(found["bad"].as_array().is_some_and(Vec::is_empty) && found["still"] == true, || format!("the sidebar: {found}"))?;
    }
    Ok(())
}

/// The recording `claude/subagent.jsonl`: a real turn in which the agent hands a search to an Explore subagent.
fn subagent_step(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/subagent.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(
        page,
        "Use the Agent tool exactly once (subagent_type Explore) to find which file in this folder defines load_config. Then answer in one short sentence.",
    )?;
    let step = css("article .agents .agent[data-state=done]");
    page.wait_visible(&step)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    // The step names the piece of work (waited for: the reply may still be settling in).
    page.wait_visible(&step.clone().css("b").has_text("Find file defining load_config"))?;
    ensure(!page.inner_text(&css("article.ai > .md"))?.contains("search recursively"), || "the subagent's brief joined the answer".into())?;
    // A click opens it in the running column (R161 step 3), with its report and without the CLI's frame.
    page.click(&step)?;
    let side = css("#running .side-body");
    page.wait_visible(&side.clone().css(".agent-report"))?;
    ensure(!page.inner_text(&side)?.contains("Subagent hand-back"), || "the report kept its frame".into())?;
    // The column finishes arriving before the picture.
    pause(0.6);
    // The step keeps inside the answer's column even with a long tool line in it.
    let spill = page.eval("(() => { const a = document.querySelector('article.ai').getBoundingClientRect(); const s = document.querySelector('.agents .agent'); return Math.max(s.getBoundingClientRect().right - a.right, s.scrollWidth - s.clientWidth); })()")?;
    ensure(spill.as_f64().is_some_and(|px| px <= 1.0), || format!("the opened step spills out of the column by {spill} px"))?;
    ctx.shot(&mut app, "subagent-step")
}

/// A finished reply whose subagent ran long commands and touched long paths, written to the data folder: what the side column
/// must wrap instead of cutting.
fn seed_long_subagent(ctx: &Ctx) -> Result {
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let deep = "crates/engine/src/very/deeply/nested/folder/with_a_long_name/another_long_folder_name/module.rs";
    let prompt = format!(
        "Find where `load_config` is defined and report every caller. Start in {deep} and do not edit anything.\n\n```\ncargo test --workspace --all-targets -- --nocapture {deep} --test-threads=1\n```\n\nReport the file and the line of each one, with the words around it."
    );
    let tools = json!([
        { "id": "a1", "name": "Agent", "ok": true, "output": "It is defined in config.rs, line 1, and called from main.rs.",
          "input": { "description": "Find file defining load_config", "subagent_type": "Explore", "prompt": prompt },
          "facts": { "edited": [deep, "build.rs"], "checks": 2, "failed": 1, "unnamed": [deep] } },
        { "id": "b1", "name": "Bash", "parent": "a1", "ok": true, "input": { "command": format!("cargo test --workspace --all-targets -- --nocapture {deep} --test-threads=1") } },
        { "id": "r1", "name": "Read", "parent": "a1", "ok": true, "input": { "file_path": deep } },
        { "id": "g1", "name": "Grep", "parent": "a1", "ok": true, "input": { "pattern": "load_config" } },
    ]);
    let messages = json!([
        { "id": "u", "role": "user", "text": "Find load_config", "at": 1_790_000_000_000u64 },
        { "id": "a", "role": "assistant", "text": "Done.", "at": 1_790_000_000_001u64, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done", "tools": tools },
    ]);
    let id = "5f1c0000-0000-4000-8000-0000000000e1";
    let conversation = json!({ "id": id, "title": "Subagent sizes", "area": "chat", "sessions": {}, "createdAt": 1_790_000_000_000u64, "updatedAt": 1_790_000_000_001u64, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())
}

/// What the opened subagent's view cuts or spills: lines wider than the column, a sideways scroll, a tool line not at two lines.
const SIDE_VIEW: &str = r#"(() => {
  const body = document.querySelector('.runcol .side-body');
  const edge = body.getBoundingClientRect().right;
  const spill = [...body.querySelectorAll('*')].filter((e) => !e.closest('svg') && e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().right > edge + 0.5).map((e) => String(e.className || e.tagName));
  const lines = [...body.querySelectorAll('.tools span.full i')].map((i) => getComputedStyle(i).getPropertyValue('-webkit-line-clamp'));
  const col = document.querySelector('.shell > .runcol, .work > .runcol').getBoundingClientRect();
  return { spill, scrolls: body.scrollWidth > body.clientWidth, lines, col: col.width, chips: document.querySelectorAll('.runcol .tools span.full').length };
})()"#;

fn subagent_column_en(ctx: &mut Ctx) -> Result {
    subagent_column(ctx, "en")
}
fn subagent_column_de(ctx: &mut Ctx) -> Result {
    subagent_column(ctx, "de")
}

/// The opened subagent at 1920, 1280 and 720 px: the column is as wide as the window allows, nothing in it is cut, and it can
/// fill the stage.
fn subagent_column(ctx: &mut Ctx, language: &str) -> Result {
    start_settings(ctx, language)?;
    seed_long_subagent(ctx)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    app.page.click(&css(".side .crow .item"))?;
    app.page.wait_visible(&css(".agents .agent"))?;
    for (width, height, column) in [(1920, 1080, 560.0), (1280, 800, 360.0), (720, 480, 0.0)] {
        set_size(&mut app.page, width, height)?;
        if app.page.count(&css(".runcol"))? == 0 {
            app.page.click(&css(".agents .agent"))?;
        }
        app.page.wait_visible(&css(".runcol .side-body .tools span.full"))?;
        pause(0.5);
        let seen = app.page.eval(SIDE_VIEW)?;
        let at = format!("{language} {width}x{height}");
        ensure(seen["spill"].as_array().is_some_and(Vec::is_empty) && seen["scrolls"] == false, || {
            format!("{at}: the subagent's view is cut: {seen}")
        })?;
        ensure(seen["lines"].as_array().is_some_and(|l| !l.is_empty() && l.iter().all(|c| c == "2")), || {
            format!("{at}: a tool line is not held to two lines: {seen}")
        })?;
        if column > 0.0 {
            // The track includes the 8 px of frame on each side of the column, its own card beside the stage.
            same(num(&seen, &["col"]) + 16.0, column, &format!("{at}: the column's width"))?;
        }
        ctx.shot(&mut app, &format!("subagent-column-{language}-{width}"))?;
        if width == 1920 {
            // The full thread: as wide as the stage, its text inside, the box below, and back.
            app.page.click(&css(".runcol .agent-wide"))?;
            app.page.wait_visible(&css(".work > .runcol.wide"))?;
            pause(0.5);
            let wide = app.page.eval(SIDE_VIEW)?;
            ensure(wide["spill"].as_array().is_some_and(Vec::is_empty) && wide["scrolls"] == false, || {
                format!("{at}, full thread: cut: {wide}")
            })?;
            ensure(num(&wide, &["col"]) > 1000.0, || format!("{at}: the full thread is {} px", wide["col"]))?;
            ensure(app.page.is_visible(&css(".dock .composer"))?, || "the box is hidden under the full thread".to_string())?;
            ctx.shot(&mut app, &format!("subagent-full-{language}-{width}"))?;
            app.page.click(&css(".runcol .agent-wide"))?;
            app.page.wait_visible(&css(".shell > .runcol"))?;
            pause(0.5);
        }
    }
    app.page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

/// Task 190: a click on the context meter shows what fills the context, as Claude Code counts it, and "Compact now" hands the
/// conversation to Claude Code's own `/compact`; the meter then says the summary's size. The recording is `chat.jsonl` with
/// Claude Code's answers to `get_context_usage` and `/compact` after it.
fn compacts_now(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/compact-now.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "In one sentence: why is the sky blue?")?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    let meter = css("[data-testid=context]");
    page.wait_visible(&meter)?;
    // The percent rounds both sizes to 0% of a large window; the tokens in the label tell them apart.
    let before = page.attr(&meter, "aria-label")?;
    page.click(&meter)?;
    let panel = css(".ctx-panel");
    page.wait_visible(&panel)?;
    // The parts Claude Code counted, in Faber's words; free room is not one of them.
    let system = page.text_in("ctx.part.system", "en")?;
    page.wait_visible(&panel.clone().css(".ctx-parts dt").has_text(&system))?;
    ensure(page.count(&panel.clone().css(".ctx-parts dt"))? == 3, || {
        "the parts are not the system, the tools and the conversation".into()
    })?;
    ctx.shot(&mut app, "context-panel")?;
    let page = &mut app.page;
    let compact = page.named("button", "ctx.compact")?;
    page.click(&compact)?;
    page.wait_gone(&panel)?;
    // The summary is what the context holds now.
    for _ in 0..100 {
        if page.attr(&meter, "aria-label")? != before {
            break;
        }
        pause(0.05);
    }
    let after = page.attr(&meter, "aria-label")?;
    ensure(after != before, || format!("the meter still says {before:?} after the compaction"))?;
    ensure(!page.inner_text(&css("main"))?.contains("/compact"), || "the compaction shows as a message".into())?;
    Ok(())
}
