//! The photographs of the public README: each scenario stages one screen from stored state a real run leaves behind (the
//! conversations and the project under `tests/fixtures/readme/`), in English and in the default look, and saves a PNG named
//! `readme-<photo>.png` with the other shots. They check nothing, so they join the suite only when `--filter` names
//! "readme shots":
//!
//! ```text
//! node scripts/cargo.mjs t --suite screen --filter "readme shots"
//! ```
//!
//! Look at each PNG, then copy the ones the page uses into `docs/media/`: hero, review, changes, auto, visual, ask and
//! catalog keep their names, `team` becomes `team-reply.png` and `home` becomes `chat-home.png`.
//!
//! Every time in a fixture is a negative number of milliseconds before the launch, so "updated 8 min ago" stays true on any
//! day. A reply may give `parts` in place of `text` and `tools`: its words and its steps in the order they happened, which
//! the loader turns into the stored form (each step's `at` is where the words stood when it started).

use super::projects::git;
use super::shell::set_size;
use super::{Result, Scenario, ask, pause, select_model, start_settings, system_motion};
use crate::app::{Ctx, Opened};
use crate::page::{Page, css};
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::path::Path;

const PROJECT: &str = "Bakery site";
const ORDERING: &str = "Add online ordering and prove the checkout works";
const PRICING: &str = "Price a box of twelve pastries";
const CHART: &str = "Draw this week's sales as a chart";
/// The first line of `tests/fixtures/readme/ask.jsonl`: the fake CLI answers only the words it recorded.
const GIFT_CARDS: &str = "Add a gift cards page to the bakery site: $10, $25 and $50.";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "readme shots: hero", run: hero },
        Scenario { name: "readme shots: team", run: team },
        Scenario { name: "readme shots: home", run: home },
        Scenario { name: "readme shots: auto", run: auto },
        Scenario { name: "readme shots: changes", run: changes },
        Scenario { name: "readme shots: ask", run: ask_first },
        Scenario { name: "readme shots: review", run: review },
        Scenario { name: "readme shots: visual", run: visual },
        Scenario { name: "readme shots: catalog", run: catalog },
    ]
}

/// The keys that hold a moment in a stored conversation. A step's `at` is a place in the reply's words and never negative.
const MOMENTS: [&str; 9] = ["at", "started", "ended", "createdAt", "updatedAt", "finishedAt", "movedAt", "since", "activityEnd"];

fn now_ms() -> Result<i64> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?;
    Ok(now.as_millis() as i64)
}

/// Turns every moment written as "so long before now" into the moment itself.
fn settle(value: &mut Value, now: i64) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map.iter_mut() {
                match inner.as_i64() {
                    Some(before) if before < 0 && MOMENTS.contains(&key.as_str()) => *inner = json!(now + before),
                    _ => settle(inner, now),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|inner| settle(inner, now)),
        _ => {}
    }
}

/// A reply's `parts` as the engine stores them: the words joined, and each step of the agent itself placed where the words
/// stood. A helper's own steps (`parent`) have no place in the words.
fn join_parts(message: &mut Value) {
    let Some(parts) = message.get("parts").and_then(Value::as_array).cloned() else { return };
    let (mut text, mut tools) = (String::new(), Vec::new());
    for part in parts {
        match part {
            Value::String(words) => text.push_str(&words),
            steps => {
                for mut tool in steps["tools"].as_array().cloned().unwrap_or_default() {
                    if tool.get("parent").is_none() {
                        tool["at"] = json!(text.encode_utf16().count());
                    }
                    tools.push(tool);
                }
            }
        }
    }
    let message = message.as_object_mut().expect("a message is an object");
    message.remove("parts");
    message.insert("text".into(), json!(text));
    message.insert("tools".into(), json!(tools));
}

fn fixture(ctx: &Ctx, path: &str) -> std::path::PathBuf {
    ctx.paths.fixtures.join("readme").join(path)
}

/// One conversation of `tests/fixtures/readme/conversations/` in its stored form.
fn stored(ctx: &Ctx, name: &str) -> Result<Value> {
    let source = fixture(ctx, &format!("conversations/{name}.json"));
    let bytes = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let mut conversation: Value = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", source.display()))?;
    for message in conversation["messages"].as_array_mut().into_iter().flatten() {
        join_parts(message);
    }
    settle(&mut conversation, now_ms()?);
    Ok(conversation)
}

fn store(ctx: &Ctx, conversation: &Value) -> Result {
    let id = conversation["id"].as_str().ok_or("the conversation has no id")?;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())
}

fn copy_tree(from: &Path, to: &Path) -> Result {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    Ok(())
}

