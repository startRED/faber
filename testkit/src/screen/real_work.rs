//! The real-work tier of the fair benchmark (R113; docs/superpowers/plans/2026-10-22-real-work-bench.md): the tasks a
//! person does, each one a run of its own inside the meter (`faber-bench`), with the CLI's answers replayed. They join
//! the suite only when `--filter` names "real work", since the first one sits idle for a minute and none checks anything
//! the other scenarios do not.
//!
//! Each one notes its times from the moment the message goes, read by polling the page, so they carry the polling's
//! step; the other app's driver polls the same way.

use super::shell::chat_with_future_resets;
use super::{Result, Scenario, ask, claude_chat, ensure, pause, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::json;
use std::time::Instant;

const QUESTION: &str = "In one sentence: why is the sky blue?";

/// The answer has shown its first words.
const FIRST_WORDS: &str = "(() => document.querySelector('article.ai .md')?.innerText.trim().length > 0)()";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "real work 1: sitting idle on a new conversation for 60 s", run: idle },
        Scenario { name: "real work 2: one chat turn streams its answer", run: one_turn },
        Scenario { name: "real work 3: closed and opened again, the same session goes on", run: reopen },
        Scenario { name: "real work 4: a 50-turn conversation opens and scrolls to its first message", run: long_thread },
        Scenario { name: "real work 5: a running turn is stopped and the next one sent", run: stop_and_send },
        Scenario { name: "real work 6: each vendor's limit shows after a turn", run: limits },
        Scenario { name: "real work 7: a project's answer edits a file and Faber's checks run on it", run: proof },
    ]
}

fn seconds(since: Instant) -> String {
    format!("{:.2} s", since.elapsed().as_secs_f64())
}

fn memory(ctx: &mut Ctx, id: usize) -> Result {
    let (mb, _) = ctx.memory_mb(id)?;
    ctx.notes.push(format!("app {mb:.0} MB"));
    Ok(())
}

fn idle(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let started = Instant::now();
    let mut app = ctx.launch(claude_chat(ctx))?;
    let title = app.page.named("heading", "empty.chatTitle")?;
    app.page.wait_visible(&title)?;
    ctx.notes.push(format!("open {}", seconds(started)));
    pause(60.0);
    memory(ctx, app.id)
}

fn one_turn(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    let sent = Instant::now();
    ask(page, QUESTION)?;
    page.wait_for("the first words of the answer", FIRST_WORDS)?;
    ctx.notes.push(format!("first words {}", seconds(sent)));
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ctx.notes.push(format!("end {}", seconds(sent)));
    memory(ctx, app.id)
}

fn reopen(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    ask(&mut app.page, QUESTION)?;
    app.page.wait_visible(&css("article.ai .actions"))?;
    ctx.close(app.id)?;
    let started = Instant::now();
    let recording = Some(ctx.paths.fixtures.join("claude/resume.jsonl"));
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording, ..Launch::default() })?;
    app.page.click(&css(".list .item").has_text(QUESTION))?;
    app.page.wait_visible(&css("article.ai .actions"))?;
    ctx.notes.push(format!("reopened {}", seconds(started)));
    let sent = Instant::now();
    ask(&mut app.page, "What single word did you reply with last time? Answer with only that word.")?;
    app.page.wait_for("the second answer to end", "document.querySelectorAll('article.ai .actions').length === 2")?;
    ctx.notes.push(format!("second turn {}", seconds(sent)));
    memory(ctx, app.id)
}

