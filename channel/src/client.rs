//! The command line's side of the channel: one connection, many requests in flight, events on request.

use crate::place::Address;
use crate::server::Hello;
use crate::wire::{self, FromFaber};
use serde_json::Value;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

/// The requests waiting for their answer; `None` once Faber closed the channel, so a later request fails at once.
type Pending = Arc<Mutex<Option<HashMap<u64, oneshot::Sender<Result<Value, wire::Error>>>>>>;

/// Events kept for a slow reader, such as `faber mcp` between two chats. Past this many, newer ones are dropped and the
/// reader is told it lagged, as Faber tells a slow connection.
pub const EVENTS_KEPT: usize = 256;

pub struct Client {
    tx: mpsc::Sender<String>,
    pending: Pending,
    next: AtomicU64,
    events: Mutex<Option<mpsc::Receiver<Value>>>,
    /// What Faber answered to `hello`: its version, protocol and command table.
    pub offer: Value,
}

fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "Faber closed the channel")
}

impl Client {
    /// Connects to the Faber whose data is in `data`. `NotFound` means no Faber serves it.
    pub async fn connect(data: &Path, mut hello: Hello) -> io::Result<Client> {
        if hello.secret.is_empty() {
            hello.secret = crate::secret::Secret::read(data).map_err(|e| io::Error::new(io::ErrorKind::NotFound, e))?.hex();
        }
        if hello.token.is_none() {
            hello.token = std::env::var("FABER_AGENT_TOKEN").ok();
        }
        match Address::of(data) {
            Address::Pipe(name) => Client::over(open_pipe(&name).await?, hello).await,
            Address::Socket(path) => Client::over(open_socket(&path).await?, hello).await,
        }
    }

    /// Talks over any connected stream, and says `hello` first.
    pub async fn over<S: AsyncRead + AsyncWrite + Send + 'static>(stream: S, hello: Hello) -> io::Result<Client> {
        let (read, mut write) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::channel::<String>(64);
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if write.write_all(line.as_bytes()).await.is_err() || write.write_all(b"\n").await.is_err() {
                    break;
                }
            }
        });
        let pending: Pending = Arc::new(Mutex::new(Some(HashMap::new())));
        let (events_tx, events_rx) = mpsc::channel(EVENTS_KEPT);
        let waiting = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(read);
            let mut buffer = Vec::new();
            let mut lagging = false;
            while let Ok(Some(line)) = wire::read_line(&mut reader, &mut buffer, wire::MAX_LINE).await {
                match wire::parse_from_faber(&line) {
                    Some(FromFaber::Reply { id, result }) => {
                        let asker = waiting.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|w| w.remove(&id));
                        if let Some(asker) = asker {
                            let _ = asker.send(result);
                        }
                    }
                    Some(FromFaber::Event(event)) => {
                        // The last place goes to the lag notice at once, so the reader learns what it missed even when no
                        // event follows. Until it reads further, newer events are dropped: the notice makes it read the state again.
                        if lagging && events_tx.capacity() >= 2 {
                            lagging = false;
                        }
                        if lagging {
                            continue;
                        }
                        if events_tx.capacity() < 2 {
                            lagging = true;
                            let _ = events_tx.try_send(serde_json::json!({ "type": "lagged" }));
                        } else {
                            let _ = events_tx.try_send(event);
                        }
                    }
                    None => {}
                }
            }
            // Every request still waiting, and every one sent from now on, learns that Faber is gone.
            waiting.lock().unwrap_or_else(|e| e.into_inner()).take();
        });
        let mut client = Client { tx, pending, next: AtomicU64::new(1), events: Mutex::new(Some(events_rx)), offer: Value::Null };
        let hello = serde_json::to_value(hello).map_err(io::Error::other)?;
        client.offer = client.call("hello", hello).await?.map_err(io::Error::other)?;
        Ok(client)
    }

    /// Sends one command and waits for its answer: `Err` for the channel, `Ok(Err(key))` for a refusal.
    pub async fn call(&self, cmd: &str, args: Value) -> io::Result<Result<Value, String>> {
        Ok(self.call_detailed(cmd, args).await?.map_err(|error| error.key))
    }

    pub async fn call_detailed(&self, cmd: &str, args: Value) -> io::Result<Result<Value, wire::Error>> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (asker, answer) = oneshot::channel();
        self.pending.lock().unwrap_or_else(|e| e.into_inner()).as_mut().ok_or_else(closed)?.insert(id, asker);
        self.tx.send(wire::request(id, cmd, args)).await.map_err(|_| closed())?;
        answer.await.map_err(|_| closed())
    }

    /// Starts the events on this connection; they arrive in order until Faber closes it. Only the first call has them.
    pub async fn subscribe(&self) -> io::Result<mpsc::Receiver<Value>> {
        let events = self.events.lock().unwrap_or_else(|e| e.into_inner()).take();
        let events = events.ok_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "already subscribed"))?;
        self.call("subscribe", Value::Null).await?.map_err(io::Error::other)?;
        Ok(events)
    }
}

#[cfg(windows)]
async fn open_pipe(name: &str) -> io::Result<tokio::net::windows::named_pipe::NamedPipeClient> {
    use tokio::net::windows::named_pipe::ClientOptions;
    const PIPE_BUSY: i32 = 231;
    let started = std::time::Instant::now();
    let pipe = loop {
        match ClientOptions::new().open(name) {
            Ok(pipe) => break pipe,
            // Every instance is taken for a moment while Faber makes the next one.
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && started.elapsed() < std::time::Duration::from_secs(2) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => return Err(e),
        }
    };
    // A pipe another account made under Faber's name would read everything sent to it: such a pipe is never used.
    if !faber_winproc::pipe_peer_is_user(&pipe, false)? {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the pipe belongs to another account"));
    }
    Ok(pipe)
}

#[cfg(not(windows))]
async fn open_pipe(_: &str) -> io::Result<tokio::io::DuplexStream> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "named pipes are Windows only"))
}

#[cfg(unix)]
async fn open_socket(path: &Path) -> io::Result<tokio::net::UnixStream> {
    let stream = tokio::net::UnixStream::connect(path).await?;
    if stream.peer_cred()?.uid() != rustix::process::geteuid().as_raw() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the socket belongs to another account"));
    }
    Ok(stream)
}

#[cfg(not(unix))]
async fn open_socket(_: &Path) -> io::Result<tokio::io::DuplexStream> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "Unix sockets are not used here"))
}