/// Both AIs signed in, each fake CLI with the plain chat recording unless `first` gives the first AI another: only the
/// permission photograph sends a message.
fn both_ais(ctx: &Ctx, first: Option<&Path>) -> Result<Launch> {
    let mut setups = Vec::new();
    for (vendor, version) in [("claude", "2.1.296"), ("codex", "0.162.1")] {
        std::fs::write(ctx.sb.state.join(format!("{vendor}.signed-in")), "true").map_err(|e| e.to_string())?;
        let plain = ctx.paths.fixtures.join(format!("{vendor}/chat.jsonl"));
        let chat = first.filter(|_| vendor == "claude").map_or(plain, Path::to_path_buf);
        let setup = json!({ "vendor": vendor, "version": version, "chat": chat,
            "models": ctx.paths.fixtures.join("codex/models.jsonl") });
        let path = ctx.sb.state.join(format!("{vendor}.setup.json"));
        std::fs::write(&path, setup.to_string()).map_err(|e| e.to_string())?;
        setups.push(path);
    }
    let codex_recording = setups.pop();
    let claude_recording = setups.pop();
    Ok(Launch { claude: CliSlot::Fake, codex: CliSlot::Fake, claude_recording, codex_recording, ..Launch::default() })
}

/// What one photograph adds to the story.
#[derive(Default)]
struct Story {
    /// The ordering conversation went on: the person asked for a review and it ran two rounds.
    reviewed: bool,
    /// The recording the first AI replays in place of the plain chat.
    recording: Option<&'static str>,
}

/// The story every photograph tells: a bakery's website as a project (a real git repository), the conversation that added
/// online ordering in a work copy of its own with the files changed, two older conversations, and four chats.
fn bakery(ctx: &mut Ctx, story: Story) -> Result<Launch> {
    start_settings(ctx, "en")?;
    let found = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    let program = found.command.as_str();
    let repo = ctx.sb.sandbox.join("bakery-site");
    copy_tree(&fixture(ctx, "site"), &repo)?;
    git(program, &repo, &["init", "--initial-branch=main"])?;
    git(program, &repo, &["config", "user.name", "Bakery"])?;
    git(program, &repo, &["config", "user.email", "bakery@example.invalid"])?;
    git(program, &repo, &["add", "--all"])?;
    git(program, &repo, &["commit", "--no-gpg-sign", "-m", "The bakery site"])?;
    let projects = json!([{ "id": "bakery-site", "name": PROJECT, "path": repo.to_string_lossy() }]);
    std::fs::write(ctx.sb.data.join("projects.json"), projects.to_string()).map_err(|e| e.to_string())?;

    let mut ordering = stored(ctx, "ordering")?;
    if story.reviewed {
        let mut more = stored(ctx, "ordering-review")?;
        let more = more["messages"].as_array_mut().ok_or("the review has no messages")?;
        let last = more.last().map(|reply| reply["finishedAt"].clone()).ok_or("the review has no reply")?;
        ordering["messages"].as_array_mut().ok_or("the conversation has no messages")?.append(more);
        ordering["updatedAt"] = last;
    }
    // A conversation's copy lives in a folder named after it, on a branch of its own.
    let copy = ctx.sb.data.join("copies").join(ordering["id"].as_str().ok_or("the conversation has no id")?);
    std::fs::create_dir_all(copy.parent().ok_or("the copy has no parent")?).map_err(|e| e.to_string())?;
    git(program, &repo, &["worktree", "add", "-b", "faber/online-ordering", copy.to_str().ok_or("the copy path is not UTF-8")?])?;
    copy_tree(&fixture(ctx, "site-after"), &copy)?;
    ordering["copy"] = json!({ "root": repo.to_string_lossy(), "path": copy.to_string_lossy(),
        "branch": "faber/online-ordering", "base": "refs/heads/main" });
    store(ctx, &ordering)?;
    for name in ["menu-speed", "opening-hours", "pastry-box", "sales-chart", "newsletter", "hydration"] {
        store(ctx, &stored(ctx, name)?)?;
    }
    let recording = story.recording.map(|name| fixture(ctx, name));
    let mut launch = both_ais(ctx, recording.as_deref())?;
    launch.git_bin = Some(Path::new(program).parent().ok_or("git has no directory")?.into());
    Ok(launch)
}

/// The test window is hidden and never has the focus by itself; a window a person looks at does, and its effects run.
fn in_front(page: &mut Page) -> Result {
    system_motion(page, false)?;
    super::app::focus(page, true)
}

fn open_ordering(page: &mut Page) -> Result {
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text(PROJECT))?;
    page.click(&css(".proj-chats .item").has_text(ORDERING))?;
    page.wait_visible(&css("article.ai .actions"))?;
    // The Changes button joins the top bar once git has listed the copy's files.
    page.wait_visible(&css(".topbar .pane-changes"))
}

/// Moves the conversation as a turn of the wheel does: down the page for a positive number of pixels.
fn scroll(page: &mut Page, pixels: i32) -> Result {
    page.eval_on(&css(".thread"), &format!("el.scrollTop += {pixels}; return {{}};")).map(|_| ())
}

