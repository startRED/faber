//! Manual spike (R161 step 7, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md): what a second webview, the
//! pane an agent browses in, costs while nobody looks at it. It starts `pane-spike-host` (src-tauri/examples), opens the
//! pane, takes a small picture after each of 20 actions (navigate, click, type, scroll) and reports stale or blank
//! pictures, the memory of the app's whole tree (before, peak, after), the processor, and any window of the tree that
//! became visible or took the foreground.
//!
//! Usage: `pane-spike HOST OUT_DIR [real-desktop] [offscreen] [pane-hidden] [pane-parked] [window=minimize|hide-window] [no-throttle] [site=URL]`.
//! Without `real-desktop` the host runs on the testkit's hidden desktop, where nothing can be seen; `real-desktop` runs
//! its never-shown window on the person's desktop and ends it at the first window it shows or focus it takes.
//! Windows only.

#[cfg(not(windows))]
fn main() {
    eprintln!("pane-spike measures WebView2, so it runs on Windows only");
}

#[cfg(windows)]
fn main() {
    spike::main();
}

#[cfg(windows)]
mod spike {
    use faber_testkit::{cdp::Cdp, desktop, proctree};
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    const ACTIONS: u32 = 20;
    const PORT: u16 = 9341;

    /// The colour the page paints its marker for step `n`; neighbours always differ.
    pub fn color(n: u32) -> [u8; 3] {
        [((n * 53) % 200 + 30) as u8, ((n * 97) % 200 + 30) as u8, ((n * 151) % 200 + 30) as u8]
    }

