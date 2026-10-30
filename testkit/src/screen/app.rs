//! The app scenarios: what a person sees and does in a chat, in settings, and how much memory the app uses.

use super::{Result, Scenario, ask, claude_chat, ensure, eventually, no_cli, pass_look, pause, same, start_settings, system_motion};
use crate::app::{Ctx, Opened};
use crate::page::{Page, css, role};
use crate::sandbox::{CliSlot, Launch};
use serde_json::json;
use std::time::{Duration, Instant};

/// What Faber may use with no CLI running, as Task Manager counts it (private working set of the app and its webview).
/// The product limit is 500 MB when Faber is complete; this test fails at 400 MB (R125), a safety net and not a goal.
const BUDGET_MB: f64 = 400.0;

const QUESTION: &str = "In one sentence: why is the sky blue?";

pub(super) const CREATIVE_WORKLOG: [&str; 3] = [
    "creative comparisons: English private drafts stay out of the work log within the memory budget",
    "creative comparisons: Portuguese private drafts stay out of the work log within the memory budget",
    "creative comparisons: German private drafts stay out of the work log within the memory budget",
];

/// The six looks, as the screen lists them, with the effects each draws on the new conversation: the projector beam with
/// its dust, the loom and the flow line. The loom needs a stage wider than 920 px and the flow line one wider than
/// 1140 px, so that neither lies over the greeting; the stage is the window less the 288 px sidebar.
const LOOKS: [(&str, bool, bool, bool); 6] = [
    ("sala", true, false, false),
    ("oficina", false, true, false),
    ("bancada", false, false, true),
    ("manha", false, false, false),
    ("alto", false, false, false),
    ("cobalto", true, false, false),
];

/// What the new conversation shows at the current window size: whether anything sticks out past the right edge, which
/// of the effects of the look are drawn, and which of them lie over the greeting.
const S1: &str = r#"(() => {
  const drawn = (selector) => { const e = document.querySelector(selector); return !!e && getComputedStyle(e).display !== 'none'; };
  const rect = (e) => e.getBoundingClientRect();
  const hit = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
  // The extent of the words themselves, not of the block that holds them.
  const ink = (e) => { const range = document.createRange(); range.selectNodeContents(e); return range.getBoundingClientRect(); };
  const text = [...document.querySelectorAll('.hello, .lead, .empty h1, .empty p')].map(ink);
  // The loom fades out from its bottom right corner (its mask is clear beyond about two thirds of the way to the far corner),
  // so words lie over it only when they come within half of that way; the flow line is drawn whole.
  const near = (r, x, y) => Math.hypot(Math.max(r.left - x, 0, x - r.right), Math.max(r.top - y, 0, y - r.bottom));
  const over = (s) => {
    const box = rect(document.querySelector(s));
    if (s === '.amb-flow') return text.some((r) => hit(box, r));
    return text.some((r) => near(r, box.right, box.bottom) < 0.5 * Math.hypot(box.width, box.height));
  };
  const collide = ['.amb-loom', '.amb-flow'].filter((s) => drawn(s) && over(s));
  const past = [...document.querySelectorAll('.top, .top *, .topbar, .topbar *, .side, .side *, .welcome-grid, .welcome-grid *, .empty, .empty *, .composer, .composer *')]
    .filter((e) => e.getBoundingClientRect().right > innerWidth + 0.5)
    .map((e) => String(e.className || e.tagName));
  const hello = document.querySelector('.hello');
  const heading = !hello || parseFloat(getComputedStyle(hello).fontSize) <= innerWidth * 0.09 + 0.5;
  return { page: document.documentElement.scrollWidth <= innerWidth, past, collide, heading, beam: drawn('.amb-beam'), dust: document.querySelectorAll('.amb-beam .dust i').length > 0, loom: drawn('.amb-loom'), flow: drawn('.amb-flow') };
})()"#;

/// The families the six looks draw with between them.
const FAMILIES: [&str; 6] = ["Archivo", "Barlow Condensed", "Bodoni Moda", "IBM Plex Mono", "IBM Plex Sans", "Newsreader"];

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "recent folders open a project in one click and a folder that is gone cannot be picked", run: recent_folders },
        Scenario { name: "the app opens on an empty chat and fits the window", run: opens_empty },
        Scenario {
            name: "a chat thread with a tool card and a question card is readable and whole in each of the six looks",
            run: thread_in_every_look,
        },
        Scenario {
            name: "running column: a held permission click waits for release before automatic opening",
            run: |ctx| held_permission_click(ctx, 1440, "en", false),
        },
        Scenario {
            name: "running column: a held permission click works at 720 px in German with reduced motion",
            run: |ctx| held_permission_click(ctx, 720, "de", true),
        },
        Scenario { name: "a chat streams an answer with the model and the reason, and lands in the sidebar", run: streams_an_answer },
        Scenario { name: CREATIVE_WORKLOG[0], run: |ctx| creative_worklog(ctx, "en") },
        Scenario { name: CREATIVE_WORKLOG[1], run: |ctx| creative_worklog(ctx, "pt-BR") },
        Scenario { name: CREATIVE_WORKLOG[2], run: |ctx| creative_worklog(ctx, "de") },
        Scenario {
            name: "with Auto, the answer names its model as soon as it starts, keeps the name to the end and after reopening",
            run: names_the_model_while_streaming,
        },
        Scenario {
            name: "with Auto on Codex, the answer names the model the CLI reports before the first word and keeps it to the end",
            run: names_the_codex_model_while_streaming,
        },
        Scenario { name: "edit-queued: edited words reach the CLI in the same queued message", run: edits_queued },
        Scenario {
            name: "review change: both buttons send the request and stream findings into the project conversation",
            run: reviews_change,
        },
        Scenario {
            name: "recap: button and slash ask the fake CLI on the side of an idle conversation without changing its thread",
            run: recap_on_demand,
        },
        Scenario { name: "editing a message sends it as a branch and keeps the original conversation", run: edits_as_a_branch },
        Scenario { name: "a retry is a version of the message and the arrows walk the versions (task 200)", run: walks_versions },
        Scenario { name: "deleting a conversation asks once, then removes it", run: deletes_after_asking },
        Scenario { name: "the sidebar footer shows the name the person gave, above the accounts", run: shows_the_name },
        Scenario { name: "with no AI installed, the app says what to do", run: says_what_to_do },
        Scenario {
            name: "the title bar shows the lockup in ink, and Curioso peeks only into the empty states of a project",
            run: shows_the_brand,
        },
        Scenario { name: "the screen speaks the chosen language and switches at once", run: switches_language },
        Scenario { name: "a look and the choices around it are used with one click and kept after reopening", run: keeps_appearance },
        Scenario {
            name: "every look fits the new conversation at 720x480, 1100x700, 1280x800 and 1440x900 and draws its own effect",
            run: fits_every_look,
        },
        Scenario { name: "a settings save that fails changes nothing, says why, and works again once the disk does", run: refused_save },
        Scenario {
            name: "an appearance saved by an older Faber starts on Dark Room, and Settings can still change the look",
            run: recovers_old_appearance,
        },
        Scenario { name: "with no CLI running, Faber stays within its memory budget", run: idle_memory },
        Scenario { name: "with reduced motion on, the dust stays off and Faber stays within its memory budget", run: idle_memory_reduced },
        Scenario {
            name: "a new project session shows the black hole, which moves only in a shown and focused window, within the memory budget",
            run: black_hole_in_a_project,
        },
        Scenario { name: "after the six looks were applied in turn, Faber stays within its memory budget", run: memory_after_every_look },
        Scenario { name: "a conversation with a thousand messages opens quickly and stays within the budget", run: thousand_messages },
        Scenario { name: "a long answer streaming hundreds of steps keeps the screen's memory flat", run: long_streaming_answer },
        Scenario { name: "control journey: reading and a draft survive pane toggles during live tool output", run: streaming_pane_reading },
        Scenario {
            name: "conversation changes: thirty 3990-line files stay within the memory budget while switching and after closing",
            run: changes_memory,
        },
        Scenario { name: "visual answers: pane memory before opening, while open and after closing", run: visual_answers_pane_memory },
        Scenario {
            name: "visual answers: one open frame stays within the memory budget while switching versions and after closing",
            run: visual_answers_memory,
        },
        Scenario {
            name: "a file a reply names opens at its line in the running column, and one outside the folder does not",
            run: file_at_a_line,
        },
        Scenario {
            name: "background commands: thirty-two capped outputs open in the column and one stops without a message",
            run: background_commands_memory,
        },
        Scenario {
            name: "each project conversation works in a copy of its own, its branch in the crumb, and the copy goes with it",
            run: copy_per_conversation,
        },
    ]
}

fn shows_the_name(ctx: &mut Ctx) -> Result {
    ctx.write_settings(
        &json!({ "language": "en", "setup": "done", "appearance": { "base": "sala" }, "tutorialDone": true, "userName": "Erick" }),
    )?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    page.wait_visible(&css(".side-foot .who"))?;
    same(page.inner_text(&css(".side-foot .who"))?, "Erick".to_string(), "the name in the footer")?;
    ctx.shot(&mut app, "sidebar-name")
}

fn opens_empty(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&heading)?;
    // The version left the sidebar: it is the tooltip of the Settings button.
    same(page.count(&css(".version"))?, 0, "the sidebar names no version")?;
    let hint = page.attr(&css(".side-foot .foot-row button"), "title")?.unwrap_or_default();
    ensure(super::has_version_after(&hint, "v"), || format!("the Settings button should carry the version, it says {hint:?}"))?;
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

/// The colour a custom property gives to text: the page's own computed colour, so the rest can be compared with it.
const COLOUR_OF: &str = "(name) => { const p = document.createElement('i'); p.style.color = `var(${name})`; document.body.append(p); const c = getComputedStyle(p).color; p.remove(); return c; }";

/// The drawings of Curioso on the stage: how many there are, how big each is, and the colour it is drawn in.
fn peeks() -> String {
    format!(
        "(() => {{ const colour = {COLOUR_OF}; return {{ ink3: colour('--ink-3'), found: [...document.querySelectorAll('.empty .peek svg, .welcome-grid .peek svg')].map((s) => {{ const r = s.getBoundingClientRect(); return {{ w: r.width, h: r.height, fill: getComputedStyle(s.querySelector('path')).fill }}; }}) }}; }})()"
    )
}

fn shows_the_brand(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    // One project with no conversations, so the screen can show the second empty state.
    let folder = ctx.sb.data.join("demo");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let projects = json!([{ "id": "demo", "name": "Demo", "path": folder.to_string_lossy() }]);
    std::fs::write(ctx.sb.data.join("projects.json"), projects.to_string()).map_err(|e| e.to_string())?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    // The title bar: the drawing of the lockup, named Faber, 20 px tall, in the ink colour of the look.
    let lockup = format!(
        "(() => {{ const colour = {COLOUR_OF}; const s = document.querySelector('header.top .brand svg'); return {{ name: s.getAttribute('aria-label'), h: s.getBoundingClientRect().height, fill: getComputedStyle(s.querySelector('path')).fill, ink: colour('--ink') }}; }})()"
    );
    let found = page.eval(&lockup)?;
    same(found["name"].as_str(), Some("Faber"), "the lockup is named")?;
    same(found["h"].as_f64(), Some(20.0), "the lockup's height")?;
    same(found["fill"].as_str(), found["ink"].as_str(), "the lockup is drawn in ink")?;
    // The new conversation has no Curioso.
    same(page.eval(&peeks())?["found"].as_array().map(Vec::len), Some(0), "drawings of Curioso on the new conversation")?;
    // The two empty states of the Projects area each have one, 64 px, in the third ink.
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    let open = page.named("heading", "empty.openProjectTitle")?;
    page.wait_visible(&open)?;
    let looks = |page: &mut Page, what: &str| -> Result {
        let found = page.eval(&peeks())?;
        let list = found["found"].as_array().cloned().unwrap_or_default();
        same(list.len(), 1, &format!("drawings of Curioso on {what}"))?;
        ensure((list[0]["w"].as_f64().unwrap_or(0.0) - 64.0).abs() < 0.01, || format!("his width: {}", list[0]["w"]))?;
        ensure((list[0]["h"].as_f64().unwrap_or(0.0) - 64.0).abs() < 0.01, || format!("his height: {}", list[0]["h"]))?;
        same(list[0]["fill"].as_str(), found["ink3"].as_str(), "his colour")
    };
    looks(page, "no project open")?;
    ctx.shot(&mut app, "peek-no-project")?;
    let page = &mut app.page;
    let demo = css(".prow .item").has_text("Demo");
    page.click(&demo)?;
    let name = role("heading", "Demo");
    page.wait_visible(&name)?;
    looks(page, "a project with no conversations")?;
    ctx.shot(&mut app, "peek-project")
}

fn recent_folders(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let work = ctx.sb.data.join("work");
    let names = ["alpha", "beta", "gamma", "delta", "epsilon"];
    for name in names {
        std::fs::create_dir_all(work.join(name)).map_err(|e| e.to_string())?;
    }
    let path = |name: &str| work.join(name).to_string_lossy().into_owned();
    // "gamma" is stored but gone from the disk by the time the screen is read.
    std::fs::remove_dir(work.join("gamma")).map_err(|e| e.to_string())?;
    let recents = json!({
        "folders": names.map(path),
        "environments": [{ "id": "env-1", "name": "Pair", "folders": [path("alpha"), path("beta")] }]
    });
    std::fs::write(ctx.sb.data.join("recents.json"), recents.to_string()).map_err(|e| e.to_string())?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    let open = page.named("heading", "empty.openProjectTitle")?;
    page.wait_visible(&open)?;
    let rows = css(".recents section:first-child .rec");
    page.wait_count(&rows, 5)?;
    let gone = css(".recents .rec.gone .rec-main");
    same(page.count(&gone)?, 1, "folders shown as gone")?;
    // It keeps its focus (the menu key reaches its removal), so it is marked, not disabled (the UI check clicks it and nothing opens).
    same(page.attr(&gone, "aria-disabled")?.as_deref(), Some("true"), "a folder that is gone is marked so")?;
    ctx.shot(&mut app, "recents-open-project")?;
    let page = &mut app.page;
    let first = rows.clone().css(".rec-main").has_text("alpha");
    page.click(&first)?;
    let name = role("heading", "alpha");
    page.wait_visible(&name)?;
    ctx.shot(&mut app, "recents-opened")?;
    let saved = std::fs::read_to_string(ctx.sb.data.join("projects.json")).map_err(|e| e.to_string())?;
    ensure(saved.contains("alpha"), || format!("the project was not saved: {saved}"))
}

