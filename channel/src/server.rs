//! Faber's side of the channel. Each connection says `hello` first, then sends commands from the command table; a
//! `subscribe` starts the events on that connection. The answers run at once and come back as they finish, in any order.

use crate::guard::Guard;
use crate::place::Address;
use crate::secret::Secret;
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
pub type DetailedAnswer = Pin<Box<dyn Future<Output = Result<Value, wire::Error>> + Send>>;

/// Whatever hosts Faber's commands: the same table, names and arguments the screen uses.
pub trait Backend: Send + Sync + 'static {
    /// Faber's version, as the screen shows it.
    fn version(&self) -> String;
    /// The command table, for `hello`: `[{"name", "args": [[name, type]], "kind"}]`.
    fn commands(&self) -> Value;
    /// Whether a command starts a turn of an AI (the table's `StartsTurn`).
    fn starts_turn(&self, cmd: &str) -> bool;
    fn dispatch(&self, who: &Who, cmd: String, args: Value) -> Answer;
    fn dispatch_detailed(&self, who: &Who, cmd: String, args: Value) -> DetailedAnswer {
        let answer = self.dispatch(who, cmd, args);
        Box::pin(async move { answer.await.map_err(Into::into) })
    }
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
    #[serde(default)]
    pub secret: String,
    #[serde(default)]
    pub token: Option<String>,
}

/// The caller and the depth used for turn limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Who {
    Person,
    Agent {
        conversation: Option<String>,
        #[serde(skip)]
        depth: u32,
    },
}

/// From this depth up, a client may read but starts no new turn.
pub const MAX_AGENT_DEPTH: u32 = 2;

/// Requests one connection may have in flight; the next waits for one to be answered before it is read.
pub const MAX_IN_FLIGHT: usize = 16;
const MAX_CONNECTIONS: usize = 16;
const MAX_REQUEST: usize = 1024 * 1024;

/// Serves the channel of the Faber whose data is in `data` until the listener fails. A channel already served (another
/// Faber on the same data) is an error at once.
pub async fn serve(data: &Path, backend: Arc<dyn Backend>) -> io::Result<()> {
    let secret = Arc::new(Secret::fresh()?);
    serve_with_secret(data, backend, secret).await
}

pub(crate) async fn serve_with_secret(data: &Path, backend: Arc<dyn Backend>, secret: Arc<Secret>) -> io::Result<()> {
    let guard = Guard::watching(backend.subscribe());
    match Address::of(data) {
        Address::Pipe(name) => serve_pipe(&name, data, backend, guard, secret).await,
        Address::Socket(path) => serve_socket(&path, data, backend, guard, secret).await,
    }
}

#[cfg(windows)]
async fn serve_pipe(name: &str, data: &Path, backend: Arc<dyn Backend>, guard: Arc<Guard>, secret: Arc<Secret>) -> io::Result<()> {
    let places = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut waiting = faber_winproc::user_pipe(name, true)?;
    secret.write(data)?;
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
            accept(pipe, backend.clone(), guard.clone(), secret.clone(), ours, places.clone());
        }
    }
}

#[cfg(not(windows))]
async fn serve_pipe(_: &str, _: &Path, _: Arc<dyn Backend>, _: Arc<Guard>, _: Arc<Secret>) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "named pipes are Windows only"))
}

#[cfg(unix)]
async fn serve_socket(path: &Path, data: &Path, backend: Arc<dyn Backend>, guard: Arc<Guard>, secret: Arc<Secret>) -> io::Result<()> {
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
    secret.write(data)?;
    let places = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        if stream.peer_cred().is_ok_and(|peer| peer.uid() == me) {
            accept(stream, backend.clone(), guard.clone(), secret.clone(), 0, places.clone());
        }
    }
}

#[cfg(not(unix))]
async fn serve_socket(_: &Path, _: &Path, _: Arc<dyn Backend>, _: Arc<Guard>, _: Arc<Secret>) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not used here"))
}

