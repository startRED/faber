//! Faber's side of the channel. Each connection says `hello` first, then sends commands from the command table; a
//! `subscribe` starts the events on that connection. The answers run at once and come back as they finish, in any order.

use crate::guard::{Guard, PERSON_ONLY};
use crate::place::Address;
use crate::wire::{self, Request};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Semaphore, broadcast, mpsc};

pub type Answer = Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>;

/// Whatever hosts Faber's commands: the same table, names and arguments the screen uses.
pub trait Backend: Send + Sync + 'static {
    /// Faber's version, as the screen shows it.
    fn version(&self) -> String;
    /// The command table, for `hello`: `[{"name", "args": [[name, type]], "kind"}]`.
    fn commands(&self) -> Value;
    /// Whether a command starts a turn of an AI (the table's `StartsTurn`).
    fn starts_turn(&self, cmd: &str) -> bool;
    fn dispatch(&self, cmd: String, args: Value) -> Answer;
    fn subscribe(&self) -> broadcast::Receiver<Value>;
    /// Whether a connected client is a program an AI that Faber started runs, whatever its hello says: on Windows, one in
    /// a job Faber's CLIs run in, which nothing they start can leave. A backend that starts no CLI knows of none.
    #[cfg(windows)]
    fn started_by_an_ai(&self, _client: &faber_winproc::PipeClient) -> bool {
        false
    }
}

/// What a client says first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    /// Who is asking (`faber 0.4.0`, `faber rpc`, `faber mcp`), for the log only.
    #[serde(default)]
    pub client: String,
    /// 0 for a person, 1 for an AI that Faber started (`FABER_AGENT_DEPTH`, set by Faber on every CLI it starts). An AI
    /// client starts turns within the limits of `guard`; a client that says more is one nobody should trust to start any.
    #[serde(default)]
    pub agent_depth: u32,
}

/// From this depth up, a client may read but starts no new turn.
pub const MAX_AGENT_DEPTH: u32 = 2;

/// Requests one connection may have in flight; the next waits for one to be answered before it is read.
pub const MAX_IN_FLIGHT: usize = 64;

/// Serves the channel of the Faber whose data is in `data` until the listener fails. A channel already served (another
/// Faber on the same data) is an error at once.
pub async fn serve(data: &Path, backend: Arc<dyn Backend>) -> io::Result<()> {
    let guard = Guard::watching(backend.subscribe());
    match Address::of(data) {
        Address::Pipe(name) => serve_pipe(&name, backend, guard).await,
        Address::Socket(path) => serve_socket(&path, backend, guard).await,
    }
}

#[cfg(windows)]
async fn serve_pipe(name: &str, backend: Arc<dyn Backend>, guard: Arc<Guard>) -> io::Result<()> {
    let mut waiting = faber_winproc::user_pipe(name, true)?;
    loop {
        let connected = waiting.connect().await;
        // A passing failure, or every instance taken, is no reason to stop serving for good.
        let next = loop {
            match faber_winproc::user_pipe(name, false) {
                Ok(next) => break next,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(500)).await,
            }
        };
        let pipe = std::mem::replace(&mut waiting, next);
        // A client that left before the connection finished is no reason to stop serving the others.
        if connected.is_ok() && faber_winproc::pipe_peer_is_user(&pipe, true).unwrap_or(false) {
            // A program an AI started is held to an AI's limits whatever its hello says: its environment may have lost
            // the mark. A client that cannot be asked is held to them too.
            let ours = u32::from(faber_winproc::PipeClient::of(&pipe).map_or(true, |client| backend.started_by_an_ai(&client)));
            tokio::spawn(connection_at(pipe, backend.clone(), guard.clone(), ours));
        }
    }
}

#[cfg(not(windows))]
async fn serve_pipe(_: &str, _: Arc<dyn Backend>, _: Arc<Guard>) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "named pipes are Windows only"))
}

#[cfg(unix)]
async fn serve_socket(path: &Path, backend: Arc<dyn Backend>, guard: Arc<Guard>) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "socket without a folder"))?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    let meta = std::fs::symlink_metadata(dir)?;
    let me = rustix::process::geteuid().as_raw();
    if !meta.is_dir() || meta.uid() != me {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the channel folder is not this user's own folder"));
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    if path.exists() {
        // A socket that answers belongs to a running Faber; one that does not was left by a crash.
        if tokio::net::UnixStream::connect(path).await.is_ok() {
            return Err(io::Error::new(io::ErrorKind::AddrInUse, "another Faber serves this channel"));
        }
        std::fs::remove_file(path)?;
    }
    let listener = tokio::net::UnixListener::bind(path)?;
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        if stream.peer_cred().is_ok_and(|peer| peer.uid() == me) {
            tokio::spawn(connection(stream, backend.clone(), guard.clone()));
        }
    }
}

