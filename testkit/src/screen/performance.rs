//! A reproducible synthetic workload, measured in the hidden native test app. No timing threshold: establish a baseline.

use super::{Result, Scenario, claude_chat, ensure};
use crate::app::Ctx;
use crate::page::{Page, css};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};

pub(super) const NAME: &str = "performance baseline: input, scroll and resize on a thousand messages within the memory budget";
const SAMPLES: usize = 120;
const WINDOW: Duration = Duration::from_secs(5);

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: NAME, run: baseline }]
}

// Two animation frames are a scheduling/layout estimate, not a paint acknowledgement or true INP.
const FRAMES: &str = "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))";

fn frames(page: &mut Page) -> Result {
    page.eval(FRAMES).map(drop)
}

fn distribution(values: &mut [f64]) -> Result<String> {
    ensure(values.len() >= 100 && values.iter().all(|v| v.is_finite() && *v >= 0.0), || "invalid baseline samples".into())?;
    values.sort_by(f64::total_cmp);
    let percentile = |p: f64| values[(values.len() as f64 * p).ceil() as usize - 1];
    Ok(format!(
        "n={}, p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms",
        values.len(),
        percentile(0.50),
        percentile(0.95),
        values[values.len() - 1]
    ))
}

fn scroll(page: &mut Page, i: usize) -> Result {
    let fraction = if i.is_multiple_of(2) { 0.25 } else { 0.75 };
    let moved = page.eval(&format!(
        "(() => {{const el=document.querySelector('.thread'); const before=el.scrollTop; el.scrollTop=(el.scrollHeight-el.clientHeight)*{fraction}; return Math.abs(el.scrollTop-before)>1;}})()"
    ))?;
    ensure(moved == true, || "synthetic scroll did not move the thread".into())
}

