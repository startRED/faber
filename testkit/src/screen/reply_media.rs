//! Native reply playback, lifetime and budget with the files from a manual media capture.
use super::{Result, Scenario, claude_chat, ensure, no_cli, pause, same, start_settings};
use crate::app::{Ctx, Opened};
use crate::page::{Page, css};
use serde_json::{Value, json};
use std::time::Instant;

pub const NAME: &str = "reply media plays, releases on scroll and stays within the memory budget";

pub fn scenario() -> Scenario {
    Scenario { name: NAME, run: reply_media }
}

fn write_media_files(work: &std::path::Path) -> Result {
    for (file, bytes) in [
        ("picture1.png", &include_bytes!("../../../tests/fixtures/media/reply/picture1.png")[..]),
        ("picture2.png", &include_bytes!("../../../tests/fixtures/media/reply/picture2.png")[..]),
        ("clip.mp4", &include_bytes!("../../../tests/fixtures/media/reply/clip.mp4")[..]),
        ("tone.wav", &include_bytes!("../../../tests/fixtures/media/reply/tone.wav")[..]),
    ] {
        std::fs::write(work.join(file), bytes).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn reply_media(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let folder = ctx.sb.data.join("conversations");
    let work = ctx.sb.data.join("reply-media-work");
    for dir in [&folder, &work] {
        std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    }
    let work = work.canonicalize().map_err(|error| error.to_string())?;
    let tail = "\n\nMore text below the media.".repeat(100);
    let control = format!(
        r#"<div class="control-gallery" style="min-height:240px"><div>Pattern</div><div>Gradient</div></div>

<div class="control-video" style="min-height:420px">Clip</div>

<div class="control-audio" style="min-height:65px">Tone</div>

See the clip for details.{tail}"#
    );
    seed(&folder, &work, "reply-control", &control)?;
    let mut control_app = ctx.launch(no_cli())?;
    let (control, _, control_dom) = measure(ctx, &mut control_app, "reply-control", false)?;
    ctx.close(control_app.id)?;
    drop(control_app);
    std::fs::remove_file(folder.join("reply-control.json")).map_err(|error| error.to_string())?;

    write_media_files(&work)?;

    let media = format!(
        "![Pattern](picture1.png)\n\n![Gradient](picture2.png)\n\n[Clip](clip.mp4)\n\n[Tone](tone.wav)\n\nSee [the clip](clip.mp4) for details.{tail}"
    );
    seed(&folder, &work, "reply-play", &media)?;
    let mut app = ctx.launch(no_cli())?;
    let (media, midpoint_dom, media_dom) = measure(ctx, &mut app, "reply-play", true)?;
    for (index, (media, control)) in media.iter().zip(&control).enumerate() {
        let delta = media.heap - control.heap;
        ensure(delta <= 15.0, || format!("cycle {}: closed JS heap is {delta:.1} MiB above control", index + 1))?;
    }
    let window = &media[CYCLES - 10..];
    let differential = |index: usize| media[index].private - control[index].private;
    let increase = differential(CYCLES - 1) - differential(CYCLES - 10);
    let gpu_min = window.iter().map(|sample| sample.gpu).fold(f64::INFINITY, f64::min);
    let gpu_max = window.iter().map(|sample| sample.gpu).fold(0.0, f64::max);
    ctx.notes.push(format!("last ten closes: private increase above control {increase:.1} MiB, GPU {gpu_min:.1}–{gpu_max:.1} MiB"));
    ensure(increase <= 15.0 && gpu_max - gpu_min <= 15.0, || {
        format!(
            "last ten closes exceeded 15 MiB: differential private net increase {increase:.1} MiB, GPU range {gpu_min:.1}–{gpu_max:.1} MiB"
        )
    })?;
    app.page.click(&css(".list .item").has_text("Reply playback"))?;
    app.page.wait_count(&css(".reply-media-gallery figure"), 2)?;
    app.page.reload()?;
    app.page.wait_count(&css(".reply-media-gallery figure"), 2)?;
    // The memory comparison needs no provider. Use a recorded provider for the composer layout check.
    ctx.close(app.id)?;
    drop(app);
    let mut app = ctx.launch(claude_chat(ctx))?;
    app.page.click(&css(".list .item").has_text("Reply playback"))?;
    app.page.wait_count(&css(".reply-media-gallery figure"), 2)?;
    narrow_reply(&mut app.page)?;
    // WebView2 can initialize documents late, so compare the same measured window as memory.
    ensure(media_dom[0] == midpoint_dom[0] && media_dom[1] <= control_dom[1] + 100 && media_dom[2] <= control_dom[2] + 5, || {
        let evidence = ctx
            .notes
            .iter()
            .filter(|note| note.contains("measured cycle") || note.contains(": sampled peak ") || note.starts_with("last ten closes"))
            .cloned()
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "closed DOM [documents, nodes, listeners]: measured cycle 10 media {midpoint_dom:?}, final media {media_dom:?}, control {control_dom:?}; {evidence}"
        )
    })
}

fn narrow_reply(page: &mut Page) -> Result {
    page.wait_visible(&css(".composer .send"))?;
    for height in [520, 480] {
        page.cdp.call("Emulation.setDeviceMetricsOverride", json!({"width":720,"height":height,"deviceScaleFactor":1,"mobile":false}))?;
        page.wait_for(
            &format!("the conversation at 720×{height} has no horizontal overflow and Send is reachable"),
            &format!(
                r#"(() => {{
                const col = document.querySelector('.thread > .col');
                const button = document.querySelector('.composer .send');
                const send = button?.getBoundingClientRect();
                return innerWidth === 720 && innerHeight === {height} && !!col && col.scrollWidth === col.clientWidth
                    && !!send?.width && send.left >= 0 && send.right <= innerWidth && send.top >= 0 && send.bottom <= innerHeight
                    && button.contains(document.elementFromPoint(send.x + send.width / 2, send.y + send.height / 2));
            }})()"#
            ),
        )?;
    }
    Ok(())
}

