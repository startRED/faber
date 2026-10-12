//! Measured regressions from the UX review, on the same isolated desktop as the other screen scenarios.
use super::{Result, Scenario, claude_chat, ensure, pause, start_settings, system_motion};
use crate::app::{Ctx, Opened};
use crate::page::css;
use serde_json::json;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "review 2: rapid column toggles on a large thread stay below 250 MB without stage clones", run: peak_memory },
        Scenario { name: "review 12: natural column motion has intermediate frames without stretching text", run: natural_motion },
        Scenario { name: "review 5: retained layout motion stays within the frame budget on a large thread", run: layout_motion },
        Scenario { name: "review 2: paint containment keeps keyboard focus rings visible", run: focus_ring },
    ]
}

fn large_thread(ctx: &mut Ctx) -> Result<Opened> {
    start_settings(ctx, "en")?;
    let id = "5f1c0000-0000-4000-8000-000000000002";
    let block = "A **complete answer** with explanatory prose, a list and code.\n\n- First result\n- Second result\n\n```js\nconst result = 42;\nconsole.log(result);\n```\n\n";
    let messages: Vec<_> = (0..40)
        .map(|i| {
            json!({ "id": format!("review-{i}"), "role": if i % 2 == 0 { "user" } else { "assistant" },
        "text": if i % 2 == 0 { "A question.".into() } else { block.repeat(60) }, "at": 0, "status": "done" })
        })
        .collect();
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(
        folder.join(format!("{id}.json")),
        json!({ "id": id, "title": "Review stress", "area": "chat", "sessions": {},
        "createdAt": 0, "updatedAt": 0, "messages": messages })
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    system_motion(&mut app.page, false)?;
    app.page.wait_for("motion preference", "document.documentElement.dataset.motion === 'full'")?;
    app.page.click(&css(".list .item").has_text("Review stress"))?;
    app.page.wait_visible(&css("article.ai").last())?;
    pause(0.4);
    app.page.wait_for(
        "the opened conversation stays at the bottom",
        "(() => { const el = document.querySelector('.thread'); return el.scrollHeight - el.scrollTop - el.clientHeight < 1; })()",
    )?;
    pause(0.2);
    Ok(app)
}

fn peak_memory(ctx: &mut Ctx) -> Result {
    let mut app = large_thread(ctx)?;
    let (before, _) = ctx.memory_mb(app.id)?;
    app.page.eval(
        r#"(() => {
      const root = document.querySelector('.main');
      window.__reviewNodes = { live: root.querySelectorAll('*').length, peak: 0, done: false };
      let turns = 0;
      const timer = setInterval(() => {
        document.querySelector('.run-btn').click();
        if (++turns === 60) { clearInterval(timer); setTimeout(() => window.__reviewNodes.done = true, 300); }
      }, 50);
      const sample = () => {
        window.__reviewNodes.peak = Math.max(window.__reviewNodes.peak,
          document.querySelector('.motion-exit')?.shadowRoot?.querySelectorAll('*').length ?? 0);
        if (!window.__reviewNodes.done) requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    })()"#,
    )?;
    let mut peak = before;
    while !app.page.eval("window.__reviewNodes.done")?.as_bool().unwrap_or(false) {
        let (mb, _) = ctx.memory_mb(app.id)?;
        peak = peak.max(mb);
        pause(0.025);
    }
    let nodes = app.page.eval("window.__reviewNodes")?;
    pause(0.4);
    let (after, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!(
        "large thread: {before:.1} MB before, {peak:.1} MB peak, {after:.1} MB after; live/exit nodes {}/{}",
        nodes["live"], nodes["peak"]
    ));
    ensure(peak <= 250.0, || format!("{before:.1} MB before, {peak:.1} MB peak, {after:.1} MB after exceeds 250 MB; nodes {nodes}"))?;
    ensure(nodes["peak"].as_u64().unwrap_or(u64::MAX) < 500, || format!("a whole stage was cloned: {nodes}"))?;
    ensure(app.page.eval("!document.querySelector('.motion-exit')")?.as_bool() == Some(true), || "exit surface survived settling".into())
}