fn creative_worklog(ctx: &mut Ctx, language: &str) -> Result {
    const ID: &str = "creative-worklog";
    const TITLE: &str = "Synthetic draft comparison";
    const PRIVATE: [&str; 2] = ["SYNTHETIC_PRIVATE_FIRST_7e13", "SYNTHETIC_PRIVATE_SECOND_9b42"];
    start_settings(ctx, language)?;
    super::shell::seed_project(ctx, "Demo")?;
    super::shell::seed(ctx, ID, TITLE, Some("demo"), 1_790_000_000_000)?;
    let path = ctx.sb.data.join("conversations").join(format!("{ID}.json"));
    let mut conversation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    conversation["messages"][1]["tools"] = json!([{
        "id": "comparison", "name": "CreativeJudge", "ok": true,
        "input": {"request": "Compare two synthetic headings", "rubric": "Clarity", "first": PRIVATE[0], "second": PRIVATE[1]},
        "output": "Synthetic scores: 8 and 6. Receipt: synthetic-review."
    }]);
    ensure(faber_engine::creative::Request::parse(&conversation["messages"][1]["tools"][0]["input"]).is_some(), || {
        "the synthetic comparison must have valid tool arguments".into()
    })?;
    let _: faber_engine::model::Conversation = serde_json::from_value(conversation.clone()).map_err(|e| e.to_string())?;
    std::fs::write(&path, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    let before = ctx.memory_mb(app.id)?.0;
    let cpu = ctx.cpu_ms(app.id)?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("Demo"))?;
    app.page.click(&css(".proj-chats .item").has_text(TITLE))?;
    app.page.wait_visible(&css("article.ai details.activity"))?;
    app.page.click(&css("article.ai details.activity > summary"))?;
    let label = app.page.text_of("tool.creativeJudge")?;
    let mut peak = before;
    for width in [720, 1028, 1280] {
        super::shell::set_size(&mut app.page, width, 800)?;
        app.page.wait_visible(&css("article.ai .tools").has_text(&label))?;
        let text = app.page.inner_text(&css("article.ai"))?;
        ensure(PRIVATE.iter().all(|draft| !text.contains(draft)), || format!("{language}/{width}: private draft shown in the answer"))?;
        ensure(app.page.eval("document.documentElement.scrollWidth <= innerWidth")? == true, || {
            format!("{language}/{width}: the page overflows")
        })?;
        let (mb, webview) = ctx.memory_mb(app.id)?;
        ensure(webview, || "the cost reading must include WebView2".into())?;
        peak = peak.max(mb);
        ctx.shot(&mut app, &format!("creative-worklog-{language}-{width}"))?;
    }
    let used_cpu = ctx.cpu_ms(app.id)? - cpu;
    ctx.notes.push(format!("Creative comparison record ({language}, synthetic saved history, no judge sessions): {before:.1} MB before opening, {peak:.1} MB largest sampled reading; open/resize/capture workflow {used_cpu:.0} ms CPU"));
    ensure(peak <= BUDGET_MB, || format!("{language}: {peak:.1} MB exceeds {BUDGET_MB} MB"))?;
    ensure(ctx.records()?.is_empty(), || "reading the saved comparison started a CLI".into())?;
    ctx.close(app.id)?;
    Ok(())
}

fn streams_an_answer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    page.wait_for(
        "the synthetic chat subscription",
        "window.__TAURI_INTERNALS__.invoke('state').then(s=>s.usage?.some(u=>u.vendor==='claude' && !u.billingUnknown && !u.billedPerUse))",
    )?;
    ask(page, QUESTION)?;
    let answer = page.named("article", "answer.label")?;
    page.wait_visible(&answer.clone().css(".actions"))?;
    page.wait_for("the completed answer text to render", "(() => { const answer = document.querySelector('article.ai'); return answer?.checkVisibility({ contentVisibilityAuto: true }) && answer.querySelector('.md')?.innerText.trim().length > 0; })()")?;
    let text = page.inner_text(&answer)?;
    ensure(text.contains("Rayleigh"), || format!("the answer should explain the sky: {text}"))?;
    ensure(text.contains("Haiku 5.5"), || format!("the answer should name its model: {text}"))?;
    // The header is the mark and the name; why Auto chose it, and the effort, are its tooltip, and the price note is the cost's.
    let reason = page.text_of("reason.autoDefault")?;
    let tips = page.eval("(() => ({ header: document.querySelector('article.ai .model')?.title, cost: document.querySelector('article.ai .actions .cost')?.title }))()")?;
    ensure(tips["header"].as_str().is_some_and(|t| t.contains(&reason)), || {
        format!("the header's tooltip should say why Auto chose it: {tips}")
    })?;
    ensure(tips["cost"].as_str().is_some_and(|t| t.contains("in API value")), || {
        format!("the cost's tooltip should show the price note: {tips}")
    })?;
    ensure(!text.contains(&reason) && !text.contains("in API value"), || format!("neither sits on the page itself: {text}"))?;
    page.wait_visible(&css(".list .item").has_text(QUESTION))?;
    ctx.shot(&mut app, "chat")
}

/// The whole answer header as text, and its height.
const HEADER_ALL: &str = "(() => { const h = document.querySelector('article.ai .model'); return h && { text: h.innerText, height: h.getBoundingClientRect().height }; })()";

/// The answer header's name slot and its height: what the person reads and where the line below sits.
const HEADER: &str = "(() => { const h = document.querySelector('article.ai .model'); return h && { name: h.querySelector('b').innerText, height: h.getBoundingClientRect().height }; })()";

fn names_the_model_while_streaming(ctx: &mut Ctx) -> Result {
    names_while_streaming(ctx, "claude", "Sunlight", "Haiku 5.5")
}

fn names_the_codex_model_while_streaming(ctx: &mut Ctx) -> Result {
    names_while_streaming(ctx, "codex", "", "GPT-6 Luna")
}

/// With Auto and one vendor: the header says only that Auto is choosing, then names the model before the first word,
/// keeps the name and the height to the end, and reads the same when the conversation is opened again. The recording
/// stops after the first words of the answer until the scenario lets it go on.
fn names_while_streaming(ctx: &mut Ctx, vendor: &str, first_words: &str, name: &str) -> Result {
    start_settings(ctx, "en")?;
    let fixtures = ctx.paths.fixtures.clone();
    let launch = |recording: &str| {
        let recording = Some(fixtures.join(format!("{vendor}/{recording}.jsonl")));
        match vendor {
            "claude" => Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording, ..Launch::default() },
            _ => Launch { claude: CliSlot::Off, codex: CliSlot::Fake, recording, ..Launch::default() },
        }
    };
    let mut app = ctx.launch(launch("chat-held"))?;
    let page = &mut app.page;
    page.wait_for(
        "the synthetic chat subscription",
        &format!("window.__TAURI_INTERNALS__.invoke('state').then(s=>s.usage?.some(u=>u.vendor==='{vendor}' && !u.billingUnknown && !u.billedPerUse))"),
    )?;
    ask(page, QUESTION)?;
    // With two vendors Auto first waits for its router (the recorded CLI never answers it, so the wait runs to the end of
    // the 30 s allowed) and the header says only that; with one vendor there is nothing to choose.
    let choosing = if vendor == "claude" {
        // The header is placed a frame before its words are; reading it at once saw "".
        page.wait_for("the header of the answer", "!!document.querySelector('article.ai .model')?.innerText.trim()")?;
        let header = page.eval(HEADER_ALL)?;
        let reason = page.text_of("reason.routing")?;
        let text = header["text"].as_str().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        same(text, format!("Auto · {reason}"), "the header while Auto is choosing")?;
        Some(header["height"].clone())
    } else {
        None
    };
    // The answer is held before its first word (Codex) or just after it (Claude): the name is there either way.
    page.wait_for_within(
        "the model's name in the header",
        &format!(
            "(() => {{ const h = document.querySelector('article.ai .model'); return h?.querySelector('b')?.innerText === '{name}' && document.querySelector('article.ai .md').innerText.includes('{first_words}'); }})()"
        ),
        Duration::from_secs(60),
    )?;
    let during = page.eval(HEADER)?;
    if let Some(height) = choosing {
        same(during["height"].clone(), height, "the header's height once the name is there")?;
    }
    std::fs::write(ctx.sb.state.join("release"), "").map_err(|e| e.to_string())?;
    let answer = page.named("article", "answer.label")?;
    page.wait_visible(&answer.css(".actions"))?;
    same(page.eval(HEADER)?, during.clone(), "the header at the end of the answer")?;
    // A message saved and opened again reads the same.
    ctx.close(app.id)?;
    let mut app = ctx.launch(launch("chat"))?;
    let page = &mut app.page;
    page.click(&css(".list .item").has_text(QUESTION))?;
    page.wait_visible(&css("article.ai .actions"))?;
    same(page.eval(HEADER)?, during, "the header after reopening the conversation")
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
    page.hover(&css(".list .crow .item"))?;
    let x = css(".list .crow .x");
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
    // The line never names a path through Settings (its sections are renamed over time): the one button goes there.
    let section = page.text_of("set.subscriptions")?;
    ensure(!body.contains("Settings") && !body.contains(&section), || format!("it should not name a Settings path: {body}"))?;
    same(page.count(&css(".empty button"))?, 1, "one button")?;
    let lower = body.to_lowercase();
    ensure(!["reopen", "restart", "relaunch"].iter().any(|w| lower.contains(w)), || format!("it should not ask to restart: {body}"))?;
    // There is nothing to type into: no composer, so no permission line or model choice either.
    same(page.count(&css(".composer"))?, 0, "no composer with no AI")?;
    // Projects says the same, not "Add project folder" for a folder no AI could work in.
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.wait_visible(&title)?;
    same(page.count(&css(".projects-root"))?, 0, "no project picker with no AI")?;
    same(page.count(&css(".composer"))?, 0, "no composer in Projects with no AI")
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
    let open = page.named_in("button", "settings.open", "pt-BR")?.exact();
    page.click(&open)?;
    let language = crate::page::label(&page.text_in("set.language", "pt-BR")?);
    page.select_option(&language, "es")?;
    let general = page.named_in("heading", "set.general", "es")?;
    page.wait_visible(&general)?;
    same(page.eval("document.documentElement.lang")?.as_str(), Some("es"), "the page language after the switch")
}

fn open_appearance(page: &mut Page) -> Result {
    let settings = page.named("button", "settings.open")?.exact();
    page.click(&settings)?;
    let appearance = page.named("button", "set.appearance")?;
    page.click(&appearance)?;
    let heading = page.named("heading", "set.appearance")?;
    page.wait_visible(&heading)
}

fn back_to_chat(page: &mut Page) -> Result {
    let back = page.named("button", "set.back")?;
    page.click(&back)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)
}

/// Clicks "Use this look": the draft is saved, and the page wears it once the save worked.
fn use_look(page: &mut Page) -> Result {
    let apply = page.named("button", "set.use")?;
    page.click(&apply)
}

/// Clicks the card of a look and, unless the page wears that look already, "Use this look"; waits until the page wears it.
fn pick_look(page: &mut Page, id: &str) -> Result {
    let card = page.named("button", &format!("look.{id}.name"))?;
    page.click(&card)?;
    if page.eval("document.documentElement.dataset.base")?.as_str() != Some(id) {
        use_look(page)?;
    }
    page.wait_for(&format!("the {id} look"), &format!("document.documentElement.dataset.base === '{id}'"))
}

fn keeps_appearance(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let background = "getComputedStyle(document.documentElement).getPropertyValue('--bg').trim()";
    let frame = "getComputedStyle(document.body).backgroundColor";
    let density = "parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--dens'))";
    same(page.eval(background)?.as_str(), Some("#0e0e10"), "the first look is Dark Room")?;
    open_appearance(page)?;
    pick_look(page, "manha")?;
    same(page.eval(background)?.as_str(), Some("#f3f5f0"), "Morning background")?;
    same(page.eval(frame)?.as_str(), Some("rgb(233, 237, 231)"), "Morning frame")?;
    let compact = page.named("button", "set.densityCompact")?;
    page.click(&compact)?;
    use_look(page)?;
    page.wait_for("compact density", &format!("{density} === 0.8"))?;
    // A new theme keeps the text size, the density and the motion.
    pick_look(page, "bancada")?;
    same(page.eval(background)?.as_str(), Some("#161916"), "Workbench background")?;
    same(page.attr(&compact, "aria-pressed")?, Some("true".to_string()), "the density survives a change of look")?;
    // The controls fade between their states in 140 ms; the picture is taken once they have settled.
    pause(0.5);
    ctx.shot(&mut app, "settings-workbench")?;
    let page = &mut app.page;
    let general = page.named("button", "set.general")?;
    page.click(&general)?;
    // In Full access, the default, the person's hooks always run and the switch is locked; it is a choice in the modes that ask.
    super::permissions::pick_mode(page, 1)?;
    let hooks = page.named("switch", "set.hooksBody")?;
    page.check(&hooks)?;
    // The look applies once it is saved, but the switch does not wait for the screen; closing earlier could lose it.
    let file = ctx.sb.data.join("settings.json");
    eventually("the choices to reach settings.json", Duration::from_secs(15), || {
        std::fs::read_to_string(&file).is_ok_and(|text| {
            text.contains("\"userHooks\": true") && text.contains("\"base\": \"bancada\"") && text.contains("\"density\": 0.8")
        })
    })?;
    ctx.close(app.id)?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    same(page.eval(background)?.as_str(), Some("#161916"), "the saved background")?;
    same(page.eval(frame)?.as_str(), Some("rgb(25, 29, 25)"), "the saved frame")?;
    same(page.eval(density)?.as_f64(), Some(0.8), "the saved density")?;
    // An unfocused offscreen window reopened on the same profile only takes keys after a click, like a real one.
    let composer = page.labelled("composer.message")?;
    page.click(&composer)?;
    page.press("Control+,")?;
    let general = page.named("heading", "set.general")?;
    page.wait_visible(&general)?;
    same(page.is_checked(&hooks)?, true, "the saved choice")
}

