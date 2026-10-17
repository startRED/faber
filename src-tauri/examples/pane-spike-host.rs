//! Manual spike (R161 step 7, docs/superpowers/specs/2026-10-17-agent-pane-webview-spike.md): one window that is never
//! shown, or shown far off screen, holding a stand-in for Faber's screen and, on request, a second child webview, the
//! pane an agent would browse in. `testkit/examples/pane-spike.rs` starts it, drives both webviews over the DevTools port
//! and measures them; this program only obeys the lines it finds in its control file:
//! `open`, `hide`, `park`, `show`, `close` (the pane), `minimize`, `hide-window` and `exit`. Each line it has carried out is copied to `<control>.ack`.
//!
//! Usage: `pane-spike-host PORT SERVER DATA_DIR CONTROL hidden|offscreen [no-throttle]`.
//! Build: `cargo build -p faber --example pane-spike-host --features pane-spike`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl};

struct Args {
    port: u16,
    server: String,
    data: PathBuf,
    control: PathBuf,
    offscreen: bool,
    no_throttle: bool,
}

fn args() -> Args {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let [port, server, data, control, window, rest @ ..] = all.as_slice() else {
        eprintln!("usage: pane-spike-host PORT SERVER DATA_DIR CONTROL hidden|offscreen [no-throttle]");
        std::process::exit(2);
    };
    Args {
        port: port.parse().expect("port"),
        server: server.clone(),
        data: data.into(),
        control: control.into(),
        offscreen: window == "offscreen",
        no_throttle: rest.iter().any(|a| a == "no-throttle"),
    }
}

/// The same switches Faber's own window gets for an automated launch, so both webviews share one browser process.
fn browser_args(args: &Args) -> String {
    let mut line = format!(
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-breakpad --disable-crash-reporter \
         --remote-debugging-port={}",
        args.port
    );
    if args.no_throttle {
        line.push_str(" --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-background-timer-throttling");
    }
    line
}

fn webview(label: &str, url: &str, args: &Args) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(label, WebviewUrl::External(url.parse().expect("url")))
        .additional_browser_args(&browser_args(args))
        .data_directory(args.data.clone())
        .focused(false)
}

fn main() {
    let args = args();
    // A windows-subsystem program has no console; its failures go next to the control file.
    let log = args.control.with_extension("log");
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::write(&log, info.to_string());
    }));
    tauri::Builder::default()
        .setup(move |app| {
            let mut builder = WindowBuilder::new(app, "main")
                .title("pane spike")
                .inner_size(1000.0, 700.0)
                .skip_taskbar(true)
                .focused(false)
                .visible(args.offscreen);
            if args.offscreen {
                builder = builder.position(-32000.0, -32000.0);
            }
            let window = builder.build()?;
            let main = format!("{}/main.html", args.server);
            window.add_child(webview("screen", &main, &args), LogicalPosition::new(0.0, 0.0), LogicalSize::new(1000.0, 700.0))?;
            let handle = app.handle().clone();
            std::thread::spawn(move || obey(handle, args));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the spike host")
        .run(|_, _| {});
}

/// Reads the control file every 100 ms and carries out each new line once.
fn obey(app: tauri::AppHandle, args: Args) {
    let mut done = 0;
    let ack = args.control.with_extension("ack");
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let text = std::fs::read_to_string(&args.control).unwrap_or_default();
        // A line still being written has no newline yet; it waits for the next look.
        let complete = text.rfind('\n').map_or("", |end| &text[..end]);
        let lines: Vec<String> = complete.lines().map(str::to_string).collect();
        for line in lines.iter().skip(done) {
            let window = app.get_window("main").expect("the window");
            let pane = app.get_webview("pane");
            let result = match (line.as_str(), pane) {
                ("open", None) => {
                    // Another site than the screen, so the pane gets its own renderer process as a real site would.
                    let url = format!("{}/page?n=0", args.server.replace("127.0.0.1", "localhost"));
                    Some(
                        window
                            .add_child(webview("pane", &url, &args), LogicalPosition::new(200.0, 0.0), LogicalSize::new(800.0, 600.0))
                            .map(drop),
                    )
                }
                ("hide", Some(pane)) => Some(pane.hide()),
                // Out of the window's area but still visible to the browser engine, so it keeps painting.
                ("park", Some(pane)) => Some(pane.set_position(LogicalPosition::new(-10000.0, -10000.0))),
                ("show", Some(pane)) => Some(pane.show()),
                ("close", Some(pane)) => Some(pane.close()),
                // The window itself, as Faber's goes while the person plays: minimized, or hidden to the tray.
                ("minimize", _) => Some(window.minimize()),
                ("hide-window", _) => Some(window.hide()),
                ("exit", _) => {
                    app.exit(0);
                    Some(Ok(()))
                }
                _ => None,
            };
            let status = match result {
                Some(Ok(())) => "ok".to_string(),
                Some(Err(e)) => format!("error {e}"),
                None => "error: not possible now".to_string(),
            };
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&ack) {
                let _ = writeln!(file, "{line} {status}");
            }
        }
        done = lines.len();
    }
}
