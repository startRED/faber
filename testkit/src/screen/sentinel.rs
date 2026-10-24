//! The UX sentinel, slice S0 (docs/superpowers/specs/2026-10-12-ux-sentinel-design.md, section 12). Three scenarios on the
//! same hidden desktop as the others: panels in motion, the composer and the end of the thread, and garbled text. The
//! probe (`probe.js`) measures; the rules are in `crate::sentinel`, where each has its planted-defect twin as a unit
//! test. Here every detector also meets a planted defect in the live page (never in the repository), and must find it.

use super::{Result, Scenario, ask, claude_chat, ensure, pause, same, start_settings, system_motion};
use crate::app::Ctx;
use crate::page::{Page, css};
use crate::sandbox::{CliSlot, Launch};
use crate::sentinel::{self, Anim, Finding, Motion, Sample, Typed};
use serde_json::{Value, json};

const PROBE: &str = include_str!("../probe.js");

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "sentinel: panels move through frames, and reduced motion moves nothing", run: motion },
        Scenario { name: "sentinel: the native select picker opens and dismisses through real frames", run: native_picker },
        Scenario { name: "sentinel: a permission card arrives through frames", run: permission },
        Scenario { name: "sentinel: the composer grows and shrinks, and the end of the thread stays clear", run: composer },
        Scenario { name: "sentinel: no text is garbled in five languages, and titles survive the engine's files", run: text },
        Scenario { name: "sentinel: the main screens keep their composition, at 1920x1080 and 1280x800", run: composition },
    ]
}

/// One cell of the smoke matrix. The full all-pairs set of the spec (section 4.2) is nine rows of width, look and
/// language; each scenario runs the rows it needs.
#[derive(Debug, Clone, Copy)]
struct Config {
    width: u32,
    height: u32,
    look: &'static str,
    language: &'static str,
    size: u32,
    motion: &'static str,
}

impl Config {
    fn label(&self) -> String {
        format!("{}x{} {} {} {}px {}", self.width, self.height, self.look, self.language, self.size, self.motion)
    }
}

/// Three rows of the all-pairs set, one per width, with a different look and language each.
const ROWS: [Config; 3] = [
    Config { width: 720, height: 480, look: "sala", language: "de", size: 18, motion: "full" },
    Config { width: 1100, height: 700, look: "manha", language: "pt-BR", size: 15, motion: "reduced" },
    Config { width: 1440, height: 900, look: "alto", language: "en", size: 15, motion: "full" },
];

// What the scenarios share.

fn viewport(page: &mut Page, width: u32, height: u32) -> Result {
    page.cdp
        .call("Emulation.setDeviceMetricsOverride", json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false }))?;
    page.wait_for("the window size", &format!("innerWidth === {width} && innerHeight === {height}"))?;
    // The screen suppresses motion for a quarter of a second after a resize.
    pause(0.4);
    Ok(())
}

/// Applies a configuration the way a person would, only faster: the settings are saved through the shell's own command,
/// the window is resized and the screen is loaded again.
fn apply(page: &mut Page, c: &Config) -> Result {
    let patch = json!({ "language": c.language, "appearance": { "base": c.look, "textSize": c.size, "motion": c.motion } });
    page.eval(&format!("window.__TAURI_INTERNALS__.invoke('save_settings', {{ patch: {patch} }}).then(() => true)"))?;
    viewport(page, c.width, c.height)?;
    page.reload()?;
    page.wait_for(
        "the settings to show",
        &format!("document.documentElement.lang === '{}' && document.documentElement.dataset.base === '{}'", c.language, c.look),
    )
}

fn inject(page: &mut Page) -> Result {
    page.eval(PROBE).map(drop)
}

/// The clock check: a hidden desktop that gives no real frames makes every motion result meaningless, so the run fails as
/// an infrastructure error. It never passes silently. Then everything is slowed to a quarter (spec section 3).
fn clock_and_slow(page: &mut Page) -> Result {
    // A page that has only just started is busy for its first second, so a stalled clock gets three tries.
    let mut last = String::new();
    for _ in 0..3 {
        let measured = page.eval("window.__sx.clock()")?;
        let frames: Vec<f64> = measured["frames"].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
        match sentinel::clock_problem(&frames, measured["ended"] == json!(true)) {
            None => return page.cdp.call("Animation.setPlaybackRate", json!({ "playbackRate": 0.25 })).map(drop),
            Some(problem) => last = format!("{problem} ({measured})"),
        }
        pause(0.5);
    }
    Err(format!("infrastructure: the hidden desktop does not give the page a real frame clock: {last}"))
}

struct Watched {
    samples: Vec<Vec<Sample>>,
    anims: Vec<Anim>,
}

/// Samples these surfaces on every frame while `act` runs, until they rest. The raw numbers are kept as the finding's
/// evidence (spec section 10.1) in `target/faber-test/ux/traces/`.
fn watch(ctx: &Ctx, label: &str, page: &mut Page, names: &[&str], act: impl FnOnce(&mut Page) -> Result) -> Result<Watched> {
    page.eval(&format!("window.__sx.start({}); true", json!(names)))?;
    act(page)?;
    let out = page.eval("window.__sx.finish(500, 350, 4000)")?;
    let folder = ctx.paths.target.join("ux/traces");
    let slug: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    if std::fs::create_dir_all(&folder).is_ok() {
        let _ = std::fs::write(folder.join(format!("{slug}.json")), out.to_string());
    }
    Ok(Watched {
        samples: names.iter().map(|n| sentinel::samples(&out["samples"][n])).collect(),
        anims: sentinel::anims(&out["animations"]),
    })
}

fn click(page: &mut Page, selector: &str) -> Result {
    page.eval(&format!(
        "(() => {{ const el = document.querySelector({}); if (!el) throw new Error('nothing matches {}'); el.click(); return true; }})()",
        json!(selector),
        selector.replace('\'', "")
    ))
    .map(drop)
}

fn screen_of(page: &mut Page) -> Result<String> {
    Ok(page.eval("document.querySelector('[data-screen]')?.dataset.screen ?? 'unknown'")?.as_str().unwrap_or("unknown").to_string())
}