fn fits_every_look(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    for (id, beam, loom, flow) in LOOKS {
        open_appearance(&mut app.page)?;
        pick_look(&mut app.page, id)?;
        back_to_chat(&mut app.page)?;
        for (width, height) in [(720, 480), (1100, 700), (1280, 800), (1440, 900)] {
            let size = json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false });
            app.page.cdp.call("Emulation.setDeviceMetricsOverride", size)?;
            let seen = app.page.eval(S1)?;
            let at = format!("{id} at {width}x{height}");
            ensure(seen["page"] == true, || format!("{at}: the page scrolls sideways"))?;
            ensure(seen["past"].as_array().is_some_and(|past| past.is_empty()), || {
                format!("{at}: these stick out past the right edge: {}", seen["past"])
            })?;
            let (loom_room, flow_room) = (width >= 1200, width >= 1400);
            same(
                (seen["beam"].as_bool(), seen["dust"].as_bool()),
                (Some(beam), Some(true)),
                &format!("{at}: the projector beam and its dust"),
            )?;
            same(seen["loom"].as_bool(), Some(loom && loom_room), &format!("{at}: the loom"))?;
            same(seen["flow"].as_bool(), Some(flow && flow_room), &format!("{at}: the flow line"))?;
            ensure(seen["collide"].as_array().is_some_and(|over| over.is_empty()), || {
                format!("{at}: these lie over the greeting: {}", seen["collide"])
            })?;
            ensure(seen["heading"] == true, || format!("{at}: the heading is wider than 9vw in its font size"))?;
            if width == 1100 {
                // A suggestion fills the composer and sends nothing.
                app.page.click(&css(".try button:nth-of-type(1)"))?;
                same(
                    app.page.eval("document.querySelector('.composer textarea').value.length > 0")?.as_bool(),
                    Some(true),
                    &format!("{at}: the suggestion in the composer"),
                )?;
                same(app.page.count(&css("article"))?, 0, &format!("{at}: nothing was sent"))?;
            }
            if width >= 1280 {
                let name = if width == 1440 { format!("s1-{id}") } else { format!("s1-{id}-{width}") };
                ctx.shot(&mut app, &name)?;
            }
        }
        app.page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    }
    Ok(())
}

/// What a conversation shows: for the parts of a thread, the contrast of their text with the colour behind it (the same
/// ratios the look engine promises for its pairs, measured on the page as drawn) and whether each one is whole inside the
/// window and not covered. Each part is `[name, selector, smallest ratio, required]`.
const THREAD: &str = r#"(() => {
  const probe = document.createElement('canvas').getContext('2d', { willReadFrequently: true });
  const rgba = (css) => {
    probe.clearRect(0, 0, 1, 1);
    probe.fillStyle = '#000';
    probe.fillStyle = css;
    probe.fillRect(0, 0, 1, 1);
    const [r, g, b, a] = probe.getImageData(0, 0, 1, 1).data;
    return [r, g, b, a / 255];
  };
  const over = (top, under) => [0, 1, 2].map((i) => top[i] * top[3] + under[i] * (1 - top[3])).concat(1);
  const lum = ([r, g, b]) => {
    const f = (c) => { c /= 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
  };
  const ratio = (x, y) => { const [hi, lo] = [lum(x), lum(y)].sort((p, q) => q - p); return (hi + 0.05) / (lo + 0.05); };
  const behind = (el) => {
    const layers = [];
    for (let p = el; p; p = p.parentElement) {
      const c = rgba(getComputedStyle(p).backgroundColor);
      if (c[3] > 0) layers.push(c);
      if (c[3] >= 1) break;
    }
    const last = layers[layers.length - 1];
    const page = last && last[3] >= 1 ? [0, 0, 0, 0] : rgba(getComputedStyle(document.body).backgroundColor);
    return layers.reduceRight((under, top) => over(top, under), page[3] > 0 ? page : [255, 255, 255, 1]);
  };
  const faded = (el) => { let o = 1; for (let p = el; p; p = p.parentElement) o *= parseFloat(getComputedStyle(p).opacity); return o; };
  const parts = [
    ['your message', '.thread .you', 4.5, true],
    ['the model name', 'article.ai .model b', 4.5, true],
    ['the activity row', 'article.ai .activity > summary', 4.5, true],
    ['the question title', '.thread .ask b', 4.5, true],
    ['the question text', '.thread .ask pre', 4.5, true],
    ['Allow', '.thread .ask .btn.primary', 4.5, true],
    ['the other answers', '.thread .ask .btn:not(.primary)', 4.5, true],
    ['the answer text', 'article.ai > .md p', 4.5, false],
    ['the crumb', '.crumb b', 4.5, true],
  ];
  const found = [];
  const problems = [];
  for (const [name, selector, least, required] of parts) {
    const all = [...document.querySelectorAll(selector)].filter((e) => e.textContent.trim());
    if (!all.length) { if (required) problems.push(name + ' is not on screen'); continue; }
    for (const e of all.slice(0, 3)) {
      e.scrollIntoView({ block: 'nearest' });
      const box = e.getBoundingClientRect();
      const text = rgba(getComputedStyle(e).color);
      text[3] *= faded(e);
      const seen = ratio(over(text, behind(e)), behind(e));
      const top = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
      found.push({ name, ratio: Math.round(seen * 100) / 100 });
      if (seen < least) problems.push(name + ' has a contrast of ' + seen.toFixed(2) + ', under ' + least);
      if (box.width < 1 || box.height < 1 || box.x < -0.5 || box.right > innerWidth + 0.5 || box.bottom > innerHeight + 0.5) problems.push(name + ' is cut off: ' + JSON.stringify([box.x, box.y, box.width, box.height]));
      else if (!top || !(e.contains(top) || top.contains(e))) problems.push(name + ' is covered by ' + (top ? top.className || top.tagName : 'nothing'));
    }
  }
  if (document.documentElement.scrollWidth > innerWidth) problems.push('the page scrolls sideways');
  return { found, problems };
})()"#;

fn held_permission_click(ctx: &mut Ctx, width: u32, lang: &str, reduce: bool) -> Result {
    start_settings(ctx, lang)?;
    let launch = super::shell::two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    super::shell::set_size(page, width, 900)?;
    system_motion(page, reduce)?;
    let motion = if reduce { "reduced" } else { "full" };
    page.wait_for("the requested motion mode", &format!("document.documentElement.dataset.motion === '{motion}'"))?;
    same(page.eval("window.innerWidth")?, json!(width), "the held permission test uses its requested viewport")?;
    super::shell::ask_two_writes(page)?;
    let composer = page.labelled("composer.message")?;
    page.press_on(&composer, "ArrowLeft")?;
    same(page.count(&css(".runcol"))?, 0, "the permission click begins before automatic opening")?;
    let allow = page.text_of("ask.allow")?;
    let target = css("article .ask").has_text("a.txt").role("button", &allow).exact();
    page.hover(&target)?;
    page.hover(&target)?;
    let at = page.eval_on(&target, "const r=el.getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};")?;
    let x = at["x"].as_f64().ok_or("permission click x missing")?;
    let y = at["y"].as_f64().ok_or("permission click y missing")?;
    let pressed = page.cdp.call(
        "Input.dispatchMouseEvent",
        json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "buttons": 1, "clickCount": 1 }),
    );
    let held = pressed.and_then(|_| {
        // Cross the 1.5 s typing pause and the column's entry motion while the real button stays pressed.
        pause(2.2);
        page.count(&css(".runcol"))
    });
    let released = page.cdp.call(
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "buttons": 0, "clickCount": 1 }),
    );
    let columns = held?;
    released?;
    same(columns, 0, "automatic opening must not move a permission button during a held click")?;
    page.wait_visible(&css("article .ask").has_text("b.txt"))?;
    page.wait_visible(&css(".runcol .ask").has_text("b.txt"))?;
    ctx.notes.push(format!(
        "held permission: actual viewport {width}px, locale {lang}, motion {motion}; first answer delivered and automatic opening resumed"
    ));
    // The assertion above covers opening; capture the column after its existing entrance finishes.
    pause(0.7);
    let shot = ctx.shot(&mut app, &format!("held-permission-{width}-{lang}"));
    app.page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    shot
}

fn thread_in_every_look(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = super::shell::two_writes(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let size = json!({ "width": 1440, "height": 900, "deviceScaleFactor": 1, "mobile": false });
    page.cdp.call("Emulation.setDeviceMetricsOverride", size)?;
    super::shell::ask_two_writes(page)?;
    // The first file is allowed, which leaves a tool card behind; the second waits as a question card.
    let allow = page.text_of("ask.allow")?;
    let question = css("article .ask").has_text("b.txt");
    page.click(&css("article .ask").has_text("a.txt").role("button", &allow).exact())?;
    page.wait_visible(&question)?;
    // Let the pending question open its column before measuring settled text contrast in each look.
    page.wait_visible(&css(".runcol"))?;
    for (id, ..) in LOOKS {
        open_appearance(&mut app.page)?;
        pick_look(&mut app.page, id)?;
        let back = app.page.named("button", "set.back")?;
        app.page.click(&back)?;
        app.page.wait_visible(&question)?;
        // The controls and the column fade for a moment; the picture and the measures are taken once they settled.
        pause(0.7);
        let seen = app.page.eval(THREAD)?;
        let problems: Vec<String> =
            seen["problems"].as_array().map(|all| all.iter().filter_map(|p| p.as_str().map(String::from)).collect()).unwrap_or_default();
        ensure(problems.is_empty(), || format!("{id}: {}; measured {}", problems.join("; "), seen["found"]))?;
        let lowest = seen["found"].as_array().into_iter().flatten().filter_map(|f| f["ratio"].as_f64()).fold(f64::INFINITY, f64::min);
        ctx.notes.push(format!("{id}: lowest contrast {lowest}"));
        ctx.shot(&mut app, &format!("chat-thread-{id}"))?;
    }
    let page = &mut app.page;
    let deny = page.text_of("ask.deny")?;
    page.click(&css("article .ask").has_text("b.txt").role("button", &deny).exact())?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn refused_save(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    open_appearance(&mut app.page)?;
    let page = &mut app.page;
    let base = "document.documentElement.dataset.base";
    same(page.eval(base)?.as_str(), Some("sala"), "the first look")?;
    // A folder where the temporary file goes makes every write of settings.json fail while the old file stays valid.
    let blocker = ctx.sb.data.join("settings.json.tmp");
    std::fs::create_dir(&blocker).map_err(|e| e.to_string())?;
    let morning = page.named("button", "look.manha.name")?;
    page.click(&morning)?;
    // Choosing a look saves nothing, so there is nothing to refuse yet: the alert comes with "Use this look".
    same(page.count(&css(".toast"))?, 0, "an alert before anything was saved")?;
    use_look(page)?;
    let toast = css(".toast[role=alert]");
    page.wait_visible(&toast)?;
    let sentence = page.text_of("error.settings.saveFailed")?;
    ensure(page.inner_text(&toast)?.contains(&sentence), || format!("the alert should say {sentence:?}"))?;
    same(page.eval(base)?.as_str(), Some("sala"), "the look after a failed save")?;
    // The draft stays, so the person can try again.
    same(page.attr(&morning, "aria-pressed")?, Some("true".to_string()), "the card of the look that was not saved")?;
    pause(0.6);
    ctx.shot(&mut app, "settings-save-failed")?;
    let page = &mut app.page;
    // A switch moves before its save is answered; a refused save puts it back.
    let general = page.named("button", "set.general")?;
    page.click(&general)?;
    let notify = page.named("switch", "set.notifyMe")?;
    let before = page.is_checked(&notify)?;
    page.click(&notify)?;
    page.wait_for("the switch to be put back", &format!("document.querySelector('#set-notify-switch').checked === {before}"))?;
    let file = ctx.sb.data.join("settings.json");
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    ensure(!text.contains("manha") && !text.contains(&format!("\"notifications\": {}", !before)), || {
        format!("settings.json should be as it was: {text}")
    })?;

    // Once the disk works again the same draft saves, and the alert goes away.
    std::fs::remove_dir(&blocker).map_err(|e| e.to_string())?;
    let appearance = page.named("button", "set.appearance")?;
    page.click(&appearance)?;
    same(page.attr(&morning, "aria-pressed")?, Some("true".to_string()), "the draft after a visit to another section")?;
    use_look(page)?;
    page.wait_for("the Morning look", &format!("{base} === 'manha'"))?;
    page.wait_for("the alert to go", "!document.querySelector('.toast')")
}

fn recovers_old_appearance(ctx: &mut Ctx) -> Result {
    // What the Mono design saved: none of these fields exist in the new shape, so the whole value is unusable.
    ctx.write_settings(&json!({
        "language": "en",
        "setup": "done",
        "tutorialDone": true,
        "appearance": { "mode": "dark", "accent": "mono", "ui": "geistmono", "read": "geist", "density": "comfortable", "corners": "round" }
    }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    // The old value is unusable, so the screen has no look to show and asks for one, once (step 2 of the first run).
    pass_look(page)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    same(page.eval("document.documentElement.dataset.base")?.as_str(), Some("sala"), "the look of an old appearance")?;
    open_appearance(page)?;
    // A card keeps the density, which an old value would have carried into the save and had refused.
    pick_look(page, "oficina")?;
    let compact = page.named("button", "set.densityCompact")?;
    page.click(&compact)?;
    use_look(page)?;
    page.wait_for("compact density", "parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--dens')) === 0.8")?;
    // Reduced motion keeps short fades only: no thumb travel and no fade longer than 120 ms.
    let reduced = page.named("button", "look.motion.reduced")?;
    page.click(&reduced)?;
    use_look(page)?;
    page.wait_for("reduced motion", "document.documentElement.dataset.motion === 'reduced'")?;
    // The effect switches sit in the closed Advanced fold.
    let advanced = page.named("button", "set.advanced")?;
    page.click(&advanced)?;
    page.wait_visible(&css(".sw2"))?;
    let seen = page.eval(
        r#"(() => {
          const thumb = getComputedStyle(document.querySelector('.sw2'), '::after');
          const longest = (e) => Math.max(...getComputedStyle(e).transitionDuration.split(', ').map(parseFloat));
          return { travels: thumb.transitionProperty.includes('transform') || thumb.transitionProperty === 'all', longest: Math.max(longest(document.querySelector('.sw2')), longest(document.querySelector('.preset .thumb')), longest(document.querySelector('.btn, .item')), parseFloat(thumb.transitionDuration)) };
        })()"#,
    )?;
    same(seen["travels"].as_bool(), Some(false), "the switch thumb travels in reduced motion")?;
    ensure(seen["longest"].as_f64().is_some_and(|s| s <= 0.12 + 1e-9), || format!("a fade lasts longer than 120 ms: {}", seen["longest"]))?;
    let file = ctx.sb.data.join("settings.json");
    eventually("the choices to reach settings.json", Duration::from_secs(15), || {
        std::fs::read_to_string(&file)
            .is_ok_and(|t| t.contains("\"base\": \"oficina\"") && t.contains("\"density\": 0.8") && t.contains("\"motion\": \"reduced\""))
    })
}

fn idle_memory(ctx: &mut Ctx) -> Result {
    idle_memory_with(ctx, false)
}

fn idle_memory_reduced(ctx: &mut Ctx) -> Result {
    idle_memory_with(ctx, true)
}

/// Tells the page whether it has the focus: the hidden test window must never take it from the person at the PC.
pub(super) fn focus(page: &mut Page, on: bool) -> Result {
    page.eval(&format!(
        "(() => {{ document.hasFocus = () => {on}; window.dispatchEvent(new Event('{}')); }})()",
        if on { "focus" } else { "blur" }
    ))
    .map(|_| ())
}

fn idle_memory_with(ctx: &mut Ctx, reduce: bool) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    // A machine that turns animations off (a CI runner does) must not change what this scenario is about.
    system_motion(&mut app.page, reduce)?;
    let motion = if reduce { "reduced" } else { "full" };
    app.page.wait_for(
        "the emulated motion preference to reach the screen",
        &format!("document.documentElement.dataset.motion === '{motion}'"),
    )?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    ensure(app.page.count(&css(".amb-beam .dust"))? == 1, || "the new conversation should show the dust".to_string())?;
    // The memory below is only worth something while the dust really twinkles, and it stops with the window or its focus.
    let states = "document.querySelector('.amb-beam .dust').dataset.live";
    let grain = "document.querySelector('.amb-beam .dust i').style.opacity";
    let hide = |page: &mut Page, on: bool| {
        page.eval(&format!(
            "(() => {{ Object.defineProperty(document, 'hidden', {{ configurable: true, get: () => {on} }}); document.dispatchEvent(new Event('visibilitychange')); }})()"
        ))
        .map(|_| ())
    };
    // The native test window stays hidden and never takes the focus; this scenario sets both explicitly.
    hide(&mut app.page, false)?;
    focus(&mut app.page, true)?;
    let moving = app.page.eval(states)?;
    if reduce {
        // Reduced motion keeps the dust on the page, still: no grain moves.
        same(moving.as_str(), Some("off"), "the dust stands still under reduced motion")?;
        pause(10.0);
        let (mb, _) = ctx.memory_mb(app.id)?;
        ctx.notes.push(format!("idle on the new conversation with the dust (reduced motion): {mb:.0} MB"));
        return ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"));
    }
    same(moving.as_str(), Some("on"), "the dust twinkles in a visible, focused window")?;
    let first = app.page.eval(grain)?;
    pause(0.5);
    let then = app.page.eval(grain)?;
    ensure(then.as_str().is_some_and(|v| !v.is_empty()) && first != then, || {
        format!("the dust should change its light over half a second, stayed at {then}")
    })?;
    hide(&mut app.page, true)?;
    let stopped = app.page.eval(states)?;
    hide(&mut app.page, false)?;
    same(stopped.as_str(), Some("off"), "the dust stops while the window is hidden")?;
    focus(&mut app.page, false)?;
    let blurred = app.page.eval(states)?;
    focus(&mut app.page, true)?;
    same(blurred.as_str(), Some("off"), "the dust stops while another app has the focus")?;
    beam_cost(ctx, &mut app)?;
    pause(10.0);
    let motion = app.page.eval("document.documentElement.dataset.motion")?;
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("idle on the new conversation with the dust ({} motion): {mb:.0} MB", motion.as_str().unwrap_or("?")));
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}

