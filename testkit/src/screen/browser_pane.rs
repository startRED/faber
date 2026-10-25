//! Native browser proof: real UI input, observed child bounds and separate child-page captures.

use super::{Result, Scenario, ensure, no_cli, same, start_settings};
use crate::app::Ctx;
use crate::page::css;
use base64::Engine as _;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const NAME: &str = "browser pane: native navigation and placement within the memory budget";
pub const PREVIEW: &str = "browser pane: baseline-compatible native preview photographs";
const OPEN_FAILURE: &str = "browser pane: baseline-compatible opening failure photograph";
const CONVERSATION: &str = "5f1c0000-0000-4000-8000-0000000000b1";
const OTHER_CONVERSATION: &str = "5f1c0000-0000-4000-8000-0000000000b2";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: NAME, run: browser_pane },
        Scenario { name: PREVIEW, run: preview_photos },
        Scenario { name: OPEN_FAILURE, run: failure_photo },
        Scenario { name: "control walk: native browser tabs switch, reopen, overflow and close", run: tabs },
        Scenario { name: "browser pane: German opening failure photograph", run: german_failure },
        Scenario { name: "browser pane: German native preview photograph", run: german_preview },
    ]
}

fn failure_photo(ctx: &mut Ctx) -> Result {
    failure_in(ctx, "en", 1280)
}

fn tabs(ctx: &mut Ctx) -> Result {
    let (mut app, fixture) = open_browser(ctx)?;
    let page = &mut app.page;
    enter(page, &fixture.url("/one"))?;
    wait_page(page, &fixture, "one")?;
    page.click(&css(".site-new"))?;
    wait_tabs(page, 2)?;
    enter(page, &fixture.url("/two"))?;
    wait_page(page, &fixture, "two")?;
    page.click(&css(".site-tab:first-child .site-tab-go"))?;
    wait_page(page, &fixture, "one")?;
    page.press_on(&css(".site-address"), "Control+Tab")?;
    wait_page(page, &fixture, "two")?;
    page.click(&css(".site-tab.front .site-tab-close"))?;
    wait_tabs(page, 1)?;
    wait_page(page, &fixture, "one")?;
    page.press_on(&css(".site-address"), "Control+t")?;
    wait_tabs(page, 2)?;
    enter(page, &fixture.url("/two"))?;
    wait_page(page, &fixture, "two")?;
    page.press_on(&css(".site-address"), "Control+Shift+Tab")?;
    wait_page(page, &fixture, "one")?;
    page.press_on(&css(".site-address"), "Control+Tab")?;
    wait_page(page, &fixture, "two")?;
    page.right_click(&css(".site-tab.front .site-tab-go"))?;
    let close_others = page.named("menuitem", "site.closeOthers")?;
    page.click(&close_others)?;
    wait_tabs(page, 1)?;
    wait_page(page, &fixture, "two")?;
    page.press_on(&css(".site-address"), "Control+t")?;
    wait_tabs(page, 2)?;
    enter(page, &fixture.url("/one"))?;
    wait_page(page, &fixture, "one")?;
    page.press_on(&css(".site-address"), "Control+w")?;
    wait_tabs(page, 1)?;
    wait_page(page, &fixture, "two")?;
    page.press_on(&css(".site-address"), "Control+Shift+t")?;
    wait_tabs(page, 2)?;
    wait_page(page, &fixture, "one")?;
    page.wait_visible(&css(".site-tab.front .site-tab-go"))?;
    let at = page.eval("(() => { const r=document.querySelector('.site-tab.front .site-tab-go').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()")?;
    for kind in ["mousePressed", "mouseReleased"] {
        page.cdp.call(
            "Input.dispatchMouseEvent",
            json!({"type":kind,"button":"middle","buttons":if kind=="mousePressed" {4}else{0},"clickCount":1,"x":at["x"],"y":at["y"]}),
        )?;
    }
    wait_tabs(page, 1)?;
    wait_page(page, &fixture, "two")?;
    // The overflow menu uses retained titles; wait for this native page title before moving it into the background.
    page.wait_visible(&css(".site-tab.front .site-tab-go").has_text("Native fixture two"))?;
    for count in 2..=12 {
        page.click(&css(".site-new"))?;
        wait_tabs(page, count)?;
        let response = fixture.request_count("/one") + 1;
        enter(page, &fixture.url("/one"))?;
        wait_page_load(page, &fixture, "one", response)?;
    }
    let all_tabs = page.named("button", "site.allTabs")?;
    page.click(&all_tabs)?;
    page.click(&css("[role=menuitem]").has_text("Native fixture two"))?;
    wait_page(page, &fixture, "two")?;
    ctx.shot(&mut app, "control-browser-tabs-overflow")?;
    app.page.right_click(&css(".site-tab.front .site-tab-go"))?;
    app.page.click(&close_others)?;
    wait_tabs(&mut app.page, 1)?;
    wait_page(&mut app.page, &fixture, "two")?;
    app.page.click(&css(".site-tab.front .site-tab-close"))?;
    app.page.wait_gone(&css(".site-address"))?;
    app.page.wait_for(
        "the last tab to release the native page",
        "window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(p=>p.shown===false)",
    )?;
    ensure(ctx.records()?.is_empty(), || "the browser-tab control walk started a CLI".into())?;
    ctx.close(app.id)
}

