//! The engine and every command of the table, behind newline-delimited JSON on stdin and stdout (`faber_host::wire`): the
//! process an Electron shell runs beside its window. Each request line is answered by one reply line tagged with its id,
//! concurrently, so replies may come out of order; every engine event is a `{"event":...}` line. When stdin closes the
//! engine is shut down cleanly and the process ends. It has no window: whatever needs one answers `error.host.noWindow`.
//!
//! What the shell must show or do, it decides here and says with an `{"order":{...}}` line (`faber_host::wire::Order`, all
//! written by the same single writer): the tray (tooltip, number, picture, menu words), a notification, keeping the PC
//! awake, starting with the system. The shell draws them with its own calls and decides nothing.
//!
//! The data folder is `FABER_DATA_DIR`, else the default under the config folder, as the Tauri shell reads it. What only the
//! shell that started this process knows comes as environment variables at spawn:
//!
//! - `FABER_VERSION`: the version of the app, which the screen shows (this crate's own version when not set).
//! - `FABER_AUTOSTART`: `1` or `0`, whether the app is registered to start with the system now. A setting saved later
//!   updates this process's copy and sends an `autostart` order, since only the shell can register itself.
//! - `FABER_HIDDEN`: `1` for a test or automated run, which notifies nobody.
//! - `FABER_SHELL_PID`: the process that owns Faber's window, so a maximized Faber in front is Faber and not a game.
//! - `FABER_UPDATES` and `FABER_INSTALL_DIR`: `1` and the folder of the app main program, set only for a packaged app that is
//!   not a hidden run. Then this process keeps the app updated (`faber-update`): it reads the release manifest, downloads and
//!   verifies the installer, and installs it through a copy of this program (`--faber-update-install`, the first argument
//!   of a start that only finishes an install). The status of the update is the event `{"type":"update",...}`; the end of the
//!   app, which the installer is about to replace, is the order `quit`. Without them nothing updates.
//!
//! - `FABER_PANE`: `1` when the shell has views for Faber's own browser pane, set only for a visible app. Then this process
//!   holds the pane's rules (`faber_host::pane`), orders the views with `{"order":{"kind":"pane",...}}` lines, and the window
//!   commands `site_*` and `cookie_*` reach that pane; what the screen should hear of it comes as events
//!   `{"type":"pane","channel":...,"payload":...}`. Without it those commands answer `error.host.noWindow`.
//!
//! The shell says what only it sees (its window's focus, whether it is on screen, the answers to its dialogs and to the pane's
//! orders, what the pane's views tell) with a `{"shell":{...}}` line (`faber_host::wire::ShellNote`).

// A windowed program on Windows, like the Tauri shell: no console is ever made for it (none to flash, no console host
// process), and its pipes work the same.
#![cfg_attr(windows, windows_subsystem = "windows")]

use faber_host::pane::{EngineHome, Pane, PaneBrowser};
use faber_host::sidecar::{SidecarOptions, SidecarShell};
use faber_host::wire::{self, Event, Incoming, Ordered, Reply};
use faber_host::{Host, StartOptions};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};

mod updates;

/// How long commands already running may take to finish once stdin closes, before they are stopped.
const FINISH: std::time::Duration = std::time::Duration::from_secs(1);