/// The Dark Room beam, its glow and its dust breathe only in a focused window. The hidden test window must never take
/// focus from the person at the PC, so the page is told whether it has focus, and the processor time of the whole tree is
/// read over a few seconds in a focused window and in an unfocused one.
fn beam_cost(ctx: &mut Ctx, app: &mut Opened) -> Result {
    let mut window = |focused: bool| -> std::result::Result<f64, String> {
        focus(&mut app.page, focused)?;
        let before = ctx.cpu_ms(app.id)?;
        pause(6.0);
        Ok((ctx.cpu_ms(app.id)? - before) / 60.0)
    };
    let (beam, beam_away) = (window(true)?, window(false)?);
    ctx.notes.push(format!("Dark Room beam, glow and dust, processor of one core: focused {beam:.1}%, unfocused {beam_away:.1}%"));
    Ok(())
}

/// The black hole behind a new project session in Dark Room: it runs in a shown, focused window and stops when the window
/// is hidden or loses the focus. The processor time of the whole tree is read at rest, while it runs, and with a ripple
/// started every 400 ms, then the memory.
fn black_hole_in_a_project(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    super::shell::seed_project(ctx, "Demo")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    // The art lives only right of the column of words and needs room there, as on the owner's own screen.
    app.page
        .cdp
        .call("Emulation.setDeviceMetricsOverride", json!({ "width": 1920, "height": 1080, "deviceScaleFactor": 1, "mobile": false }))?;
    system_motion(&mut app.page, false)?;
    let live = "document.querySelector('canvas.amb-hole')?.dataset.live";
    let hide = |page: &mut Page, on: bool| {
        page.eval(&format!(
            "(() => {{ Object.defineProperty(document, 'hidden', {{ configurable: true, get: () => {on} }}); document.dispatchEvent(new Event('visibilitychange')); }})()"
        ))
        .map(|_| ())
    };
    hide(&mut app.page, false)?;
    focus(&mut app.page, true)?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    let demo = css(".prow .item").has_text("Demo");
    app.page.wait_visible(&demo)?;
    app.page.click(&demo)?;
    app.page.wait_visible(&css(".welcome-grid .p-header h2").has_text("Demo"))?;
    app.page.wait_for("the black hole to run in a shown, focused window", &format!("{live} === 'on'"))?;
    // The entrance plays once: Curioso wakes, the spark flies, the scene lights up from the hole, and then it ends.
    let intro = "document.querySelector('canvas.amb-hole')?.dataset.intro";
    same(app.page.eval(intro)?.as_str(), Some("on"), "a new session opens with the entrance")?;
    let wakes = app.page.eval("getComputedStyle(document.querySelector('.peek svg')).animationName")?;
    same(wakes.as_str(), Some("wake"), "Curioso wakes during the entrance")?;
    std::thread::sleep(std::time::Duration::from_millis(950));
    ctx.shot(&mut app, "project-hole-spark")?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    ctx.shot(&mut app, "project-hole-entrance")?;
    app.page.wait_for("the entrance to end", &format!("{intro} === undefined"))?;
    // Something is drawn: the canvas has lit pixels near the top of the hole.
    let lit = "(() => { const c = document.querySelector('canvas.amb-hole'); const d = c.getContext('2d').getImageData(0, 0, c.width, c.height).data; let n = 0; for (let i = 3; i < d.length; i += 4) if (d[i] > 20) n++; return n; })()";
    let pixels = app.page.eval(lit)?.as_u64().unwrap_or(0);
    ensure(pixels > 5000, || format!("the black hole should be drawn, {pixels} lit pixels"))?;
    hide(&mut app.page, true)?;
    let hidden = app.page.eval(live)?;
    hide(&mut app.page, false)?;
    same(hidden.as_str(), Some("off"), "the black hole stops while the window is hidden")?;
    focus(&mut app.page, false)?;
    same(app.page.eval(live)?.as_str(), Some("off"), "the black hole stops while another app has the focus")?;
    let window = |ctx: &mut Ctx, app: &mut Opened| -> std::result::Result<(f64, f64), String> {
        let before = ctx.cpu_ms(app.id)?;
        pause(6.0);
        let used = ctx.cpu_ms(app.id)? - before;
        Ok((used / 60.0, ctx.memory_mb(app.id)?.0))
    };
    let (away, _) = window(ctx, &mut app)?;
    focus(&mut app.page, true)?;
    // The same window with the hole switched off, so the readings below are the hole's own.
    // A pixel less of width makes the hole lay itself out again and read the token, as a change of look does.
    app.page.eval("document.documentElement.style.setProperty('--amb-hole', 'none'); document.querySelector('.ambient:has(.amb-hole)').style.right = '1px'")?;
    app.page.wait_for("the black hole to rest when the look turns it off", &format!("{live} === 'off'"))?;
    let (base, _) = window(ctx, &mut app)?;
    app.page.eval(
        "document.documentElement.style.removeProperty('--amb-hole'); document.querySelector('.ambient:has(.amb-hole)').style.right = ''",
    )?;
    app.page.wait_for("the black hole to run again", &format!("{live} === 'on'"))?;
    let (running, running_mb) = window(ctx, &mut app)?;
    app.page.eval(
        "(() => { const area = document.querySelector('.work'); const box = area.getBoundingClientRect(); window.__ripples = setInterval(() => area.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0, clientX: box.left + box.width * 0.6, clientY: box.top + box.height * 0.3 })), 400); })()",
    )?;
    let (ripple, ripple_mb) = window(ctx, &mut app)?;
    app.page.eval("clearInterval(window.__ripples)")?;
    focus(&mut app.page, false)?;
    ctx.notes.push(format!(
        "black hole, processor of one core and memory: same window without it {base:.1}%, unfocused {away:.1}%, running {running:.1}% {running_mb:.0} MB, rippling {ripple:.1}% {ripple_mb:.0} MB"
    ));
    // How it sits on a full HD screen and on a small one.
    for (width, height) in [(1920, 1040), (1280, 800)] {
        super::shell::set_size(&mut app.page, width, height)?;
        pause(0.6);
        ctx.shot(&mut app, &format!("project-hole-{width}"))?;
    }
    ensure(ripple_mb.max(running_mb) <= BUDGET_MB, || format!("{:.0} MB is over the {BUDGET_MB} MB budget", ripple_mb.max(running_mb)))
}

fn memory_after_every_look(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    open_appearance(&mut app.page)?;
    for (id, ..) in LOOKS {
        pick_look(&mut app.page, id)?;
    }
    back_to_chat(&mut app.page)?;
    // Each look drew with its own families, so by now the page has downloaded every face.
    let loaded = app
        .page
        .eval("[...new Set([...document.fonts].filter((f) => f.status === 'loaded').map((f) => f.family.replace(/['\"]/g, '')))].sort()")?;
    for family in FAMILIES {
        ensure(loaded.as_array().is_some_and(|all| all.iter().any(|f| f == family)), || {
            format!("{family} was never loaded; loaded: {loaded}")
        })?;
    }
    pause(3.0);
    let (mb, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("after the six looks: {mb:.0} MB"));
    ensure(mb <= BUDGET_MB, || format!("{mb:.0} MB is over the {BUDGET_MB} MB budget"))
}

/// Parity P3: two new conversations in one project, each in its own copy on its own branch, named in the crumb with the copy's
/// folder as its tooltip; deleting one removes its copy and leaves the other's.
fn copy_per_conversation(ctx: &mut Ctx) -> Result {
    let fixture = super::projects_ui::fixture(ctx)?;
    let mut launch = claude_chat(ctx);
    launch.git_bin = fixture.git_bin;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Demo"))?;
    page.wait_visible(&css(".welcome-grid .p-header h2").has_text("Demo"))?;
    let mut copies = vec![];
    for question in ["Fix the first thing", "Fix the second thing"] {
        if !copies.is_empty() {
            page.click(&css("button.newchat"))?;
            page.wait_visible(&css(".welcome-grid .p-header h2").has_text("Demo"))?;
        }
        // A chosen model starts at once; Auto would wait for its routing question, which this recording does not hold.
        let model = page.labelled("composer.model")?;
        page.select_option(&model, "Claude Haiku")?;
        ask(page, question)?;
        let crumb = css("[data-testid=crumb-copy]");
        page.wait_visible(&crumb)?;
        let branch = page.inner_text(&crumb)?;
        let tip = page.attr(&crumb, "title")?.unwrap_or_default();
        // The app sees its data folder through the sandbox; the same copy is found here by its name.
        let shown = std::path::PathBuf::from(tip.lines().last().unwrap_or_default());
        let named = shown.parent().and_then(|p| p.file_name()).is_some_and(|n| n == "copies");
        let path = ctx.sb.data.join("copies").join(shown.file_name().unwrap_or_default());
        // The crumb names the copy as soon as it is planned; the checkout lands a moment later.
        for _ in 0..40 {
            if path.is_dir() {
                break;
            }
            pause(0.25);
        }
        ensure(branch.starts_with("faber/fix-the-"), || format!("the crumb names the copy's branch: {branch}"))?;
        ensure(named && path.is_dir(), || format!("the tooltip is the copy's folder, which exists: {tip}"))?;
        copies.push((branch, path));
        // The copy is what this checks, not the answer: the reply is stopped so the conversation can go.
        let stop = page.named("button", "composer.stop")?;
        page.click(&stop)?;
        page.wait_gone(&stop)?;
    }
    ensure(copies[0].0 != copies[1].0 && copies[0].1 != copies[1].1, || format!("each conversation has its own: {copies:?}"))?;
    let first = css(".proj-chats .crow").has_text("Fix the first thing");
    page.hover(&first.clone().css(".item"))?;
    let x = first.css(".x");
    page.click(&x)?;
    page.click(&x)?;
    page.wait_gone(&css(".proj-chats .crow").has_text("Fix the first thing"))?;
    for _ in 0..40 {
        if !copies[0].1.exists() {
            break;
        }
        pause(0.25);
    }
    ensure(!copies[0].1.exists(), || format!("the deleted conversation's copy is gone: {:?}", copies[0].1))?;
    ensure(copies[1].1.is_dir(), || format!("the other copy stays: {:?}", copies[1].1))?;
    ctx.shot(&mut app, "copy-per-conversation")
}