/// Checks the findings of a scenario against the known ones, writes them down, and fails on any new critical or high one.
fn gate(ctx: &mut Ctx, name: &str, findings: &[Finding]) -> Result {
    let known = sentinel::known(&ctx.paths.root)?;
    sentinel::write_findings(&ctx.paths.target.join("ux"), name, findings)?;
    let blocking = sentinel::blocking(findings, &known);
    ctx.notes.push(format!("{} findings, {} blocking", findings.len(), blocking.len()));
    ensure(blocking.is_empty(), || {
        let lines: Vec<String> = blocking
            .iter()
            .map(|f| {
                format!("{} {} {} {}: {} [{}] fingerprint {}", f.kind, f.screen, f.surface, f.element, f.metric, f.config, f.fingerprint())
            })
            .collect();
        format!("new findings that block:\n  {}", lines.join("\n  "))
    })
}

fn caught(what: &str, found: bool) -> Result {
    ensure(found, || format!("planted defect not caught: {what}"))
}

// 1. Motion.

/// What a watched surface did: finding on a jump, a note of how many frames it took, an error if nothing happened.
fn judge_motion(ctx: &mut Ctx, found: &mut Vec<Finding>, screen: &str, surface: &str, what: &str, config: &str, rows: &[Sample]) -> Result {
    match sentinel::teleport(rows) {
        Motion::Still => Err(format!("{surface} {what}: nothing moved, so there is nothing to judge (the interaction did not happen)")),
        Motion::Smooth(frames) => {
            ctx.notes.push(format!("{surface} {what}: {frames} frames"));
            Ok(())
        }
        Motion::Teleport(metric) => {
            found.push(Finding::new("M1", screen, surface, &format!(".{surface}"), config, format!("{what}: {metric}")));
            Ok(())
        }
    }
}

fn native_picker(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "pt-BR")?;
    let mut app = ctx.launch(claude_chat(ctx))?;
    let page = &mut app.page;
    let folder = ctx.paths.target.join("ux/picker");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let capability = page.eval("({ supported: CSS.supports('appearance', 'base-select'), userAgent: navigator.userAgent })")?;
    std::fs::write(folder.join("capability.json"), capability.to_string()).map_err(|e| e.to_string())?;
    ensure(capability["supported"] == json!(true), || {
        format!("infrastructure: this WebView cannot qualify the customizable picker: {capability}")
    })?;
    let mut detached = Vec::new();
    for (width, height) in [(720, 480), (1280, 800), (1920, 1080)] {
        for mode in ["full", "reduced", "none"] {
            let config =
                Config { width, height, look: "sala", language: "pt-BR", size: 15, motion: if mode == "none" { "none" } else { "full" } };
            apply(page, &config)?;
            inject(page)?;
            if mode == "full" {
                system_motion(page, false)?;
                page.cdp.call("Animation.setPlaybackRate", json!({ "playbackRate": 1.0 }))?;
                clock_and_slow(page)?;
            }
            system_motion(page, mode == "reduced")?;
            pause(0.3);
            page.press("Control+,")?;
            let select = page.labelled("set.language")?;
            page.wait_visible(&select)?;
            page.wait_for("Settings to finish arriving", "document.getAnimations().every(a => a.effect.getTiming().iterations === Infinity || (!a.pending && a.playState !== 'running'))")?;
            page.eval_on(&select, "el.dataset.pickerProbe = 'true'; return {};")?;
            let before = page.eval("document.querySelector('[data-picker-probe]').value")?;
            // The picker is a pseudo-element, so the DOM-surface probe cannot observe it.
            page.eval(r#"(() => {
          const el = document.querySelector('[data-picker-probe]');
          const state = () => { const s = getComputedStyle(el, '::picker(select)'); return {
            at: performance.now(), open: el.matches(':open'), display: s.display,
            opacity: s.opacity === '' ? null : Number(s.opacity), transform: s.transform, connected: el.isConnected,
            animations: document.getAnimations().filter(a => a.effect?.getTiming().iterations !== Infinity).map(a => ({ state:a.playState, pseudo:a.effect?.pseudoElement,
              duration:a.effect?.getTiming().duration, progress:a.effect?.getComputedTiming().progress })) }; };
          window.__pickerProbe = { state, rows: [], running: false,
            start() { this.rows = [state()]; this.running = true; const frame = () => {
              if (!this.running) return; this.rows.push(state()); requestAnimationFrame(frame); }; requestAnimationFrame(frame); },
            finish() { return new Promise(resolve => setTimeout(() => { this.running = false; this.rows.push(state()); resolve(this.rows); }, 1600)); } };
          return true;
        })()"#)?;
            for step in ["opens", "closes"] {
                page.eval("window.__pickerProbe.start(); true")?;
                if step == "opens" {
                    page.click(&select)?;
                    page.wait_for("the real native picker to open", "document.querySelector('[data-picker-probe]').matches(':open')")?;
                } else {
                    page.press("Escape")?;
                }
                let immediate = page.eval("window.__pickerProbe.state()")?;
                let rows = page.eval("window.__pickerProbe.finish()")?;
                let evidence = json!({ "mode": mode, "step": step, "immediate": immediate, "rows": rows });
                std::fs::write(folder.join(format!("{width}-{mode}-{step}.json")), evidence.to_string()).map_err(|e| e.to_string())?;
                page.screenshot(&folder.join(format!("{width}-{mode}-{step}.png")))?;
                let samples = rows.as_array().ok_or("picker probe must return frame samples")?;
                ensure(samples.len() >= 3, || format!("infrastructure: no picker frame clock: {evidence}"))?;
                let last = samples.last().ok_or("picker samples missing")?;
                if last["connected"] != json!(true) {
                    detached.push(format!("{width} {mode}: Escape removed Settings instead of only dismissing its picker"));
                    break;
                }
                same(last["open"].clone(), json!(step == "opens"), "native picker open state after settling")?;
                same(last["display"] == json!("none"), step == "closes", "native picker display after settling")?;
                if mode == "full" {
                    ensure(
                        samples.iter().any(|r| r["display"] != json!("none") && r["opacity"].as_f64().is_some_and(|o| o > 0.0 && o < 1.0)),
                        || format!("the full-motion picker must have a real intermediate opacity: {evidence}"),
                    )?;
                } else {
                    same(immediate["display"] == json!("none"), step == "closes", "reduced/none picker changes display immediately")?;
                    ensure(samples.iter().all(|r| r["animations"].as_array().is_some_and(Vec::is_empty)), || {
                        format!("{mode} picker must have no animations: {evidence}")
                    })?;
                }
            }
            if page.eval("!!document.querySelector('[data-picker-probe]')")? != json!(true) {
                continue;
            }
            same(page.eval("document.querySelector('[data-picker-probe]').value")?, before, "Escape preserves the selected language")?;
            same(
                page.eval("document.activeElement === document.querySelector('[data-picker-probe]')")?,
                json!(true),
                "picker returns focus to its select",
            )?;
            page.press("Escape")?;
            page.wait_gone(&css(".settings"))?;
            ctx.notes.push(format!("native picker {mode}: real open/close frames and focus recorded"));
        }
    }
    ensure(detached.is_empty(), || detached.join("; "))?;
    Ok(())
}