fn main() -> ExitCode {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.first().is_some_and(|arg| arg == faber_update::handoff::FLAG) {
            return ExitCode::from(u8::try_from(faber_update::handoff::finish(&args[1..])).unwrap_or(1));
        }
    }
    // The engine starts a CLI through this very program on Windows (a guard that joins a job object first).
    if let Some(code) = faber_engine::proc::guard_mode() {
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    // Before any HTTPS client: reqwest is built without a crypto provider of its own.
    faber_engine::tls::install();
    match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime.block_on(run()),
        Err(error) => {
            eprintln!("Faber failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> ExitCode {
    let Some(data_dir) = faber_host::data_dir_from_env() else {
        eprintln!("Faber failed to start: no data folder (set FABER_DATA_DIR)");
        return ExitCode::FAILURE;
    };
    let hidden = std::env::var("FABER_HIDDEN").is_ok_and(|v| v == "1");

    // One writer, so two lines never mix on stdout.
    let (out, mut lines) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = lines.recv().await {
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.flush().await.is_err() {
                break;
            }
        }
    });

    // The shell, which the host keeps to the end, sends orders through this; it is emptied at the end so the writer can finish.
    let orders = Arc::new(Mutex::new(Some(out.clone())));
    let shell = Arc::new(SidecarShell::new(SidecarOptions {
        data_dir: data_dir.clone(),
        version: std::env::var("FABER_VERSION").ok().filter(|version| !version.is_empty()),
        autostart: std::env::var("FABER_AUTOSTART").is_ok_and(|v| v == "1"),
        hidden,
        runtime: tokio::runtime::Handle::current(),
        send: {
            let orders = orders.clone();
            Arc::new(move |order| {
                if let Some(out) = orders.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    let _ = out.send(wire::line(&Ordered { order }));
                }
            })
        },
        attention: faber_host::tray::front::attention,
        shell_process: std::env::var("FABER_SHELL_PID").ok().and_then(|pid| pid.parse().ok()),
        chime: faber_host::tray::chime::play,
    }));
    let update_data = data_dir.clone();
    // Faber's own browser pane, when the shell has views for it: the pane is made before the engine, which is handed its browser.
    // Its events go out through `orders`, which is emptied at the end, so the pane never keeps the writer open.
    let pane = (!hidden && std::env::var("FABER_PANE").is_ok_and(|v| v == "1")).then(|| {
        let home = EngineHome::new({
            let orders = orders.clone();
            move |event| {
                let (channel, payload) = event.wire();
                let event = serde_json::json!({ "type": "pane", "channel": channel, "payload": payload });
                if let Some(out) = orders.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    let _ = out.send(wire::line(&Event { event }));
                }
            }
        });
        let pane = Pane::new(shell.pane_driver(), home.clone(), tokio::runtime::Handle::current());
        shell.attach_pane(pane.clone());
        (pane, home)
    });
    let options = StartOptions {
        data_dir,
        hidden,
        runtime: tokio::runtime::Handle::current(),
        installed_data_dir: faber_host::installed_data_dir(hidden),
        game_guard: None,
        browser: pane.as_ref().map(|(pane, _)| Arc::new(PaneBrowser(pane.clone())) as Arc<dyn faber_engine::browser::Browser>),
    };
    let host: Arc<Host> = match faber_host::start(options, shell.clone()) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("Faber failed to start: {error}");
            return ExitCode::FAILURE;
        }
    };
    // The engine exists: the tray is told for the first time, and its settings decide the notifications from now on.
    let engine = host.engine.clone();
    shell.attach(move || engine.settings());
    if let Some((pane, home)) = pane {
        home.attach(host.engine.clone());
        host.set_pane(pane);
    }

    // Self-updates, only in a packaged, visible app that says so (`updates`): the screen then sees where the update is.
    if !hidden
        && let Some(dir) = updates::install_dir(std::env::var("FABER_UPDATES").ok().as_deref(), std::env::var_os("FABER_INSTALL_DIR"))
    {
        let write: updates::Write = {
            let orders = orders.clone();
            Arc::new(move |text| {
                if let Some(out) = orders.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                    let _ = out.send(text);
                }
            })
        };
        let version = std::env::var("FABER_VERSION").ok().filter(|version| !version.is_empty());
        let version = version.unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
        updates::start(&host, &shell, write, update_data, &version, dir);
    }

    let events = {
        let (mut heard, out) = (host.subscribe(), out.clone());
        tokio::spawn(async move {
            loop {
                let event = match heard.recv().await {
                    Ok(event) => event,
                    // Behind by `missed` events: the peer reads `state` again, as the wire's doc says.
                    Err(broadcast::error::RecvError::Lagged(missed)) => serde_json::json!({ "type": "lagged", "missed": missed }),
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if out.send(wire::line(&Event { event })).is_err() {
                    break;
                }
            }
        })
    };

    // A line is read through a limit, never whole: one that never ends costs `MAX_LINE` bytes at most, is answered
    // `error.host.badRequest`, and the rest of it up to its line break is dropped without being kept.
    let mut input = BufReader::new(tokio::io::stdin());
    let mut line = Vec::new();
    let mut running = tokio::task::JoinSet::new();
    loop {
        line.clear();
        let Ok(read) = (&mut input).take(wire::MAX_LINE as u64 + 1).read_until(b'\n', &mut line).await else { break };
        if read == 0 {
            break;
        }
        // A line of exactly `MAX_LINE` bytes ends inside the limit with its break: nothing of the next line is dropped.
        let answer = if line.len() > wire::MAX_LINE && line.last() != Some(&b'\n') {
            if skip_line(&mut input).await.is_err() {
                break;
            }
            Err("error.host.badRequest".to_string())
        } else {
            wire::incoming(&String::from_utf8_lossy(&line))
        };
        match answer {
            Ok(Incoming::Note(note)) => shell.note(note),
            Ok(Incoming::Request(request)) => {
                let (host, out) = (host.clone(), out.clone());
                running.spawn(async move {
                    // The sidecar serves the screen, so its calls use the screen dispatcher.
                    let result = host.dispatch(&request.cmd, request.args).await;
                    let _ = out.send(wire::line(&Reply::of(request.id, result)));
                });
            }
            // A line that is not a request has no id to answer under: the peer hears id 0 and the stream goes on.
            Err(key) => {
                let _ = out.send(wire::line(&Reply::of(0, Err(key.into()))));
            }
        }
        // Finished commands leave the set as they are met, so it never grows with the session.
        while running.try_join_next().is_some() {}
    }

    // stdin closed: whoever ran this is gone or done. Commands already running get a moment to finish (a save in progress
    // ends); the ones that wait on something long (a turn) are then stopped, the engine closes, and the writer drains.
    let _ = tokio::time::timeout(FINISH, async { while running.join_next().await.is_some() {} }).await;
    running.shutdown().await;
    let closing = host.clone();
    let _ = tokio::task::spawn_blocking(move || closing.shut_down()).await;
    events.abort();
    orders.lock().unwrap_or_else(|e| e.into_inner()).take();
    drop(out);
    let _ = writer.await;
    ExitCode::SUCCESS
}

/// Drops the rest of an over-long line, up to and including its line break, without keeping any of it.
async fn skip_line<R: tokio::io::AsyncBufRead + Unpin>(input: &mut R) -> std::io::Result<()> {
    loop {
        let chunk = input.fill_buf().await?;
        if chunk.is_empty() {
            return Ok(());
        }
        let found = chunk.iter().position(|byte| *byte == b'\n');
        let used = found.map_or(chunk.len(), |at| at + 1);
        input.consume(used);
        if found.is_some() {
            return Ok(());
        }
    }
}
