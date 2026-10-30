//! The engine and every command of the table, behind newline-delimited JSON on stdin and stdout (`faber_host::wire`): the
//! process an Electron shell runs beside its window. Each request line is answered by one reply line tagged with its id,
//! concurrently, so replies may come out of order; every engine event is a `{"event":...}` line. When stdin closes the
//! engine is shut down cleanly and the process ends. It has no window: whatever needs one answers `error.host.noWindow`.
//!
//! The data folder is `FABER_DATA_DIR`, else the default under the config folder, as the Tauri shell reads it.

// A windowed program on Windows, like the Tauri shell: no console is ever made for it (none to flash, no console host
// process), and its pipes work the same.
#![cfg_attr(windows, windows_subsystem = "windows")]

use faber_host::wire::{self, Event, Reply};
use faber_host::{Host, Shell, StartOptions};
use std::process::ExitCode;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc};

/// How long commands already running may take to finish once stdin closes, before they are stopped.
const FINISH: std::time::Duration = std::time::Duration::from_secs(1);

/// Every default of the trait: no window, no tray, no updates.
struct Headless;

impl Shell for Headless {}

fn main() -> ExitCode {
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
    let options = StartOptions {
        data_dir,
        hidden,
        runtime: tokio::runtime::Handle::current(),
        installed_data_dir: faber_host::installed_data_dir(hidden),
        game_guard: None,
        browser: None,
    };
    let host: Arc<Host> = match faber_host::start(options, Arc::new(Headless)) {
        Ok(host) => host,
        Err(error) => {
            eprintln!("Faber failed to start: {error}");
            return ExitCode::FAILURE;
        }
    };

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
            wire::request(&String::from_utf8_lossy(&line))
        };
        match answer {
            Ok(request) => {
                let (host, out) = (host.clone(), out.clone());
                running.spawn(async move {
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