fn motion(ctx: &mut Ctx) -> Result {
    // No look chosen yet: the first run shows its look step, and the tutorial that follows has a card to watch.
    ctx.write_settings(&json!({ "language": "en", "setup": "done" }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let mut found = Vec::new();
    viewport(page, 1440, 900)?;
    system_motion(page, false)?;
    inject(page)?;
    clock_and_slow(page)?;

    // The probe sees through the shadow root of an exit copy: a host at half opacity makes its content half visible.
    let seen = page.eval(
        "(() => { const host = document.createElement('div'); host.className = 'motion-exit'; host.style.opacity = '0.5'; document.body.append(host);
          const child = document.createElement('div'); child.dataset.surface = 'probe.twin'; child.style.cssText = 'width:10px;height:10px';
          host.attachShadow({ mode: 'open' }).append(child); const state = window.__sx.stateOf('probe.twin'); host.remove(); return state; })()",
    )?;
    ensure(seen[0] == json!(1) && seen[5].as_f64().is_some_and(|o| (o - 0.5).abs() < 0.01), || {
        format!("the probe must count the opacity of a shadow host (present, then 0.5 opacity wanted): {seen}")
    })?;

    // The tutorial card, coming in and moving to the next step.
    let heading = page.named("heading", "first.look.title")?;
    page.wait_visible(&heading)?;
    let beginner = page.text_of("first.xp.beginner")?;
    let pick = css(".choice2 button").has_text(&beginner);
    page.click(&pick)?;
    let config = Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "full" }.label();
    let go = page.named("button", "first.continue")?;
    let screen = screen_of(page)?;
    let w = watch(ctx, "tutorial appears", page, &["tutorial.card"], |p| {
        p.eval_on(&go, "el.click(); return {};")?;
        // The card is drawn once the settings are saved, a moment after the click.
        p.wait_visible(&css(".coach"))
    })?;
    judge_motion(ctx, &mut found, &screen, "tutorial.card", "appears", &config, &w.samples[0])?;
    let page = &mut app.page;
    let next = page.named("button", "tour.next")?;
    let w = watch(ctx, "tutorial next", page, &["tutorial.card"], |p| p.eval_on(&next, "el.click(); return {};").map(drop))?;
    // The next card is a new element at another place: the old one leaves, the new one comes in.
    judge_motion(ctx, &mut found, "chat.new", "tutorial.card", "changes step", &config, &w.samples[0])?;
    let page = &mut app.page;
    super::skip_tour(page)?;
    page.wait_for("the new conversation", "!!document.querySelector('.composer textarea')")?;

    // The panels, at the three widths where the shell changes.
    for (width, height, look) in [(1440, 900, "wide"), (1000, 700, "mid"), (720, 480, "narrow")] {
        let page = &mut app.page;
        viewport(page, width, height)?;
        let config = Config { width, height, look: "sala", language: "en", size: 15, motion: "full" }.label();
        let screen = screen_of(page)?;
        for step in ["opens", "closes"] {
            let w = watch(ctx, &format!("running {step} {width}"), page, &["running"], |p| click(p, ".run-btn"))?;
            let surface = if look == "wide" { "running (docked)" } else { "running (over the stage)" };
            let rows = &w.samples[0];
            judge_motion(ctx, &mut found, &screen, "running", &format!("{step}, {surface}"), &config, rows)?;
        }
        if look == "narrow" {
            for step in ["opens", "closes"] {
                let w = watch(ctx, &format!("sidebar {step}"), page, &["sidebar"], |p| {
                    click(p, if step == "opens" { ".topbar .menu" } else { ".scrim" })
                })?;
                judge_motion(ctx, &mut found, &screen, "sidebar", &format!("{step}, over the stage"), &config, &w.samples[0])?;
            }
        }
    }

    // Settings, opened and closed by the keyboard.
    let page = &mut app.page;
    viewport(page, 1440, 900)?;
    let config = Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "full" }.label();
    let screen = screen_of(page)?;
    let w = watch(ctx, "settings opens", page, &["settings"], |p| p.press("Control+,"))?;
    judge_motion(ctx, &mut found, &screen, "settings", "opens", &config, &w.samples[0])?;
    let page = &mut app.page;
    let w = watch(ctx, "settings closes", page, &["settings"], |p| p.press("Escape"))?;
    judge_motion(ctx, &mut found, "settings", "settings", "closes", &config, &w.samples[0])?;

    // M6: with less motion asked for, nothing may travel. Reduced (the system's preference): only quick fades.
    let page = &mut app.page;
    system_motion(page, true)?;
    pause(0.3);
    let reduced = Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "reduced" }.label();
    for step in ["opens", "closes"] {
        let w = watch(ctx, &format!("reduced {step}"), page, &["running"], |p| click(p, ".run-btn"))?;
        for problem in sentinel::reduced_problems("reduced", &w.anims) {
            found.push(Finding::new("M6", "chat.new", "running", ".running", &reduced, format!("{step}: {problem}")));
        }
        ctx.notes.push(format!("reduced {step}: {} animations", w.anims.len()));
    }

    // The planted twins, in the live page. An animation that travels for 400 ms under reduced motion must be a movement;
    // a column that has no motion at all must be a teleport.
    let w = watch(ctx, "planted reduced", page, &["running"], |p| {
        click(p, ".run-btn")?;
        p.eval("document.querySelector('.run-btn').animate([{ transform: 'translateX(40px)' }, { transform: 'none' }], { duration: 400 }); true").map(drop)
    })?;
    caught("a 400 ms slide under reduced motion (M6)", !sentinel::reduced_problems("reduced", &w.anims).is_empty())?;
    click(page, ".run-btn")?;
    pause(0.6);
    system_motion(page, false)?;
    pause(0.3);
    page.eval(
        "Element.prototype.animate = function () { return { finished: Promise.resolve(), cancel() {}, playState: 'finished' }; }; true",
    )?;
    let mut planted = Vec::new();
    let w = watch(ctx, "planted teleport", page, &["running"], |p| click(p, ".run-btn"))?;
    judge_motion(ctx, &mut planted, "chat.new", "running", "opens with no animation", "planted", &w.samples[0])?;
    caught("a column with no motion (M1)", planted.iter().any(|f| f.kind == "M1"))?;

    // None, the person's own setting: nothing may run at all, ambient effects included. The page is loaded again, which
    // also removes the planted defect.
    let none = Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "none" };
    apply(page, &none)?;
    inject(page)?;
    page.cdp.call("Animation.setPlaybackRate", json!({ "playbackRate": 0.25 }))?;
    pause(0.4);
    for step in ["opens", "closes"] {
        let w = watch(ctx, &format!("none {step}"), page, &["running"], |p| click(p, ".run-btn"))?;
        for problem in sentinel::reduced_problems("none", &w.anims) {
            found.push(Finding::new("M6", "chat.new", "running", ".running", &none.label(), format!("{step}: {problem}")));
        }
        ctx.notes.push(format!("none {step}: {} animations", w.anims.len()));
    }

    gate(ctx, "motion", &found)?;
    ctx.close(app.id)
}