fn photograph(ctx: &Ctx, app: &mut Opened, name: &str) -> Result {
    // Opening a pane or a conversation ends with a short motion.
    pause(1.2);
    ctx.shot(app, &format!("readme-{name}"))
}

fn hero(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1600, 1200)?;
    in_front(&mut app.page)?;
    open_ordering(&mut app.page)?;
    // A conversation opens at its last words; the person scrolls up to the request.
    scroll(&mut app.page, -100_000)?;
    photograph(ctx, &mut app, "hero")?;
    ctx.close(app.id)
}

fn team(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1440, 900)?;
    in_front(&mut app.page)?;
    open_ordering(&mut app.page)?;
    app.page.click(&css("article .team .team-fold"))?;
    let member = css("article .team .team-member").has_text("Checkout tests");
    app.page.wait_visible(&member)?;
    app.page.click(&member)?;
    app.page.wait_visible(&css("#running .side-body"))?;
    // The reply's first line sits half under the top bar otherwise.
    scroll(&mut app.page, -10)?;
    photograph(ctx, &mut app, "team")?;
    ctx.close(app.id)
}

fn home(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1280, 800)?;
    in_front(&mut app.page)?;
    app.page.wait_visible(&css(".hello"))?;
    pause(1.5);
    photograph(ctx, &mut app, "home")?;
    ctx.close(app.id)
}

fn auto(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1280, 900)?;
    in_front(&mut app.page)?;
    app.page.click(&css(".list .item").has_text(PRICING))?;
    app.page.wait_visible(&css("article.ai .actions"))?;
    scroll(&mut app.page, -100_000)?;
    photograph(ctx, &mut app, "auto")?;
    ctx.close(app.id)
}

fn changes(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1600, 1000)?;
    in_front(&mut app.page)?;
    open_ordering(&mut app.page)?;
    app.page.click(&css(".topbar .pane-changes"))?;
    app.page.wait_count(&css(".change-file"), 5)?;
    app.page.click(&css(".change-file").has_text("cart.ts").css("summary"))?;
    app.page.wait_visible(&css(".change-file[open] .patch-line"))?;
    scroll(&mut app.page, -100_000)?;
    photograph(ctx, &mut app, "changes")?;
    ctx.close(app.id)
}

/// The only photograph that sends a message: the card exists only while a turn waits for its answer. The recording asks to
/// write one file and nobody answers. It is a chat: the card shows the whole path, and inside a project that path would be
/// the sandbox.
fn ask_first(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story { recording: Some("ask.jsonl"), ..Story::default() })?;
    let path = ctx.sb.data.join("settings.json");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut settings: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    settings["permissions"] = "ask".into();
    std::fs::write(path, settings.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    set_size(page, 1440, 960)?;
    in_front(page)?;
    page.wait_visible(&css(".hello"))?;
    // A chosen model starts at once; Auto would first ask its routing question, which this recording does not hold.
    select_model(page, "Haiku")?;
    ask(page, GIFT_CARDS)?;
    page.wait_visible(&css("article .ask").has_text("gift-cards.tsx"))?;
    // The Running column opens by itself a moment after the last key.
    page.wait_visible(&css(".runcol .ask").has_text("gift-cards.tsx"))?;
    page.wait_visible(&css(".run-btn.need"))?;
    photograph(ctx, &mut app, "ask")?;
    ctx.close(app.id)
}

fn review(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story { reviewed: true, ..Story::default() })?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1440, 900)?;
    in_front(&mut app.page)?;
    open_ordering(&mut app.page)?;
    app.page.click(&css("article .agents .agent").has_text("Round 1"))?;
    app.page.wait_visible(&css("#running .side-body .review-round"))?;
    photograph(ctx, &mut app, "review")?;
    ctx.close(app.id)
}

fn visual(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1280, 900)?;
    in_front(&mut app.page)?;
    app.page.click(&css(".list .item").has_text(CHART))?;
    app.page.click(&css(".visual-card .visual-open"))?;
    app.page.wait_count(&css(".visual-frame"), 1)?;
    // The page inside the frame loads behind a sandbox boundary nothing here can look across.
    pause(2.5);
    photograph(ctx, &mut app, "visual")?;
    ctx.close(app.id)
}

fn catalog(ctx: &mut Ctx) -> Result {
    let launch = bakery(ctx, Story::default())?;
    let mut app = ctx.launch(launch)?;
    set_size(&mut app.page, 1440, 900)?;
    in_front(&mut app.page)?;
    app.page.click(&css("[data-catalog-tab]"))?;
    app.page.wait_visible(&css("[data-catalog-card]"))?;
    // A little down the page, where the filters and the shelves begin.
    app.page.eval_on(
        &css(".catalog"),
        "el.scrollTop += el.querySelector('.catalog-bar').getBoundingClientRect().top - el.getBoundingClientRect().top - 22; return {};",
    )?;
    photograph(ctx, &mut app, "catalog")?;
    ctx.close(app.id)
}
