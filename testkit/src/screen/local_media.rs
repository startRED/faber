//! The same native video element and host commands run against either shell.
use super::{Result, Scenario, ensure, no_cli, pause, same};
use crate::app::Ctx;
use crate::page::Page;
use serde_json::json;
use std::time::Instant;

pub fn scenario() -> Scenario {
    Scenario { name: "local media plays and seeks, then releases its source and grant", run: local_media }
}

fn local_media(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "tutorialDone": true }))?;
    let folder = ctx.sb.data.join("conversations");
    let working = ctx.sb.data.join("local-media-work");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&working).map_err(|e| e.to_string())?;
    let working = working.canonicalize().map_err(|e| e.to_string())?;
    std::fs::write(working.join("seek.webm"), include_bytes!("../../../tests/fixtures/media/seek.webm")).map_err(|e| e.to_string())?;
    std::fs::write(working.join("wrong.webm"), b"<html>Not a video</html>").map_err(|e| e.to_string())?;
    let conversation = json!({
        "id": "local-media", "title": "Local media", "area": "chat", "sessions": {}, "workingFolder": working,
        "createdAt": 1, "updatedAt": 2, "messages": []
    });
    std::fs::write(folder.join("local-media.json"), conversation.to_string()).map_err(|e| e.to_string())?;
    let mut app = ctx.launch(no_cli())?;
    let playback = r#"
        window.mediaInvoke = (command, args) => window.faberShell
            ? window.faberShell.invoke(command, args) : window.__TAURI_INTERNALS__.invoke(command, args);
        window.mediaCheck = async () => {
            const invoke = window.mediaInvoke;
            const id = 'local-media';
            const lease = await invoke('media_enter', { id });
            const url = await invoke('media_grant', { id, path: 'seek.webm' });
            const video = document.createElement('video');
            document.body.append(video);
            video.muted = true;
            const wait = (name) => new Promise((resolve, reject) => {
                const timeout = setTimeout(() => reject(new Error('No video event: ' + name)), 6000);
                video.addEventListener(name, () => { clearTimeout(timeout); resolve(); }, { once: true });
                video.addEventListener('error', () => { clearTimeout(timeout); reject(new Error(video.error?.message)); }, { once: true });
            });
            try {
                let event = wait('loadedmetadata');
                video.src = url;
                await event;
                await video.play();
                event = wait('timeupdate');
                await event;
                if (video.currentTime <= 0) throw new Error('The video did not play');
                video.pause();
                const seek = wait('seeked');
                video.currentTime = 4.5;
                await seek;
                if (Math.abs(video.currentTime - 4.5) > 0.1) throw new Error('The video did not seek');
                window.mediaRun = { video, lease, url };
                return url;
            } catch (error) {
                await window.mediaStop(video, lease);
                throw error;
            }
        };
        window.mediaStop = async (video, lease, url) => {
                video.pause();
                video.removeAttribute('src');
                video.load();
                video.remove();
                if (url) await window.mediaInvoke('media_release', { lease, url });
                await window.mediaInvoke('media_leave', { lease });
                delete window.mediaRun;
        };
    "#;
    app.page.eval(playback)?;
    let errors = app.page.eval(
        r#"
        (async () => {
            const id = 'local-media';
            const lease = await window.mediaInvoke('media_enter', { id });
            const paths = ['../outside.webm', '.codex/auth.json', 'wrong.webm'];
            try {
                return await Promise.all(paths.map(path =>
                    window.mediaInvoke('media_grant', { id, path }).then(() => 'allowed', err => err.key ?? err)
                ));
            } finally {
                await window.mediaInvoke('media_leave', { lease });
            }
        })()
    "#,
    )?;
    same(errors, json!(["error.media.path", "error.media.path", "error.media.gone"]), "the screen receives typed file refusals")?;
    let (cold, _) = ctx.memory_mb(app.id)?;
    // Compare repeated loads after Chromium has initialized its decoder and media buffers once.
    app.page.eval("window.mediaCheck()")?;
    app.page.eval("window.mediaStop(window.mediaRun.video, window.mediaRun.lease, window.mediaRun.url)")?;
    app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
    pause(1.0);
    let (baseline, _) = ctx.memory_mb(app.id)?;
    let mut peak = cold.max(baseline);
    let mut old = serde_json::Value::Null;
    let cpu_before = ctx.cpu_ms(app.id)?;
    for _ in 0..20 {
        let url = app.page.eval("window.mediaCheck()")?;
        ensure(
            url.as_str().is_some_and(|url| url.starts_with("faber-media://") || url.starts_with("http://faber-media.localhost/")),
            || format!("unexpected media URL: {url}"),
        )?;
        ensure(url != old, || "reopened view reused a previous grant".into())?;
        old = url;
        let (mb, _) = ctx.memory_mb(app.id)?;
        peak = peak.max(mb);
        app.page.eval("window.mediaStop(window.mediaRun.video, window.mediaRun.lease, window.mediaRun.url)")?;
    }
    video_refused(&mut app.page, &old, "the protocol refuses a URL from a left view")?;
    let active_cpu = ctx.cpu_ms(app.id)? - cpu_before;
    app.page.cdp.call("HeapProfiler.collectGarbage", json!({}))?;
    pause(1.0);
    let idle_start = Instant::now();
    let idle_before = ctx.cpu_ms(app.id)?;
    pause(2.0);
    let idle_cpu = (ctx.cpu_ms(app.id)? - idle_before) / idle_start.elapsed().as_secs_f64() / 10.0;
    let (retained, _) = ctx.memory_mb(app.id)?;
    ctx.notes.push(format!("20 local video cycles: cold {cold:.1} MB, warm {baseline:.1} MB, sampled peak {peak:.1} MB, after release {retained:.1} MB, CPU {active_cpu:.0} ms, idle {idle_cpu:.1}% of one core"));
    ensure(peak < 400.0 && retained < 400.0 && retained - cold < 25.0 && retained - baseline < 15.0 && idle_cpu < 5.0, || {
        format!(
            "media budget exceeded: {cold:.1} MB cold, {baseline:.1} MB warm, {peak:.1} MB sampled peak, {retained:.1} MB after, {idle_cpu:.1}% idle CPU"
        )
    })?;

    let before_restart = app.page.eval("window.mediaCheck()")?;
    app.page.eval("window.mediaRun.video.removeAttribute('src');window.mediaRun.video.load();window.mediaRun.video.remove()")?;
    let outside = ctx.sb.data.join("outside.webm");
    let leaf = working.join("seek.webm");
    std::fs::copy(&leaf, &outside).map_err(|e| e.to_string())?;
    std::fs::remove_file(&leaf).map_err(|e| e.to_string())?;
    std::fs::hard_link(&outside, &leaf).map_err(|e| e.to_string())?;
    video_refused(&mut app.page, &before_restart, "a link swapped in after playback cannot be loaded")?;
    std::fs::remove_file(&leaf).map_err(|e| e.to_string())?;
    std::fs::copy(&outside, &leaf).map_err(|e| e.to_string())?;

    ctx.close(app.id)?;
    let mut app = ctx.launch(no_cli())?;
    video_refused(&mut app.page, &before_restart, "a restart does not restore an old grant")?;
    app.page.eval(playback)?;
    let reopened = app.page.eval("window.mediaCheck()")?;
    ensure(reopened != before_restart, || "reopening after restart reused an old grant".into())?;
    app.page.eval("window.mediaStop(window.mediaRun.video, window.mediaRun.lease, window.mediaRun.url)")?;
    Ok(())
}

fn video_refused(page: &mut Page, url: &serde_json::Value, what: &str) -> Result {
    // Exercise a new protocol request, even if Chromium retained already delivered media in the renderer.
    page.cdp.call("Network.enable", json!({}))?;
    page.cdp.call("Network.setCacheDisabled", json!({"cacheDisabled":true}))?;
    let refused = page.eval(&format!(
        "new Promise(resolve=>{{const v=document.createElement('video');v.onerror=()=>resolve(true);v.onloadedmetadata=()=>resolve(false);v.src={url};}})"
    ));
    page.cdp.call("Network.setCacheDisabled", json!({"cacheDisabled":false}))?;
    same(refused?, json!(true), what)
}