fn focus_ring(ctx: &mut Ctx) -> Result {
    let mut app = large_thread(ctx)?;
    app.page.press("Tab")?;
    let measured = app.page.eval(r#"(() => {
      const article = document.querySelector('.col > article.ai:last-child');
      const paragraph = article.querySelector('.md p');
      const link = document.createElement('a'); link.href = 'https://example.test'; link.textContent = 'A reference'; paragraph.prepend(link);
      return [article.querySelector('.actions button'), link].map(el => {
        el.focus();
        const style = getComputedStyle(el); const outset = Math.max(0, parseFloat(style.outlineWidth) + parseFloat(style.outlineOffset));
        const paint = el.closest('.md p') ?? el.closest('.ai'); const edge = paint.getBoundingClientRect(); const rect = el.getBoundingClientRect();
        return { keyboard: el.matches(':focus-visible'), visible: rect.left - outset >= edge.left - 1 && rect.right + outset <= edge.right + 1 && rect.bottom + outset <= edge.bottom + 1 };
      });
    })()"#)?;
    ensure(measured.as_array().is_some_and(|rows| rows.iter().all(|row| row["keyboard"].as_bool() == Some(true))), || {
        format!("probe did not use visible keyboard focus: {measured}")
    })?;
    ensure(measured.as_array().is_some_and(|rows| rows.iter().all(|row| row["visible"].as_bool() == Some(true))), || {
        format!("paint containment clips keyboard focus: {measured}")
    })
}

fn natural_motion(ctx: &mut Ctx) -> Result {
    let mut app = large_thread(ctx)?;
    app.page.eval(
        "document.querySelector('.run-btn').getAttribute('aria-expanded') === 'true' && document.querySelector('.run-btn').click()",
    )?;
    pause(0.4);
    for _ in 0..2 {
        let samples = app.page.eval(r#"new Promise((resolve, reject) => {
          const thread = document.querySelector('.thread');
          const samples = [];
          const sample = now => {
            const view = thread.getBoundingClientRect();
            let paragraph;
            for (let y = view.top + 10; y < view.bottom - 10 && !paragraph; y += 16) {
              const candidate = document.elementFromPoint(view.left + view.width * .4, y)?.closest('.ai > .md p');
              if (candidate && candidate.firstChild?.nodeType === Node.TEXT_NODE && candidate.firstChild.textContent.startsWith('A ')) paragraph = candidate;
            }
            if (!paragraph) throw new Error('the probe must find an actually painted answer glyph');
            const range = document.createRange(); range.setStart(paragraph.firstChild, 0); range.setEnd(paragraph.firstChild, 1);
            samples.push([now, view.width, range.getBoundingClientRect().width]);
          };
          sample(performance.now()); document.querySelector('.run-btn').click();
          const started = performance.now();
          const frame = now => { try { sample(now); if (now - started < 350) requestAnimationFrame(frame); else resolve(samples); } catch (error) { reject(error); } };
          requestAnimationFrame(frame);
        })"#)?;
        let frames = samples.as_array().ok_or("missing frame samples")?;
        let first = frames[0][1].as_f64().unwrap();
        let last = frames.last().unwrap()[1].as_f64().unwrap();
        let glyph = frames[0][2].as_f64().unwrap();
        let travel = (first - last).abs();
        let intermediate: Vec<_> = frames
            .iter()
            .filter(|f| (f[1].as_f64().unwrap() - first).abs() > 0.01 && (f[1].as_f64().unwrap() - last).abs() > 0.01)
            .collect();
        ctx.notes.push(format!("natural toggle: {} frames, {travel:.1}px travel", frames.len()));
        ensure(travel > 300.0 && intermediate.len() >= 4, || format!("no continuous width motion: {samples}"))?;
        ensure(intermediate.last().unwrap()[0].as_f64().unwrap() - intermediate[0][0].as_f64().unwrap() >= 120.0, || {
            "motion has less than 120ms of intermediate frames".into()
        })?;
        ensure(frames.iter().all(|f| (f[2].as_f64().unwrap() - glyph).abs() < 0.1), || {
            format!("text is horizontally stretched: {samples}")
        })?;
        ensure(frames.windows(2).all(|f| (f[1][1].as_f64().unwrap() - f[0][1].as_f64().unwrap()).abs() < travel * 0.5), || {
            format!("thread jumps between adjacent frames: {samples}")
        })?;
        let mut intervals: Vec<_> = frames.windows(2).map(|f| f[1][0].as_f64().unwrap() - f[0][0].as_f64().unwrap()).collect();
        intervals.sort_by(f64::total_cmp);
        let p95 = intervals[(intervals.len() as f64 * 0.95) as usize];
        ctx.notes.push(format!("column p95 frame interval {p95:.1} ms"));
        ensure(p95 < 26.0, || format!("column motion missed the frame budget: {p95:.1} ms"))?;
    }
    Ok(())
}