fn wait_tabs(page: &mut crate::page::Page, count: usize) -> Result {
    page.wait_for("the real browser tab count", &format!("document.querySelectorAll('.site-tab').length==={count}"))
}

fn german_failure(ctx: &mut Ctx) -> Result {
    failure_in(ctx, "de", 720)
}

fn failure_in(ctx: &mut Ctx, language: &str, width: u32) -> Result {
    let (mut app, fixture) = open_browser_in(ctx, language)?;
    app.page.eval(&format!("window.__TAURI_INTERNALS__.invoke('test_host_size',{{width:{width},height:800}})"))?;
    app.page.wait_for("the native host width", &format!("innerWidth==={width} && innerHeight===800"))?;
    // A failed native open is injected at the command boundary, not presented as a real network failure.
    app.page.eval("window.__TAURI_INTERNALS__.invoke('test_site_refuse_once')")?;
    enter(&mut app.page, &fixture.url("/one"))?;
    app.page.wait_visible(&css("#running [role=alert]"))?;
    app.page.eval("document.fonts.ready.then(()=>true)")?;
    app.page.wait_for(
        "the error composition to stop moving",
        "(() => { document.body.getBoundingClientRect(); return document.getAnimations().every(a=>a.effect.getTiming().iterations===Infinity || a.playState!=='running'); })()",
    )?;
    ctx.shot(&mut app, &format!("browser-open-error-{language}-{width}"))?;
    enter(&mut app.page, &fixture.url("/one"))?;
    if let Err(error) = wait_page(&mut app.page, &fixture, "one") {
        let state = app.page.eval("window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(native=>({native,address:document.querySelector('.site-address')?.value,alert:document.querySelector('#running [role=alert]')?.textContent,empty:!!document.querySelector('.site-empty')}))");
        return Err(format!("{error}; retry state: {state:?}; fixture requests: {:?}; {}", fixture.requests(), ctx.stderr(app.id)));
    }
    app.page.wait_gone(&css("#running [role=alert]"))?;
    ctx.notes.push("one native open was deliberately refused; the same address retried successfully without a second refusal".into());
    ctx.close(app.id)
}

fn open_browser(ctx: &mut Ctx) -> Result<(crate::app::Opened, Fixture)> {
    open_browser_in(ctx, "en")
}

fn open_browser_in(ctx: &mut Ctx, language: &str) -> Result<(crate::app::Opened, Fixture)> {
    start_settings(ctx, language)?;
    super::shell::seed_project(ctx, "Browser proof")?;
    super::shell::seed(ctx, CONVERSATION, "Browser proof", Some("demo"), 1_790_000_000_000)?;
    super::shell::seed(ctx, OTHER_CONVERSATION, "Other fixture", Some("demo"), 1_790_000_000_100)?;
    let fixture = Fixture::start()?;
    let mut app = ctx.launch(no_cli())?;
    let projects = app.page.named("button", "area.projects")?;
    app.page.click(&projects)?;
    app.page.click(&css(".prow .item").has_text("Browser proof"))?;
    app.page.click(&css(".proj-chats .item").has_text("Browser proof"))?;
    app.page.wait_visible(&css("article").has_text("Browser proof: the answer"))?;
    app.page.click(&css(".pane-site"))?;
    app.page.wait_visible(&css(".site-address"))?;
    Ok((app, fixture))
}

