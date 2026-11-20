//! Reaching the running Faber, and starting it hidden in the tray when nothing answers.

use crate::args::Args;
use faber_channel::wire::Error;
use faber_channel::{Client, Hello};
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a Faber started hidden has to open its channel.
const START_WAIT: Duration = Duration::from_secs(10);

/// The argument that starts Faber with no window, as autostart does.
pub const HIDDEN_START: &str = "--autostart";

pub fn data_dir(args: &Args) -> Option<PathBuf> {
    args.value("data").map(PathBuf::from).or_else(faber_channel::place::data_dir)
}

/// How many AI turns sit above this process: Faber sets it on every CLI it starts, so an AI in Faber that calls `faber` says so.
pub fn agent_depth() -> u32 {
    std::env::var("FABER_AGENT_DEPTH").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}

pub fn hello(client: &str) -> Hello {
    Hello { client: format!("{client} {}", env!("CARGO_PKG_VERSION")), agent_depth: agent_depth(), ..Default::default() }
}

/// Uses the connected app's read capabilities for both the terminal and MCP.
pub async fn call(client: &Client, command: &str, args: Value) -> io::Result<Result<Value, Error>> {
    if let Some(detail) = ignored_filter(&client.offer, command, &args) {
        return Ok(Err(Error { key: "cli.error.refused".into(), detail: Some(detail.into()), data: None }));
    }
    client.call_detailed(command, args).await
}

fn ignored_filter(offer: &Value, command: &str, args: &Value) -> Option<&'static str> {
    let properties = || {
        offer["commands"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|entry| entry["name"] == command)
            .map(|entry| &entry["inputSchema"]["properties"])
    };
    match command {
        // Older hosts ignore unknown arguments and would read the entire history.
        "conversation" if !args["from"].is_null() || !args["limit"].is_null() => {
            (!properties().is_some_and(|p| p.get("from").is_some() && p.get("limit").is_some())).then_some("conversation: from, limit")
        }
        "conversation_command_results" if !args["runningOnly"].is_null() && args["runningOnly"] != false => {
            properties().is_none_or(|p| p.get("runningOnly").is_none()).then_some("conversation_command_results: runningOnly")
        }
        "conversations" => {
            let supported = properties().and_then(|p| p["state"]["items"]["enum"].as_array());
            let ignores_state = args["state"].as_array().into_iter().flatten().filter_map(Value::as_str).any(|state| {
                !matches!(state, "working" | "waiting" | "idle" | "running")
                    && !supported.is_some_and(|words| words.iter().any(|word| word == state))
            });
            ignores_state.then_some("conversations: state")
        }
        _ => None,
    }
}

#[derive(Debug)]
pub enum Unreached {
    /// No data folder could be found (no home folder).
    NoData,
    /// Nothing answers, and Faber was not to be started, or could not be.
    NotRunning,
    /// The channel exists but refused or broke.
    Broken,
    Revoked,
}

impl Unreached {
    pub fn key(&self) -> &'static str {
        match self {
            Unreached::NoData => "cli.error.noData",
            Unreached::NotRunning => "cli.error.noFaber",
            Unreached::Broken => "cli.error.channel",
            Unreached::Revoked => "error.channel.revoked",
        }
    }
}

/// Connects to Faber, starting it hidden first unless `--no-start` says not to.
pub async fn connect(args: &Args, client: &str) -> Result<Client, Unreached> {
    let data = data_dir(args).ok_or(Unreached::NoData)?;
    match Client::connect(&data, hello(client)).await {
        Ok(client) => return Ok(client),
        Err(e) if e.to_string() == "error.channel.revoked" => return Err(Unreached::Revoked),
        Err(e) if !absent(&e) => return Err(Unreached::Broken),
        // An AI inside Faber reaches the Faber that started it, or none: it never starts one with its own environment.
        Err(_) if args.on("no-start") || agent_depth() > 0 => return Err(Unreached::NotRunning),
        Err(_) => {}
    }
    start_hidden(&data).map_err(|_| Unreached::NotRunning)?;
    wait_for_started(async || Client::connect(&data, hello(client)).await).await
}

async fn wait_for_started<T>(mut connect: impl AsyncFnMut() -> io::Result<T>) -> Result<T, Unreached> {
    let deadline = tokio::time::sleep(START_WAIT);
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => return Err(Unreached::NotRunning),
            _ = tokio::time::sleep(Duration::from_millis(150)) => {}
        }
        let result = tokio::select! {
            biased;
            _ = &mut deadline => return Err(Unreached::Broken),
            result = connect() => result,
        };
        match result {
            Ok(client) => return Ok(client),
            Err(e) if e.to_string() == "error.channel.revoked" => return Err(Unreached::Revoked),
            Err(e) if !absent(&e) => return Err(Unreached::Broken),
            Err(_) => {}
        }
    }
}

/// Nothing listens: no pipe or socket, or (on macOS and Linux) a socket a crash left behind, which Faber clears when it starts.
fn absent(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::NotFound || (cfg!(unix) && e.kind() == io::ErrorKind::ConnectionRefused)
}

/// The app beside this program: `bin\faber.exe` ships in the app's folder, so the app is one folder up. `FABER_APP` names
/// another one, for development.
fn app() -> Option<PathBuf> {
    if let Some(app) = std::env::var_os("FABER_APP") {
        return Some(PathBuf::from(app));
    }
    let here = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "faber.exe" } else { "faber" };
    let app = here.parent()?.parent()?.join(name);
    app.is_file().then_some(app)
}

/// Starts Faber with no window, detached from this terminal and never through a shell. The data folder goes with it only
/// when it is not the default one, so a test Faber keeps its own.
fn start_hidden(data: &Path) -> io::Result<()> {
    let app = app().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Faber's program"))?;
    #[cfg(windows)]
    let app = std::path::absolute(app)?;
    let mut command = std::process::Command::new(app);
    command.arg(HIDDEN_START);
    if faber_channel::place::data_dir().as_deref() != Some(data) {
        command.env("FABER_DATA_DIR", data);
    }
    // The agent identity is this terminal's, never Faber's.
    for key in ["FABER_AGENT_DEPTH", "FABER_AGENT_AUTH", "FABER_AGENT_TOKEN", "FABER_CONVERSATION"] {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        faber_winproc::spawn_detached(&command)
    }
    #[cfg(not(windows))]
    {
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant;

    #[tokio::test(start_paused = true)]
    async fn a_start_that_never_opens_the_channel_is_not_running() {
        let started = Instant::now();
        let result = wait_for_started(async || Err::<(), _>(io::ErrorKind::NotFound.into())).await;
        assert!(matches!(result, Err(Unreached::NotRunning)));
        assert_eq!(started.elapsed(), START_WAIT);
    }

    #[tokio::test(start_paused = true)]
    async fn a_late_connection_does_not_get_extra_time_after_start() {
        let started = Instant::now();
        let result = wait_for_started(async || {
            if started.elapsed() < Duration::from_secs(9) {
                return Err(io::ErrorKind::NotFound.into());
            }
            std::future::pending::<io::Result<()>>().await
        })
        .await;
        assert!(matches!(result, Err(Unreached::Broken)));
        assert_eq!(started.elapsed(), START_WAIT);
    }
}
