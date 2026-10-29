//! The terminal screen: Faber's conversations, the thread and the box, in the terminal.
//!
//! One process and one waiting loop. It wakes for a key, an event from Faber, an answer to its own request, or the working
//! mark's next frame (at most 8 a second, and only while an answer streams); with nothing running it sleeps until the
//! next key. `app.rs` holds the state, `update.rs` and `keys.rs` change it, `view.rs` draws it.

mod app;
mod card;
mod editor;
mod keys;
mod markdown;
mod overlay;
mod palette;
#[cfg(test)]
mod tests;
mod transcript;
mod update;
mod view;

use crate::args::Args;
use crate::connect;
use crate::out::{DONE, Out, UNREACHED};
use crate::words::{self, Words};
use app::{App, Effect, FRAME, Input, Leave};
use crossterm::event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event, MouseEventKind};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use faber_channel::Client;
use palette::{Colors, Palette};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::{Value, json};
use std::io::{Stdout, stdout};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

/// The screen is drawn at most this often, however fast events come.
const MIN_DRAW: Duration = Duration::from_millis(30);

type Tx = UnboundedSender<Input>;

pub async fn run(args: Args, out: Out) -> i32 {
    let client = match connect::connect(&args, "faber").await {
        Ok(client) => Arc::new(client),
        Err(why) => return out.fail(why.key(), UNREACHED),
    };
    let state = match ask(&client, "state", Value::Null).await {
        Ok(state) => state,
        Err(key) => return out.fail(&key, UNREACHED),
    };
    let Ok(mut events) = client.subscribe().await else { return out.fail("cli.error.channel", UNREACHED) };
    let inner = if state.get("state").is_some() { &state["state"] } else { &state };
    let words = Words::new(&words::language(inner["settings"]["language"].as_str()));
    let look = if state.get("look").is_some() { &state["look"] } else { &inner["look"] };
    let colors = Colors::detect(out.color, &|name| std::env::var(name).ok());
    let reduced = matches!(look["motion"].as_str(), Some("reduced" | "none"))
        || std::env::var("FABER_REDUCED_MOTION").is_ok_and(|v| !v.is_empty() && v != "0");
    let now = Instant::now();
    let mut app = App::new(words, Palette::new(colors, look), reduced, now);
    app.apply_state(&state);
    if let Ok((w, h)) = crossterm::terminal::size() {
        app.size = (w, h);
    }

    let (tx, mut rx) = unbounded_channel::<Input>();
    let forward = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            if forward.send(Input::Faber(event)).is_err() {
                return;
            }
        }
        let _ = forward.send(Input::Lost);
    });
    run_effect(&client, &tx, Effect::Accounts);
    app.checked = Some(now);

    let Ok(mut terminal) = open_terminal() else { return out.fail("cli.error.channel", UNREACHED) };
    let _restore = Restore;
    read_keys(tx.clone());

    let mut dirty = true;
    let mut last_draw: Option<Instant> = None;
    let mut next_tick: Option<Instant> = None;
    loop {
        if dirty && last_draw.is_none_or(|at| at.elapsed() >= MIN_DRAW) {
            if terminal.draw(|frame| view::render(frame, &mut app)).is_err() {
                break;
            }
            last_draw = Some(Instant::now());
            dirty = false;
        }
        if app.leave.is_some() {
            break;
        }
        let tick_at = if app.animating() { Some(*next_tick.get_or_insert_with(|| Instant::now() + FRAME)) } else { None };
        if tick_at.is_none() {
            next_tick = None;
        }
        let draw_at = if dirty { last_draw.map(|at| at + MIN_DRAW) } else { None };
        let wake = [tick_at, draw_at].into_iter().flatten().min();
        let sleeping = async {
            match wake {
                Some(at) => tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            input = rx.recv() => {
                let Some(input) = input else { break };
                handle(&mut app, input, &client, &tx);
                // Whatever else is waiting is part of the same picture.
                while let Ok(more) = rx.try_recv() {
                    handle(&mut app, more, &client, &tx);
                }
                dirty = true;
            }
            () = sleeping => {
                let now = Instant::now();
                if tick_at.is_some_and(|at| now >= at) {
                    next_tick = Some(now + FRAME);
                    handle(&mut app, Input::Tick, &client, &tx);
                }
                dirty = true;
            }
        }
    }
    drop(terminal);
    drop(_restore);
    if app.leave == Some(Leave::Gone) {
        return out.fail("cli.error.gone", UNREACHED);
    }
    DONE
}