// The same UI actions run on the version before the preview notice; no assertion depends on that notice.
fn preview_photos(ctx: &mut Ctx) -> Result {
    let (mut app, fixture) = open_browser(ctx)?;
    enter(&mut app.page, &fixture.url("/one"))?;
    wait_page(&mut app.page, &fixture, "one")?;
    select_preview(&mut app.page)?;
    for (width, height) in [(1280, 800), (720, 800), (1920, 1080)] {
        resize(&mut app.page, width, height)?;
        ctx.shot(&mut app, &format!("browser-preview-{width}"))?;
        let child = probe(&mut app.page, true)?;
        save_picture(ctx, &child, &format!("browser-preview-child-{width}.png"))?;
    }
    ctx.close(app.id)
}

fn german_preview(ctx: &mut Ctx) -> Result {
    let (mut app, fixture) = open_browser_in(ctx, "de")?;
    enter(&mut app.page, &fixture.url("/one"))?;
    wait_page(&mut app.page, &fixture, "one")?;
    select_preview(&mut app.page)?;
    resize(&mut app.page, 720, 800)?;
    ctx.shot(&mut app, "browser-preview-de-720")?;
    ctx.close(app.id)
}

fn select_preview(page: &mut crate::page::Page) -> Result {
    page.click(&css(".site-more"))?;
    page.wait_for(
        "the menu to park the native page it covers",
        "window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(p=>p.shown===false && p.x < -10000)",
    )?;
    let size_label = page.text_with("site.screenSize", &json!({"size":""}), None)?;
    page.click(&css("[role=menuitem]").has_text(size_label.trim()))?;
    page.click(&css("[role=menuitem]").has_text("1920 × 1080"))
}

fn resize(page: &mut crate::page::Page, width: u32, height: u32) -> Result {
    page.eval(&format!("window.__TAURI_INTERNALS__.invoke('test_host_size', {{width:{width},height:{height}}})"))?;
    page.wait_for("the real window to reach its requested size", &format!("innerWidth === {width} && innerHeight === {height}"))?;
    page.wait_for(
        "the child WebView2 to align with the resized site box",
        "window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(p => { const r=document.querySelector('.site-view').getBoundingClientRect(),d=devicePixelRatio; return p.shown && Math.abs(p.x-r.left*d)<=2 && Math.abs(p.y-r.top*d)<=2 && Math.abs(p.width-r.width*d)<=2 && Math.abs(p.height-r.height*d)<=2; })",
    )
}

