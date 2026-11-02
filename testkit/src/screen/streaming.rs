//! A long reply streaming into a long conversation (the 2026-10-31 rework, W1): 320 earlier messages, then a reply of 24
//! rounds of words and a command each, the words in small pieces as the CLI sends them. The processor the app spends while
//! it streams and the memory it reaches are the measure the thread's rebuild is judged by, before and after.

use super::{Result, Scenario, ask, ensure, start_settings};
use crate::app::Ctx;
use crate::page::css;
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Instant;

pub const NAME: &str = "streaming: a long reply into a 320-message conversation, its processor and memory while it streams";

/// The earlier messages, the rounds of the reply, and the pieces of words in each round.
const EARLIER: u64 = 320;
const ROUNDS: usize = 24;
const PIECES: usize = 30;
/// Milliseconds between two pieces: about how fast a fast model writes.
const PIECE_MS: u64 = 12;
const BUDGET_MB: f64 = 400.0;
const ASK: &str = "Stream the long report.";
const SESSION: &str = "5f1c0000-0000-4000-8000-0000000000aa";

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: streaming }]
}

fn seed(ctx: &Ctx) -> Result {
    start_settings(ctx, "en")?;
    let at: u64 = 1_790_000_000_000;
    let paragraph = "Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ";
    let messages: Vec<Value> = (0..EARLIER)
        .map(|i| {
            let id = format!("5f1d{i:04x}-0000-4000-8000-000000000000");
            if i % 2 == 0 {
                json!({ "id": id, "role": "user", "text": format!("Question {}: why is the sky blue?", i / 2), "at": at + i })
            } else {
                json!({ "id": id, "role": "assistant", "text": paragraph.repeat(4), "at": at + i, "vendor": "claude",
                    "model": "claude-haiku-5-5", "status": "done",
                    "tools": [{ "id": format!("seed-{i}"), "name": "Read", "input": { "file_path": "notes.md" }, "ok": true,
                        "output": "Synthetic bounded output.\n".repeat(20) }] })
            }
        })
        .collect();
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let conversation = json!({ "id": "5f1d0000-0000-4000-8000-0000000000ff", "title": "Long stream", "area": "chat",
        "sessions": { "claude": SESSION }, "createdAt": at, "updatedAt": at + EARLIER, "messages": messages });
    std::fs::write(folder.join("5f1d0000-0000-4000-8000-0000000000ff.json"), conversation.to_string()).map_err(|e| e.to_string())
}

fn delta(text: &str) -> Value {
    json!({ "dir": "out", "msg": { "type": "stream_event", "event": { "type": "content_block_delta", "index": 0,
        "delta": { "type": "text_delta", "text": text } }, "session_id": SESSION, "parent_tool_use_id": null } })
}

/// The reply as the CLI would send it: per round, words in pieces, the whole text block, then a command and its result.
fn recording() -> Vec<Value> {
    let out = |msg: Value| json!({ "dir": "out", "msg": msg });
    let mut lines = vec![
        json!({ "dir": "in", "msg": { "type": "user", "message": { "role": "user", "content": ASK }, "parent_tool_use_id": null, "session_id": "" } }),
        out(json!({ "type": "system", "subtype": "init", "session_id": SESSION, "model": "claude-haiku-5-5" })),
    ];
    for round in 0..ROUNDS {
        lines.push(out(json!({ "type": "stream_event", "event": { "type": "message_start", "message": { "model": "claude-haiku-5-5",
            "id": format!("msg-{round}"), "role": "assistant", "content": [] } }, "session_id": SESSION, "parent_tool_use_id": null })));
        lines.push(out(json!({ "type": "stream_event", "event": { "type": "content_block_start", "index": 0,
            "content_block": { "type": "text", "text": "" } }, "session_id": SESSION, "parent_tool_use_id": null })));
        let mut said = String::new();
        for piece in 0..PIECES {
            let words = if piece + 1 == PIECES { format!("step {round} is clear.\n\n") } else { format!("word{piece} of round {round}, ") };
            said.push_str(&words);
            lines.push(delta(&words));
            lines.push(json!({ "dir": "out", "msg": { "faber_pause": PIECE_MS } }));
        }
        lines.push(out(json!({ "type": "assistant", "message": { "role": "assistant", "id": format!("msg-{round}"),
            "content": [{ "type": "text", "text": said }] }, "session_id": SESSION, "parent_tool_use_id": null })));
        if round + 1 < ROUNDS {
            let tool = format!("bash-{round}");
            lines.push(out(json!({ "type": "assistant", "message": { "role": "assistant", "id": format!("msg-{round}-tool"),
                "content": [{ "type": "tool_use", "id": tool, "name": "Bash", "input": { "command": format!("node step-{round}.js") } }] },
                "session_id": SESSION, "parent_tool_use_id": null })));
            lines.push(json!({ "dir": "out", "msg": { "faber_pause": 40 } }));
            lines.push(out(json!({ "type": "user", "message": { "role": "user", "content": [{ "type": "tool_result", "tool_use_id": tool,
                "content": "Output line of the step.\n".repeat(30) }] }, "session_id": SESSION, "parent_tool_use_id": null })));
        }
    }
    lines.push(out(json!({ "type": "result", "subtype": "success", "is_error": false, "session_id": SESSION, "total_cost_usd": 0.001,
        "usage": { "input_tokens": 10, "output_tokens": 2000 } })));
    lines
}