const CYCLES: usize = 20;
const WARM_CYCLES: usize = 3;
const MEDIA_NODES: &str = ".reply-media-shell :is(img, video, audio)";
// Chromium can report the previous URL in currentSrc after load() resets the detached player.
// Check that the old resource has no loaded data and there is no active load or playback.
const RELEASED: &str = r#"window.__s5Media.length === 4 && window.__s5Media.every(el =>
    !el.hasAttribute('src') && (el.tagName === 'IMG' ? !el.currentSrc :
        el.paused && el.readyState === HTMLMediaElement.HAVE_NOTHING && el.networkState !== HTMLMediaElement.NETWORK_LOADING))"#;
const MEDIA_STATE: &str = r#"window.__s5Media.map(el => ({tag:el.tagName, connected:el.isConnected,
    src:el.getAttribute('src'), currentSrc:el.currentSrc, paused:el.paused, readyState:el.readyState, networkState:el.networkState}))"#;
const AT_TAIL: &str = r#"(() => {
    const t = document.querySelector('.thread');
    return t.scrollTop > 1000 && t.scrollHeight - t.scrollTop - t.clientHeight < 2;
})()"#;

fn seed(folder: &std::path::Path, work: &std::path::Path, id: &str, text: &str) -> Result {
    for (id, title, text) in [(id, "Reply playback", text), ("reply-away", "Another conversation", "No media here.")] {
        let conversation = json!({
            "id": id, "title": title, "area": "chat", "sessions": {}, "workingFolder": work,
            "createdAt": 1, "updatedAt": 2,
            "messages": [{"id":"answer", "role":"assistant", "text":text, "at":2, "status":"done"}]
        });
        std::fs::write(folder.join(format!("{id}.json")), conversation.to_string()).map_err(|error| error.to_string())?;
    }
    Ok(())
}

struct ClosedSample {
    private: f64,
    gpu: f64,
    heap: f64,
}