fn browser_pane(ctx: &mut Ctx) -> Result {
    let (mut app, fixture) = open_browser(ctx)?;
    let page = &mut app.page;
    page.eval("localStorage.setItem('faber.browser-proof.host','keep-host-marker'); true")?;
    super::system_motion(page, false)?;
    let started = Instant::now();
    let cpu_start = ctx.cpu_ms(app.id)?;
    let mut sampled_peak = 0.0;

    ensure(
        page.eval(
            "window.__TAURI_INTERNALS__.invoke('test_host_size',{width:1,height:1}).then(()=>false,e=>String(e).includes('outside'))",
        )? == true,
        || "invalid test host dimensions were not refused".into(),
    )?;

    enter(page, &fixture.url("/one"))?;
    wait_page_load(page, &fixture, "one", 1).map_err(|error| format!("{error}; {}", ctx.stderr(app.id)))?;
    wait_storage(page, "storage: written")?;
    let one = probe(page, true)?;
    same(one["private"].clone(), json!(true), "the observed browser profile must remain InPrivate")?;
    save_picture(ctx, &one, "browser-pane-one.png")?;
    sample_memory(ctx, app.id, &mut sampled_peak)?;
    enter(page, &fixture.url("/two"))?;
    wait_page_load(page, &fixture, "two", 1)?;
    wait_storage(page, "storage: synthetic-conversation-one")?;
    let two = probe(page, true)?;
    save_picture(ctx, &two, "browser-pane-two.png")?;
    ensure(one["picture"] != two["picture"], || "child screenshots for /one and /two were identical".into())?;
    sample_memory(ctx, app.id, &mut sampled_peak)?;

    page.wait_enabled(&css(".site-back"))
        .map_err(|error| format!("{error}; native history: {}", probe(page, false).unwrap_or(Value::Null)))?;
    page.click(&css(".site-back"))?;
    wait_page(page, &fixture, "one")?;
    page.wait_enabled(&css(".site-forward"))?;
    page.click(&css(".site-forward"))?;
    wait_page(page, &fixture, "two")?;
    // At 1280 px the browser folds the sidebar away; close it before choosing another conversation.
    page.click(&css(".site-head button[aria-label='Close the running list']"))?;
    page.wait_gone(&css(".site-address"))?;
    page.wait_visible(&css(".proj-chats .item").has_text("Other fixture"))?;
    page.click(&css(".proj-chats .item").has_text("Other fixture"))?;
    page.wait_visible(&css("article").has_text("Other fixture: the answer"))?;
    page.wait_gone(&css(".site-address"))?;
    page.click(&css(".pane-site"))?;
    page.wait_visible(&css(".site-address"))?;
    let next_load = fixture.request_count("/two") + 1;
    enter(page, &fixture.url("/two"))?;
    wait_page_load(page, &fixture, "two", next_load)?;
    wait_storage(page, "storage: absent")?;
    let other = probe(page, false)?;
    same(other["owner"].clone(), json!(OTHER_CONVERSATION), "the new pane belongs to the selected conversation")?;
    same(other["private"].clone(), json!(true), "the replacement browser remains InPrivate")?;
    same(
        page.eval("localStorage.getItem('faber.browser-proof.host')")?,
        json!("keep-host-marker"),
        "private-pane replacement preserves the host's synthetic storage",
    )?;
    ctx.notes.push("same-origin synthetic localStorage survived navigation within one conversation, was absent in the second conversation, and the main screen's independent synthetic marker remained; no real profile or credentials were read".into());
    let reload_count = fixture.request_count("/two") + 1;
    page.click(&css(".site-reload"))?;
    wait_page_load(page, &fixture, "two", reload_count)?;

    select_preview(page)?;
    page.wait_visible(&css(".site-preview"))?;
    page.wait_for("the native page to finish applying its selected viewport",
        "window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(p => p.shown && p.page?.width===1920 && p.page?.height===1080)")?;
    let sized = probe(page, false)?;
    same(sized["page"]["width"].clone(), json!(1920), "the selected native browser page width")?;
    same(sized["page"]["height"].clone(), json!(1080), "the selected native browser page height")?;
    page.click(&css(".site-fit"))?;
    page.wait_gone(&css(".site-preview"))?;

    page.click(&css(".site-wide"))?;
    page.wait_for("the browser column to widen", "document.querySelector('.runcol')?.classList.contains('wide')")?;
    page.click(&css(".site-wide"))?;
    page.wait_for("the browser column to return", "!document.querySelector('.runcol')?.classList.contains('wide')")?;

    for (width, height) in [(720, 800), (1280, 800), (1920, 1080)] {
        resize(page, width, height)?;
        let bounds = page.eval("(() => { const r=document.querySelector('.site-view').getBoundingClientRect(); const d=devicePixelRatio; return {x:r.left*d,y:r.top*d,width:r.width*d,height:r.height*d,dpr:d}; })()")?;
        let native = probe(page, false)?;
        same(native["shown"].clone(), json!(true), "child remains shown at every physical window size")?;
        for (key, wanted) in [
            ("x", bounds["x"].as_f64().unwrap_or_default()),
            ("y", bounds["y"].as_f64().unwrap_or_default()),
            ("width", bounds["width"].as_f64().unwrap_or_default()),
            ("height", bounds["height"].as_f64().unwrap_or_default()),
        ] {
            let actual = native[key].as_f64().ok_or_else(|| format!("native probe omitted {key}: {native}"))?;
            ensure((actual - wanted).abs() <= 2.0, || {
                format!("at {width}×{height}, native {key}={actual:.1}px but site box × DPR is {wanted:.1}px")
            })?;
        }
        ctx.notes.push(format!(
            "native browser placement at {width}×{height} logical px: child rect {:.0},{:.0} {:.0}×{:.0} physical px; screen DPR {:.2}",
            native["x"].as_f64().unwrap_or_default(),
            native["y"].as_f64().unwrap_or_default(),
            native["width"].as_f64().unwrap_or_default(),
            native["height"].as_f64().unwrap_or_default(),
            bounds["dpr"].as_f64().unwrap_or(1.0)
        ));
    }

    for width in [720, 1920] {
        resize(page, width, 800)?;
        // Native resize intentionally suppresses travel for 250 ms; exercise a close after that resize has settled.
        page.eval("new Promise(resolve=>setTimeout(resolve,300))")?;
        page.eval(
            r#"(() => {
          window.__browserCloseTrace = new Promise(resolve => {
            const samples = [], end = performance.now() + 800;
            const frame = async () => {
              const column = document.querySelector('.runcol');
              const box = column?.getBoundingClientRect();
              const opacity = column ? Number(getComputedStyle(column).opacity) : null;
              const p = await window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false});
              const after=document.querySelector('.runcol')?.getBoundingClientRect();
          samples.push({x:box?.x,afterX:after?.x,viewport:innerWidth,opacity,shown:p.shown,nativeX:p.x});
              if (performance.now()<end) requestAnimationFrame(frame); else resolve(samples);
            };
            requestAnimationFrame(frame);
          });
          return true;
        })()"#,
        )?;
        reopen_snapshot(ctx, page, width, "before")?;
        page.click(&css(".site-head button[aria-label='Close the running list']"))?;
        page.wait_gone(&css(".site-address"))?;
        let trace = page.eval("window.__browserCloseTrace")?;
        let samples = trace.as_array().ok_or("native browser close trace was not an array")?;
        let visible: Vec<_> = samples
            .iter()
            .filter(|sample| {
                sample["x"].as_f64().is_some_and(|x| {
                    x + 24.0 < sample["viewport"].as_f64().unwrap_or_default()
                        && sample["afterX"].as_f64().is_some_and(|after| after + 24.0 < sample["viewport"].as_f64().unwrap_or_default())
                })
            })
            .collect();
        ensure(!visible.is_empty(), || format!("no visible close sample captured at {width}px: {trace}"))?;
        ensure(visible.iter().all(|sample| sample["shown"] == true && sample["nativeX"].as_f64().is_some_and(|x| x > -10000.0)), || {
            format!("native page parked while its departing column was still visible at {width}px: {trace}")
        })?;
        let xs: Vec<f64> = samples
            .iter()
            .filter(|sample| sample["shown"] == true)
            .filter_map(|sample| sample["nativeX"].as_f64())
            .filter(|x| *x > -10000.0)
            .collect();
        let left = xs.iter().copied().fold(f64::INFINITY, f64::min);
        let right = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let intermediate = xs.iter().filter(|x| **x > left + 2.0 && **x < right - 2.0).count();
        ensure(intermediate > 0, || format!("the native page jumped instead of following close motion: {trace}"))?;
        ensure(samples.iter().filter_map(|sample| sample["opacity"].as_f64()).all(|opacity| opacity >= 0.99), || {
            format!("the closing column faded instead of sliding: {trace}")
        })?;
        std::fs::write(ctx.paths.target.join(format!("browser-close-motion-{width}.json")), trace.to_string())
            .map_err(|e| e.to_string())?;
        ctx.notes.push(format!("native browser close at {width}px: {intermediate} intermediate native positions in {} observations, column opacity stayed at least 0.99; driver sampling, not an FPS benchmark", samples.len()));
        page.wait_for(
            "the closed browser pane to park",
            "window.__TAURI_INTERNALS__.invoke('test_site_probe', {capture:false}).then(p => p.shown === false && p.x < -10000 && p.y < -10000)",
        )?;
        page.click(&css(".pane-site"))?;
        page.wait_visible(&css(".site-address"))?;
        reopen_snapshot(ctx, page, width, "after")?;
        wait_page(page, &fixture, "two")?;
    }
    ensure(ctx.records()?.is_empty(), || "the browser scenario started a CLI".into())?;
    sample_memory(ctx, app.id, &mut sampled_peak)?;
    let cpu_ms = ctx.cpu_ms(app.id)? - cpu_start;
    ctx.notes.push(format!(
        "browser scenario sampled peak: app tree private working set {sampled_peak:.1} MiB, highest of three samples (WebView2 present at each sample); CPU time {cpu_ms:.0} ms between readings over {:.1} s wall time; not a peak CPU or performance benchmark",
        started.elapsed().as_secs_f64()
    ));
    let minimize = page.named("button", "window.minimize")?;
    page.click(&minimize)?;
    // CDP focus emulation normally keeps an offscreen document visible even after native minimization.
    page.cdp.call("Emulation.setFocusEmulationEnabled", json!({"enabled": false}))?;
    page.wait_for(
        "the native host to report minimized",
        "window.__TAURI_INTERNALS__.invoke('test_site_probe',{capture:false}).then(p=>p.hostMinimized===true)",
    )?;
    let document_hidden = page.eval("document.hidden")?;
    let hidden_cpu = ctx.cpu_ms(app.id)?;
    let hidden_started = Instant::now();
    super::pause(5.0);
    let hidden_wall = hidden_started.elapsed().as_secs_f64() * 1000.0;
    let hidden_cpu = ctx.cpu_ms(app.id)? - hidden_cpu;
    let (hidden_mb, _) = ctx.memory_mb(app.id)?;
    ensure(hidden_mb <= 400.0, || format!("minimized browser app sample {hidden_mb:.1} MiB exceeds 400 MiB"))?;
    ctx.notes.push(format!("real titlebar Minimize made native is_minimized=true; offscreen CDP document.hidden={document_hidden}; minimized app tree sample {hidden_mb:.1} MiB, {hidden_cpu:.0} ms CPU over {hidden_wall:.0} ms wall; five-second static fixture sample, not a continuous peak or a zero-cost claim"));
    ctx.close(app.id)
}