fn layout_motion(ctx: &mut Ctx) -> Result {
    let mut app = large_thread(ctx)?;
    for view in ["fold", "disclosure", "composer", "welcome"] {
        if view == "welcome" {
            app.page.press("Control+n")?;
            app.page.wait_visible(&css(".thread.welcome"))?;
            pause(0.3);
        }
        app.page.eval(&format!("window.__reviewLayoutView = '{view}'"))?;
        let measured = app.page.eval(r#"new Promise(resolve => {
      const view = window.__reviewLayoutView;
      const welcome = document.querySelector('.thread.welcome');
      const fold = document.createElement('div'); fold.className = 'fold';
      fold.innerHTML = '<div class="fold-inner"><div style="height:240px">Project rows</div></div>';
      const details = document.createElement('details'); details.className = 'activity';
      details.innerHTML = '<summary>Measured work</summary><div class="activity-body"><div class="md">' + '<p>Work step</p>'.repeat(200) + '</div></div>';
      const field = document.querySelector('.composer textarea');
      const original = field.style.height;
      if (view === 'fold') document.querySelector('.projects').append(fold);
      if (view === 'disclosure') {
        const answer = document.querySelector('.col > article.ai:last-child');
        answer.insertBefore(details, answer.querySelector(':scope > .md'));
        details.querySelector('summary').scrollIntoView();
      }
      getComputedStyle(fold).gridTemplateRows;
      // Measure size transitions after fixture insertion and its separate arrival motion have settled.
      setTimeout(() => {
      const intervals = [], costs = []; let previous, animated = 0;
      const elements = [view === 'fold' ? fold : view === 'disclosure' ? details : view === 'composer' ? field : welcome];
      const start = performance.now();
      if (welcome) welcome.style.paddingRight = '352px';
      else if (view === 'fold') fold.classList.add('open');
      else if (view === 'disclosure') details.querySelector('summary').click();
      else field.style.height = '200px';
      const frame = now => {
        if (previous !== undefined) intervals.push(now - previous); previous = now;
        const begin = performance.now();
        for (const el of elements) el.getBoundingClientRect();
        costs.push(performance.now() - begin);
        animated = Math.max(animated, elements.filter(el => el.getAnimations().length > 0).length);
        if (now - start < 280) requestAnimationFrame(frame);
        else { fold.remove(); details.remove(); if (welcome) welcome.style.removeProperty('padding-right'); field.style.height = original;
          intervals.sort((a,b) => a-b); costs.sort((a,b) => a-b);
          resolve({frames: intervals.length, animated, expected: elements.length, p95: intervals[Math.floor(intervals.length*.95)], layout: costs[Math.floor(costs.length*.95)]}); }
      };
      requestAnimationFrame(frame);
      }, 300);
    })"#)?;
        ctx.notes.push(format!(
            "{view} layout motion: {} frames, p95 interval {} ms, p95 layout read {} ms",
            measured["frames"], measured["p95"], measured["layout"]
        ));
        ensure(measured["animated"] == measured["expected"], || format!("the size transitions did not run: {measured}"))?;
        ensure(measured["frames"].as_u64().unwrap_or(0) >= 12, || format!("not enough frames: {measured}"))?;
        ensure(measured["p95"].as_f64().unwrap_or(f64::MAX) < 25.0 && measured["layout"].as_f64().unwrap_or(f64::MAX) < 8.0, || {
            format!("layout animations exceed the frame budget: {measured}")
        })?;
    }
    Ok(())
}