fn streaming(ctx: &mut Ctx) -> Result {
    seed(ctx)?;
    let path = ctx.sb.sandbox.join("streaming.jsonl");
    let text = recording().iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(Launch { claude: CliSlot::Fake, recording: Some(path), ..Launch::default() })?;
    let page = &mut app.page;
    super::shell::set_size(page, 1280, 800)?;
    page.click(&css(".list .item").has_text("Long stream"))?;
    page.wait_visible(&css("article.ai").last())?;
    let (loaded, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the memory reading must include WebView2".into())?;
    // Main-thread work while it streams: the sum of long tasks (50 ms and over) the page reports.
    app.page.eval(
        "(() => { window.__long = { n: 0, ms: 0 }; new PerformanceObserver((list) => { for (const e of list.getEntries()) { window.__long.n++; window.__long.ms += e.duration; } }).observe({ type: 'longtask' }); return true; })()",
    )?;
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = app.page.labelled("composer.model")?;
    app.page.select_option(&model, "Claude Haiku")?;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    ask(&mut app.page, ASK)?;
    let mut peak = loaded;
    let last = format!("step {} is clear.", ROUNDS - 1);
    loop {
        peak = peak.max(ctx.memory_mb(app.id)?.0);
        let done = app.page.eval(&format!(
            "(() => {{ const a = [...document.querySelectorAll('article.ai')].at(-1); return !!a && a.querySelector('.actions') !== null && a.textContent.includes({last:?}); }})()"
        ))?;
        if done == true {
            break;
        }
        ensure(started.elapsed().as_secs() < 120, || "the long reply did not finish streaming in two minutes".into())?;
        super::pause(0.25);
    }
    let stream_ms = started.elapsed().as_secs_f64() * 1000.0;
    let cpu_ms = ctx.cpu_ms(app.id)? - cpu_before;
    peak = peak.max(ctx.memory_mb(app.id)?.0);
    let long = app.page.eval("window.__long")?;
    let nodes = app.page.eval("[...document.querySelectorAll('article.ai')].at(-1).querySelectorAll('*').length")?;
    let numbers = format!(
        "Streaming {ROUNDS} rounds x {PIECES} pieces + {} commands into {EARLIER} messages: loaded {loaded:.1} MB, sampled peak {peak:.1} MB; {cpu_ms:.0} ms CPU over {stream_ms:.0} ms ({:.1}% of one core); main-thread long tasks {} totalling {:.0} ms; the reply holds {} elements",
        ROUNDS - 1,
        cpu_ms / stream_ms * 100.0,
        long["n"],
        long["ms"].as_f64().unwrap_or(0.0),
        nodes
    );
    ctx.notes.push(numbers.clone());
    ensure(peak <= BUDGET_MB, || format!("{numbers}; over the {BUDGET_MB} MB budget"))?;
    ctx.close(app.id)?;
    Ok(())
}