fn reopen_snapshot(ctx: &Ctx, page: &mut crate::page::Page, width: u32, phase: &str) -> Result {
    let snapshot = page.eval(&format!("Promise.all([window.__TAURI_INTERNALS__.invoke('site_tabs',{{id:{}}}),window.__TAURI_INTERNALS__.invoke('test_site_probe',{{capture:false}})]).then(([tabs,native])=>({{tabs,native,address:document.querySelector('.site-address')?.value,strip:[...document.querySelectorAll('.site-tab')].map(t=>({{text:t.textContent,front:t.classList.contains('front')}}))}}))", json!(OTHER_CONVERSATION)))?;
    std::fs::write(ctx.paths.target.join(format!("browser-reopen-{phase}-{width}.json")), snapshot.to_string()).map_err(|e| e.to_string())
}
fn enter(page: &mut crate::page::Page, url: &str) -> Result {
    page.fill(&css(".site-address"), url)?;
    page.press_on(&css(".site-address"), "Enter")
}

fn wait_page(page: &mut crate::page::Page, fixture: &Fixture, name: &str) -> Result {
    let expected_url = fixture.url(&format!("/{name}"));
    page.wait_for(
        &format!("the child WebView2 to show {name}"),
        &format!(
            "window.__TAURI_INTERNALS__.invoke('test_site_probe', {{capture:false}}).then(p => document.querySelector('.site-address')?.value === {} && !document.querySelector('.site-empty') && p.shown && p.page?.url === {} && p.page?.title === {} && p.page?.text.includes({})).catch(e=>{{if(String(e)==='the browser closed'||String(e)==='the test browser closed')return false;throw e;}})",
            json!(expected_url), json!(expected_url), json!(format!("Native fixture {name}")), json!(format!("Native page {name}"))
        ),
    )?;
    let p = probe(page, false)?;
    same(p["page"]["title"].clone(), json!(format!("Native fixture {name}")), "native child document title")?;
    ensure(p["page"]["text"].as_str().is_some_and(|text| text.contains(&format!("Native page {name}"))), || {
        format!("native child document text did not contain expected page content: {}", p["page"]["text"])
    })
}