/// The request of the recording `claude/tools.jsonl`: the CLI asks to write a.txt, and after the answer b.txt.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

/// M1 on the permission card (a dialog). The card arrives as shipped; put away and shown again with its animation taken
/// away, it must be found.
fn permission(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    };
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    page.eval("window.__TAURI_INTERNALS__.invoke('save_settings', { patch: { permissions: 'ask' } }).then(() => true)")?;
    let mut found = Vec::new();
    viewport(page, 1440, 900)?;
    // The machine's own motion preference is not under test (a runner may ask for less): pin it, as the motion scenario does.
    system_motion(page, false)?;
    inject(page)?;
    clock_and_slow(page)?;
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    let config = Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "full" }.label();
    let w = watch(ctx, "ask appears", page, &["ask"], |p| {
        ask(p, TWO_WRITES)?;
        p.wait_visible(&css(".ask").has_text("a.txt"))
    })?;
    judge_motion(ctx, &mut found, "chat.thread", "ask", "appears", &config, &w.samples[0])?;
    let page = &mut app.page;
    page.eval(
        "(() => { const s = document.createElement('style'); s.id = 'sx-plant'; s.textContent = '.ask { animation: none !important; }'; document.head.append(s); return true; })()",
    )?;
    // The same card put away and shown again, now with no animation.
    // The running column opens by itself next to the card; the twin starts when everything has come to rest.
    page.wait_for("the running column to open", "!!document.querySelector('aside.runcol')")?;
    page.wait_for(
        "the page to rest",
        "document.getAnimations().every((a) => a.effect.getTiming().iterations === Infinity || a.playState !== 'running')",
    )?;
    page.eval("document.querySelectorAll('.ask').forEach((el) => { el.style.display = 'none'; }); true")?;
    pause(0.6);
    let w = watch(ctx, "ask planted", page, &["ask"], |p| {
        p.eval("document.querySelectorAll('.ask').forEach((el) => { el.style.display = ''; }); true").map(drop)
    })?;
    let mut planted = Vec::new();
    judge_motion(ctx, &mut planted, "chat.thread", "ask", "appears with no animation", "planted", &w.samples[0])?;
    caught("a permission card with no motion (M1)", planted.iter().any(|f| f.kind == "M1"))?;
    gate(ctx, "permission", &found)?;
    ctx.close(app.id)
}

// 2. The composer and the end of the thread.

/// Where the last message ends, where the composer starts, and where the scroll area ends, with the thread at its end.
const THREAD_END: &str = r#"(() => {
  const thread = document.querySelector('.thread');
  thread.scrollTop = thread.scrollHeight;
  const col = thread.querySelector('.col');
  let last = col?.lastElementChild;
  while (last && !last.getBoundingClientRect().height) last = last.previousElementSibling;
  if (!last) return null;
  const box = last.getBoundingClientRect();
  const at = document.elementFromPoint(box.left + box.width / 2, Math.min(box.bottom - 2, innerHeight - 1));
  return { covered: !at || !last.contains(at), lastBottom: last.getBoundingClientRect().bottom, composerTop: document.querySelector('.composer').getBoundingClientRect().top,
           areaBottom: thread.getBoundingClientRect().bottom, fromEnd: thread.scrollHeight - thread.scrollTop - thread.clientHeight };
})()"#;

/// The composer's field after typing: the numbers `Typed` holds.
const FIELD: &str = r#"(() => {
  const f = document.querySelector('.composer textarea');
  const bar = document.querySelector('.composer .send').getBoundingClientRect();
  const card = document.querySelector('.composer').getBoundingClientRect();
  const line = parseFloat(getComputedStyle(f).lineHeight);
  return { height: f.getBoundingClientRect().height, scrollHeight: f.scrollHeight, barGap: card.bottom - bar.bottom, line,
           caret: f.scrollTop + f.clientHeight >= f.scrollHeight - line - 1, min: parseFloat(getComputedStyle(f).minHeight) };
})()"#;

/// L4 and L5 at the end of the thread. With `follow`, the thread is left where the app keeps it (I5: it must still be at
/// its end after the composer grew); otherwise it is scrolled to its end first.
fn thread_end(page: &mut Page, config: &str, follow: bool, found: &mut Vec<Finding>) -> Result<Value> {
    let end = page.eval(
        &THREAD_END.replace("thread.scrollTop = thread.scrollHeight;", if follow { "" } else { "thread.scrollTop = thread.scrollHeight;" }),
    )?;
    ensure(!end.is_null(), || "the thread has no message".to_string())?;
    let n = |key: &str| end[key].as_f64().unwrap_or_default();
    if follow && let Some(metric) = sentinel::kept_problem(n("fromEnd")) {
        found.push(Finding::new("I5", "chat.thread", "thread", ".thread", config, metric));
        return Ok(end);
    }
    if let Some(metric) = sentinel::gap_problem(n("lastBottom"), n("composerTop")) {
        found.push(Finding::new("L4", "chat.thread", "thread", ".thread", config, metric));
    }
    if let Some(metric) = sentinel::cut_problem(n("lastBottom"), n("areaBottom"), end["covered"] == json!(true)) {
        found.push(Finding::new("L5", "chat.thread", "thread", ".thread", config, metric));
    }
    Ok(end)
}