fn accept<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    mut stream: S,
    backend: Arc<dyn Backend>,
    guard: Arc<Guard>,
    secret: Arc<Secret>,
    at_least: u32,
    places: Arc<Semaphore>,
) {
    match places.try_acquire_owned() {
        Ok(place) => {
            tokio::spawn(async move {
                let _place = place;
                connection_at(stream, backend, guard, secret, at_least).await;
            });
        }
        Err(_) => {
            tokio::spawn(async move {
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(5),
                    stream.write_all(format!("{{\"id\":0,\"err\":\"{}\"}}\n", "error.channel.busy").as_bytes()),
                )
                .await;
            });
        }
    }
}

/// One client whose agent depth is at least `at_least`, whatever its hello says.
pub async fn connection_at<S: AsyncRead + AsyncWrite + Send + 'static>(
    stream: S,
    backend: Arc<dyn Backend>,
    guard: Arc<Guard>,
    secret: Arc<Secret>,
    at_least: u32,
) {
    let (read, mut write) = tokio::io::split(stream);
    let (tx, mut rx) = mpsc::channel::<String>(256);
    let writer_token = Arc::new(std::sync::Mutex::new(None::<String>));
    let writer_identity = writer_token.clone();
    let writer_secret = secret.clone();
    let mut writer = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let token = writer_identity.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let revoked = || token.as_deref().is_some_and(|value| writer_secret.conversation_of(value).is_none());
            let refusal = serde_json::from_str::<Value>(&line)
                .is_ok_and(|value| value.get("id").is_some() && value["err"] == "error.channel.revoked");
            if revoked() && !refusal {
                break;
            }
            // A peer that stopped reading must not keep a connection slot forever.
            let send = async {
                write.write_all(line.as_bytes()).await?;
                write.write_all(b"\n").await
            };
            let mut changes = writer_secret.changes();
            let wait_revoked = async {
                loop {
                    if revoked() && !refusal {
                        break;
                    }
                    if changes.changed().await.is_err() {
                        break;
                    }
                }
            };
            let sent = tokio::select! {
                biased;
                _ = wait_revoked => break,
                sent = tokio::time::timeout(std::time::Duration::from_secs(5), send) => sent,
            };
            if !matches!(sent, Ok(Ok(()))) {
                break;
            }
        }
    });
    let mut reader = BufReader::new(read);
    let mut buffer = Vec::new();
    let mut who: Option<Who> = None;
    let mut token: Option<String> = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut events: Option<tokio::task::JoinHandle<()>> = None;
    let in_flight = Arc::new(Semaphore::new(MAX_IN_FLIGHT));
    loop {
        let line = tokio::select! {
            line = async {
                if who.is_none() {
                    tokio::time::timeout_at(deadline, wire::read_line(&mut reader, &mut buffer, MAX_REQUEST)).await.ok().and_then(Result::ok).flatten()
                } else {
                    wire::read_line(&mut reader, &mut buffer, MAX_REQUEST).await.ok().flatten()
                }
            } => match line { Some(line) => line, None => break },
            _ = tx.closed() => break,
        };
        // A line that is not a request has no id to answer: the client broke the wire, so the connection ends.
        let Ok(Request { id, cmd, args }) = serde_json::from_str::<Request>(&line) else { break };
        let answer = |result: Result<Value, String>| wire::reply(id, &result);
        if token.as_deref().is_some_and(|value| secret.conversation_of(value).is_none()) {
            let _ = tx.send(answer(Err("error.channel.revoked".into()))).await;
            break;
        }
        match (cmd.as_str(), who.as_ref()) {
            ("hello", None) => {
                let said: Hello = serde_json::from_value(args).unwrap_or_default();
                if !secret.said_by(&said.secret) {
                    let _ = tx.send(answer(Err("error.channel.secret".into()))).await;
                    break;
                }
                let conversation = match said.token {
                    Some(value) => match secret.conversation_of(&value) {
                        Some(id) => {
                            *writer_token.lock().unwrap_or_else(|e| e.into_inner()) = Some(value.clone());
                            token = Some(value);
                            Some(id)
                        }
                        None => {
                            let key = if secret.signed(&value) { "error.channel.revoked" } else { "error.channel.token" };
                            let _ = tx.send(answer(Err(key.into()))).await;
                            break;
                        }
                    },
                    None => None,
                };
                let depth = said.agent_depth.max(at_least);
                let caller = if conversation.is_some() || depth > 0 { Who::Agent { conversation, depth } } else { Who::Person };
                let offer =
                    json!({ "version": backend.version(), "protocol": crate::PROTOCOL, "caller": caller, "commands": backend.commands() });
                who = Some(caller);
                let _ = tx.send(answer(Ok(offer))).await;
            }
            ("hello", Some(_)) | (_, None) => {
                let _ = tx.send(answer(Err("error.channel.hello".into()))).await;
                if who.is_none() {
                    break;
                }
            }
            ("subscribe", Some(_)) => {
                if events.is_none() {
                    events = Some(tokio::spawn(forward(backend.subscribe(), tx.clone(), secret.clone(), token.clone())));
                }
                let _ = tx.send(answer(Ok(Value::Bool(true)))).await;
            }
            (_, Some(Who::Agent { depth, .. })) if *depth >= MAX_AGENT_DEPTH && backend.starts_turn(&cmd) => {
                let _ = tx.send(answer(Err("error.channel.agentDepth".into()))).await;
            }
            (_, Some(caller)) => {
                // The writer is gone: nobody reads the answers, so no more work is taken.
                if tx.is_closed() {
                    break;
                }
                let Ok(place) = in_flight.clone().acquire_owned().await else { break };
                if token.as_deref().is_some_and(|value| secret.conversation_of(value).is_none()) {
                    let _ = tx.send(answer(Err("error.channel.revoked".into()))).await;
                    break;
                }
                let limited = matches!(caller, Who::Agent { .. }) && backend.starts_turn(&cmd);
                let ticket = match limited.then(|| guard.admit(std::time::Instant::now())) {
                    Some(Err(key)) => {
                        let _ = tx.send(answer(Err(key.into()))).await;
                        continue;
                    }
                    Some(Ok(ticket)) => Some((ticket, args.clone())),
                    None => None,
                };
                let (tx, guard) = (tx.clone(), guard.clone());
                let run = backend.dispatch_detailed(caller, cmd, args);
                tokio::spawn(async move {
                    let result = run.await;
                    if let Some((ticket, asked)) = ticket {
                        if matches!(&result, Err(error) if error.key.starts_with("error.reach.") || error.key == "error.host.badArgs") {
                            guard.refund(&ticket);
                        }
                        guard.answered(ticket, &asked, result.as_ref().ok());
                    }
                    let _ = tx.send(wire::reply_detailed(id, &result)).await;
                    drop(place);
                });
            }
        }
    }
    if let Some(events) = events {
        events.abort();
    }
    drop(tx);
    if tokio::time::timeout(std::time::Duration::from_secs(5), &mut writer).await.is_err() {
        writer.abort();
    }
}

/// Sends every event to one client. A client too slow to keep up is told how many it missed, so it reads the state again.
async fn forward(mut events: broadcast::Receiver<Value>, tx: mpsc::Sender<String>, secret: Arc<Secret>, token: Option<String>) {
    let mut changes = secret.changes();
    loop {
        if token.as_deref().is_some_and(|value| secret.conversation_of(value).is_none()) {
            break;
        }
        let received = tokio::select! {
            result = events.recv() => result,
            result = changes.changed() => { if result.is_err() { break; } continue; }
        };
        let line = match received {
            Ok(event) => wire::event(&event),
            Err(broadcast::error::RecvError::Lagged(skipped)) => wire::event(&json!({ "type": "lagged", "skipped": skipped })),
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if token.as_deref().is_some_and(|value| secret.conversation_of(value).is_none()) {
            break;
        }
        if tx.send(line).await.is_err() {
            return;
        }
    }
}