fn changes_memory(ctx: &mut Ctx) -> Result {
    let fixture = super::projects_ui::fixture(ctx)?;
    let repo = ctx.sb.sandbox.join("repo");
    let copy = ctx.sb.data.join("copies/changed-project");
    std::fs::create_dir_all(copy.parent().ok_or("the copy has no parent")?).map_err(|e| e.to_string())?;
    let git = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    super::projects::git(
        &git.command,
        &repo,
        &["worktree", "add", "-b", "faber/changes-budget", copy.to_str().ok_or("the copy path is not UTF-8")?],
    )?;
    // Match the DOM check: one hunk header and 3,989 added lines, just below the patch cap.
    let text: String = (0..3989).map(|i| format!("line {i}\n")).collect();
    let mut tools = vec![];
    for i in 0..30 {
        let path = format!("file-{i:02}.txt");
        std::fs::write(copy.join(&path), &text).map_err(|e| e.to_string())?;
        tools.push(json!({ "id": format!("edit-{i}"), "name": "Write", "input": { "file_path": path }, "ok": true }));
    }
    let mut recorded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ctx.paths.fixtures.join("conversation-changes.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let conversation = &mut recorded["conversation"];
    conversation["projectId"] = json!("demo");
    conversation["messages"][1]["tools"] = json!(tools);
    conversation["copy"] = json!({
        "root": repo.to_string_lossy(), "path": copy.to_string_lossy(),
        "branch": "faber/changes-budget", "base": "refs/heads/owner"
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("changed-project.json"), conversation.to_string()).map_err(|e| e.to_string())?;

    let mut launch = claude_chat(ctx);
    launch.git_bin = fixture.git_bin;
    let mut app = ctx.launch(launch)?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("Demo"))?;
    app.page.click(&css(".proj-chats .item").has_text("Update the greeting"))?;
    app.page.click(&css(".topbar .pane-changes"))?;
    app.page.wait_count(&css(".change-file"), 30)?;
    let (baseline, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the memory reading must include WebView2".into())?;
    let mut peak = baseline;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    for i in 1..=30 {
        let row = format!(".change-file:nth-child({i})");
        app.page.click(&css(&format!("{row} > summary")))?;
        app.page.wait_count(&css(&format!("{row}[open] .patch-line")), 3990)?;
        same(app.page.count(&css(".change-patch"))?, 1, "only the current file's patch stays mounted")?;
        peak = peak.max(ctx.memory_mb(app.id)?.0);
    }
    let cpu_ms = ctx.cpu_ms(app.id)? - cpu_before;
    let switch_ms = started.elapsed().as_secs_f64() * 1000.0;
    let close = app.page.named("button", "run.close")?;
    app.page.click(&close)?;
    app.page.wait_count(&css(".changes-view"), 0)?;
    same(app.page.count(&css(".patch-line"))?, 0, "closing Changes releases every patch line")?;
    let (closed, _) = ctx.memory_mb(app.id)?;
    let numbers = format!(
        "Changes 30 x 3990 lines: before {baseline:.1} MB, sampled peak {peak:.1} MB, after closing {closed:.1} MB; switching {cpu_ms:.0} ms CPU ({:.1}% of one core over {switch_ms:.0} ms)",
        cpu_ms / switch_ms * 100.0
    );
    ctx.notes.push(numbers.clone());
    ensure(peak <= BUDGET_MB && closed <= BUDGET_MB, || format!("{numbers}; over the {BUDGET_MB} MB budget"))
}

// Weak references prove DOM disposal; process identities and working sets show Chromium's delayed teardown separately.
fn visual_frames_collected(
    ctx: &mut Ctx,
    app: &mut Opened,
    baseline: f64,
    margin_mb: f64,
    before: &[crate::proctree::ProcRow],
    what: &str,
) -> Result<f64> {
    let started = Instant::now();
    let mut closed;
    let mut retained;
    let mut extra: Vec<u32>;
    loop {
        app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
        let label = format!("{what} +{:.1}s", started.elapsed().as_secs_f64());
        closed = super::setup::measure(ctx, app, &label)?;
        let rows = crate::proctree::live_tree(&crate::proctree::snapshot()?, app.pid);
        extra = rows
            .iter()
            .filter(|row| row.name.to_lowercase().contains("msedgewebview2") && !crate::proctree::same_process(row, before))
            .map(|row| row.pid)
            .collect();
        retained = app.page.eval(
            "window.__visualFrames.flatMap((ref,i)=>{const frame=ref.deref();return frame?[{i,connected:frame.isConnected,srcdocBytes:frame.srcdoc.length,urlBytes:frame.src.length}]:[]})",
        )?;
        ctx.notes.push(format!("{label}: extra WebView2 pids {extra:?}; remaining frame references: {retained}"));
        if (started.elapsed() >= Duration::from_secs(2) && closed <= baseline + margin_mb && retained == json!([]) && extra.is_empty())
            || started.elapsed() >= Duration::from_secs(15)
        {
            break;
        }
        pause(1.0);
    }
    let dom = app.page.cdp.call("Memory.getDOMCounters", json!({}))?;
    ctx.notes.push(format!("{what}: renderer DOM counters {dom}"));
    let numbers = format!(
        "{what}: {closed:.1} MB, baseline {baseline:.1} MB, retained difference {:.1} MB after {:.1}s; remaining frame references: {retained}; exact bytes: baseline {:.0}, current {:.0}, retained delta {:.0}, margin {:.0}",
        closed - baseline,
        started.elapsed().as_secs_f64(),
        baseline * 1024.0 * 1024.0,
        closed * 1024.0 * 1024.0,
        (closed - baseline) * 1024.0 * 1024.0,
        margin_mb * 1024.0 * 1024.0
    );
    ctx.notes.push(numbers.clone());
    let failed = !extra.is_empty() || retained != json!([]) || closed > baseline + margin_mb;
    if failed {
        // These observations follow the captured failure; diagnostic errors cannot change its result.
        let capture_started = Instant::now();
        let processes = crate::proctree::private_processes(app.pid);
        let previous_deadline = app.page.cdp.set_deadline(Some(Instant::now() + Duration::from_secs(3)));
        let heap = app.page.cdp.call("Runtime.getHeapUsage", json!({}));
        app.page.cdp.set_deadline(previous_deadline);
        let roles = app.page.cdp.process_roles();
        ctx.notes.push(format!("POST-FAILURE ONLY: capture ended {:.3}s after predicate; owned process memory {processes:?}; heap {heap:?}; process roles {roles:?}", capture_started.elapsed().as_secs_f64()));
        let previous_deadline = app.page.cdp.set_deadline(Some(Instant::now() + Duration::from_secs(3)));
        let pressure = app.page.cdp.call("Memory.simulatePressureNotification", json!({ "level": "critical" }));
        let pressure_heap = app.page.cdp.call("Runtime.getHeapUsage", json!({}));
        app.page.cdp.set_deadline(previous_deadline);
        let pressure_memory = crate::proctree::private_processes(app.pid);
        ctx.notes.push(format!("POST-FAILURE ONLY: native cache pressure {pressure:?}; heap {pressure_heap:?}; owned process memory {pressure_memory:?}; captured predicate unchanged"));
    }
    ensure(extra.is_empty(), || format!("{numbers}; extra WebView2 processes still alive: {extra:?}\n{}", ctx.notes.join("\n")))?;
    ensure(retained == json!([]), || format!("{numbers}; retired frames must be collectible\n{}", ctx.notes.join("\n")))?;
    ensure(closed <= baseline + margin_mb, || {
        format!("{numbers}; exceeds the {margin_mb} MB post-close margin after collection\n{}", ctx.notes.join("\n"))
    })?;
    Ok(closed)
}

// DevTools reads the rendered DOM directly; the frame gains neither parent access nor a test reporting channel.
fn wait_for_mermaid_drawing(page: &mut Page) -> Result {
    let url = page.eval("document.querySelector('.visual-frame').src")?;
    let targets = page.cdp.call("Target.getTargets", json!({}))?;
    let target = targets["targetInfos"]
        .as_array()
        .and_then(|targets| targets.iter().find(|target| target["type"] == "iframe" && target["url"] == url));
    let session = if let Some(target) = target {
        Some(
            page.cdp.call("Target.attachToTarget", json!({ "targetId": target["targetId"], "flatten": true }))?["sessionId"]
                .as_str()
                .ok_or("the visual frame has no DevTools session")?
                .to_string(),
        )
    } else {
        None
    };
    let mut params = json!({
        "awaitPromise": true, "returnByValue": true,
        "expression": r#"new Promise(resolve => {
          const until = performance.now() + 15000;
          const timer = setInterval(() => {
            const svg = document.querySelector('#drawing > svg');
            const box = svg?.getBoundingClientRect();
            const ready = svg?.namespaceURI === 'http://www.w3.org/2000/svg'
              && box.width > 0 && box.height > 0
              && svg.textContent.includes('Memory sample') && svg.textContent.includes('Renderer released')
              && document.getElementById('source')?.hidden === true && document.getElementById('fallback')?.hidden === true;
            if (ready || performance.now() >= until) {
              clearInterval(timer);
              resolve({ ready, svg: !!svg, sourceHidden: document.getElementById('source')?.hidden,
                fallbackHidden: document.getElementById('fallback')?.hidden });
            }
          }, 50);
        })"#
    });
    if session.is_none() {
        // A webview without a separate renderer still has an opaque child frame: inspect its own isolated world.
        let tree = page.cdp.call("Page.getFrameTree", json!({}))?;
        let child = tree["frameTree"]["childFrames"]
            .as_array()
            .and_then(|frames| frames.iter().find(|frame| frame["frame"]["url"] == url))
            .ok_or("the visual frame has no inspectable DevTools document")?;
        params["contextId"] = page.cdp.call(
            "Page.createIsolatedWorld",
            json!({
                "frameId": child["frame"]["id"], "worldName": "faber-screen-mermaid"
            }),
        )?["executionContextId"]
            .clone();
    }
    let inspected = page.cdp.call_in_session(session.as_deref(), "Runtime.evaluate", params);
    if let Some(session) = session {
        page.cdp.call("Target.detachFromTarget", json!({ "sessionId": session }))?;
    }
    let inspected = inspected?;
    ensure(inspected["exceptionDetails"].is_null() && inspected["result"]["value"]["ready"] == json!(true), || {
        format!(
            "the real pinned Mermaid renderer must produce its SVG under the unchanged Live sandbox (internet required, no stub or offline fallback): {inspected}"
        )
    })
}

