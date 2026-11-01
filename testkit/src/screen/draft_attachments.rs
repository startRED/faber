//! A real hidden WebView restart, not a component remount, proves that committed draft bytes survive.

use super::{Result, Scenario, claude_chat, ensure, eventually, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css};
use base64::Engine as _;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const WORDS: &str = "Synthetic restart check: keep these unsent words and attachments.";
const IMAGE: &str = "synthetic-draft-picture.png";
const PDF: &str = "synthetic-attachment-with-a-long-readable-name.pdf";
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
const PDF_BYTES: &str = "%PDF-1.4\nSynthetic draft fixture; no private content.\n%%EOF\n";
const KEY: &str = "new-chat-";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "draft attachments: committed bytes survive a real restart, removals stay gone, within the memory budget",
            run: restart,
        },
        Scenario { name: "draft attachments: edited inline file metadata is removed when the draft reopens", run: edited_inline },
        Scenario {
            name: "draft attachments: refused disk save preserves words and bytes, retry sends once within the memory budget",
            run: refused_send,
        },
    ]
}

// Read only after the product creates its database: opening a missing version-one database here would alter the test.
// Resolve after the read transaction completes, then inspect actual Blobs; a localStorage marker alone is not a receipt.
const READ: &str = r#"(async () => {
  if (!(await indexedDB.databases()).some(db => db.name === 'faber.unsent')) return {missing: true};
  const db = await new Promise((resolve, reject) => {
    const request = indexedDB.open('faber.unsent');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  if (!db.objectStoreNames.contains('attachments')) { db.close(); return {missing: true}; }
  let record;
  try {
    record = await new Promise((resolve, reject) => {
      const tx = db.transaction('attachments', 'readonly');
      const request = tx.objectStore('attachments').get('new-chat-');
      tx.oncomplete = () => resolve(request.result);
      tx.onabort = () => reject(tx.error || new Error('draft read aborted'));
      tx.onerror = () => reject(tx.error || new Error('draft read failed'));
    });
  } finally { db.close(); }
  if (!record) return {empty: true};
  const bytes = async blob => btoa(String.fromCharCode(...new Uint8Array(await blob.arrayBuffer())));
  return {
    images: await Promise.all(record.images.map(async image => ({name: image.name, mediaType: image.mediaType, base64: await bytes(image.bytes)}))),
    files: await Promise.all(record.files.map(async file => ({name: file.name, base64: await bytes(file.bytes)}))),
    inlined: record.inlined
  };
})()"#;

fn committed(page: &mut Page, pdf: bool) -> Result {
    let expected = json!({
        "images": [{"name": IMAGE, "mediaType": "image/png", "base64": PNG}],
        "files": if pdf { vec![json!({"name": PDF, "base64": base64::engine::general_purpose::STANDARD.encode(PDF_BYTES)})] } else { vec![] },
        "inlined": []
    });
    page.wait_for("the actual draft Blob transaction to commit its names and bytes", &format!(
        "(async () => {{ const actual = await ({READ}); const expected = {expected}; return Object.entries(expected).every(([key, items]) => Array.isArray(actual[key]) && actual[key].length === items.length && items.every((item, index) => Object.entries(item).every(([field, value]) => actual[key][index][field] === value))); }})()"
    ))
}

fn restored(page: &mut Page, pdf: bool) -> Result {
    page.wait_for(
        "the draft text after the app restarted",
        &format!("document.querySelector('.composer textarea')?.value === {}", json!(WORDS)),
    )?;
    page.wait_count(&css(".composer .attach-row .thumb"), if pdf { 2 } else { 1 })?;
    same(page.eval("document.querySelector('.composer .attach-row img')?.alt")?, json!(IMAGE), "the recovered image's name")?;
    same(
        page.eval("document.querySelector('.composer .attach-row img')?.getAttribute('src')")?,
        json!(format!("data:image/png;base64,{PNG}")),
        "the recovered image's exact bytes",
    )?;
    let names = page.eval("[...document.querySelectorAll('.composer .attach-row .thumb b')].map(node => node.textContent)")?;
    same(names, if pdf { json!([PDF]) } else { json!([]) }, "the recovered file names")?;
    committed(page, pdf)
}