fn long_thread(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let at: u64 = 1_790_000_000_000;
    let paragraph = "Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ";
    let fence = "`".repeat(3);
    let id = "5f1d0000-0000-4000-8000-000000000001";
    let messages: Vec<_> = (0..100u64)
        .map(|i| {
            let message_id = format!("5f1d{i:04x}-0000-4000-8000-0000000000{i:02x}");
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
        json!({ "id": id, "title": "Fifty turns", "area": "chat", "sessions": {}, "createdAt": at, "updatedAt": at, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    let started = Instant::now();
    page.click(&css(".list .item").has_text("Fifty turns"))?;
    page.wait_visible(&css("article.ai").last())?;
    ctx.notes.push(format!("open {}", seconds(started)));
    let scrolled = Instant::now();
    // Scrolling up draws the earlier messages a part at a time, until the first one is there.
    let first = "t.innerText.includes('Question 0:')";
    page.wait_for_within(
        "the first message to be drawn",
        &format!("(() => {{ const t = document.querySelector('.thread'); t.scrollTop = 0; return {first}; }})()"),
        std::time::Duration::from_secs(30),
    )?;
    ctx.notes.push(format!("scrolled to the top {}", seconds(scrolled)));
    memory(ctx, app.id)
}

fn stop_and_send(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let recording = Some(ctx.paths.fixtures.join("claude/interrupt.jsonl"));
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording, ..Launch::default() })?;
    let page = &mut app.page;
    // A chosen model starts at once; Auto would wait for its routing question, which this recording does not hold.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, "Write a 600-word essay about the history of bread.")?;
    // The recording streams its first words, then waits for the interrupt.
    page.wait_for("the essay's first words", "document.querySelector('article.ai .md')?.innerText.includes('Few') === true")?;
    let stopped = Instant::now();
    let stop = page.named("button", "composer.stop")?;
    page.click(&stop)?;
    page.wait_gone(&stop)?;
    ctx.notes.push(format!("stopped {}", seconds(stopped)));
    let sent = Instant::now();
    ask(page, "Reply with just the word: pong")?;
    page.wait_for("the next answer", "[...document.querySelectorAll('article.ai .md')].some(m => m.innerText.trim() === 'pong')")?;
    ctx.notes.push(format!("next answer {}", seconds(sent)));
    let count = page.count(&css("article.ai"))?;
    ensure(count == 2, || format!("two answers, the stopped one kept: {count}"))?;
    memory(ctx, app.id)
}

fn limits(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_future_resets(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let sent = Instant::now();
    ask(page, QUESTION)?;
    // The CLI reports its limits with the turn: the tightest window of the recording is 47 % of the 7-day one.
    page.wait_for("the limit to show", "/^(≥ )?47%$/.test(document.querySelector('.usage .share')?.textContent ?? '')")?;
    ctx.notes.push(format!("turn and limit shown {}", seconds(sent)));
    memory(ctx, app.id)
}

/// The sample project's proof is git's own whitespace check: the sandbox's PATH has git and no Node.
fn proof(ctx: &mut Ctx) -> Result {
    let fixture = super::projects_ui::fixture(ctx)?;
    let repo = ctx.sb.sandbox.join("repo");
    let git = fixture.git_bin.as_ref().ok_or("the fixture has no git")?.join("git");
    std::fs::write(repo.join("AGENTS.md"), "# Rules\n\n## Proof commands\n\n```bash\ngit diff --check\n```\n")
        .map_err(|e| e.to_string())?;
    let git = git.to_string_lossy();
    super::projects::git(&git, &repo, &["add", "--", "AGENTS.md"])?;
    super::projects::git(&git, &repo, &["commit", "--no-gpg-sign", "-m", "proof"])?;
    let recording = Some(ctx.paths.fixtures.join("claude/real-work-proof.jsonl"));
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording, ..fixture })?;
    let page = &mut app.page;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Demo"))?;
    page.wait_visible(&css(".welcome-grid .p-header h2").has_text("Demo"))?;
    // A chosen model starts at once; Auto would wait for its routing question, which this recording does not hold.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    let sent = Instant::now();
    ask(page, "Add a line to notes.txt.")?;
    // The fake CLI writes nothing, so while the recording holds after its Write step the file lands in the work copy
    // here: Faber's checks run only on a copy whose files changed. The copy is found by its name, as in the app suite.
    page.click(&css(".det-line button"))?;
    let folder = css("[data-testid=details-copy] span");
    page.wait_visible(&folder)?;
    let shown = std::path::PathBuf::from(page.inner_text(&folder)?.trim());
    let copy = ctx.sb.data.join("copies").join(shown.file_name().unwrap_or_default());
    for _ in 0..40 {
        if copy.is_dir() {
            break;
        }
        pause(0.25);
    }
    ensure(copy.is_dir(), || format!("the work copy is not at {}", copy.display()))?;
    std::fs::write(copy.join("notes.txt"), "a line\n").map_err(|e| e.to_string())?;
    std::fs::write(ctx.sb.state.join("release"), "").map_err(|e| e.to_string())?;
    page.wait_for("the answer", "document.querySelector('article.ai')?.innerText.includes('Added a line to notes.txt.') === true")?;
    // A project shows its checks in the details beside the thread; the start of `checks.passed`, whose count is a plural.
    page.click(&css(".det-line button"))?;
    page.wait_for_within(
        "the checks to pass",
        "document.querySelector('.details-view')?.innerText.includes(\"Faber's checks passed\") === true",
        std::time::Duration::from_secs(60),
    )?;
    ctx.notes.push(format!("checks passed {}", seconds(sent)));
    memory(ctx, app.id)
}