/// Types 1, 2, 5, 10 and 40 lines into the composer, and judges what the field did (I1, I2, I4, and I5 at ten lines).
fn type_lines(page: &mut Page, config: &str, found: &mut Vec<Finding>) -> Result {
    let composer = page.labelled("composer.message")?;
    let mut rows = Vec::new();
    // The one-line height is read before anything is typed: a field that keeps its height must not move the yardstick.
    let before = page.eval(FIELD)?;
    let (mut line, min) = (before["line"].as_f64().unwrap_or(24.0), before["min"].as_f64().unwrap_or(40.0));
    for lines in [1u32, 2, 5, 10, 40] {
        let text = (1..=lines).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        page.fill(&composer, &text)?;
        pause(0.12);
        let f = page.eval(FIELD)?;
        let n = |key: &str| f[key].as_f64().unwrap_or_default();
        line = n("line");
        rows.push(Typed {
            lines,
            height: n("height"),
            scroll_height: n("scrollHeight"),
            bar_gap: n("barGap"),
            caret_visible: f["caret"] == json!(true),
        });
        if lines == 10 {
            // I5: the composer has grown at the end of the thread; the last message must still clear it.
            thread_end(page, &format!("{config}, composer at 10 lines"), true, found)?;
        }
    }
    let stage = page.eval("innerHeight")?.as_f64().unwrap_or(900.0);
    for (kind, metric) in sentinel::composer_problems(&rows, line, stage) {
        found.push(Finding::new(kind, "chat.thread", "composer", ".composer textarea", config, metric));
    }
    // I3: cleared, the field is one line again.
    page.eval_on(&composer, "el.focus(); el.select(); document.execCommand('delete'); return {};")?;
    pause(0.5);
    let after = page.eval(FIELD)?["height"].as_f64().unwrap_or_default();
    if let Some(metric) = sentinel::shrink_problem(after, min) {
        found.push(Finding::new("I3", "chat.thread", "composer", ".composer textarea", config, metric));
    }
    Ok(())
}

/// A conversation long enough to scroll at every size, so that its end can be looked at.
const LONG_TITLE: &str = "Sentinel thread";

fn seed_long_thread(ctx: &Ctx) -> Result {
    let id = "5f1c0000-0000-4000-8000-0000000000b2";
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let block = "A **complete answer** with explanatory prose, a list and code.

- First result
- Second result

```js
const result = 42;
```

";
    let messages: Vec<Value> = (0..16)
        .map(|i| {
            json!({ "id": format!("s-{i}"), "role": if i % 2 == 0 { "user" } else { "assistant" },
                    "text": if i % 2 == 0 { "A question.".into() } else { block.repeat(2) }, "at": 0, "status": "done" })
        })
        .collect();
    let body =
        json!({ "id": id, "title": LONG_TITLE, "area": "chat", "sessions": {}, "createdAt": 1, "updatedAt": 1, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), body.to_string()).map_err(|e| e.to_string())
}

fn composer(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    seed_long_thread(ctx)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let mut found = Vec::new();
    // Widest first: the sidebar is there to open the conversation from; the narrower rows find it remembered.
    for row in ROWS.iter().rev() {
        let page = &mut app.page;
        apply(page, row)?;
        if page.eval(&format!("document.querySelector('.crumb b')?.textContent === {}", json!(LONG_TITLE)))? != json!(true) {
            page.click(&css(".list .item").has_text(LONG_TITLE))?;
        }
        page.wait_for(
            "the long thread to be open",
            &format!(
                "document.querySelector('.crumb b')?.textContent === {} && !!document.querySelector('.col > article.ai')",
                json!(LONG_TITLE)
            ),
        )?;
        pause(0.5);
        let config = row.label();
        thread_end(page, &config, false, &mut found)?;
        type_lines(page, &config, &mut found)?;
        thread_end(page, &format!("{config}, composer cleared"), false, &mut found)?;
    }

    // The planted twins, on the last page: a fixed composer height, no room under the last message, a last item the
    // scroll area cuts off. Each must be found by the same code that finds nothing above.
    let page = &mut app.page;
    viewport(page, 1440, 900)?;
    // The settings are those of the last row the loop applied (the narrowest), seen at the widest size.
    let last = format!("{}, resized to 1440x900", ROWS[0].label());
    let plant = |page: &mut Page, css: &str| {
        page.eval(&format!("(() => {{ const s = document.createElement('style'); s.id = 'sx-plant'; s.textContent = {}; document.head.append(s); return true; }})()", json!(css))).map(drop)
    };
    let unplant = |page: &mut Page| page.eval("document.getElementById('sx-plant')?.remove(); true").map(drop);
    pause(0.3);
    plant(page, ".composer textarea { height: 60px !important; min-height: 60px !important; max-height: 60px !important; }")?;
    let mut twin = Vec::new();
    type_lines(page, &last, &mut twin)?;
    ensure(twin.iter().any(|f| f.kind == "I1") && twin.iter().any(|f| f.kind == "I2"), || {
        format!(
            "planted defect not caught: a fixed composer height (I1, I2): {:?}",
            twin.iter().map(|f| format!("{} {}", f.kind, f.metric)).collect::<Vec<_>>()
        )
    })?;
    unplant(page)?;
    pause(0.3);

    // A composer that only ever grows (I3), a send button that follows the text up (I4), a thread that loses its place
    // when the field grows (I5): three defects of behaviour, so they are planted as an input handler, not as a style.
    page.eval(
        "(() => { const field = document.querySelector('.composer textarea'), send = document.querySelector('.composer .send'), thread = document.querySelector('.thread');
          window.__sxLoose = () => { field.style.setProperty('min-height', field.getBoundingClientRect().height + 'px', 'important');
            send.style.transform = `translateY(${-field.scrollHeight / 3}px)`; setTimeout(() => { thread.scrollTop = 0; }, 30); };
          document.addEventListener('input', window.__sxLoose, true); return true; })()",
    )?;
    let mut twin = Vec::new();
    type_lines(page, &last, &mut twin)?;
    let kinds: Vec<&str> = twin.iter().map(|f| f.kind).collect();
    for kind in ["I3", "I4", "I5"] {
        caught(
            &format!("{kind} by a handler that only grows the field, moves the button and scrolls the thread away (found {kinds:?})"),
            kinds.contains(&kind),
        )?;
    }
    page.eval(
        "document.removeEventListener('input', window.__sxLoose, true); document.querySelector('.composer textarea').style.removeProperty('min-height'); document.querySelector('.composer .send').style.transform = ''; true",
    )?;
    pause(0.3);
    plant(page, ".thread { padding-bottom: 8px !important; }")?;
    let mut twin = Vec::new();
    let end = thread_end(page, &last, false, &mut twin)?;
    ensure(twin.iter().any(|f| f.kind == "L4"), || format!("planted defect not caught: 8 px under the last message (L4): {end}"))?;
    unplant(page)?;
    plant(page, ".thread { margin-bottom: -80px !important; }")?;
    let mut twin = Vec::new();
    thread_end(page, &last, false, &mut twin)?;
    caught("a last item with the composer over it (L5)", twin.iter().any(|f| f.kind == "L5"))?;
    unplant(page)?;

    gate(ctx, "composer", &found)?;
    ctx.close(app.id)
}