fn handle(app: &mut App, input: Input, client: &Arc<Client>, tx: &Tx) {
    for effect in app.update(input, Instant::now()) {
        run_effect(client, tx, effect);
    }
}

/// One command, with a refusal turned into the key of its reason.
async fn ask(client: &Client, cmd: &str, args: Value) -> Result<Value, String> {
    match client.call(cmd, args).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(key)) => Err(key),
        Err(_) => Err("cli.error.channel".to_string()),
    }
}

/// What the program does for an effect: the request goes out on its own task, and its answer comes back as an input.
fn run_effect(client: &Arc<Client>, tx: &Tx, effect: Effect) {
    let (client, tx) = (client.clone(), tx.clone());
    tokio::spawn(async move {
        let failed = |key: String| {
            let _ = tx.send(Input::Failed(key));
        };
        match effect {
            Effect::State => match ask(&client, "state", Value::Null).await {
                Ok(state) => {
                    let _ = tx.send(Input::State(state));
                }
                Err(key) => failed(key),
            },
            Effect::Open(id) => match ask(&client, "open", json!({ "id": id })).await {
                Ok(conversation) => {
                    let _ = tx.send(Input::Opened(id, conversation));
                }
                Err(key) => failed(key),
            },
            Effect::Send(req) => match ask(&client, "send", json!({ "req": req })).await {
                Ok(sent) => {
                    let id = sent
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| req["conversationId"].as_str().map(str::to_string))
                        .unwrap_or_default();
                    let _ = tx.send(Input::Sent(id));
                }
                Err(key) => failed(key),
            },
            Effect::Call { cmd, args, refresh } => match ask(&client, cmd, args).await {
                Ok(_) if refresh => match ask(&client, "state", Value::Null).await {
                    Ok(state) => {
                        let _ = tx.send(Input::State(state));
                    }
                    Err(key) => failed(key),
                },
                Ok(_) => {}
                Err(key) => failed(key),
            },
            Effect::Accounts => {
                for vendor in app::VENDORS {
                    if let Ok(check) = ask(&client, "setup_check", json!({ "vendor": vendor })).await {
                        let _ = tx.send(Input::Account(check));
                    }
                }
            }
        }
    });
}

// ---- the terminal ----

fn open_terminal() -> std::io::Result<Terminal<CrosstermBackend<Stdout>>> {
    // A panic leaves the terminal as it found it before the message is printed.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
    enable_raw_mode()?;
    let mut out = stdout();
    crossterm::execute!(out, EnterAlternateScreen)?;
    // A console without these still works: the wheel and pasting just arrive as plain keys.
    let _ = crossterm::execute!(out, EnableMouseCapture);
    let _ = crossterm::execute!(out, EnableBracketedPaste);
    Terminal::new(CrosstermBackend::new(out))
}

fn restore() {
    let _ = disable_raw_mode();
    let mut out = stdout();
    // Each on its own: one the console never had must not keep the others from running.
    let _ = crossterm::execute!(out, DisableBracketedPaste);
    let _ = crossterm::execute!(out, DisableMouseCapture);
    let _ = crossterm::execute!(out, LeaveAlternateScreen, crossterm::cursor::Show);
}

/// Puts the terminal back when the screen ends, however it ends.
struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        restore();
    }
}

/// One thread waits for the terminal's input and hands it over; it never wakes on its own.
fn read_keys(tx: Tx) {
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            let input = match event {
                Event::Key(key) => Input::Key(key),
                Event::Paste(text) => Input::Paste(text),
                Event::Resize(w, h) => Input::Resize(w, h),
                // Moving the pointer is not worth a redraw.
                Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Drag(_) | MouseEventKind::Up(_)) => {
                    continue;
                }
                Event::Mouse(mouse) => Input::Mouse(mouse),
                _ => continue,
            };
            if tx.send(input).is_err() {
                return;
            }
        }
    });
}