fn sent(ctx: &Ctx) -> Option<Value> {
    std::fs::read_dir(ctx.sb.data.join("conversations")).ok()?.flatten().find_map(|entry| {
        if entry.path().extension()?.to_str()? != "json" {
            return None;
        }
        let value: Value = serde_json::from_slice(&std::fs::read(entry.path()).ok()?).ok()?;
        value["messages"].as_array()?.iter().any(|message| message["role"] == "user" && message["text"] == WORDS).then_some(value)
    })
}

fn refused_send(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let id = "5f1c0000-0000-4000-8000-0000000000ad";
    let cwd = ctx.sb.data.with_file_name("data-chat-work").join(id);
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis() as u64;
    let conversation = json!({
        "id": id, "title": "Blocked send fixture", "area": "chat", "sessions": {"claude": "synthetic-admission-session"},
        "createdAt": at, "updatedAt": at, "queued": [],
        "messages": [
            {"id": "previous-question", "role": "user", "text": "Previous synthetic question", "at": at},
            {"id": "previous-answer", "role": "assistant", "text": "Previous synthetic answer", "at": at,
             "vendor": "claude", "model": "claude-haiku-5-5", "status": "done"}
        ]
    });
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let file = folder.join(format!("{id}.json"));
    std::fs::write(&file, conversation.to_string()).map_err(|e| e.to_string())?;
    let mut launch = claude_chat(ctx);
    let source = std::fs::read_to_string(launch.recording.as_ref().ok_or("missing chat recording")?).map_err(|e| e.to_string())?;
    let mut lines =
        source.lines().map(serde_json::from_str::<Value>).collect::<std::result::Result<Vec<_>, _>>().map_err(|e| e.to_string())?;
    // The refused attempt copies the PDF before admission; its retry keeps that orphan and chooses the next safe name.
    let retry_pdf = PDF.replace(".pdf", "-1.pdf");
    let prompt = format!("{WORDS}\n\nAttached files (in your working folder):\n- attachments/{retry_pdf}");
    lines[0]["msg"]["message"]["content"] = json!([
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG}},
        {"type": "text", "text": prompt}
    ]);
    let recording = ctx.sb.data.join("admission-chat.jsonl");
    std::fs::write(&recording, lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).map_err(|e| e.to_string())?;
    let setup = ctx.sb.data.join("admission.setup.json");
    std::fs::write(&setup, json!({"vendor": "claude", "version": "2.1.296", "chat": "admission-chat.jsonl"}).to_string())
        .map_err(|e| e.to_string())?;
    std::fs::write(ctx.sb.state.join("claude.signed-in"), "synthetic fixture").map_err(|e| e.to_string())?;
    launch.recording = Some(setup);
    let mut app = ctx.launch(launch)?;
    app.page.click(&css(".list .item").has_text("Blocked send fixture"))?;
    app.page.wait_count(&css("article.ai"), 1)?;
    super::select_model(&mut app.page, "Claude Haiku")?;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    app.page.fill(&css(".composer textarea"), WORDS)?;
    app.page.eval(&format!(r#"(() => {{
        const transfer = new DataTransfer();
        transfer.items.add(new File([Uint8Array.from(atob({png}), c => c.charCodeAt(0))], {image}, {{type: 'image/png'}}));
        transfer.items.add(new File([{bytes}], {pdf}, {{type: 'application/pdf'}}));
        document.querySelector('.composer').dispatchEvent(new DragEvent('drop', {{dataTransfer: transfer, bubbles: true, cancelable: true}}));
        return true;
    }})()"#, png=json!(PNG), image=json!(IMAGE), bytes=json!(PDF_BYTES), pdf=json!(PDF)))?;
    app.page.wait_count(&css(".composer .attach-row .thumb"), 2)?;
    let read = READ.replace("'new-chat-'", &json!(id).to_string());
    app.page.wait_for(
        "the fixture PNG and PDF bytes to commit",
        &format!(
            "(async () => {{ const d = await ({read}); return d.images?.[0]?.base64 === {} && d.files?.[0]?.base64 === {}; }})()",
            json!(PNG),
            json!(base64::engine::general_purpose::STANDARD.encode(PDF_BYTES))
        ),
    )?;
    let draft = app.page.eval(&read)?;
    eventually("the existing draft warmup", Duration::from_secs(10), || {
        ctx.records().is_ok_and(|r| r.iter().any(|r| r.args.iter().any(|a| a == "--resume")))
    })?;
    let warm_commands = ctx.records()?;
    let before = std::fs::read(&file).map_err(|e| e.to_string())?;
    let (before_mb, webview) = ctx.memory_mb(app.id)?;
    ensure(webview, || "the admission cost must include the WebView tree".into())?;
    let blocked = folder.join(format!("{id}.json.tmp"));
    std::fs::create_dir(&blocked).map_err(|e| e.to_string())?;
    let send = app.page.named("button", "composer.send")?.exact();
    app.page.wait_enabled(&send)?;
    app.page.click(&send)?;
    app.page.wait_visible(&css(".toast[role=alert]"))?;
    app.page.wait_for("the refused send to restore its words and both attachments", &format!(
        "document.querySelector('.composer textarea')?.value === {} && document.querySelectorAll('.composer .attach-row .thumb').length === 2", json!(WORDS)))?;
    app.page.wait_for(
        "the restored attachment bytes to commit",
        &format!("(async () => {{ const actual = await ({read}); const expected = {draft}; return Object.entries(expected).every(([key, items]) => Array.isArray(actual[key]) && actual[key].length === items.length && items.every((item, index) => Object.entries(item).every(([field, value]) => actual[key][index][field] === value))); }})()"),
    )?;
    same(app.page.eval(&read)?, draft, "the refused send retains the exact committed draft names and bytes")?;
    same(std::fs::read(&file).map_err(|e| e.to_string())?, before, "the refused turn changes no saved history")?;
    same(app.page.count(&css("article.ai"))?, 1, "the refusal adds no answer")?;
    same(app.page.count(&css(".mine"))?, 1, "the refusal adds no user turn")?;
    same(app.page.count(&css(".toast[role=alert]"))?, 1, "the failed event and rejected command share one warning")?;
    ctx.notes.push(format!(
        "recorded CLI launches before refusal {}, after restoration {}; warmup is not human input evidence",
        warm_commands.len(),
        ctx.records()?.len()
    ));
    let refused_mb = ctx.memory_mb(app.id)?.0;
    ctx.shot(&mut app, "send-admission-refused-draft")?;
    // This fixture owns this exact empty directory; remove no cache, copied attachment or other sandbox path.
    ensure(std::fs::read_dir(&blocked).map_err(|e| e.to_string())?.next().is_none(), || "the admission blocker must remain empty".into())?;
    std::fs::remove_dir(&blocked).map_err(|e| e.to_string())?;
    app.page.click(&css(".toast button"))?;
    app.page.wait_enabled(&send)?;
    app.page.click(&send)?;
    app.page.wait_for("the recorded answer after retry", "document.querySelectorAll('article.ai .actions').length === 2")?;
    // Settled shows the actions before the CLI's final result has been saved.
    eventually("the retry's saved completion", Duration::from_secs(20), || {
        std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|saved| saved["messages"][3]["status"] == "done")
    })?;
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let messages = saved["messages"].as_array().ok_or("missing saved messages")?;
    same(messages.len(), 4, "retry saves exactly one new user turn and reply")?;
    same(messages[2]["text"].clone(), json!(WORDS), "retry sends the original words")?;
    same(messages[2]["images"].clone(), json!([{"mediaType": "image/png", "base64": PNG}]), "retry sends the exact PNG bytes")?;
    same(messages[2]["attachments"].clone(), json!([retry_pdf]), "retry names the retained PDF copy once")?;
    same(
        std::fs::read(cwd.join("attachments").join(&retry_pdf)).map_err(|e| e.to_string())?,
        PDF_BYTES.as_bytes().to_vec(),
        "retry sends the exact PDF bytes",
    )?;
    same(messages[3]["status"].clone(), json!("done"), "the exact recording completed once")?;
    app.page.wait_for(
        "the accepted draft bytes and text to clear",
        &format!("(async () => (await ({read})).empty === true && document.querySelector('.composer textarea')?.value === '')()"),
    )?;
    let accepted_mb = ctx.memory_mb(app.id)?.0;
    let cpu = ctx.cpu_ms(app.id)? - cpu_before;
    ctx.notes.push(format!("admission PNG/PDF full warmup/refusal/retry flow: {cpu:.0} ms CPU over {:.0} ms; app tree before {before_mb:.1}, refused {refused_mb:.1}, accepted {accepted_mb:.1} MB (not idle overhead)", started.elapsed().as_secs_f64()*1000.0));
    ensure(before_mb.max(refused_mb).max(accepted_mb) <= 400.0, || "the measured admission flow exceeded 400 MB".into())?;
    ctx.shot(&mut app, "send-admission-retry-once")?;
    ctx.close(app.id)?;
    let output = ctx.output(app.id);
    ensure(!output.contains("sent user") && !output.contains("recorded user"), || format!("the exact one-input replay differed: {output}"))
}

