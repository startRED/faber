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

/// The six looks, as the screen lists them, with the effects each draws on the new conversation: the projector beam with
/// its dust, the loom and the flow line. The loom needs a stage wider than 920 px and the flow line one wider than
/// 1140 px, so that neither lies over the greeting; the stage is the window less the 248 px sidebar.
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
        Scenario { name: "a chat streams an answer with the model and the reason, and lands in the sidebar", run: streams_an_answer },
        Scenario {
            name: "with Auto, the answer names its model as soon as it starts, keeps the name to the end and after reopening",
            run: names_the_model_while_streaming,
        },
        Scenario {
            name: "with Auto on Codex, the answer names the model the CLI reports before the first word and keeps it to the end",
            run: names_the_codex_model_while_streaming,
        },
        Scenario { name: "editing a message sends it as a branch and keeps the original conversation", run: edits_as_a_branch },
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

fn streams_an_answer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
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
    ensure(tips["cost"].as_str().is_some_and(|t| t.contains("at API prices")), || {
        format!("the cost's tooltip should show the price note: {tips}")
    })?;
    ensure(!text.contains(&reason) && !text.contains("at API prices"), || format!("neither sits on the page itself: {text}"))?;
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
    same(page.count(&css(".composer"))?, 0, "no composer with no AI")
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
    ['the activity row', 'article.ai .activity summary', 4.5, true],
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
    page.click(&css("article .ask").has_text("a.txt").role("button", &allow).exact())?;
    let question = css("article .ask").has_text("b.txt");
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