// 3. Text.

/// Every rendered text of the page and of the attributes a person reads or hears, run through T1.
const SCAN: &str = r#"(() => {
  const bad = [];
  const where = (el) => ({ screen: el.closest('[data-screen]')?.dataset.screen ?? 'unknown', surface: el.closest('[data-surface]')?.dataset.surface ?? '',
    element: el.tagName.toLowerCase() + (typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\s+/)[0] : '') });
  const test = (el, text, via) => { const why = window.__faberMojibake(text); if (why) bad.push({ ...where(el), via, why }); };
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  while (walker.nextNode()) { const n = walker.currentNode; if (n.parentElement && !n.parentElement.closest('script,style')) test(n.parentElement, n.nodeValue, 'text'); }
  for (const el of document.querySelectorAll('[aria-label],[title],[placeholder],[alt]'))
    for (const a of ['aria-label', 'title', 'placeholder', 'alt']) if (el.hasAttribute(a)) test(el, el.getAttribute(a), a);
  test(document.documentElement, document.title, 'title');
  return bad;
})()"#;

fn scan(page: &mut Page, config: &str, found: &mut Vec<Finding>) -> Result {
    let bad = page.eval(SCAN)?;
    let list = bad.as_array().ok_or("the text scan returned no list")?;
    for b in list {
        let s = |key: &str| b[key].as_str().unwrap_or_default().to_string();
        found.push(Finding::new("T1", &s("screen"), &s("surface"), &s("element"), config, format!("{} {}", s("via"), s("why"))));
    }
    Ok(())
}

/// A title with every kind of character the app meets, and the answer that came with it.
const SEEDED_ID: &str = "5f1c0000-0000-4000-8000-0000000000a1";
const TITLE: &str = "A\u{e7}\u{e3}o \u{203a} \u{ab}pergunta\u{bb} \u{df} \u{f1} \u{fc} \u{2014} \u{65e5}\u{672c}\u{8a9e}";

fn seed_conversation(ctx: &Ctx) -> Result {
    let id = SEEDED_ID;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let messages = json!([
        { "id": "t-1", "role": "user", "text": format!("Ol\u{e1} \u{203a} {TITLE}"), "at": 0, "status": "done" },
        { "id": "t-2", "role": "assistant", "text": "Resposta com acentua\u{e7}\u{e3}o \u{203a} \u{ab}ok\u{bb}.", "at": 0, "status": "done" }
    ]);
    let body = json!({ "id": id, "title": TITLE, "area": "chat", "sessions": {}, "createdAt": 1, "updatedAt": 1, "messages": messages });
    std::fs::write(folder.join(format!("{id}.json")), body.to_string()).map_err(|e| e.to_string())
}

fn open_seeded(page: &mut Page) -> Result {
    page.click(&css(".list .item").has_text(TITLE))?;
    page.wait_for("the crumb to name the conversation", &format!("document.querySelector('.crumb b')?.textContent === {}", json!(TITLE)))
}

fn text(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    seed_conversation(ctx)?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let mut found = Vec::new();
    // Each language, with the look, size and motion that the all-pairs set pairs it with, at the narrowest and widest
    // widths: the home screen, the conversation with its crumb, the running column and Settings.
    let sweep = [
        Config { width: 1440, height: 900, look: "sala", language: "en", size: 15, motion: "full" },
        Config { width: 1440, height: 900, look: "manha", language: "pt-BR", size: 18, motion: "reduced" },
        Config { width: 1440, height: 900, look: "alto", language: "es", size: 15, motion: "none" },
        Config { width: 1440, height: 900, look: "sala", language: "fr", size: 18, motion: "full" },
        Config { width: 1440, height: 900, look: "manha", language: "de", size: 18, motion: "full" },
    ];
    for c in sweep {
        let page = &mut app.page;
        apply(page, &c)?;
        let config = c.label();
        scan(&mut app.page, &format!("{config}, home"), &mut found)?;
        let page = &mut app.page;
        open_seeded(page)?;
        scan(&mut app.page, &format!("{config}, conversation"), &mut found)?;
        let page = &mut app.page;
        click(page, ".run-btn")?;
        pause(0.5);
        scan(&mut app.page, &format!("{config}, running column"), &mut found)?;
        let page = &mut app.page;
        click(page, ".run-btn")?;
        page.press("Control+,")?;
        let general = page.named("heading", "set.general")?;
        page.wait_visible(&general)?;
        scan(&mut app.page, &format!("{config}, settings"), &mut found)?;
        let page = &mut app.page;
        page.press("Escape")?;
        // The narrow window: the same conversation and its crumb, behind the menu button.
        viewport(page, 720, 480)?;
        let narrow = Config { width: 720, height: 480, ..c }.label();
        scan(&mut app.page, &format!("{narrow}, conversation"), &mut found)?;
        let page = &mut app.page;
        viewport(page, 1440, 900)?;
    }

    // The round trip: a new title goes to the engine, which writes it into the conversation's JSON file; the file is read
    // back as UTF-8, and the screen, loaded again, shows the title whole in the list and in the crumb.
    let page = &mut app.page;
    apply(page, &sweep[0])?;
    let renamed = "Renomeada › ação «ß» ñ ü — 日本語 ✓";
    page.eval(&format!(
        "window.__TAURI_INTERNALS__.invoke('rename', {{ id: {}, title: {} }}).then(() => true)",
        json!(SEEDED_ID),
        json!(renamed)
    ))?;
    let file = ctx.sb.data.join("conversations").join(format!("{SEEDED_ID}.json"));
    let mut stored = String::new();
    super::eventually("the engine to write the new title", std::time::Duration::from_secs(10), || {
        stored = std::fs::read_to_string(&file)
            .ok()
            .and_then(|body| serde_json::from_str::<Value>(&body).ok())
            .and_then(|v| v["title"].as_str().map(String::from))
            .unwrap_or_default();
        stored == renamed
    })
    .map_err(|e| format!("{e}; the file holds the title {stored:?}"))?;
    let page = &mut app.page;
    page.reload()?;
    page.wait_visible(&css(".list .item").has_text(renamed))?;
    page.click(&css(".list .item").has_text(renamed))?;
    page.wait_for("the crumb to show the new title", &format!("document.querySelector('.crumb b')?.textContent === {}", json!(renamed)))?;
    scan(&mut app.page, "round trip, conversation", &mut found)?;

    // The planted twin: the garbled crumb that once reached the screen, put back by hand.
    let page = &mut app.page;
    page.eval("(() => { const s = document.createElement('span'); s.className = 'sep'; s.textContent = '\u{e2}\u{20ac}\u{ba}'; document.body.append(s); return true; })()")?;
    let mut twin = Vec::new();
    scan(&mut app.page, "planted", &mut twin)?;
    caught("a crumb separator garbled into three characters (T1)", twin.iter().any(|f| f.kind == "T1"))?;

    ctx.notes.push(format!("{} garbled texts in {} screens", found.len(), sweep.len() * 5 + 1));
    gate(ctx, "text", &found)?;
    ctx.close(app.id)
}