fn restart(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "de")?;
    // Replay checks the entire request, including image bytes; adapt the existing chat recording's first input.
    let mut launch = claude_chat(ctx);
    let source = std::fs::read_to_string(launch.recording.as_ref().ok_or("missing chat recording")?).map_err(|e| e.to_string())?;
    let mut lines = source
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).map_err(|e| e.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    lines[0]["msg"]["message"]["content"] = json!([
        {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": PNG}},
        {"type": "text", "text": WORDS}
    ]);
    let recording = ctx.sb.data.join("draft-chat.jsonl");
    std::fs::write(&recording, lines.iter().map(Value::to_string).collect::<Vec<_>>().join("\n")).map_err(|e| e.to_string())?;
    launch.recording = Some(recording);
    let mut app = ctx.launch(launch.clone())?;
    app.page.wait_visible(&css(".composer textarea"))?;
    super::shell::set_size(&mut app.page, 720, 800)?;
    let (before, has_webview) = ctx.memory_mb(app.id)?;
    ensure(has_webview, || "draft cost must include the WebView process".into())?;
    let cpu_before = ctx.cpu_ms(app.id)?;
    let started = Instant::now();
    app.page.eval("window.__draftStorageOpen = IDBFactory.prototype.open; IDBFactory.prototype.open = () => { throw new Error('synthetic draft storage failure'); }; true")?;
    app.page.fill(&css(".composer textarea"), WORDS)?;
    app.page.eval(&format!(
        r#"(() => {{
      const transfer = new DataTransfer();
      transfer.items.add(new File([Uint8Array.from(atob({png}), c => c.charCodeAt(0))], {image}, {{type: 'image/png'}}));
      transfer.items.add(new File([{bytes}], {pdf}, {{type: 'application/pdf'}}));
      document.querySelector('.composer').dispatchEvent(new DragEvent('drop', {{dataTransfer: transfer, bubbles: true, cancelable: true}}));
      return true;
    }})()"#,
        png = json!(PNG),
        image = json!(IMAGE),
        bytes = json!(PDF_BYTES),
        pdf = json!(PDF)
    ))?;
    app.page.wait_count(&css(".composer .attach-row .thumb"), 2)?;
    app.page.wait_visible(&css(".composer [role=status] button"))?;
    same(
        app.page.eval("document.querySelector('.composer textarea').value")?,
        json!(WORDS),
        "a failed attachment save preserves the words",
    )?;
    same(app.page.eval("document.querySelector('.composer textarea').getAttribute('aria-describedby') === document.querySelector('.composer [role=status]').id")?, json!(true), "the failure is associated with its field")?;
    ensure(
        app.page.eval("[...document.querySelectorAll('.composer [role=status], .composer [role=status] button')].every(el => { const r = el.getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.width > 0 && el.scrollWidth <= el.clientWidth + 1; })")? == json!(true),
        || "the long-language warning and retry must fit at 720 px".into(),
    )?;
    ctx.shot(&mut app, "draft-attachments-save-failed-720-de")?;
    app.page.eval("IDBFactory.prototype.open = window.__draftStorageOpen; delete window.__draftStorageOpen; true")?;
    app.page.click(&css(".composer [role=status] button"))?;
    committed(&mut app.page, true)?;
    app.page.wait_count(&css(".composer [role=status]"), 0)?;
    let saved = ctx.memory_mb(app.id)?.0;
    let cpu = ctx.cpu_ms(app.id)? - cpu_before;
    ctx.notes.push(format!(
        "draft PNG/PDF synthetic fixtures: before {before:.1} MB, committed {saved:.1} MB; full failed-save/retry flow including existing warmup and photo {cpu:.0} ms CPU over {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0,
    ));
    ensure(saved <= 400.0, || format!("committed draft uses {saved:.1} MB, above 400 MB"))?;
    let text_key = app.page.eval(&format!("localStorage.getItem({})", json!(format!("faber.unsent.{KEY}"))))?;
    same(text_key, json!(WORDS), "the draft words are durable alongside the attachment bytes")?;
    ctx.shot(&mut app, "draft-attachments-before-restart-720-de")?;

    // close ends the actual app process; finish would delete the sandbox and its WebView profile.
    let first_pid = app.pid;
    ctx.close(app.id)?;
    ensure(ctx.exited(app.id).is_some(), || "the first app process must exit before reopening its profile".into())?;
    drop(app);
    let mut app = ctx.launch(launch.clone())?;
    ctx.notes
        .push(format!("draft restart: ended app {first_pid}, reopened app {} with the same sandbox data and WebView profile", app.pid));
    app.page.wait_visible(&css(".composer textarea"))?;
    super::shell::set_size(&mut app.page, 720, 800)?;
    restored(&mut app.page, true)?;
    let restored_mb = ctx.memory_mb(app.id)?.0;
    ctx.notes.push(format!("draft PNG/PDF synthetic fixtures after real app restart: {restored_mb:.1} MB"));
    ensure(restored_mb <= 400.0, || format!("restored draft uses {restored_mb:.1} MB, above 400 MB"))?;
    ctx.shot(&mut app, "draft-attachments-after-restart-720-de")?;

    app.page.click(&css(".composer .thumb").has_text(PDF).css(".thumb-x"))?;
    app.page.wait_count(&css(".composer .attach-row .thumb"), 1)?;
    committed(&mut app.page, false)?;
    // The recording contains a chat answer, not Auto's separate routing question.
    super::select_model(&mut app.page, "Claude Haiku")?;
    let send = app.page.named("button", "composer.send")?;
    app.page.wait_enabled(&send)?;
    app.page.click(&send)?;
    eventually("the engine to save the restored message", Duration::from_secs(10), || sent(ctx).is_some())?;
    let conversation = sent(ctx).ok_or("the sent conversation disappeared")?;
    let messages = conversation["messages"].as_array().ok_or("the conversation has no messages")?;
    let mine: Vec<_> = messages.iter().filter(|message| message["role"] == "user").collect();
    same(mine.len(), 1, "exactly one user message was saved")?;
    same(mine[0]["images"].clone(), json!([{"mediaType": "image/png", "base64": PNG}]), "the restored image reaches the engine unchanged")?;
    ensure(mine[0]["attachments"].is_null() || mine[0]["attachments"] == json!([]), || "the removed PDF must not be sent".into())?;
    app.page.wait_for("the sent draft Blob transaction to clear", &format!("(async () => (await ({READ})).empty === true)()"))?;
    app.page.wait_for("the sent draft text to clear", &format!("!localStorage.getItem({})", json!(format!("faber.unsent.{KEY}"))))?;
    // The answer must end before the abrupt app close so the harness audits no interrupted fake CLI.
    app.page.wait_for("the recorded answer to finish", "!!document.querySelector('article.ai .actions')")?;
    ctx.close(app.id)?;
    drop(app);
    let mut app = ctx.launch(launch)?;
    app.page.wait_for("the clean composer after sending and restarting", "document.querySelector('.composer textarea')?.value === ''")?;
    app.page.wait_count(&css(".composer .attach-row .thumb"), 0)?;
    app.page.wait_enabled(&css(".composer textarea"))?;
    same(
        app.page.eval(&format!("(async () => (await ({READ})).empty === true)()"))?,
        json!(true),
        "neither the removed PDF nor sent image remains in the durable draft",
    )?;
    same(
        sent(ctx).and_then(|value| {
            value["messages"].as_array().map(|messages| messages.iter().filter(|message| message["role"] == "user").count())
        }),
        Some(1),
        "restarting never resends the message",
    )
}