fn measure(ctx: &mut Ctx, app: &mut Opened, id: &str, media: bool) -> Result<(Vec<ClosedSample>, [u64; 3], [u64; 3])> {
    let play = css(".list .item").has_text("Reply playback");
    let away = css(".list .item").has_text("Another conversation");
    let label = if media { "media" } else { "control" };
    let mut samples = Vec::with_capacity(CYCLES);
    let mut midpoint_dom = None;
    let mut final_dom = None;
    let mut peak = 0.0_f64;
    for cycle in 1..=WARM_CYCLES + CYCLES {
        app.page.click(&play)?;
        if media {
            app.page.wait_count(&css(".reply-media-gallery figure"), 2)?;
        } else {
            app.page.wait_count(&css(".control-gallery > div"), 2)?;
            app.page.wait_count(&css(".reply-media-shell"), 0)?;
        }
        let opened = private_under_cap(ctx, app, label, cycle, "open")?;
        wheel_to_start(&mut app.page)?;
        if media {
            play_media(&mut app.page)?;
        } else {
            scroll_to(&mut app.page, ".control-video")?;
            scroll_to(&mut app.page, ".control-audio")?;
        }
        let played = private_under_cap(ctx, app, label, cycle, "play")?;

        app.page.eval("(() => {const t=document.querySelector('.thread');t.scrollTo({top:t.scrollHeight,behavior:'instant'});})()")?;
        app.page.wait_for("scroll returns to the tail", AT_TAIL)?;
        if media {
            app.page.wait_for("no sourced media remain in the reply", &format!("!document.querySelector('{MEDIA_NODES}[src]')"))?;
            if let Err(error) = app.page.wait_for("all four retained media elements release off screen", RELEASED) {
                return Err(format!("cycle {cycle}: {error}; {}", app.page.eval(MEDIA_STATE)?));
            }
            if cycle == 1 {
                let still_open = app.page.eval(&lease_probe(id))?;
                ensure(closed_lease(still_open.clone()).is_err(), || "the close check accepted a lease that was left open".into())?;
                same(still_open, json!("error.media.path"), "the offscreen reply still owns its lease")?;
                ctx.notes.push("negative control: the host close check rejects a lease left open".into());
            }
        }
        app.page.click(&away)?;
        app.page.wait_visible(&css(".thread article.ai").has_text("No media here."))?;
        if media {
            app.page.wait_count(&css(".reply-media-shell"), 0)?;
            app.page.wait_count(&css(MEDIA_NODES), 0)?;
            app.page.wait_for(
                "the host refuses a grant for the departed conversation",
                &format!("({}).then(key => key === 'error.media.closed')", lease_probe(id)),
            )?;
            closed_lease(app.page.eval(&lease_probe(id))?)?;
            same(app.page.eval(RELEASED)?, json!(true), "the retained elements stay released after closing")?;
            app.page.eval("delete window.__s5Media")?;
        }

        let checkpoint = cycle.checked_sub(WARM_CYCLES);
        let repeated = cycle == 1 || matches!(checkpoint, Some(10)) || checkpoint == Some(CYCLES) || (!media && checkpoint == Some(1));
        let dom = if repeated {
            Some(settled_dom(&mut app.page)?)
        } else {
            app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
            None
        };
        if checkpoint == Some(CYCLES) {
            final_dom = dom;
        }
        if checkpoint == Some(10) {
            midpoint_dom = dom;
        }
        let heap = app.page.cdp.call("Runtime.getHeapUsage", json!({}))?["usedSize"].as_f64().ok_or("missing renderer JS heap size")?
            / 1_048_576.0;
        let roles = app.page.cdp.process_roles()?;
        let gpu_pid = roles["processInfo"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["type"].as_str().is_some_and(|kind| kind.eq_ignore_ascii_case("gpu"))))
            .and_then(|row| row["id"].as_u64())
            .ok_or("missing GPU process in the fixture browser")?;
        let processes = crate::proctree::private_processes(app.pid)?;
        let gpu = processes
            .iter()
            .find(|(row, _)| u64::from(row.pid) == gpu_pid)
            .map(|(_, mib)| *mib)
            .ok_or("the fixture GPU process was absent from the memory sample")?;
        let private = processes.iter().map(|(_, mib)| mib).sum::<f64>();
        peak = peak.max(opened).max(played).max(private);
        let reading =
            format!("{label} cycle {cycle}: open {opened:.1}, play {played:.1}, close {private:.1}, GPU {gpu:.1}, JS heap {heap:.2} MiB");
        ctx.notes.push(reading.clone());
        if let Some(dom) = dom {
            let point = checkpoint.map_or_else(|| "first closed cycle".into(), |cycle| format!("measured cycle {cycle}"));
            ctx.notes.push(format!("{label} {point}: closed DOM [documents, nodes, listeners] {dom:?}, JS heap {heap:.2} MiB"));
        }
        ensure(private <= 400.0, || reading)?;
        if cycle > WARM_CYCLES {
            samples.push(ClosedSample { private, gpu, heap });
        }
    }
    let idle_before = ctx.cpu_ms(app.id)?;
    let idle = Instant::now();
    pause(2.0);
    let idle_cpu = (ctx.cpu_ms(app.id)? - idle_before) / idle.elapsed().as_secs_f64() / 10.0;
    let retained = private_under_cap(ctx, app, label, WARM_CYCLES + CYCLES, "idle")?;
    ctx.notes.push(format!("{label}: sampled peak {:.1} MiB, idle {retained:.1} MiB, CPU {idle_cpu:.1}% of one core", peak.max(retained)));
    ensure(!media || idle_cpu < 5.0, || format!("reply media idle CPU {idle_cpu:.1}%, limit 5%"))?;
    Ok((
        samples,
        midpoint_dom.ok_or("missing measured cycle 10 DOM measurement")?,
        final_dom.ok_or("missing final closed DOM measurement")?,
    ))
}