// 4. Composition.

/// The owner's own screens: two projects, conversations in Chat and in each project, the owner's name and Dark Room.
const OWNED: [(&str, &str, Option<&str>); 4] = [
    ("Quero uma reformulação completa do Iridium", "project", Some("iridium")),
    ("Configure o projeto para atualizar no momento certo", "project", Some("iridium")),
    ("Revisar o teste que falha às vezes", "project", Some("demo")),
    ("Explique a cota em palavras simples", "chat", None),
];

fn seed_owner(ctx: &mut Ctx) -> Result<Launch> {
    let fixture = super::projects_ui::fixture(ctx)?;
    let projects = ctx.sb.data.join("projects.json");
    let mut list: Vec<Value> = serde_json::from_slice(&std::fs::read(&projects).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let folder = ctx.sb.data.join("iridium");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    list.push(json!({ "id": "iridium", "name": "Iridium", "path": folder.to_string_lossy() }));
    std::fs::write(&projects, serde_json::to_vec(&list).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let folder = ctx.sb.data.join("conversations");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis() as u64;
    for (i, (title, area, project)) in OWNED.into_iter().enumerate() {
        let at = now - 3_600_000 * (i as u64 + 1);
        // The first message is not the title, so an open conversation does not show its name a third time by itself.
        let mut body = json!({ "id": format!("o{i}"), "title": title, "area": area, "sessions": {}, "createdAt": at, "updatedAt": at,
            "messages": [{ "id": format!("o{i}-q"), "role": "user", "text": "Uma pergunta.", "at": at, "status": "done" },
                         { "id": format!("o{i}-a"), "role": "assistant", "text": "Uma resposta curta.", "at": at, "status": "done" }] });
        if let Some(project) = project {
            body["projectId"] = json!(project);
        }
        // The conversation the critique opens has an agent that reported its context, so the topbar's meter is in the photo.
        if i == 3 {
            body["context"] = json!({ "used": 150_000, "window": 300_000 });
        }
        std::fs::write(folder.join(format!("o{i}.json")), body.to_string()).map_err(|e| e.to_string())?;
    }
    let mut launch = claude_chat(ctx);
    launch.git_bin = fixture.git_bin;
    Ok(launch)
}

/// The names the person gave that the screens may show: the projects and the conversations.
fn owned_names() -> Vec<String> {
    let mut names: Vec<String> = OWNED.iter().map(|(title, _, _)| (*title).to_string()).collect();
    names.extend(["Demo".to_string(), "Iridium".to_string()]);
    names
}

/// V1, V2 and V3 on the screen as it is now. For V1 the screen is photographed twice without its letters, with and
/// without its art, and the difference under each text is the light the art adds there.
fn compose(page: &mut Page, config: &str, found: &mut Vec<Finding>) -> Result {
    let screen = screen_of(page)?;
    let photo = |page: &mut Page| -> Result<String> {
        let shot = page.cdp.call("Page.captureScreenshot", json!({ "format": "png" }))?;
        Ok(shot["data"].as_str().ok_or("the screenshot had no data")?.to_string())
    };
    if page.eval("window.__sx.hasArt()")? == json!(true) {
        page.eval("window.__sx.hide('text')")?;
        pause(0.1);
        let with = photo(page)?;
        page.eval("window.__sx.hide('text art')")?;
        pause(0.1);
        let without = photo(page)?;
        page.eval("window.__sx.hide('')")?;
        // The letters come back in one step; nothing may still be fading in when the screen is photographed next.
        rest(page)?;
        let measured = page.eval(&format!("window.__sx.light({}, {})", json!(with), json!(without)))?;
        for (element, metric) in sentinel::art_problems(&sentinel::lit(&measured)) {
            found.push(Finding::new("V1", &screen, "art", &element, config, metric));
        }
    }
    let seen: Vec<(String, String)> = page
        .eval("window.__sx.texts()")?
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some((r[0].as_str()?.to_string(), r[1].as_str()?.to_string())))
        .collect();
    let mut labels = owned_names();
    labels.extend(page.eval("window.__sx.headings()")?.as_array().into_iter().flatten().filter_map(|h| h.as_str().map(String::from)));
    for (label, metric) in sentinel::repeated(&seen, &labels) {
        found.push(Finding::new("V2", &screen, "text", &label, config, metric));
    }
    for (grid, metric) in sentinel::uneven(&sentinel::columns(&page.eval("window.__sx.columns()")?)) {
        found.push(Finding::new("V3", &screen, "columns", &grid, config, metric));
    }
    Ok(())
}

/// The planted twins, on a project's home: a line of text over a lit band of art, the project's name three more
/// times, and two columns that end 40 px apart. Each must be found by the same code that judged the screens.
fn composition_twins(page: &mut Page) -> Result {
    page.eval(
        "(() => { const art = document.createElement('div'); art.dataset.sxArt = ''; art.id = 'sx-twin-art';
          art.style.cssText = 'position:fixed;left:700px;top:480px;width:460px;height:64px;background:#fff;z-index:40;pointer-events:none';
          const p = document.createElement('p'); p.id = 'sx-twin-text'; p.textContent = 'Texto legível sobre a arte';
          p.style.cssText = 'position:fixed;left:720px;top:496px;margin:0;font:600 24px/1.3 sans-serif;color:#fff;z-index:41;pointer-events:none';
          document.body.append(art, p);
          for (let i = 0; i < 3; i++) { const s = document.createElement('span'); s.className = 'sx-twin-name'; s.textContent = 'Demo';
            s.style.cssText = `position:fixed;left:${400 + i * 80}px;top:120px;z-index:41`; document.body.append(s); }
          const g = document.createElement('div'); g.id = 'sx-twin-grid';
          g.style.cssText = 'position:fixed;left:400px;top:600px;width:640px;display:grid;grid-template-columns:300px 300px;gap:40px;z-index:41';
          g.innerHTML = '<div><div style=\"height:160px\"></div></div><div><div style=\"height:200px\"></div></div>';
          document.body.append(g); return true; })()",
    )?;
    pause(0.3);
    let mut twin = Vec::new();
    let result = compose(page, "planted", &mut twin);
    page.eval("document.querySelectorAll('#sx-twin-art, #sx-twin-text, .sx-twin-name, #sx-twin-grid').forEach((el) => el.remove()); true")?;
    result?;
    let seen: Vec<String> = twin.iter().map(|f| format!("{} {} {}", f.kind, f.element, f.metric)).collect();
    caught(&format!("a line of text on lit art (V1), found {seen:?}"), twin.iter().any(|f| f.kind == "V1" && f.element == "p"))?;
    caught(
        &format!("the project's name three more times (V2), found {seen:?}"),
        twin.iter().any(|f| f.kind == "V2" && f.element == "demo" && f.metric.contains("span.sx-twin-name")),
    )?;
    caught(
        &format!("two columns that end 40 px apart (V3), found {seen:?}"),
        twin.iter().any(|f| f.kind == "V3" && f.element == "div" && f.metric.contains("40 px")),
    )
}

