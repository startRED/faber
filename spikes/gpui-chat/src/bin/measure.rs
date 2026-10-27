#![windows_subsystem = "windows"]
#[path = "measure/inspect.rs"]
mod inspect;
use faber_testkit::{desktop, proctree};
use std::{fs, path::{Path, PathBuf}, thread, time::{Duration, Instant}};

fn wait_for_builds() -> Result<(), String> {
    loop {
        if !proctree::snapshot()?.iter().any(|p| p.name.eq_ignore_ascii_case("cargo.exe") || p.name.eq_ignore_ascii_case("rustc.exe")) { return Ok(()); }
        thread::sleep(Duration::from_secs(30));
    }
}
fn inspect_window(desktop_name: &str, pid: u32, output: &Path) -> Result<(), String> {
    let own = std::env::current_exe().map_err(|e| e.to_string())?;
    let args = vec!["--uia".into(), pid.to_string(), output.to_string_lossy().into_owned()];
    let helper = desktop::launch(desktop_name, &own.to_string_lossy(), &args)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(code) = helper.try_wait() { return if code == 0 { Ok(()) } else { Err(format!("Inspection failed: {code}")) }; }
        if Instant::now() > deadline { helper.kill(); return Err("Inspection timed out".into()); }
        thread::sleep(Duration::from_millis(20));
    }
}
fn run() -> Result<(), String> {
    let directory = std::env::current_dir().map_err(|e| e.to_string())?;
    let out = directory.join("out");
    let webview = std::env::args().any(|arg| arg == "--webview");
    let executable: PathBuf = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name(if webview { "webview.exe" } else { "gpui-chat.exe" });
    let desktop = desktop::name()?;
    let fixture = directory.join("../../tests/fixtures/claude/chat.jsonl");
    let mut report = String::new();
    let states: Vec<(&str, &str, bool, bool)> = if webview { vec![("webview", "small.json", false, false)] } else { vec![("idle", "small.json", false, false), ("bottom", "thousand.json", false, false), ("top", "thousand.json", true, false), ("stream", "small.json", false, true)] };
    for (label, file, top, streaming) in states {
        for iteration in 1..=3 {
            wait_for_builds()?;
            let marker = out.join(format!("{label}-{iteration}.ready"));
            if marker.exists() { fs::remove_file(&marker).map_err(|e| e.to_string())?; }
            let mut args = vec![out.join(file).to_string_lossy().into_owned(), "--ready".into(), marker.to_string_lossy().into_owned()];
            if top { args.push("--scroll-top".into()); }
            if streaming { args.extend(["--stream".into(), fixture.to_string_lossy().into_owned()]); }
            let started = Instant::now();
            let app = desktop::launch(desktop, &executable.to_string_lossy(), &args)?;
            let result = (|| -> Result<String, String> {
                loop {
                    if let Some(status) = app.try_wait() { return Err(format!("App exited before its frame: {status}")); }
                    if let Ok(text) = fs::read_to_string(&marker) { if !top || text.contains("top_frame_ms=") { break; } }
                    if started.elapsed() > Duration::from_secs(60) { return Err("First frame timed out".into()); }
                    thread::sleep(Duration::from_millis(1));
                }
                let startup_ms = started.elapsed().as_millis();
                let mut stream_cpu = None;
                if streaming {
                    let cpu_start = proctree::cpu_ms(app.pid)?;
                    let wall = Instant::now();
                    let mut inspected = false;
                    loop {
                        if fs::read_to_string(&marker).unwrap_or_default().contains("stream_done_ms=") { break; }
                        if app.try_wait().is_some() { return Err("App exited during streaming".into()); }
                        if wall.elapsed() > Duration::from_secs(90) { return Err("Stream timed out".into()); }
                        if iteration == 1 && !inspected && wall.elapsed() >= Duration::from_secs(9) {
                            inspected = true;
                            if let Err(error) = inspect_window(desktop, app.pid, &out.join("mid-stream")) { let _ = fs::write(out.join("mid-stream-error.txt"), error); }
                        }
                        thread::sleep(Duration::from_millis(20));
                    }
                    stream_cpu = Some((proctree::cpu_ms(app.pid)? - cpu_start) / wall.elapsed().as_secs_f64() / 10.0);
                } else {
                    thread::sleep(Duration::from_secs(5));
                    if iteration == 1 { if let Err(error) = inspect_window(desktop, app.pid, &out.join(label)) { let _ = fs::write(out.join(format!("{label}-error.txt")), error); } }
                }
                if webview { fs::write(out.join(format!("webview-tree-{iteration}.txt")), format!("{:#?}", proctree::live_tree(&proctree::snapshot()?, app.pid))).map_err(|e| e.to_string())?; }
                let memory = proctree::private_mb(app.pid)?;
                use std::os::windows::process::CommandExt;
                let gpu = std::process::Command::new("powershell.exe").creation_flags(0x08000000)
                    .args(["-NoProfile", "-NonInteractive", "-File", "gpu-counter.ps1", &app.pid.to_string(), if webview { "tree" } else { "window" }])
                    .output().map_err(|e| e.to_string())?;
                if !gpu.status.success() { return Err(format!("GPU counter failed: {}", String::from_utf8_lossy(&gpu.stderr))); }
                let mut gpu = String::from_utf8_lossy(&gpu.stdout).trim().to_owned();
                if webview {
                    let window_gpu = std::process::Command::new("powershell.exe").creation_flags(0x08000000)
                        .args(["-NoProfile", "-NonInteractive", "-File", "gpu-counter.ps1", &app.pid.to_string(), "window"])
                        .output().map_err(|e| e.to_string())?;
                    if !window_gpu.status.success() { return Err(format!("Window GPU counter failed: {}", String::from_utf8_lossy(&window_gpu.stderr))); }
                    gpu.push_str(" window_");
                    gpu.push_str(&String::from_utf8_lossy(&window_gpu.stdout).trim().replace(" gpu_", " window_gpu_"));
                }
                let before = proctree::cpu_ms(app.pid)?;
                let begin = Instant::now();
                thread::sleep(Duration::from_secs(10));
                let cpu = (proctree::cpu_ms(app.pid)? - before) / begin.elapsed().as_secs_f64() / 10.0;
                let marker_text = fs::read_to_string(&marker).map_err(|e| e.to_string())?.replace('\n', " ");
                Ok(format!("{label} run {iteration}: process_to_marker_ms={startup_ms} {marker_text} private_mb={memory:.2} {gpu} idle_cpu_percent={cpu:.2} stream_cpu_percent={stream_cpu:?}\n"))
            })();
            app.kill();
            report.push_str(&result?);
            fs::write(out.join(if webview { "webview-measurements.txt" } else { "round2-measurements.txt" }), &report).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|arg| arg == "--uia") {
        let result = args.get(2).and_then(|arg| arg.parse().ok()).zip(args.get(3)).ok_or("Invalid UIA arguments".to_owned()).and_then(|(pid, path)| inspect::run(pid, Path::new(path)));
        if let Err(error) = result { let _ = fs::write("out/inspect-error.txt", error); std::process::exit(1); }
    } else if let Err(error) = run() { let _ = fs::write("out/measure-error.txt", error); std::process::exit(1); }
}