#[cfg(not(unix))]
async fn serve_socket(_: &Path, _: Arc<dyn Backend>, _: Arc<Guard>) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not used here"))
}

/// One client, from its first line to its last.
pub async fn connection<S: AsyncRead + AsyncWrite + Send + 'static>(stream: S, backend: Arc<dyn Backend>, guard: Arc<Guard>) {
    connection_at(stream, backend, guard, 0).await;
}

/// One client whose agent depth is at least `at_least`, whatever its hello says.
pub async fn connection_at<S: AsyncRead + AsyncWrite + Send + 'static>(
    stream: S,
    backend: Arc<dyn Backend>,
    guard: Arc<Guard>,
    at_least: u32,
) {
    let (read, mut write) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<String>(256);
    let writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if write.write_all(line.as_bytes()).await.is_err() || write.write_all(b"\n").await.is_err() {
                break;
            }
        }
    });
    let mut reader = BufReader::new(read);
    let mut buffer = Vec::new();
    let mut hello: Option<Hello> = None;
    let mut events: Option<tokio::task::JoinHandle<()>> = None;
    let in_flight = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
    while let Ok(Some(line)) = wire::read_line(&mut reader, &mut buffer).await {
        // A line that is not a request has no id to answer: the client broke the wire, so the connection ends.
        let Ok(Request { id, cmd, args }) = serde_json::from_str::<Request>(&line) else { break };
        let answer = |result: Result<Value, String>| wire::reply(id, &result);
        let depth = hello.as_ref().map(|said| said.agent_depth.max(at_least));
        match (cmd.as_str(), depth) {
            ("hello", _) => {
                let said: Hello = serde_json::from_value(args).unwrap_or_default();
                hello = Some(said);
                let offer = json!({ "version": backend.version(), "protocol": crate::PROTOCOL, "commands": backend.commands() });
                let _ = tx.send(answer(Ok(offer))).await;
            }
            (_, None) => {
                let _ = tx.send(answer(Err("error.channel.hello".into()))).await;
            }
            ("subscribe", Some(_)) => {
                if events.is_none() {
                    events = Some(tokio::spawn(forward(backend.subscribe(), tx.clone())));
                }
                let _ = tx.send(answer(Ok(Value::Bool(true)))).await;
            }
            (_, Some(depth)) if depth > 0 && PERSON_ONLY.contains(&cmd.as_str()) => {
                let _ = tx.send(answer(Err("error.channel.personOnly".into()))).await;
            }
            (_, Some(depth)) if depth >= MAX_AGENT_DEPTH && backend.starts_turn(&cmd) => {
                let _ = tx.send(answer(Err("error.channel.agentDepth".into()))).await;
            }
            (_, Some(depth)) => {
                // The writer is gone: nobody reads the answers, so no more work is taken.
                if tx.is_closed() {
                    break;
                }
                let Ok(place) = in_flight.clone().acquire_owned().await else { break };
                let limited = depth > 0 && backend.starts_turn(&cmd);
                let ticket = match limited.then(|| guard.admit(std::time::Instant::now())) {
                    Some(Err(key)) => {
                        let _ = tx.send(answer(Err(key.into()))).await;
                        continue;
                    }
                    Some(Ok(ticket)) => Some((ticket, args.clone())),
                    None => None,
                };
                let (tx, guard) = (tx.clone(), guard.clone());
                let run = backend.dispatch(cmd, args);
                tokio::spawn(async move {
                    let result = run.await;
                    if let Some((ticket, asked)) = ticket {
                        guard.answered(ticket, &asked, result.as_ref().ok());
                    }
                    let _ = tx.send(wire::reply(id, &result)).await;
                    drop(place);
                });
            }
        }
    }
    if let Some(events) = events {
        events.abort();
    }
    drop(tx);
    let _ = writer.await;
}

/// Sends every event to one client. A client too slow to keep up is told how many it missed, so it reads the state again.
async fn forward(mut events: broadcast::Receiver<Value>, tx: mpsc::Sender<String>) {
    loop {
        let line = match events.recv().await {
            Ok(event) => wire::event(&event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => wire::event(&json!({ "type": "lagged", "skipped": skipped })),
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if tx.send(line).await.is_err() {
            return;
        }
    }
}
