//! A long session with big conversations (R461): the installed Faber reached 1.3 GB after three hours of real use, 707 MB
//! of it in the engine, because every conversation opened stayed in memory with all its tool inputs and outputs. Twelve
//! conversations of about 5.6 MB each are opened one after the other; back on a small one, the app must come back close to
//! where it started, and never pass the R125 limit on the way.

use super::{Result, Scenario, ensure, no_cli};
use crate::app::Ctx;
use crate::page::css;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub const NAME: &str = "long session: twelve big conversations opened in turn leave the memory where it started";

/// Conversations opened in turn, and the tool calls of each reply: about 5.6 MB of JSON per conversation, the size real
/// agent conversations reach (the owner's largest is 83 MB, 60 % of it tool output).
const BIG: usize = 12;
const TOOLS: usize = 35;
/// What the app may keep after the big ones close: the screen's caches and the allocator's slack, not the conversations.
const GROWTH_MB: f64 = 60.0;

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: long_session }]
}

/// A finished Chat conversation of 30 turns, each reply with `tools` calls whose input and output are as large as the
/// engine keeps them (a tool result is cut at 4,000 characters).
fn big(i: usize, tools: usize) -> Value {
    let output = "Output line of a long command run by the agent.\n".repeat(80);
    let input = "echo a long command the agent wrote; ".repeat(40);
    let messages: Vec<Value> = (0..60)
        .map(|m| {
            let id = format!("long-{i}-{m}");
            if m % 2 == 0 {
                json!({ "id": id, "role": "user", "at": m, "text": format!("Step {m} of the long task.") })
            } else {
                let calls: Vec<Value> = (0..tools)
                    .map(|t| json!({ "id": format!("{id}-t{t}"), "name": "Bash", "input": { "command": input }, "ok": true, "output": output }))
                    .collect();
                json!({ "id": id, "role": "assistant", "at": m, "status": "done", "text": format!("Done with step {m} of task {i}."), "tools": calls })
            }
        })
        .collect();
    json!({ "id": format!("long-{i:02}"), "title": format!("Long task {i:02}"), "area": "chat", "createdAt": 1,
        "updatedAt": 10 + i, "sessions": {}, "queued": [], "messages": messages })
}

/// Each process of the app with its private memory, the engine's first: where the memory sits when the check fails.
fn parts(app: &crate::app::Opened) -> Result<String> {
    let mut rows = crate::proctree::private_processes(app.pid)?;
    rows.sort_by_key(|(row, _)| row.pid != app.pid);
    Ok(rows.iter().map(|(row, mb)| format!("{} {mb:.1}", row.name)).collect::<Vec<_>>().join(", "))
}

fn long_session(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language":"en", "setup":"done", "tutorialDone":true,
        "appearance": {"base":"sala", "motion":"none", "effects":{"ambient":false,"texture":false,"glow":false}} }))?;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let mut small = big(99, 0);
    small["id"] = json!("small");
    small["title"] = json!("A small conversation");
    small["updatedAt"] = json!(1);
    std::fs::write(folder.join("small.json"), small.to_string()).map_err(|e| e.to_string())?;
    for i in 0..BIG {
        std::fs::write(folder.join(format!("long-{i:02}.json")), big(i, TOOLS).to_string()).map_err(|e| e.to_string())?;
    }

    let mut app = ctx.launch(no_cli())?;
    super::shell::set_size(&mut app.page, 1280, 800)?;
    // The last reply names its conversation, so each wait ends only once that conversation is drawn.
    let open = |app: &mut crate::app::Opened, title: &str, i: usize| -> Result {
        app.page.click(&css(".crow .item").has_text(title))?;
        app.page.wait_visible(&css("[data-message-index=\"59\"]").has_text(&format!("Done with step 59 of task {i}.")))
    };
    open(&mut app, "A small conversation", 99)?;
    let (baseline, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the memory reading must include WebView2".into())?;
    let heap = |app: &mut crate::app::Opened| -> Result<f64> {
        let used = app.page.cdp.call("Runtime.getHeapUsage", json!({}))?;
        Ok(used["usedSize"].as_f64().unwrap_or(0.0) / 1_048_576.0)
    };
    let heap_start = heap(&mut app)?;
    let at_start = parts(&app)?;
    let mut peak = baseline;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    for i in 0..BIG {
        open(&mut app, &format!("Long task {i:02}"), i)?;
        let (mb, _) = ctx.memory_mb(app.id)?;
        peak = peak.max(mb);
        ensure(peak <= 400.0, || format!("opening big conversation {i} reached {peak:.1} MB, above the 400 MB limit (R125)"))?;
    }
    let cpu_ms = ctx.cpu_ms(app.id)? - cpu_before;
    let walk_ms = started.elapsed().as_secs_f64() * 1000.0;
    open(&mut app, "A small conversation", 99)?;
    // The screen frees the big thread when it collects; give it a few seconds before the last reading counts.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut after = f64::MAX;
    while Instant::now() < deadline {
        app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
        after = after.min(ctx.memory_mb(app.id)?.0);
        if after - baseline <= GROWTH_MB {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let at_end = parts(&app)?;
    let heap_end = heap(&mut app)?;
    ctx.notes.push(format!("long session: processes at the start [{at_start}], at the end [{at_end}]"));
    ctx.notes.push(format!(
        "long session: baseline {baseline:.1} MB, peak {peak:.1} MB, after {after:.1} MB, {BIG} big conversations in {walk_ms:.0} ms, {cpu_ms:.0} ms processor"
    ));
    ensure(after - baseline <= GROWTH_MB, || {
        format!(
            "back on a small conversation the app kept {:.1} MB more than at the start (limit {GROWTH_MB} MB): the big ones stayed in memory (at the start: {at_start}, JS heap {heap_start:.1}; at the end: {at_end}, JS heap {heap_end:.1})",
            after - baseline
        )
    })
}