fn wait_page_load(page: &mut crate::page::Page, fixture: &Fixture, name: &str, count: usize) -> Result {
    let marker = format!("Native page {name} · load {count}");
    let url = fixture.url(&format!("/{name}"));
    page.wait_for(
        &format!("the child WebView2 to display {name} response {count}"),
        &format!(
            "window.__TAURI_INTERNALS__.invoke('test_site_probe',{{capture:false}}).then(p => document.querySelector('.site-address')?.value === {} && !document.querySelector('.site-empty') && p.shown && p.page?.url === {} && p.page?.title === {} && p.page?.text.includes({})).catch(e=>{{if(String(e)==='the browser closed'||String(e)==='the test browser closed')return false;throw e;}})",
            json!(url), json!(url), json!(format!("Native fixture {name}")), json!(marker)
        ),
    )?;
    ensure(fixture.request_count(&format!("/{name}")) >= count, || {
        format!("fixture did not receive /{name} response {count}: {:?}", fixture.requests())
    })
}

fn wait_storage(page: &mut crate::page::Page, expected: &str) -> Result {
    page.wait_for(
        "the synthetic storage marker in the native page",
        &format!(
            "window.__TAURI_INTERNALS__.invoke('test_site_probe',{{capture:false}}).then(p=>p.page?.text.includes({})).catch(e=>{{if(String(e)==='the browser closed'||String(e)==='the test browser closed')return false;throw e;}})",
            json!(expected)
        ),
    )
}