    const PAGE: &str = r#"<!doctype html><meta charset=utf-8><title>pane</title>
<style>body{margin:0;font:16px sans-serif;height:6000px;background:#fff}
#m{position:fixed;left:0;top:0;width:160px;height:120px;font:bold 48px sans-serif;display:flex;align-items:center;justify-content:center}
#b{position:fixed;left:300px;top:200px;width:160px;height:60px}#t{position:fixed;left:300px;top:300px;width:300px;height:40px}
p{margin:0 20px 0 200px;padding-top:420px;max-width:500px}</style>
<div id=m></div><button id=b>Next</button><input id=t><p>A page an agent might read: some words, a button, a field, and a body
long enough to scroll. Each action repaints the square in the corner with the colour of its step.</p>
<script>const color=n=>`rgb(${(n*53)%200+30},${(n*97)%200+30},${(n*151)%200+30})`;
window.paint=n=>{m.style.background=color(n);m.textContent=n;m.dataset.n=n};
paint(Number(new URLSearchParams(location.search).get('n')||0));window.next=0;
b.onclick=()=>paint(next);t.oninput=()=>paint(next);addEventListener('scroll',()=>paint(next));</script>"#;

    const MAIN: &str = "<!doctype html><meta charset=utf-8><title>screen</title><body style=\"font:16px sans-serif\">\
        <h1>Faber</h1><p>A stand-in for the screen: the pane is measured against it.</p>";

    /// A loopback server for the two pages; every other path is a 404.
    fn serve() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || answer(stream));
            }
        });
        base
    }

    fn answer(mut stream: TcpStream) {
        let mut line = String::new();
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        if reader.read_line(&mut line).is_err() {
            return;
        }
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header).map_or(true, |n| n == 0) || header.trim().is_empty() {
                break;
            }
        }
        let path = line.split_whitespace().nth(1).unwrap_or("/");
        let (status, body) = match path.split('?').next() {
            Some("/main.html") => ("200 OK", MAIN),
            Some("/page") => ("200 OK", PAGE),
            _ => ("404 Not Found", "not here"),
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }

    struct Options {
        host: String,
        out: PathBuf,
        real_desktop: bool,
        offscreen: bool,
        pane_hidden: bool,
        pane_parked: bool,
        no_throttle: bool,
        site: Option<String>,
        /// What happens to the shown window once the pane is open: `minimize` or `hide-window` (to the tray).
        window: Option<String>,
    }

    fn options() -> Options {
        let all: Vec<String> = std::env::args().skip(1).collect();
        if let [flag, file] = all.as_slice()
            && flag == "--probe"
        {
            probe(Path::new(file));
        }
        let [host, out, flags @ ..] = all.as_slice() else {
            eprintln!(
                "usage: pane-spike HOST OUT_DIR [real-desktop] [offscreen] [pane-hidden] [pane-parked] [window=minimize|hide-window] [no-throttle] [site=URL]"
            );
            std::process::exit(2);
        };
        let has = |f: &str| flags.iter().any(|a| a == f);
        let options = Options {
            host: host.clone(),
            out: out.into(),
            real_desktop: has("real-desktop"),
            offscreen: has("offscreen"),
            pane_hidden: has("pane-hidden"),
            pane_parked: has("pane-parked"),
            no_throttle: has("no-throttle"),
            site: flags.iter().find_map(|a| a.strip_prefix("site=")).map(str::to_string),
            window: flags.iter().find_map(|a| a.strip_prefix("window=")).map(str::to_string),
        };
        if options.window.is_some() && !options.offscreen {
            eprintln!("window=minimize|hide-window changes a shown window, so it needs offscreen");
            std::process::exit(2);
        }
        if options.real_desktop && options.offscreen {
            eprintln!("an offscreen window is shown; on the person's desktop it may flash, so the spike refuses it");
            std::process::exit(2);
        }
        options
    }

    /// The top-level windows on this thread's desktop that Windows calls visible, as `pid class width x height at x,y`,
    /// each with whether a person could see it. Two kinds cannot be seen: a window of no area, and tao's event window,
    /// which every Tauri app (Faber too) keeps "visible" but layered and transparent, with nothing ever drawn in it.
    fn visible_windows() -> Vec<(u32, String, bool)> {
        let mut found = Vec::new();
        let _ = winsafe::EnumWindows(|hwnd| {
            if hwnd.IsWindowVisible() {
                let pid = hwnd.GetWindowThreadProcessId().1;
                let class = hwnd.GetClassName().unwrap_or_default();
                let r = hwnd.GetWindowRect().unwrap_or_default();
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                let seen = w > 0 && h > 0 && class != "Tao Thread Event Target";
                found.push((pid, format!("{pid} {class} {w}x{h} at {},{}", r.left, r.top), seen));
            }
            true
        });
        found
    }

    /// Run on the hidden desktop: writes its visible windows to `file`, one per line.
    fn probe(file: &Path) -> ! {
        let lines: Vec<String> = visible_windows().into_iter().map(|(_, line, _)| line).collect();
        let _ = std::fs::write(file, lines.join("\n"));
        std::process::exit(0);
    }

    /// What the watcher saw on the person's desktop.
    #[derive(Default)]
    struct Seen {
        foreground: Vec<String>,
        visible: Vec<String>,
        /// Windows Windows calls visible that no person can see (see `visible_windows`).
        unseen: Vec<String>,
    }

    /// Every 20 ms: does a window of the app's tree hold the foreground, or show on this desktop? At the first one the
    /// app is ended, so a flash lasts one tick at most.
    fn watch(root: u32, stop: Arc<AtomicBool>, seen: Arc<Mutex<Seen>>) {
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut pids = vec![root];
            let mut refreshed = Instant::now() - Duration::from_secs(1);
            while !stop.load(Ordering::SeqCst) {
                if refreshed.elapsed() > Duration::from_millis(500) {
                    if let Ok(rows) = proctree::snapshot() {
                        pids = proctree::live_tree(&rows, root).iter().map(|r| r.pid).collect();
                        pids.push(root);
                    }
                    refreshed = Instant::now();
                }
                let at = started.elapsed().as_millis();
                let mut found = false;
                if let Some(hwnd) = winsafe::HWND::GetForegroundWindow() {
                    let (_, pid) = hwnd.GetWindowThreadProcessId();
                    if pids.contains(&pid) {
                        seen.lock().expect("seen").foreground.push(format!("{at} ms: pid {pid}"));
                        found = true;
                    }
                }
                for (pid, line, can_be_seen) in visible_windows() {
                    if !pids.contains(&pid) {
                        continue;
                    }
                    let mut seen = seen.lock().expect("seen");
                    if can_be_seen {
                        seen.visible.push(format!("{at} ms: {line}"));
                        found = true;
                    } else if !seen.unseen.contains(&line) {
                        seen.unseen.push(line);
                    }
                }
                if found {
                    if let Ok(process) = winsafe::HPROCESS::OpenProcess(winsafe::co::PROCESS::TERMINATE, false, root) {
                        let _ = process.TerminateProcess(1);
                    }
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }

    #[derive(Default, Clone)]
    struct Shot {
        kind: &'static str,
        applied: bool,
        first: &'static str,
        first_ms: f64,
        fresh_ms: Option<f64>,
        bytes: usize,
    }

    /// Takes one picture and says what its corner shows for step `n`: fresh, stale (an earlier step), blank or other.
    fn picture(cdp: &mut Cdp, n: u32, save: Option<&Path>) -> Result<(&'static str, usize), String> {
        cdp.set_deadline(Some(Instant::now() + Duration::from_secs(2)));
        let shot = cdp.call("Page.captureScreenshot", json!({ "format": "png" }));
        cdp.set_deadline(None);
        let data = shot?["data"].as_str().ok_or("no data")?.to_string();
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
        if let Some(path) = save {
            std::fs::write(path, &bytes).map_err(|e| e.to_string())?;
        }
        let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes)).read_info().map_err(|e| e.to_string())?;
        let mut buf = vec![0; reader.output_buffer_size().ok_or("picture too big")?];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let channels = match info.color_type {
            png::ColorType::Rgb => 3,
            png::ColorType::Rgba => 4,
            other => return Err(format!("unexpected colour type {other:?}")),
        };
        let at = (5 * info.line_size) + 5 * channels;
        let pixel = [buf[at], buf[at + 1], buf[at + 2]];
        let near = |c: [u8; 3]| c.iter().zip(pixel).all(|(a, b)| a.abs_diff(b) <= 8);
        let kind = if near(color(n)) {
            "fresh"
        } else if (0..n).any(|k| near(color(k))) {
            "stale"
        } else if pixel.iter().all(|v| *v >= 245) || pixel.iter().all(|v| *v <= 10) {
            "blank"
        } else {
            "other"
        };
        Ok((kind, bytes.len()))
    }

    fn act(cdp: &mut Cdp, server: &str, n: u32) -> Result<&'static str, String> {
        let kind = ["navigate", "click", "type", "scroll"][(n % 4) as usize];
        match kind {
            "navigate" => {
                cdp.call("Page.navigate", json!({ "url": format!("{server}/page?n={n}") }))?;
                let ready = format!("document.readyState === 'complete' && location.search === '?n={n}'");
                cdp.wait_for("the page to load", &ready, Duration::from_secs(10))?;
            }
            "click" => {
                cdp.eval(&format!("window.next = {n}"))?;
                cdp.click_at(380.0, 230.0)?;
            }
            "type" => {
                cdp.eval(&format!("window.next = {n}; t.focus()"))?;
                cdp.insert_text("x")?;
            }
            _ => {
                cdp.eval(&format!("window.next = {n}"))?;
                let wheel = json!({ "type": "mouseWheel", "x": 400, "y": 300, "deltaX": 0, "deltaY": 300 });
                cdp.call("Input.dispatchMouseEvent", wheel)?;
            }
        }
        Ok(kind)
    }

    /// A job that ends `pid` and everything it starts once this program lets go of it.
    fn job(pid: u32) -> win32job::Job {
        let job = win32job::Job::create().expect("job");
        let mut info = job.query_extended_limit_info().expect("job limits");
        info.limit_kill_on_job_close();
        job.set_extended_limit_info(&info).expect("job limits");
        let access = winsafe::co::PROCESS::SET_QUOTA | winsafe::co::PROCESS::TERMINATE;
        let process = winsafe::HPROCESS::OpenProcess(access, false, pid).expect("the host");
        job.assign_process(process.ptr() as isize).expect("the host joins the job");
        job
    }

    fn memory(root: u32) -> f64 {
        proctree::private_mb(root).unwrap_or(f64::NAN)
    }

    /// The tree's processor time so far and when the reading ended. Each WMI reading takes time of its own, so a share
    /// is taken between the ends of two readings, not over the sleep between them. A process that ends in between
    /// drops out of the sum, so the share is a floor when renderers come and go.
    fn cpu_reading(root: u32) -> (f64, Instant) {
        (proctree::cpu_ms(root).unwrap_or(f64::NAN), Instant::now())
    }

    fn core_share(from: (f64, Instant), to: (f64, Instant)) -> f64 {
        (to.0 - from.0) / to.1.duration_since(from.1).as_secs_f64() / 10.0
    }

    /// The share of one core the tree used over about `span`.
    fn core_percent(root: u32, span: Duration) -> f64 {
        let before = cpu_reading(root);
        std::thread::sleep(span);
        core_share(before, cpu_reading(root))
    }

    /// Asks the host for one thing and waits for its answer; a refusal or no answer ends the run.
    fn command(control: &Path, line: &str) {
        let mut file = std::fs::OpenOptions::new().create(true).append(true).open(control).expect("control file");
        file.write_all(format!("{line}\n").as_bytes()).expect("control write");
        let ack = control.with_extension("ack");
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let answers = std::fs::read_to_string(&ack).unwrap_or_default();
            if let Some(status) = answers.lines().find_map(|l| l.strip_prefix(line).and_then(|rest| rest.strip_prefix(' '))) {
                assert_eq!(status, "ok", "the host could not {line}");
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the host did not carry out {line}");
    }

    pub fn main() {
        let o = options();
        std::fs::create_dir_all(&o.out).expect("out dir");
        let work = o.out.join("host");
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).expect("work dir");
        let control = work.join("control.txt");
        std::fs::write(&control, "").expect("control");
        let server = serve();
        // The pane is on another site than the screen (see the host).
        let pane_site = server.replace("127.0.0.1", "localhost");
        let mut args = vec![
            PORT.to_string(),
            server.clone(),
            work.join("webview").display().to_string(),
            control.display().to_string(),
            if o.offscreen { "offscreen" } else { "hidden" }.to_string(),
        ];
        if o.no_throttle {
            args.push("no-throttle".into());
        }
        let place = if o.real_desktop { "WinSta0\\Default" } else { desktop::name().expect("hidden desktop") };
        let host = desktop::launch(place, &o.host, &args).expect("start the host");
        let root = host.pid;
        // The host and its webview processes end with this program, whatever stops it.
        let _job = job(root);
        let stop = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(Mutex::new(Seen::default()));
        watch(root, stop.clone(), seen.clone());
        let mut report = serde_json::Map::new();
        let mut put = |key: &str, value: Value| {
            println!("{key}: {value}");
            report.insert(key.into(), value);
        };
        put(
            "mode",
            json!(format!(
                "{} desktop, window {}{}, pane {}, throttling {}",
                if o.real_desktop { "person's" } else { "hidden" },
                if o.offscreen { "shown far off screen" } else { "never shown" },
                o.window.as_ref().map_or(String::new(), |w| format!(" then {w}")),
                if o.pane_hidden {
                    "hidden"
                } else if o.pane_parked {
                    "parked outside the window"
                } else {
                    "shown in the window"
                },
                if o.no_throttle { "off" } else { "default" }
            )),
        );

        let started = Instant::now();
        let mut screen = match Cdp::connect_where(PORT, Duration::from_secs(60), |u| u.contains("/main.html")) {
            Ok(cdp) => cdp,
            Err(e) => {
                eprintln!("the screen: {e}; the host {}", host.try_wait().map_or("still runs".into(), |c| format!("ended with {c:#x}")));
                host.kill();
                std::process::exit(1);
            }
        };
        screen.wait_for("the screen", "document.readyState === 'complete'", Duration::from_secs(20)).expect("screen ready");
        put("screen_ready_ms", json!(started.elapsed().as_millis()));
        std::thread::sleep(Duration::from_secs(8));
        let before = memory(root);
        put("memory_before_mb", json!(before));
        put("cpu_idle_screen_pct_core", json!(core_percent(root, Duration::from_secs(5))));

        let opened = Instant::now();
        command(&control, "open");
        let mut pane = Cdp::connect_where(PORT, Duration::from_secs(30), |u| u.contains("/page")).expect("the pane");
        pane.wait_for("the pane", "document.readyState === 'complete'", Duration::from_secs(20)).expect("pane ready");
        put("pane_ready_ms", json!(opened.elapsed().as_millis()));
        if o.pane_hidden {
            command(&control, "hide");
        }
        if o.pane_parked {
            command(&control, "park");
        }
        if let Some(window) = &o.window {
            command(&control, window);
        }
        std::thread::sleep(Duration::from_secs(3));
        put("memory_pane_open_mb", json!(memory(root)));
        put("cpu_idle_pane_pct_core", json!(core_percent(root, Duration::from_secs(5))));
        let first = picture(&mut pane, 0, Some(&o.out.join("shot-00.png")));
        put("picture_0", json!(format!("{first:?}")));

        let mut shots = Vec::new();
        let mut peak = memory(root);
        let cpu_before = cpu_reading(root);
        let window = Instant::now();
        for n in 1..=ACTIONS {
            let mut shot = Shot::default();
            match act(&mut pane, &pane_site, n) {
                Ok(kind) => shot.kind = kind,
                Err(e) => {
                    eprintln!("step {n}: {e}");
                    shot.kind = "failed";
                }
            }
            let t = Instant::now();
            let save = o.out.join(format!("shot-{n:02}.png"));
            match picture(&mut pane, n, Some(&save)) {
                Ok((kind, bytes)) => {
                    shot.first = kind;
                    shot.bytes = bytes;
                }
                Err(e) => {
                    eprintln!("step {n}: picture: {e}");
                    shot.first = "none";
                }
            }
            shot.first_ms = t.elapsed().as_secs_f64() * 1000.0;
            if shot.first == "fresh" {
                shot.fresh_ms = Some(shot.first_ms);
            } else {
                while t.elapsed() < Duration::from_secs(3) {
                    std::thread::sleep(Duration::from_millis(50));
                    if let Ok(("fresh", _)) = picture(&mut pane, n, None) {
                        shot.fresh_ms = Some(t.elapsed().as_secs_f64() * 1000.0);
                        break;
                    }
                }
            }
            shot.applied = pane.eval("m.dataset.n").ok().and_then(|v| v.as_str().map(str::to_string)) == Some(n.to_string());
            println!(
                "step {n:2} {:8} applied {:5} first {:6} in {:5.0} ms, fresh after {:?} ms, {} bytes",
                shot.kind,
                shot.applied,
                shot.first,
                shot.first_ms,
                shot.fresh_ms.map(|v| v.round()),
                shot.bytes
            );
            shots.push(shot);
            peak = peak.max(memory(root));
        }
        let span = window.elapsed();
        let cpu = core_share(cpu_before, cpu_reading(root));
        put("actions_seconds", json!(span.as_secs_f64()));
        put("cpu_actions_pct_core", json!(cpu));
        let count = |k: &str| shots.iter().filter(|s| s.first == k).count();
        put(
            "first_pictures",
            json!({
                "fresh": count("fresh"), "stale": count("stale"), "blank": count("blank"), "other": count("other"), "none": count("none")
            }),
        );
        put("never_fresh_within_3s", json!(shots.iter().filter(|s| s.fresh_ms.is_none()).count()));
        put("not_applied", json!(shots.iter().filter(|s| !s.applied).count()));
        let mut times: Vec<f64> = shots.iter().map(|s| s.first_ms).collect();
        times.sort_by(f64::total_cmp);
        put("picture_ms_median_max", json!([times[times.len() / 2], times[times.len() - 1]]));
        let mut sizes: Vec<usize> = shots.iter().map(|s| s.bytes).collect();
        sizes.sort();
        put("picture_bytes_median", json!(sizes[sizes.len() / 2]));

        if !o.real_desktop {
            let file = o.out.join("probe.txt");
            let me = std::env::current_exe().expect("exe").display().to_string();
            if let Ok(p) = desktop::launch(place, &me, &["--probe".into(), file.display().to_string()]) {
                let deadline = Instant::now() + Duration::from_secs(10);
                while p.try_wait().is_none() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            let tree: Vec<u32> =
                proctree::snapshot().map(|rows| proctree::live_tree(&rows, root).iter().map(|r| r.pid).collect()).unwrap_or_default();
            let visible = std::fs::read_to_string(&file).unwrap_or_default();
            let ours: Vec<&str> = visible
                .lines()
                .filter(|l| l.split(' ').next().and_then(|p| p.parse::<u32>().ok()).is_some_and(|p| p == root || tree.contains(&p)))
                .collect();
            put("visible_windows_on_hidden_desktop", json!(ours));
        }

        if let Some(site) = &o.site {
            let _ = pane.call("Page.navigate", json!({ "url": site }));
            let _ = pane.wait_for("the site", "document.readyState === 'complete'", Duration::from_secs(30));
            std::thread::sleep(Duration::from_secs(5));
            let m = memory(root);
            peak = peak.max(m);
            put("memory_site_mb", json!(m));
            let _ = picture(&mut pane, 999, Some(&o.out.join("shot-site.png")));
        }
        put("memory_peak_mb", json!(peak));

        drop(pane);
        command(&control, "close");
        std::thread::sleep(Duration::from_secs(5));
        put("memory_after_close_5s_mb", json!(memory(root)));
        std::thread::sleep(Duration::from_secs(10));
        put("memory_after_close_15s_mb", json!(memory(root)));
        put("memory_added_by_pane_peak_mb", json!(peak - before));

        let tree: Vec<u32> =
            proctree::snapshot().map(|rows| proctree::live_tree(&rows, root).iter().map(|r| r.pid).collect()).unwrap_or_default();
        stop.store(true, Ordering::SeqCst);
        drop(screen);
        command(&control, "exit");
        let deadline = Instant::now() + Duration::from_secs(10);
        while host.try_wait().is_none() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        host.kill();
        let exited = Instant::now();
        let left = || proctree::snapshot().map(|rows| rows.iter().filter(|r| tree.contains(&r.pid)).count()).unwrap_or(0);
        while left() > 0 && exited.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(200));
        }
        put("processes_left_after_exit", json!(left()));
        put("seconds_until_tree_gone", json!(exited.elapsed().as_secs_f64()));
        let seen = seen.lock().expect("seen");
        put("foreground_taken", json!(seen.foreground));
        put("visible_on_person_desktop", json!(seen.visible));
        put("unseeable_on_person_desktop", json!(seen.unseen));
        std::fs::write(o.out.join("summary.json"), serde_json::to_string_pretty(&report).expect("json")).expect("summary");
    }
}