fn visual_answers_pane_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let conversation = json!({
        "id": "visual-pane-memory", "title": "Visual pane memory", "area": "chat", "sessions": {},
        "createdAt": 1, "updatedAt": 1,
        "messages": [{ "id": "visual-page", "role": "assistant", "vendor": "claude", "at": 1, "status": "done",
            "text": "```html\n<title>Memory chart</title><h1>Memory chart</h1><svg viewBox=\"0 0 300 160\"><rect x=\"20\" y=\"20\" width=\"80\" height=\"120\" fill=\"teal\"/></svg>\n```" },
          { "id": "visual-mermaid", "role": "assistant", "vendor": "claude", "at": 2, "status": "done",
            "text": "```mermaid\nflowchart LR\nA[Memory sample] --> B[Renderer released]\n```" }]
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("visual-pane-memory.json"), conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    app.page.click(&css(".list .item").has_text("Visual pane memory"))?;
    app.page.wait_count(&css(".visual-card"), 2)?;
    same(app.page.count(&css("iframe"))?, 0, "a closed visual pane keeps no frame")?;
    app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
    pause(0.5);
    let before_processes = crate::proctree::live_tree(&crate::proctree::snapshot()?, app.pid);
    let baseline = super::setup::measure(ctx, &app, "visual answer before opening")?;
    let dom = app.page.cdp.call("Memory.getDOMCounters", json!({}))?;
    ctx.notes.push(format!("visual answer before opening: renderer DOM counters {dom}"));
    // Wait for the opaque frame's real load in its parent, without reading across the sandbox boundary.
    app.page.eval("document.addEventListener('load',function ready(e){if(e.target instanceof HTMLIFrameElement && e.target.matches('.visual-frame')){document.removeEventListener('load',ready,true);document.body.dataset.visualMemoryReady='yes';}},true)")?;
    app.page.click(&css(".visual-card").has_text("Memory chart").css(".visual-open"))?;
    app.page.wait_count(&css(".visual-frame"), 1)?;
    app.page.wait_for("the visual page to finish loading", "document.body.dataset.visualMemoryReady==='yes'")?;
    app.page.eval("window.__visualFrames=[new WeakRef(document.querySelector('.visual-frame'))];void 0")?;
    super::setup::measure(ctx, &app, "visual answer while open")?;
    let close = app.page.named("button", "run.close")?;
    app.page.click(&close)?;
    app.page.wait_count(&css(".visual-pane"), 0)?;
    same(app.page.count(&css("iframe"))?, 0, "closing the visual pane removes its frame")?;
    visual_frames_collected(ctx, &mut app, baseline, 15.0, &before_processes, "visual answer after closing")?;

    if std::env::var("FABER_LIVE_CDN").as_deref() != Ok("1") {
        ctx.notes.push("Mermaid Live not measured; set FABER_LIVE_CDN=1 for the lead's real-CDN renderer and cost check".into());
        return Ok(());
    }
    // Real CDN bytes, the production integrity pin and native guards: no bundled library or replacement renderer.
    app.page.eval("window.__TAURI_INTERNALS__.invoke('save_settings',{patch:{webSearch:'live'}}).then(()=>true)")?;
    let mermaid_baseline = super::setup::measure(ctx, &app, "Mermaid Live before opening")?;
    let before_processes = crate::proctree::live_tree(&crate::proctree::snapshot()?, app.pid);
    let idle_before = ctx.cpu_ms(app.id)?;
    let idle_started = Instant::now();
    pause(2.0);
    let idle_cpu = (ctx.cpu_ms(app.id)? - idle_before) / idle_started.elapsed().as_secs_f64() / 10.0;
    let render_before = ctx.cpu_ms(app.id)?;
    let render_started = Instant::now();
    app.page.eval("document.addEventListener('load',function ready(e){if(e.target===document.querySelector('.visual-frame')){document.removeEventListener('load',ready,true);document.body.dataset.mermaidMemoryLoaded='yes';}},true)")?;
    app.page.click(&css(".visual-card .visual-open").last())?;
    app.page.wait_count(&css(".visual-frame"), 1)?;
    app.page
        .wait_for("the Mermaid frame document to load before inspecting its target", "document.body.dataset.mermaidMemoryLoaded==='yes'")?;
    wait_for_mermaid_drawing(&mut app.page)?;
    let render_cpu_ms = ctx.cpu_ms(app.id)? - render_before;
    let render_wall_ms = render_started.elapsed().as_secs_f64() * 1000.0;
    same(
        app.page.eval("document.querySelector('.visual-frame').getAttribute('sandbox')")?,
        json!("allow-scripts"),
        "Mermaid keeps the opaque sandbox",
    )?;
    app.page.eval("window.__visualFrames=[new WeakRef(document.querySelector('.visual-frame'))];void 0")?;
    let mermaid_open = super::setup::measure(ctx, &app, "Mermaid Live after actual SVG rendering")?;
    let open_before = ctx.cpu_ms(app.id)?;
    let open_started = Instant::now();
    pause(2.0);
    let open_cpu = (ctx.cpu_ms(app.id)? - open_before) / open_started.elapsed().as_secs_f64() / 10.0;
    ctx.shot(&mut app, "visual-pane-memory-mermaid-live")?;
    app.page.click(&close)?;
    app.page.wait_count(&css(".visual-pane"), 0)?;
    same(app.page.count(&css("iframe"))?, 0, "closing the Mermaid pane removes its real renderer frame")?;
    let mermaid_closed = visual_frames_collected(ctx, &mut app, mermaid_baseline, 15.0, &before_processes, "Mermaid Live after closing")?;
    let closed_before = ctx.cpu_ms(app.id)?;
    let closed_started = Instant::now();
    pause(2.0);
    let closed_cpu = (ctx.cpu_ms(app.id)? - closed_before) / closed_started.elapsed().as_secs_f64() / 10.0;
    ctx.notes.push(format!(
        "Mermaid Live, real integrity-checked CDN renderer: before {mermaid_baseline:.1} MB, rendered {mermaid_open:.1} MB (+{:.1} MB), after close {mermaid_closed:.1} MB (+{:.1} MB); opening and rendering {render_cpu_ms:.0} ms CPU over {render_wall_ms:.0} ms including download; idle processor of one core before {idle_cpu:.1}%, open {open_cpu:.1}%, closed {closed_cpu:.1}%",
        mermaid_open - mermaid_baseline, mermaid_closed - mermaid_baseline
    ));
    Ok(())
}

fn visual_answers_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let messages: Vec<_> = (1..=2)
        .map(|version| {
            let source = format!(
                "```html\n<!doctype html><html><head><title>Budget chart</title></head><body data-version=\"{version}\"><h1>Budget chart</h1><svg viewBox=\"0 0 300 160\"><rect x=\"20\" y=\"20\" width=\"80\" height=\"{}\" fill=\"teal\"/></svg><button onclick=\"this.textContent=Number(this.textContent)+1\">0</button></body></html>\n```",
                version * 50
            ).replace("</body>", r#"<input style="position:fixed;inset:0;width:100%;height:100%;box-sizing:border-box" aria-label="Visual input"><script>
              const input=document.querySelector('input');
              const counter=document.querySelector('button');
              document.addEventListener('focusin',e=>parent.postMessage({visualTestFocus:document.body.dataset.version+':'+e.target.tagName},'*'));
              counter.addEventListener('click',()=>setTimeout(()=>parent.postMessage({visualTestCounter:counter.textContent,version:document.body.dataset.version},'*'),0));
              input.addEventListener('input',()=>parent.postMessage({visualTestInput:input.value,version:document.body.dataset.version,focused:document.activeElement===input},'*'));
              addEventListener('contextmenu',e=>parent.postMessage({visualTestMenu:e.defaultPrevented,trusted:e.isTrusted},'*'));
              parent.postMessage({visualTestReady:document.body.dataset.version},'*');
            </script></body>"#);
            json!({ "id": format!("visual-{version}"), "role": "assistant", "text": source,
                "at": version, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" })
        })
        .collect();
    let conversation = json!({
        "id": "visual-budget", "title": "Visual memory", "area": "chat", "sessions": {},
        "createdAt": 1, "updatedAt": 2, "messages": messages
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("visual-budget.json"), conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    app.page.click(&css(".list .item").has_text("Visual memory"))?;
    app.page.wait_count(&css(".visual-card"), 2)?;
    same(app.page.count(&css("iframe"))?, 0, "closed visual answers keep no frame alive")?;
    app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
    pause(0.5);
    let before_processes = crate::proctree::live_tree(&crate::proctree::snapshot()?, app.pid);
    let baseline = super::setup::measure(ctx, &app, "visual versions before opening")?;
    let dom = app.page.cdp.call("Memory.getDOMCounters", json!({}))?;
    ctx.notes.push(format!("visual versions before opening: renderer DOM counters {dom}"));
    let before_idle = ctx.cpu_ms(app.id)?;
    let idle_started = Instant::now();
    pause(2.0);
    let idle_cpu = (ctx.cpu_ms(app.id)? - before_idle) / idle_started.elapsed().as_secs_f64() / 10.0;
    app.page.eval("window.addEventListener('message',e=>{if(e.source!==document.querySelector('.visual-frame')?.contentWindow)return; if(e.data?.visualTestReady)document.body.dataset.visualReadyVersion=e.data.visualTestReady; if(e.data?.visualTestFocus)document.body.dataset.visualFocused=e.data.visualTestFocus; if(e.data?.visualTestCounter)document.body.dataset.visualCounter=e.data.version+':'+e.data.visualTestCounter; if(e.data?.visualTestInput && e.data.focused)document.body.dataset.visualTyped=e.data.version+':'+e.data.visualTestInput; if(e.data?.visualTestMenu===true && e.data.trusted)document.body.dataset.visualMenuBlocked='yes';})")?;
    app.page.eval("window.__visualLoads=0;document.addEventListener('load',e=>{if(e.target===document.querySelector('.visual-frame'))window.__visualLoads++;},true)")?;
    app.page.click(&css(".visual-card .visual-open").last())?;
    app.page.wait_visible(&css(".visual-pane"))?;
    app.page.wait_count(&css(".visual-frame"), 1)?;
    app.page.wait_for("the first visual version to load", "window.__visualLoads===1")?;
    let visual_history = app.page.eval("history.length")?;
    app.page.eval("window.__visualFrames=[new WeakRef(document.querySelector('.visual-frame'))];void 0")?;
    let before_open = ctx.cpu_ms(app.id)?;
    let open_started = Instant::now();
    pause(2.0);
    let open_cpu = (ctx.cpu_ms(app.id)? - before_open) / open_started.elapsed().as_secs_f64() / 10.0;
    let opened = super::setup::measure(ctx, &app, "visual versions while open")?;
    let mut peak = opened.max(baseline);
    let cpu_before = ctx.cpu_ms(app.id)?;
    let switch_started = Instant::now();
    let mut loads = 1;
    for _ in 0..10 {
        for (control, version) in [(".visual-prev", 1), (".visual-next", 2)] {
            loads += 1;
            app.page.click(&css(control))?;
            app.page.wait_for(
                "the chosen visual version to load in the same frame",
                &format!("window.__visualLoads==={loads} && document.body.dataset.visualReadyVersion==='{version}'"),
            )?;
            same(app.page.eval("history.length")?, visual_history.clone(), "version switches replace their browser history entry")?;
            same(app.page.count(&css("iframe"))?, 1, "switching versions keeps only one frame mounted")?;
            same(
                app.page.eval("document.querySelector('.visual-frame')===window.__visualFrames[0].deref()")?,
                json!(true),
                "every version reuses the original frame",
            )?;
            peak = peak.max(super::setup::measure(ctx, &app, &format!("visual version switch {} to {version}", loads - 1))?);
        }
    }
    let switch_ms = switch_started.elapsed().as_secs_f64() * 1000.0;
    let switch_cpu = ctx.cpu_ms(app.id)? - cpu_before;
    let close = app.page.named("button", "run.close")?;
    app.page.click(&close)?;
    app.page.wait_count(&css(".visual-pane"), 0)?;
    same(app.page.count(&css("iframe"))?, 0, "closing the visual answer removes its frame")?;
    let numbers = format!(
        "visual answers: before {baseline:.1} MB, open {opened:.1} MB, sampled peak {peak:.1} MB; processor of one core idle {idle_cpu:.1}%, open {open_cpu:.1}%; twenty version switches {switch_cpu:.0} ms CPU ({:.1}% of one core over {switch_ms:.0} ms)",
        switch_cpu / switch_ms * 100.0
    );
    ctx.notes.push(numbers.clone());
    // One-time browser cache warm-up: CI +5..+7 MB, Erick's PC +14.9 MB; allow 25 MB over the cold baseline.
    let closed = visual_frames_collected(ctx, &mut app, baseline, 25.0, &before_processes, "visual versions after closing")?;
    // Real browser input must cross the opaque frame, including the modal pane at the minimum window width.
    super::shell::set_size(&mut app.page, 720, 600)?;
    app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
    let resized = super::setup::measure(ctx, &app, "visual versions closed after resize to 720 px")?;
    let resized_heap = app.page.cdp.call("Runtime.getHeapUsage", json!({}))?;
    let resized_dom = app.page.cdp.call("Memory.getDOMCounters", json!({}))?;
    let resized_animations =
        app.page.eval("document.getAnimations().map(a=>({state:a.playState,time:a.currentTime,timing:a.effect?.getComputedTiming()}))")?;
    ctx.notes.push(format!("visual narrow baseline diagnostics: heap {resized_heap}; DOM {resized_dom}; animations {resized_animations}"));
    ctx.notes.push(format!(
        "visual resize-only delta before reopening: {:.1} MB; compare retention against this same-width closed state",
        resized - closed
    ));
    peak = peak.max(resized);
    same(app.page.count(&css("iframe"))?, 0, "the narrow baseline has no open visual frame")?;
    let mut previous_closed = resized;
    for cycle in 1..=2 {
        app.page.eval("delete document.body.dataset.visualReadyVersion;delete document.body.dataset.visualFocused;delete document.body.dataset.visualCounter;delete document.body.dataset.visualTyped;delete document.body.dataset.visualMenuBlocked")?;
        app.page.click(&css(".visual-card .visual-open").last())?;
        app.page.wait_visible(&css(".visual-frame"))?;
        app.page.eval("window.__visualFrames.push(new WeakRef(document.querySelector('.visual-frame')));void 0")?;
        // Visible belongs to the outer iframe, not its asynchronous document or that document's input listeners.
        loads += 1;
        app.page.wait_for(
            "the reopened visual document to load and install its input handlers",
            &format!("window.__visualLoads==={loads} && document.body.dataset.visualReadyVersion==='2'"),
        )?;
        same(app.page.eval("document.querySelector('#running').getAttribute('aria-modal')")?, json!("true"), "the 720 px pane is modal")?;
        app.page.click(&css(".visual-frame"))?;
        app.page.wait_for("the reopened page input to receive real mouse focus", "document.body.dataset.visualFocused==='2:INPUT'")?;
        app.page.eval("delete document.body.dataset.visualReadyVersion;delete document.body.dataset.visualFocused;delete document.body.dataset.visualCounter;delete document.body.dataset.visualTyped")?;
        app.page.press_on(&css(".visual-prev"), "ArrowLeft")?;
        loads += 1;
        app.page.wait_for(
            "the keyboard-selected version to load its own input handlers",
            &format!("window.__visualLoads==={loads} && document.body.dataset.visualReadyVersion==='1'"),
        )?;
        same(
            app.page.eval("document.querySelector('.visual-frame')===window.__visualFrames.at(-1).deref()")?,
            json!(true),
            "the keyboard version switch reuses the reopened frame",
        )?;
        // Repeat the twenty-switch workload on the warmed browser so the 5 MB margin below also bounds retention per switch.
        for _ in 0..10 {
            for (control, version) in [(".visual-next", 2), (".visual-prev", 1)] {
                loads += 1;
                app.page.click(&css(control))?;
                app.page.wait_for(
                    "the warmed visual version to load in the same frame",
                    &format!("window.__visualLoads==={loads} && document.body.dataset.visualReadyVersion==='{version}'"),
                )?;
            }
        }
        same(app.page.count(&css("iframe"))?, 1, "the warmed switches keep only one frame mounted")?;
        app.page.press_on(&css(".visual-next"), "Tab")?;
        // The page's counter precedes its input in DOM order, even though the input covers it in this pointer fixture.
        app.page.wait_for("Tab to enter the new version at its first control", "document.body.dataset.visualFocused==='1:BUTTON'")?;
        app.page.press("Enter")?;
        app.page.wait_for("Enter to activate the new version's counter", "document.body.dataset.visualCounter==='1:1'")?;
        app.page.press("Tab")?;
        app.page.wait_for("the next Tab to reach the new version's input", "document.body.dataset.visualFocused==='1:INPUT'")?;
        app.page.press("Space")?;
        app.page.wait_for("the new document to receive the Space key as text", "document.body.dataset.visualTyped==='1: '")?;
        app.page.eval("delete document.body.dataset.visualMenuBlocked")?;
        app.page.right_click(&css(".visual-frame"))?;
        app.page.wait_for(
            "the real native context menu to be canceled inside the visual frame",
            "document.body.dataset.visualMenuBlocked==='yes'",
        )?;
        app.page.press("Escape")?;
        app.page.wait_count(&css(".visual-pane"), 0)?;
        same(app.page.count(&css("iframe"))?, 0, "Escape from the page input removes the frame at 720 px")?;
        let margin = 5.0_f64.min(baseline + 25.0 - previous_closed);
        previous_closed = visual_frames_collected(
            ctx,
            &mut app,
            previous_closed,
            margin,
            &before_processes,
            &format!("visual answer after Escape at 720 px cycle {cycle}"),
        )?;
        peak = peak.max(previous_closed);
        ensure(previous_closed <= baseline + 25.0, || {
            format!("the complete narrow cycle {cycle} retains {:.1} MB over the cold reading; limit 25 MB", previous_closed - baseline)
        })?;
    }
    ensure(peak <= BUDGET_MB && closed <= BUDGET_MB, || format!("{numbers}; over the {BUDGET_MB} MB budget"))
}

/// Parity P2: a reply names `src/app.ts:240` and links a file outside the conversation's folder. The first opens read only in
/// the running column with line 240 marked and in view; the second is refused by the engine, which reads nothing.
fn file_at_a_line(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let folder = ctx.sb.sandbox.join("named-files");
    std::fs::create_dir_all(folder.join("src")).map_err(|e| e.to_string())?;
    let text: String = (1..=300).map(|i| format!("line {i}\n")).collect();
    std::fs::write(folder.join("src/app.ts"), text).map_err(|e| e.to_string())?;
    std::fs::write(ctx.sb.sandbox.join("outside.txt"), "not for the reply").map_err(|e| e.to_string())?;
    let at: u64 = 1_790_000_000_000;
    let id = "5f1c0000-0000-4000-8000-0000000000f1";
    let reply = "[The old note](../outside.txt) is gone; the greeting is in `src/app.ts:240`.";
    let conversation = json!({
        "id": id, "title": "Named file", "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at,
        "workingFolder": folder.to_string_lossy(),
        "messages": [
            { "id": "5f1c0000-0000-4000-8000-0000000000f2", "role": "user", "text": "Where is the greeting?", "at": at },
            { "id": "5f1c0000-0000-4000-8000-0000000000f3", "role": "assistant", "text": reply, "at": at + 1, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" }
        ]
    });
    let conversations = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&conversations).map_err(|e| e.to_string())?;
    std::fs::write(conversations.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())?;

    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    // The CI window's size: the running column covers the conversation there instead of sitting beside it.
    super::shell::set_size(page, 1028, 779)?;
    page.click(&css(".list .item").has_text("Named file"))?;
    page.click(&css("article.ai a.file-ref").has_text("src/app.ts:240"))?;
    page.wait_visible(&css(".file-view .file-line.mark"))?;
    same(page.count(&css(".file-line.mark"))?, 1, "one line is marked")?;
    same(page.inner_text(&css(".file-line.mark"))?, "line 240".to_string(), "the marked line is the one the reply named")?;
    same(page.count(&css(".file-line"))?, 300, "the whole short file is shown")?;
    page.wait_for(
        "the marked line to be in view",
        "(() => { const m = document.querySelector('.file-line.mark').getBoundingClientRect(); const b = document.querySelector('.file-view').getBoundingClientRect(); return m.top >= b.top && m.bottom <= b.bottom; })()",
    )?;
    // The column's entrance has ended before the picture the visual critique reads.
    pause(0.5);
    ctx.shot(&mut app, "file-at-a-line")?;
    let page = &mut app.page;
    let back = page.named("button", "agent.back")?;
    page.click(&back)?;
    page.wait_gone(&css(".file-view"))?;
    // A column that covers the conversation leaves it inert: it closes before the next link is clicked.
    let close = page.named("button", "run.close")?;
    page.click(&close)?;
    page.wait_gone(&css(".runcol"))?;
    page.click(&css("article.ai a.file-ref").has_text("The old note"))?;
    let refused = page.text_of("error.file.outside")?;
    page.wait_visible(&css(".file-view [role=status]").has_text(&refused))?;
    same(page.count(&css(".file-line"))?, 0, "nothing outside the folder is shown")
}

fn background_commands_memory(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let output = format!("{}\n", "x".repeat(407)).repeat(19) + &"x".repeat(440);
    same(output.len(), 8192, "the recorded output reaches the byte cap")?;
    same(output.lines().count(), 20, "the recorded output reaches the line cap")?;
    let mut recording = vec![
        json!({ "dir": "in", "msg": { "type": "user", "message": { "role": "user", "content": "Start background commands" }, "parent_tool_use_id": null, "session_id": "" } }),
        json!({ "dir": "out", "msg": { "type": "system", "subtype": "init", "session_id": "commands-budget", "model": "claude-haiku-5-5" } }),
    ];
    for i in 0..32 {
        recording.extend([
            json!({ "dir": "out", "msg": { "type": "assistant", "message": { "role": "assistant", "content": [{ "type": "tool_use", "id": format!("bash-{i}"), "name": "Bash", "input": { "command": format!("node task-{i}.js"), "run_in_background": true } }] } } }),
            json!({ "dir": "out", "msg": { "type": "system", "subtype": "task_started", "task_type": "local_bash", "task_id": format!("task-{i}"), "tool_use_id": format!("bash-{i}") } }),
            json!({ "dir": "in", "msg": { "type": "control_request", "request_id": format!("faber-{}", i + 1), "request": { "subtype": "get_task_output", "task_id": format!("task-{i}") } } }),
            json!({ "dir": "out", "msg": { "type": "control_response", "response": { "subtype": "success", "request_id": format!("faber-{}", i + 1), "response": { "output": output } } } }),
        ]);
    }
    recording.extend([
        json!({ "dir": "in", "msg": { "type": "control_request", "request_id": "faber-33", "request": { "subtype": "stop_task", "task_id": "task-0" } } }),
        json!({ "dir": "out", "msg": { "type": "control_response", "response": { "subtype": "success", "request_id": "faber-33", "response": {} } } }),
        json!({ "dir": "out", "msg": { "type": "system", "subtype": "task_notification", "task_id": "task-0", "tool_use_id": "bash-0", "status": "stopped" } }),
        json!({ "dir": "in", "msg": { "type": "control_request", "request_id": "faber-34", "request": { "subtype": "get_task_output", "task_id": "task-0" } } }),
        json!({ "dir": "out", "msg": { "type": "control_response", "response": { "subtype": "success", "request_id": "faber-34", "response": { "output": output } } } }),
    ]);
    // Each output read gates the next start, so the control ids and the fully loaded 32-command snapshot are deterministic.
    let path = ctx.sb.sandbox.join("background-commands.jsonl");
    let text = recording.iter().map(serde_json::Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, recording: Some(path), ..Launch::default() })?;
    let model = app.page.labelled("composer.model")?;
    app.page.select_option(&model, "Claude Haiku")?;
    let (baseline, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the memory reading must include WebView2".into())?;
    ask(&mut app.page, "Start background commands")?;
    app.page.click(&css(".run-btn"))?;
    app.page.wait_for("all background commands to start", "document.querySelector('.rc-commands .num')?.textContent === '32'")?;
    let (loaded, _) = ctx.memory_mb(app.id)?;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    app.page.click(&css(".rc-commands"))?;
    app.page.wait_count(&css(".command-output"), 32)?;
    app.page.wait_for(
        "all capped command output to arrive",
        "[...document.querySelectorAll('.command-output')].every(p => p.textContent.length === 8192 && p.textContent.split('\\n').length === 20 && p.childElementCount === 0)",
    )?;
    let (opened, _) = ctx.memory_mb(app.id)?;
    let messages = app.page.eval("[...document.querySelectorAll('.mine .you')].map(e => e.textContent)")?;
    same(messages.clone(), json!(["Start background commands"]), "only the person's original message exists before Stop")?;
    let row = css(".command-row").has_text("node task-0.js");
    app.page.click(&row.clone().css(".command-stop"))?;
    app.page.wait_count(&css(".command-stop"), 31)?;
    let stopped = app.page.text_of("commands.stopped")?;
    same(app.page.inner_text(&row.css(".verb"))?, stopped, "the selected command is confirmed stopped by the CLI")?;
    same(app.page.eval("[...document.querySelectorAll('.mine .you')].map(e => e.textContent)")?, messages, "Stop adds no message")?;
    same(app.page.count(&css(".command-row"))?, 32, "the finished command stays until its turn ends")?;
    let cpu_ms = ctx.cpu_ms(app.id)? - cpu_before;
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    let resting_cpu = ctx.cpu_ms(app.id)?;
    let resting = Instant::now();
    pause(1.0);
    let idle_cpu_ms = ctx.cpu_ms(app.id)? - resting_cpu;
    let resting_ms = resting.elapsed().as_secs_f64() * 1000.0;
    let mut peak = baseline.max(loaded).max(opened).max(ctx.memory_mb(app.id)?.0);
    for (width, height) in [(720, 800), (1280, 800), (1920, 1080)] {
        super::shell::set_size(&mut app.page, width, height)?;
        let fits = app.page.eval("(() => { const c = document.querySelector('#running'); return c.scrollWidth <= c.clientWidth && [...c.querySelectorAll('.command-output, .command-line')].every(e => e.scrollWidth <= e.clientWidth); })()")?;
        same(fits.as_bool(), Some(true), "command lines and output wrap inside the running column")?;
        ctx.shot(&mut app, &format!("background-commands-{width}x{height}"))?;
        peak = peak.max(ctx.memory_mb(app.id)?.0);
    }
    app.page.click(&css("#running .side-head .back"))?;
    app.page.wait_gone(&css(".commands-view"))?;
    same(app.page.count(&css(".command-output"))?, 0, "Back releases every output element")?;
    app.page.wait_for("only running commands counted", "document.querySelector('.rc-commands .num')?.textContent === '31'")?;
    let (closed, _) = ctx.memory_mb(app.id)?;
    let numbers = format!(
        "Background commands 32 x 8192 bytes / 20 lines: baseline {baseline:.1} MB, loaded {loaded:.1} MB, opened {opened:.1} MB, sampled peak {peak:.1} MB, after Back {closed:.1} MB; open and Stop {cpu_ms:.0} ms CPU ({:.1}% of one core over {open_ms:.0} ms); no events {idle_cpu_ms:.0} ms CPU ({:.1}% over {resting_ms:.0} ms)",
        cpu_ms / open_ms * 100.0,
        idle_cpu_ms / resting_ms * 100.0
    );
    ctx.notes.push(numbers.clone());
    ensure(peak <= BUDGET_MB && closed <= BUDGET_MB, || format!("{numbers}; over the {BUDGET_MB} MB budget"))?;
    ctx.close(app.id)?;
    let log = ctx.output(app.id);
    ensure(!log.contains("sent user") && !log.contains("recorded user"), || {
        format!("the fake CLI received an unexpected agent message: {log}")
    })
}

pub(super) fn seed_thousand_messages(ctx: &Ctx, with_tools: bool) -> Result {
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
                let mut message = json!({ "id": message_id, "role": "assistant", "text": text, "at": at + i, "vendor": "claude", "model": "claude-haiku-5-5", "status": "done" });
                if with_tools {
                    message["tools"] = json!([{ "id": format!("tool-{i}"), "name": "Read", "input": { "file_path": "example.txt" }, "ok": true, "output": "Synthetic bounded output.\n".repeat(40) }]);
                }
                message
            }
        })
        .collect();
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let conversation =
        json!({ "id": id, "title": "Long one", "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at, "messages": messages });
    if with_tools {
        let parsed: faber_engine::model::Conversation =
            serde_json::from_value(conversation.clone()).map_err(|e| format!("baseline fixture contract: {e}"))?;
        let tools: Vec<_> = parsed.messages.iter().flat_map(|message| message.tools.iter().flatten()).collect();
        ensure(
            parsed.messages.len() == 1000
                && tools.len() == 500
                && tools.iter().all(|tool| tool.output.as_ref().is_some_and(|output| output.lines().count() == 40 && output.len() <= 2048)),
            || "baseline fixture must contain 1000 messages and 500 bounded tool outputs".into(),
        )?;
    }
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())?;

    Ok(())
}

fn thousand_messages(ctx: &mut Ctx) -> Result {
    seed_thousand_messages(ctx, false)?;
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

fn reviews_change(ctx: &mut Ctx) -> Result {
    let mut launch = super::projects_ui::fixture(ctx)?;
    let repo = ctx.sb.sandbox.join("repo");
    let copy = ctx.sb.data.join("copies/changed-project");
    std::fs::create_dir_all(copy.parent().ok_or("the copy has no parent")?).map_err(|e| e.to_string())?;
    let git = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    super::projects::git(
        &git.command,
        &repo,
        &["worktree", "add", "-b", "faber/review-change", copy.to_str().ok_or("the copy path is not UTF-8")?],
    )?;
    std::fs::write(copy.join("fixture"), "changed").map_err(|e| e.to_string())?;
    let recorded: serde_json::Value =
        serde_json::from_slice(&std::fs::read(ctx.paths.fixtures.join("conversation-changes.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut conversation = recorded["conversation"].clone();
    conversation["projectId"] = json!("demo");
    conversation["copy"] = json!({
        "root": repo.to_string_lossy(), "path": copy.to_string_lossy(),
        "branch": "faber/review-change", "base": "refs/heads/owner"
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("changed-project.json"), conversation.to_string()).map_err(|e| e.to_string())?;
    launch.claude = CliSlot::Fake;
    launch.codex = CliSlot::Off;
    launch.recording = Some(ctx.paths.fixtures.join("claude/review-change.jsonl"));
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Demo"))?;
    page.click(&css(".proj-chats .item").has_text("Update the greeting"))?;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    for (key, finding, scope) in [
        ("changes.review", "The changed greeting has no fallback.", "for bugs."),
        ("changes.reviewSecurity", "The changed path accepts untrusted input.", "for bugs and security issues."),
    ] {
        page.click(&css(".topbar .pane-changes"))?;
        page.wait_count(&css(".changes-review button"), 2)?;
        let review = page.named("button", key)?.exact();
        page.click(&review)?;
        page.wait_gone(&css(".changes-view"))?;
        page.wait_for(
            "the review request in the person's conversation message",
            &format!("[...document.querySelectorAll('.mine .you')].some(e => e.textContent.includes({}))", json!(scope)),
        )?;
        page.wait_for(
            "the review findings streamed from the fake CLI",
            &format!("[...document.querySelectorAll('article.ai .md')].some(e => e.textContent.includes({}))", json!(finding)),
        )?;
        page.wait_for("the review answer to settle", "document.querySelector('article.ai:last-of-type .actions') !== null")?;
    }
    ctx.shot(&mut app, "review-change-findings")?;
    ctx.close(app.id)?;
    let output = ctx.output(app.id);
    ensure(!output.contains("sent user") && !output.contains("recorded user"), || {
        format!("the fake CLI received different review words: {output}")
    })
}

fn recap_on_demand(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let conversation = json!({
        "id": "recap-idle", "title": "Parser recap", "area": "chat", "createdAt": 1, "updatedAt": 2,
        "sessions": { "claude": "parser-session" }, "queued": [],
        "messages": [
            { "id": "q1", "role": "user", "text": "Refactor the parser, then test it.", "at": 1 },
            { "id": "a1", "role": "assistant", "text": "The parser is done. Tests remain open.", "at": 2,
              "vendor": "claude", "model": "claude-sonnet-5-5", "status": "done" }
        ]
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join("recap-idle.json");
    std::fs::write(&file, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        recording: Some(ctx.paths.fixtures.join("claude/recap-on-demand.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    page.click(&css(".list .item").has_text("Parser recap"))?;
    page.wait_visible(&css("article.ai .actions"))?;
    let thread = "JSON.stringify([...document.querySelectorAll('article')].map(e => e.textContent))";
    let before = page.eval(thread)?;
    page.click(&css(".topbar .pane-aside"))?;
    page.wait_visible(&css("#aside-box"))?;
    for entry in ["button", "menu", "send"].iter() {
        match *entry {
            "button" => page.click(&css(".aside-recap"))?,
            "menu" => {
                // In a narrow window the open column holds the focus: close it, as a person would, before typing.
                let close = page.text_of("run.close")?;
                page.click(&css(".side-head").role("button", &close).exact())?;
                page.fill(&css(".composer textarea"), "/recap")?;
                // The list opens after its own short delay, longer on a slow machine.
                page.wait_visible(&css(".mentions [role=option]").has_text("/recap"))?;
                page.click(&css(".mentions [role=option]").has_text("/recap"))?;
            }
            _ => {
                let close = page.text_of("run.close")?;
                page.click(&css(".side-head").role("button", &close).exact())?;
                page.fill(&css(".composer textarea"), "/recap")?;
                // Answers on the side carry their own "Send to the agent" buttons: only the composer's own.
                let send = page.text_of("composer.send")?;
                page.click(&css(".composer").role("button", &send).exact())?;
            }
        }
        page.wait_count(&css(".aside[data-state=done]"), 1)?;
        same(
            page.inner_text(&css(".aside-answer").last())?,
            "You asked for a parser refactor. The parser is done. Tests remain open.".to_string(),
            "the fake CLI's side answer",
        )?;
        same(page.eval(thread)?, before.clone(), "the main thread after a recap")?;
        // Clear in the head empties the side chat, so the next entry asks the recorded recap again with no earlier turns.
        if *entry != "send" {
            let clear = page.text_of("aside.clear")?;
            page.click(&css(".side-head").role("button", &clear).exact())?;
            page.wait_count(&css(".aside"), 0)?;
        }
        same(page.eval("document.querySelector('.composer textarea').value")?, json!(""), "recap inserts nothing")?;
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for key in ["messages", "queued", "sessions"] {
            same(&saved[key], &conversation[key], &format!("saved {key} after {entry}"))?;
        }
    }
    const SETTLED: &str =
        "document.getAnimations().every((a) => a.effect.getTiming().iterations === Infinity || a.playState !== 'running')";
    // The side chat is kept with the conversation: closed and opened again, the answer is still there.
    let page = &mut app.page;
    let close = page.text_of("run.close")?;
    page.click(&css(".side-head").role("button", &close).exact())?;
    page.click(&css(".topbar .pane-aside"))?;
    page.wait_count(&css(".aside[data-state=done]"), 1)?;
    // The column slides in; each picture is taken once it has settled.
    page.wait_for("the side chat to settle", SETTLED)?;
    ctx.shot(&mut app, "recap-idle")?;
    ctx.close(app.id)?;
    let output = ctx.output(app.id);
    ensure(!output.contains("sent user") && !output.contains("recorded user"), || {
        format!("the fake CLI received a different question: {output}")
    })?;
    // And after a restart, from its own file beside the conversation, which it never touched.
    let side = ctx.sb.data.join("side-chats").join("recap-idle.json");
    let turns: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(&side).map_err(|e| format!("no side chat file: {e}"))?).map_err(|e| e.to_string())?;
    same(turns.len(), 1, "turns kept in the side chat file")?;
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, ..Launch::default() })?;
    let page = &mut app.page;
    page.click(&css(".list .item").has_text("Parser recap"))?;
    page.wait_visible(&css("article.ai .actions"))?;
    page.click(&css(".topbar .pane-aside"))?;
    page.wait_count(&css(".aside[data-state=done]"), 1)?;
    same(page.eval(thread)?, before, "the main thread after a restart")?;
    page.wait_for("the side chat to settle", SETTLED)?;
    ctx.shot(&mut app, "side-chat-kept")?;
    super::shell::set_size(&mut app.page, 720, 800)?;
    pause(0.5);
    ctx.shot(&mut app, "side-chat-720")?;
    ctx.close(app.id)
}

fn edits_queued(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(Launch {
        claude: CliSlot::Fake,
        recording: Some(ctx.paths.fixtures.join("claude/edit-queued.jsonl")),
        ..Launch::default()
    })?;
    let page = &mut app.page;
    // This recording starts with the person's message, not Auto's separate routing question.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "Start queued edit")?;
    // The CLI has ended its answer but is still closing the turn, so the next message waits.
    page.wait_for("the answer to settle", "document.querySelector('article.ai .actions') !== null")?;
    ask(page, "Original queued words")?;
    let edit = page.named("button", "queue.editLabel")?;
    page.click(&edit)?;
    let field = page.labelled("queue.editLabel")?;
    page.fill(&field, "Edited queued words\nSecond line")?;
    ctx.shot(&mut app, "edit-queued-editor")?;
    let page = &mut app.page;
    page.press_on(&field, "Enter")?;
    page.wait_for("the saved queued words", "document.querySelector('.queue-text')?.textContent === 'Edited queued words\\nSecond line'")?;
    ctx.shot(&mut app, "edit-queued-saved")?;
    std::fs::write(ctx.sb.state.join("release"), "").map_err(|e| e.to_string())?;
    let page = &mut app.page;
    // The fake CLI echoes its expected input, so reading only the bubble would not prove what it received.
    page.wait_for(
        "the edited message to reach the CLI",
        "[...document.querySelectorAll('article.ai .md')].some(e => e.textContent.includes('Received edited queue words.'))",
    )?;
    page.wait_for("the queue to empty", "document.querySelectorAll('.mine.queued').length === 0")?;
    ensure(page.inner_text(&css(".mine .you").has_text("Edited queued words"))?.contains("Second line"), || {
        "the sent message lost the edit".into()
    })?;
    ctx.close(app.id)?;
    let output = ctx.output(app.id);
    ensure(!output.contains("sent user") && !output.contains("recorded user"), || {
        format!("the fake CLI received different words: {output}")
    })
}

/// Parity T7: a retry is the message's second version; the arrows under it walk the versions by mouse and by keyboard, each
/// opening its own conversation, and "Branch from here" on an answer opens one more where nothing is sent.
fn walks_versions(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    let retry = page.text_of("answer.retry")?.split('{').next().unwrap_or_default().trim().to_string();
    page.right_click(&css("article.ai"))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&retry))?;
    page.wait_for("two conversations in the list", "document.querySelectorAll('.list .item').length === 2")?;
    page.wait_visible(&actions)?;
    let count = "document.querySelector('.versions span')?.textContent";
    page.wait_for("the retry to be version 2 of 2", &format!("{count} === '2/2'"))?;
    let back = page.named("button", "mine.versionBack")?;
    page.click(&back)?;
    page.wait_for("the first version to open", &format!("{count} === '1/2'"))?;
    let next = page.named("button", "mine.versionNext")?;
    page.press_on(&next, "ArrowRight")?;
    page.wait_for("the arrow key to open the second version", &format!("{count} === '2/2'"))?;
    // Focus stays on the arrows, so the keys keep walking.
    page.wait_for("focus to come back to the arrows", "!!document.activeElement?.closest('.versions')")?;
    page.press("ArrowLeft")?;
    page.wait_for("the arrow keys to walk back", &format!("{count} === '1/2'"))?;
    page.wait_for("focus to come back to the arrows", "!!document.activeElement?.closest('.versions')")?;
    page.press("ArrowRight")?;
    page.wait_for("and forth again", &format!("{count} === '2/2'"))?;
    ctx.shot(&mut app, "versions")?;
    let page = &mut app.page;
    let here = page.text_of("answer.branchHere")?;
    page.wait_visible(&actions)?;
    page.right_click(&css("article.ai .model"))?;
    page.wait_visible(&css(".cmenu [role=menuitem]").has_text(&here))?;
    page.click(&css(".cmenu [role=menuitem]").has_text(&here))?;
    page.wait_for("three conversations in the list", "document.querySelectorAll('.list .item').length === 3")?;
    // The newest conversation is the branch, and it is the open one; its question still has two versions, since it holds the
    // retried one.
    page.wait_for("the branch to open", "document.querySelector('.list .item')?.getAttribute('aria-current') === 'true'")?;
    page.wait_for("its question to show its versions", &format!("{count} === '2/2'"))?;
    same(page.count(&css(".you"))?, 1, "the branch holds the question")?;
    same(page.count(&css("article.ai"))?, 1, "and its answer, with nothing sent")
}

/// The recorded chat with `steps` tool calls before its answer, each read returning `output` bytes and followed by a short
/// pause, so the reply streams like a long agent turn (a 2026-10-22 project reply ran 793 steps and 2 MB).
fn long_recording(ctx: &Ctx, steps: usize, output: usize) -> std::result::Result<std::path::PathBuf, String> {
    let chat = std::fs::read_to_string(ctx.paths.fixtures.join("claude/chat.jsonl")).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = chat.lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    let session = "c5b9834c-dfc7-4086-beed-2212be93b722";
    let body = "fn line() { /* read */ }\n".repeat(output / 25);
    let pause = json!({ "dir": "out", "msg": { "faber_pause": 30 } }).to_string();
    let mut work = Vec::new();
    for i in 0..steps {
        let id = format!("toolu_long{i:05}");
        let call = json!({ "type": "tool_use", "id": id, "name": "Read", "input": { "file_path": format!("src/part_{i}.rs") } });
        let usage = json!({ "input_tokens": 2, "output_tokens": 10, "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 0 });
        let message = json!({ "model": "claude-haiku-5-5", "id": format!("msg_long{i}"), "type": "message", "role": "assistant", "content": [call], "usage": usage });
        work.push(
            json!({ "dir": "out", "msg": { "type": "assistant", "message": message, "parent_tool_use_id": null, "session_id": session } })
                .to_string(),
        );
        work.push(pause.clone());
        let result = json!({ "type": "tool_result", "tool_use_id": id, "content": format!("{i}\n{body}") });
        work.push(json!({ "dir": "out", "msg": { "type": "user", "message": { "role": "user", "content": [result] }, "parent_tool_use_id": null, "session_id": session } }).to_string());
        work.push(pause.clone());
    }
    // After the person's message and the CLI's start, before the answer's own text.
    let at = lines.iter().position(|l| l.contains(r#""subtype":"init""#)).ok_or("chat.jsonl has no init line")? + 1;
    lines.splice(at..at, work);
    let path = ctx.sb.tmp.join("long-answer.jsonl");
    std::fs::write(&path, lines.join("\n")).map_err(|e| e.to_string())?;
    Ok(path)
}

fn streaming_pane_reading(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let recording = long_recording(ctx, 360, 2500)?;
    let mut app = ctx.launch(Launch { recording: Some(recording), ..claude_chat(ctx) })?;
    super::system_motion(&mut app.page, false)?;
    let model = app.page.labelled("composer.model")?;
    app.page.select_option(&model, "Claude Haiku")?;
    ask(&mut app.page, QUESTION)?;
    app.page
        .wait_for("live tool output to fill the thread", "document.querySelectorAll('article.ai .activity .tools > span').length >= 25")?;
    app.page.fill(&css(".composer textarea"), "Synthetic unsent draft")?;
    let point = app.page.eval(
        "(() => { const r=document.querySelector('.thread').getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()",
    )?;
    app.page
        .cdp
        .call("Input.dispatchMouseEvent", json!({"type":"mouseWheel","x":point["x"],"y":point["y"],"deltaX":0,"deltaY":-100000}))?;
    app.page.wait_for("the person to reach the old question", "document.querySelector('.thread').scrollTop < 2")?;
    let before = app.page.eval("(() => {window.__streamQuestion=document.querySelector('.you'); const t=document.querySelector('.thread');return {offset:window.__streamQuestion.getBoundingClientRect().top-t.getBoundingClientRect().top,tools:document.querySelectorAll('article.ai .activity .tools > span').length};})()")?;
    for reduced in [false, true] {
        super::system_motion(&mut app.page, reduced)?;
        for _ in 0..3 {
            app.page.click(&css(".pane-aside"))?;
            app.page.wait_visible(&css(".runcol"))?;
            app.page.click(&css(".pane-aside"))?;
            app.page.wait_for("the side chat to finish leaving", "!document.querySelector('.runcol')")?;
            let state = app.page.eval("(() => {const t=document.querySelector('.thread'),q=document.querySelector('.you');return {same:q===window.__streamQuestion,offset:q.getBoundingClientRect().top-t.getBoundingClientRect().top,top:t.scrollTop,draft:document.querySelector('.composer textarea').value,tools:document.querySelectorAll('article.ai .activity .tools > span').length,finished:!!document.querySelector('article.ai .actions')};})()")?;
            ensure(state["same"] == true && state["top"].as_f64().is_some_and(|n| n < 2.0), || {
                format!("pane toggling replaced the old question or stole reading position: {state}")
            })?;
            ensure(state["offset"].as_f64().zip(before["offset"].as_f64()).is_some_and(|(now, old)| (now - old).abs() <= 2.0), || {
                format!("the old question moved after closing the pane: {before} -> {state}")
            })?;
            same(state["draft"].clone(), json!("Synthetic unsent draft"), "the main draft survives pane toggles")?;
            ensure(state["finished"] == false, || "the pane sequence did not overlap live output".into())?;
        }
    }
    app.page.wait_for(
        "more tool output after the reading probe",
        &format!("document.querySelectorAll('article.ai .activity .tools > span').length > {}", before["tools"]),
    )?;
    ctx.shot(&mut app, "streaming-panes-reading-preserved")?;
    let stop = app.page.named("button", "composer.stop")?;
    app.page.click(&stop)?;
    app.page.wait_gone(&stop)?;
    same(
        app.page.eval("document.querySelector('.composer textarea').value")?,
        json!("Synthetic unsent draft"),
        "stopping the live turn retains the unsent draft",
    )?;
    ctx.close(app.id)
}

/// While a long reply streams, the screen hears only the steps that changed (`src-tauri/src/feed.rs`). Sent whole, each
/// update of this 600-step reply was a script of up to 1.6 MB, and the app grew from 157 MB to 863 MB in 40 s; the 2026-10-22
/// install reached 8 GB after hours of such replies. Memory is read as it streams, with no forced collection, since that
/// hid the growth; then every step must be on screen, in order, each with its own file.
fn long_streaming_answer(ctx: &mut Ctx) -> Result {
    const STEPS: usize = 600;
    /// What the app may grow by while the reply streams: the steps themselves take a few MB on screen.
    const GROWTH_MB: f64 = 100.0;
    start_settings(ctx, "en")?;
    let recording = long_recording(ctx, STEPS, 2500)?;
    let launch = Launch { recording: Some(recording), ..claude_chat(ctx) };
    let mut app = ctx.launch(launch)?;
    ask(&mut app.page, QUESTION)?;
    // Read once the message is sent, before the reply streams: the screen is drawn and the CLI has started.
    let (before, _) = ctx.memory_mb(app.id)?;
    let started = Instant::now();
    let mut samples = Vec::new();
    let mut peak = before;
    loop {
        pause(3.0);
        let (mb, _) = ctx.memory_mb(app.id)?;
        peak = peak.max(mb);
        samples.push(format!("{mb:.0}"));
        if app.page.eval("!!document.querySelector('article.ai .actions')")?.as_bool() == Some(true) {
            break;
        }
        ensure(started.elapsed() < Duration::from_secs(150), || format!("the reply did not end in 150 s; MB: {}", samples.join(" ")))?;
    }
    ctx.notes.push(format!(
        "{STEPS} steps in {:.0} s: {before:.0} MB before, peak {peak:.0} MB (every 3 s: {})",
        started.elapsed().as_secs_f64(),
        samples.join(" ")
    ));
    let shown = app.page.eval(
        "(() => { const chips = [...document.querySelectorAll('article.ai .activity .tools > span')]; return { n: chips.length, first: chips[0]?.textContent, last: chips.at(-1)?.textContent, summary: document.querySelector('article.ai .activity summary')?.textContent }; })()",
    )?;
    same(shown["n"].as_u64(), Some(STEPS as u64), "steps on screen")?;
    ensure(shown["first"].as_str().is_some_and(|t| t.contains("src/part_0.rs")), || format!("the first step: {shown}"))?;
    ensure(shown["last"].as_str().is_some_and(|t| t.contains(&format!("src/part_{}.rs", STEPS - 1))), || {
        format!("the last step: {shown}")
    })?;
    ensure(shown["summary"].as_str().is_some_and(|t| t.contains(&format!("{STEPS} steps"))), || format!("the work log: {shown}"))?;
    ensure(peak - before <= GROWTH_MB, || {
        format!("the app grew {:.0} MB while the reply streamed (from {before:.0} to {peak:.0} MB)", peak - before)
    })?;
    ensure(peak <= BUDGET_MB, || format!("{peak:.0} MB is over the {BUDGET_MB} MB budget"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port completeness: the nine tests of `tests/e2e/app.e2e.ts` are all still here, under their old names.
    #[test]
    fn the_nine_scenarios_of_the_node_suite_are_all_ported() {
        let names: Vec<&str> = scenarios().iter().map(|s| s.name).collect();
        for name in [
            "the app opens on an empty chat and fits the window",
            "a chat streams an answer with the model and the reason, and lands in the sidebar",
            "editing a message sends it as a branch and keeps the original conversation",
            "deleting a conversation asks once, then removes it",
            "with no AI installed, the app says what to do",
            "the screen speaks the chosen language and switches at once",
            "a look and the choices around it are used with one click and kept after reopening",
            "with no CLI running, Faber stays within its memory budget",
            "a conversation with a thousand messages opens quickly and stays within the budget",
        ] {
            assert!(names.contains(&name), "scenario lost: {name}");
        }
    }

    #[test]
    fn the_six_looks_are_the_six_the_screen_lists() {
        let ids: Vec<&str> = LOOKS.iter().map(|l| l.0).collect();
        assert_eq!(ids, ["sala", "oficina", "bancada", "manha", "alto", "cobalto"]);
    }
}