fn sample_memory(ctx: &mut Ctx, id: usize, peak: &mut f64) -> Result {
    let (mb, webview) = ctx.memory_mb(id)?;
    ensure(webview, || "the browser-pane memory sample must include WebView2".into())?;
    *peak = peak.max(mb);
    ensure(*peak <= 400.0, || format!("browser-pane sampled peak {peak:.1} MB exceeds 400 MB"))
}

fn probe(page: &mut crate::page::Page, capture: bool) -> Result<Value> {
    page.eval(&format!("window.__TAURI_INTERNALS__.invoke('test_site_probe', {{capture:{capture}}})"))
}

fn save_picture(ctx: &Ctx, probe: &Value, name: &str) -> Result {
    let data = probe["picture"].as_str().ok_or("native child screenshot missing")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    ensure(!bytes.is_empty() && bytes.len() <= 1_000_000, || {
        format!("child screenshot size is outside the 1 MB evidence limit: {} bytes", bytes.len())
    })?;
    std::fs::create_dir_all(&ctx.paths.shots).map_err(|e| e.to_string())?;
    std::fs::write(ctx.paths.shots.join(name), bytes).map_err(|e| e.to_string())
}

struct Fixture {
    origin: String,
    requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    fn start() -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let origin = format!("http://{}", listener.local_addr().map_err(|e| e.to_string())?);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_requests, thread_stop) = (requests.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => serve(stream, &thread_requests),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
                    Err(_) => break,
                }
            }
        });
        Ok(Self { origin, requests, stop, thread: Some(thread) })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin)
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn request_count(&self, path: &str) -> usize {
        self.requests().iter().filter(|request| request.as_str() == path).count()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve(mut stream: TcpStream, requests: &Mutex<Vec<String>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut raw = [0u8; 4096];
    let Ok(count) = stream.read(&mut raw) else { return };
    let request = String::from_utf8_lossy(&raw[..count]);
    let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");
    let mut log = requests.lock().unwrap_or_else(|e| e.into_inner());
    if log.len() >= 256 {
        return;
    }
    log.push(path.to_string());
    drop(log);
    let load = requests.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|p| p.as_str() == path).count();
    let (status, title, body, color, link) = match path {
        "/one" => ("200 OK", "Native fixture one", "Native page one", "#f5b942", "/two"),
        "/two" => ("200 OK", "Native fixture two", "Native page two", "#397fbd", "/one"),
        _ => ("404 Not Found", "Not found", "Not found", "#ddd", "/one"),
    };
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><style>body{{font:24px sans-serif;padding:32px;background:{color}}}</style><h1>{body} · load {load}</h1><p id=storage-proof></p><script>if(location.pathname==='/one'){{localStorage.setItem('faber-browser-proof','synthetic-conversation-one');document.querySelector('#storage-proof').textContent='storage: written'}}else{{document.querySelector('#storage-proof').textContent='storage: '+(localStorage.getItem('faber-browser-proof')??'absent')}}</script><a href='{link}'>Next page</a>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}