fn baseline(ctx: &mut Ctx) -> Result {
    super::app::seed_thousand_messages(ctx, true)?;
    let fixture = std::fs::read(ctx.sb.data.join("conversations/5f1c0000-0000-4000-8000-000000000001.json")).map_err(|e| e.to_string())?;
    ctx.notes.push(format!("Baseline synthetic fixture SHA-256: {}", faber_engine::hex(&Sha256::digest(&fixture))));
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    page.click(&css(".list .item").has_text("Long one"))?;
    page.wait_visible(&css("article.ai").last())?;
    let drawn = page.count(&css("article.ai"))?;
    ensure(drawn > 0 && drawn <= 25, || format!("expected bounded initial answers, found {drawn}"))?;
    page.wait_visible(&css("article.ai details.activity").last())?;
    ensure(page.count(&css("article.ai details.activity[open]"))? == 0, || "the fixture's tools should start collapsed".into())?;
    // Exercise the real older-message loader once before measuring a stable scrollable slice.
    page.eval_on(&css(".thread"), "el.scrollTop=0; return {};")?;
    page.wait_for("older answers", &format!("document.querySelectorAll('article.ai').length>{drawn}"))?;
    super::shell::set_size(page, 1028, 779)?;
    frames(page)?;
    let mounted = page.count(&css("article.ai"))?;
    ensure(mounted > drawn && mounted <= drawn + 25, || format!("older-message load mounted {mounted} answers from {drawn}"))?;
    ensure(page.count(&css("article.ai details.activity"))? == mounted, || "mounted answers lost their tool groups".into())?;
    let loaded_mb = ctx.memory_mb(app.id)?.0;
    let mut peak_mb = loaded_mb;

    page.eval(
        r#"(() => {
      const input=document.querySelector('.composer textarea');
      if (!input) throw Error('missing composer');
      input.focus();
      window.__baseline={samples:[],trusted:0,target:input};
      input.addEventListener('input', event => {
        const started=performance.now();
        if (event.isTrusted) window.__baseline.trusted++;
        requestAnimationFrame(() => requestAnimationFrame(() => window.__baseline.samples.push(performance.now()-started)));
      });
      return true;
    })()"#,
    )?;
    for i in 0..SAMPLES {
        // The shortcut-key helper omits letter text; reuse the native text insertion path.
        page.cdp.insert_text("a")?;
        if let Err(error) = page.wait_for("native input frame sample", &format!("window.__baseline.samples.length==={}", i + 1)) {
            let state = page.eval(r#"(() => {
              const input=document.querySelector('.composer textarea'), active=document.activeElement, baseline=window.__baseline;
              return {active: {tag:active?.tagName,id:active?.id,class:active?.className},composer: {value:input?.value,id:input?.id,connected:input?.isConnected},listenerTargetConnected:baseline.target.isConnected,sameListenerTarget:baseline.target===input,trusted:baseline.trusted,samples:baseline.samples.length,visibility:document.visibilityState,now:performance.now()};
            })()"#)?;
            let raf = page.eval(r#"Promise.race([
              new Promise(resolve => {const started=performance.now(); requestAnimationFrame(()=>requestAnimationFrame(()=>resolve({completed:true,elapsed:performance.now()-started})));}),
              new Promise(resolve=>setTimeout(()=>resolve({completed:false}),1000))
            ])"#)?;
            return Err(format!("{error}; native input state {state}; isolated two-rAF probe {raf}"));
        }
    }
    let input: Value = page.eval(
        "({values:window.__baseline.samples,trusted:window.__baseline.trusted,value:document.querySelector('.composer textarea').value})",
    )?;
    ensure(input["trusted"] == SAMPLES && input["value"] == "a".repeat(SAMPLES), || {
        "native typing did not reach the composer exactly once per sample".into()
    })?;
    let mut input_samples: Vec<f64> = serde_json::from_value(input["values"].clone()).map_err(|e| e.to_string())?;
    ctx.notes.push(format!("Baseline native CDP insertText input event to two rAF (not INP): {}", distribution(&mut input_samples)?));
    peak_mb = peak_mb.max(ctx.memory_mb(app.id)?.0);

    for operation in ["synthetic scrollTop", "CDP viewport resize"] {
        let mut samples = Vec::with_capacity(SAMPLES);
        for i in 0..SAMPLES {
            let started = Instant::now();
            if operation == "synthetic scrollTop" {
                scroll(page, i)?;
            } else {
                let width = if i % 2 == 0 { 720 } else { 1028 };
                super::shell::set_size(page, width, 779)?;
                ensure(page.eval("innerWidth")? == width, || "viewport did not resize".into())?;
            }
            frames(page)?;
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
            if i % 20 == 0 {
                peak_mb = peak_mb.max(ctx.memory_mb(app.id)?.0);
            }
        }
        ctx.notes.push(format!("Baseline {operation} round trip + two rAF (includes driver overhead): {}", distribution(&mut samples)?));
    }

    // Matched fixed windows include process sampling outside their wall-clock intervals.
    let idle_before = ctx.cpu_ms(app.id)?;
    let idle_started = Instant::now();
    page.eval("new Promise(resolve=>setTimeout(resolve,5000))")?;
    let idle_wall = idle_started.elapsed().as_secs_f64() * 1000.0;
    let idle_cpu = ctx.cpu_ms(app.id)? - idle_before;
    let active_before = ctx.cpu_ms(app.id)?;
    let active_started = Instant::now();
    let mut actions = 0;
    while active_started.elapsed() < WINDOW {
        if actions % 2 == 0 {
            for kind in ["keyDown", "keyUp"] {
                page.cdp.call(
                    "Input.dispatchKeyEvent",
                    serde_json::json!({ "type": kind, "key": "Backspace", "code": "Backspace", "windowsVirtualKeyCode": 8 }),
                )?;
            }
        } else {
            page.cdp.insert_text("a")?;
        }
        scroll(page, actions)?;
        super::shell::set_size(page, if actions % 2 == 0 { 720 } else { 1028 }, 779)?;
        frames(page)?;
        actions += 1;
    }
    let active_wall = active_started.elapsed().as_secs_f64() * 1000.0;
    let active_cpu = ctx.cpu_ms(app.id)? - active_before;
    let final_input = page.eval("({trusted:window.__baseline.trusted,value:document.querySelector('.composer textarea').value})")?;
    let expected_length = SAMPLES - actions % 2;
    ensure(final_input["trusted"] == SAMPLES + actions && final_input["value"] == "a".repeat(expected_length), || {
        "active native edits did not reach the composer exactly once per cycle".into()
    })?;
    peak_mb = peak_mb.max(ctx.memory_mb(app.id)?.0);
    let final_groups = page.count(&css("article.ai details.activity"))?;
    ensure(final_groups > 0 && final_groups <= mounted && page.count(&css("article.ai details.activity[open]"))? == 0, || {
        format!("expected bounded collapsed tool groups, found {final_groups}")
    })?;
    ensure(
        actions > 0 && [loaded_mb, peak_mb, idle_wall, idle_cpu, active_wall, active_cpu].iter().all(|v| v.is_finite() && *v >= 0.0),
        || "invalid baseline resource measurement".into(),
    )?;
    ctx.notes.push(format!(
        "Baseline synthetic 1000 messages, 500 collapsed tools x 40 output lines: whole app loaded {loaded_mb:.1} MB, sampled peak {peak_mb:.1} MB; matched 5 s windows: idle {idle_cpu:.0} ms CPU over {idle_wall:.0} ms ({:.1}% one core), active {active_cpu:.0} ms CPU over {active_wall:.0} ms ({:.1}% one core), {actions} input/scroll/resize cycles; one workload, no optimization claim",
        idle_cpu / idle_wall * 100.0, active_cpu / active_wall * 100.0
    ));
    ensure(peak_mb <= 400.0, || format!("baseline sampled peak {peak_mb:.1} MB exceeds 400 MB"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributions_use_nearest_rank_and_reject_invalid_samples() {
        let mut samples: Vec<f64> = (1..=120).map(f64::from).collect();
        samples.reverse();
        assert_eq!(distribution(&mut samples).unwrap(), "n=120, p50 60.00 ms, p95 114.00 ms, max 120.00 ms");
        assert!(distribution(&mut [1.0; 99]).is_err());
        assert!(distribution(&mut [f64::NAN; 120]).is_err());
        assert!(distribution(&mut [-1.0; 120]).is_err());
    }
}