/// Lets the screen come to rest before it is judged or photographed: the fonts, the lists that animate in, and the
/// black hole's entrance, which lights the scene within three seconds.
fn settle(page: &mut Page) -> Result {
    page.eval("document.fonts.ready.then(() => true)")?;
    pause(3.5);
    rest(page)
}

/// No animation or transition runs, except the endless ones of the look's effects.
fn rest(page: &mut Page) -> Result {
    pause(0.1);
    page.wait_for(
        "the page to rest",
        "document.getAnimations().every((a) => a.effect.getTiming().iterations === Infinity || a.playState !== 'running')",
    )
}

/// The main screens a visual critique looks at (`scripts/shots.mjs`, R172), photographed at the two sizes the owner
/// uses (`critique-<screen>-<width>x<height>.png`) and judged for their composition (V1 to V3).
fn composition(ctx: &mut Ctx) -> Result {
    let launch = seed_owner(ctx)?;
    let mut app = ctx.launch(launch)?;
    // The owner's language and name, saved through the shell's own command as a person would, then the screen again.
    let page = &mut app.page;
    page.eval("window.__TAURI_INTERNALS__.invoke('save_settings', { patch: { language: 'pt-BR', userName: 'Erick' } }).then(() => true)")?;
    page.reload()?;
    page.wait_for("the owner's language", "document.documentElement.lang === 'pt-BR'")?;
    system_motion(&mut app.page, false)?;
    // The effects rest while another app has the focus; the hidden test window never has it on its own.
    super::app::focus(&mut app.page, true)?;
    inject(&mut app.page)?;
    let mut found = Vec::new();
    for (width, height) in [(1920, 1080), (1280, 800)] {
        viewport(&mut app.page, width, height)?;
        let size = format!("{width}x{height}");
        let config = Config { width, height, look: "sala", language: "pt-BR", size: 15, motion: "full" }.label();
        let page = &mut app.page;
        let chat = page.named("button", "area.chat")?.exact();
        page.click(&chat)?;
        // A new conversation, whatever the last turn of the loop left open.
        page.press("Control+n")?;
        page.wait_visible(&css(".hello"))?;
        settle(page)?;
        compose(page, &format!("{config}, chat home"), &mut found)?;
        ctx.shot(&mut app, &format!("critique-chat-home-{size}"))?;

        let page = &mut app.page;
        let projects = page.named("button", "area.projects")?;
        page.click(&projects)?;
        page.click(&css(".prow .item").has_text("Demo"))?;
        page.wait_visible(&css(".welcome-grid .hello"))?;
        settle(page)?;
        compose(page, &format!("{config}, project home"), &mut found)?;
        ctx.shot(&mut app, &format!("critique-project-home-{size}"))?;

        let page = &mut app.page;
        page.click(&chat)?;
        page.click(&css(".list .item").has_text(OWNED[3].0))?;
        page.wait_visible(&css(".col > article.ai"))?;
        settle(page)?;
        compose(page, &format!("{config}, conversation"), &mut found)?;
        ctx.shot(&mut app, &format!("critique-conversation-{size}"))?;

        let page = &mut app.page;
        page.press("Control+,")?;
        let general = page.named("heading", "set.general")?;
        page.wait_visible(&general)?;
        settle(page)?;
        compose(page, &format!("{config}, settings"), &mut found)?;
        ctx.shot(&mut app, &format!("critique-settings-{size}"))?;
        app.page.press("Escape")?;
    }
    let page = &mut app.page;
    viewport(page, 1920, 1080)?;
    let projects = page.named("button", "area.projects")?;
    page.click(&projects)?;
    page.click(&css(".prow .item").has_text("Demo"))?;
    page.wait_visible(&css(".welcome-grid .hello"))?;
    settle(page)?;
    composition_twins(page)?;
    gate(ctx, "composition", &found)?;
    ctx.close(app.id)
}