fn settled_dom(page: &mut Page) -> Result<[u64; 3]> {
    let mut previous = None;
    for _ in 0..5 {
        page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
        let dom = page.cdp.call("Memory.getDOMCounters", json!({}))?;
        let counts = [
            dom["documents"].as_u64().ok_or("missing renderer document count")?,
            dom["nodes"].as_u64().ok_or("missing renderer node count")?,
            dom["jsEventListeners"].as_u64().ok_or("missing renderer listener count")?,
        ];
        if previous == Some(counts) {
            return Ok(counts);
        }
        previous = Some(counts);
    }
    Err(format!("closed DOM did not stabilize after five collections; last counts {previous:?}"))
}

fn private_under_cap(ctx: &mut Ctx, app: &Opened, label: &str, cycle: usize, stage: &str) -> Result<f64> {
    let (mib, renderer) = ctx.memory_mb(app.id)?;
    ensure(renderer && mib <= 400.0, || {
        format!("{label} cycle {cycle} {stage}: {mib:.1} MiB, renderer present: {renderer} (cap 400 MiB)")
    })?;
    Ok(mib)
}

fn wheel_to_start(page: &mut Page) -> Result {
    page.wait_for(
        "reply tail and viewport settle",
        &format!(
            r#"(async () => {{
        const t = document.querySelector('.thread');
        const view = () => [t.scrollTop, t.scrollHeight, t.clientHeight, t.clientWidth].join();
        const before = view();
        await new Promise(done => setTimeout(done, 250));
        return t.isConnected && {AT_TAIL} && before === view();
    }})()"#
        ),
    )?;
    let point = page.eval(
        "(() => {const r = document.querySelector('.thread').getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2};})()",
    )?;
    page.cdp.call("Input.dispatchMouseEvent", json!({"type":"mouseWheel","x":point["x"],"y":point["y"],"deltaX":0,"deltaY":-100000}))?;
    page.wait_for("trusted scroll reaches the start of the reply", "document.querySelector('.thread').scrollTop < 2")
}

fn scroll_to(page: &mut Page, selector: &str) -> Result {
    page.eval(&format!("document.querySelector('{selector}').scrollIntoView({{block:'center',behavior:'instant'}})"))?;
    Ok(())
}

fn play_media(page: &mut Page) -> Result {
    page.wait_for(
        "both gallery photos are decoded in the viewport",
        r#"(() => {
        const images = [...document.querySelectorAll('.reply-media-gallery img')];
        const thread = document.querySelector('.thread').getBoundingClientRect();
        if (images.length !== 2 || !images.every(img => {
            const r = img.getBoundingClientRect();
            return img.complete && img.naturalWidth > 0 && r.width > 0 && r.height > 0
                && r.bottom > thread.top && r.top < thread.bottom;
        })) return false;
        window.__s5Media = images;
        return true;
    })()"#,
    )?;
    for tag in ["video", "audio"] {
        let position = if tag == "audio" { "nearest" } else { "center" };
        page.eval(&format!(
            "document.querySelector('figure.reply-media-{tag}').scrollIntoView({{block:'{position}',behavior:'instant'}})"
        ))?;
        let selector = format!(".reply-media-{tag} {tag}");
        page.wait_for("player source and metadata", &format!("document.querySelector('{selector}')?.readyState >= 1"))?;
        let playing = page.eval(&format!(
            r#"(async () => {{
            const player = document.querySelector('{selector}');
            window.__s5Media.push(player);
            player.muted = true;
            if ('{tag}' === 'audio') {{
                await player.play();
                return true;
            }}
            const presented = new Promise(resolve => {{
                const frame = player.requestVideoFrameCallback((_, info) => {{
                    clearTimeout(timer);
                    resolve(info.width > 0 && info.height > 0);
                }});
                const timer = setTimeout(() => {{
                    player.cancelVideoFrameCallback(frame);
                    resolve(false);
                }}, 4000);
            }});
            await player.play();
            return presented;
        }})()"#
        ))?;
        same(playing, json!(true), "the player starts and the video presents a decoded frame")?;
        if tag == "audio" {
            page.wait_for(
                "audio advances and the first player pauses",
                "window.__s5Media[3].currentTime > 0 && window.__s5Media[2].paused",
            )?;
        }
    }
    page.wait_for(
        "all live reply media nodes are tracked",
        &format!("[...document.querySelectorAll('{MEDIA_NODES}')].every(el => window.__s5Media.includes(el))"),
    )
}

fn lease_probe(id: &str) -> String {
    // A path refused by an open lease gives a probe that cannot create grants while close is in flight.
    format!(
        r#"(() => {{
        const args = {{id:{}, path:'../clip.mp4'}};
        const shell = window.faberShell ?? window.__TAURI_INTERNALS__;
        return shell.invoke('media_grant', args).then(() => 'allowed', err => err.key ?? err);
    }})()"#,
        json!(id)
    )
}

fn closed_lease(key: Value) -> Result {
    same(key, json!("error.media.closed"), "the conversation has no active media lease or grants")
}