fn edited_inline(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    app.page.wait_visible(&css(".composer textarea"))?;
    app.page.eval(
        r#"(() => {
      const transfer = new DataTransfer();
      transfer.items.add(new File(['Synthetic private words that the person will remove.'], 'edited.txt', {type: 'text/plain'}));
      document.querySelector('.composer').dispatchEvent(new DragEvent('drop', {dataTransfer: transfer, bubbles: true, cancelable: true}));
      return true;
    })()"#,
    )?;
    app.page.wait_count(&css(".composer .attach-row .thumb"), 1)?;
    app.page
        .wait_for("the inline-file removal metadata to be saved", &format!("(async () => (await ({READ})).inlined?.length === 1)()"))?;
    app.page.fill(&css(".composer textarea"), "The person's replacement words")?;
    app.page.wait_for(
        "the replacement words to be saved",
        "localStorage.getItem('faber.unsent.new-chat-') === \"The person's replacement words\"",
    )?;
    // The app starts its CLIs (sign-in probes, a warm one) in its first seconds. Each is created paused and only then joins
    // the job that ends with the app, so a hard close in between leaves it outside, paused (task 023). The close waits until
    // the app has started no CLI for 2 s. shortcut: a quiet period narrows that window but cannot prove a CLI joined its job;
    // task 023 closes it in the app, and this wait can go then.
    let mut last = (0u64, Instant::now());
    eventually("the app's CLI starts to settle", Duration::from_secs(20), || {
        let Ok(rows) = crate::proctree::snapshot() else {
            last.1 = Instant::now();
            return false;
        };
        let newest = rows.iter().filter(|r| r.parent == app.pid && r.name.starts_with("fake-cli")).map(|r| r.born).max().unwrap_or(0);
        if newest != last.0 {
            last = (newest, Instant::now());
        }
        last.1.elapsed() >= Duration::from_secs(2)
    })?;
    ctx.close(app.id)?;
    drop(app);
    let mut app = ctx.launch(claude_chat(ctx))?;
    app.page.wait_for("the edited text to return without an obsolete file chip", "document.querySelector('.composer textarea')?.value === \"The person's replacement words\" && document.querySelectorAll('.composer .attach-row .thumb').length === 0")?;
    app.page.wait_for(
        "obsolete private inline-file metadata to leave durable storage",
        &format!("(async () => (await ({READ})).empty === true)()"),
    )
}
