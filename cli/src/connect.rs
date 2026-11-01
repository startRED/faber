//! Reaching the running Faber, and starting it hidden in the tray when nothing answers.

use crate::args::Args;
use faber_channel::{Client, Hello};
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
    let started = std::time::Instant::now();
    while started.elapsed() < START_WAIT {
        tokio::time::sleep(Duration::from_millis(150)).await;
        match Client::connect(&data, hello(client)).await {
            Ok(client) => return Ok(client),
            Err(e) if e.to_string() == "error.channel.revoked" => return Err(Unreached::Revoked),
            Err(e) if !absent(&e) => return Err(Unreached::Broken),
            Err(_) => {}
        }
    }
    Err(Unreached::NotRunning)
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
    let mut command = std::process::Command::new(app);
    command.arg(HIDDEN_START).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    if faber_channel::place::data_dir().as_deref() != Some(data) {
        command.env("FABER_DATA_DIR", data);
    }
    // The agent marker is this terminal's, never Faber's.
    command.env_remove("FABER_AGENT_DEPTH");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: closing this terminal or pressing Ctrl+C here leaves Faber running.
        command.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    command.spawn().map(drop)
}
