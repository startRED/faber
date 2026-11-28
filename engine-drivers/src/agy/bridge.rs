//! The one local MCP connection owned by an Antigravity process.

use crate::events::Delivery;
use crate::helper::{self, Mcp, Tools};
use serde_json::{Value, json};
use std::io;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;

pub(super) const ENV: &str = "FABER_AGY_MCP_SECRET";
const MAX_REQUEST: u64 = 1024 * 1024;
const MAX_PENDING: usize = 32;

pub(super) struct Bridge {
    listener: TcpListener,
    pub secret: String,
    pub port: u16,
}

pub(super) struct Reply {
    value: Value,
    ack: Option<oneshot::Sender<Delivery>>,
}

impl Bridge {
    pub fn bind() -> io::Result<Self> {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let secret = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
        Ok(Self { listener: TcpListener::from_std(listener)?, secret, port })
    }

    pub async fn run(self, requests: mpsc::Sender<Value>, replies: mpsc::Receiver<Reply>) -> io::Result<()> {
        let mut handshakes = JoinSet::new();
        loop {
            tokio::select! {
                biased;
                attempt = handshakes.join_next(), if !handshakes.is_empty() => {
                    if let Some(Ok(Some(stream))) = attempt {
                        // One authenticated connection spends the secret, even if two attempts raced.
                        drop(self.listener);
                        drop(handshakes);
                        return relay(stream, requests, replies).await;
                    }
                }
                // Leave queued sockets in the OS backlog instead of dropping a legitimate child behind slow peers.
                incoming = self.listener.accept(), if handshakes.len() < 16 => {
                    let (mut stream, _) = match incoming {
                        Ok(connection) => connection,
                        Err(error) if matches!(error.kind(), io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset) => continue,
                        Err(error) => return Err(error),
                    };
                    let secret = self.secret.clone();
                    handshakes.spawn(async move {
                        matches!(tokio::time::timeout(Duration::from_secs(1), authenticate(&mut stream, secret.as_bytes())).await, Ok(true))
                            .then_some(stream)
                    });
                }
            }
        }
    }
}

async fn authenticate(stream: &mut (impl AsyncRead + Unpin), secret: &[u8]) -> bool {
    let mut candidate = [0u8; 64];
    stream.read_exact(&mut candidate).await.is_ok() && candidate.iter().zip(secret).fold(0, |diff, (a, b)| diff | (a ^ b)) == 0
}

async fn relay(
    stream: impl AsyncRead + AsyncWrite + Unpin,
    requests: mpsc::Sender<Value>,
    mut replies: mpsc::Receiver<Reply>,
) -> io::Result<()> {
    let (read, mut write) = tokio::io::split(stream);
    let receive = async {
        let mut reader = BufReader::new(read);
        let mut line = Vec::new();
        loop {
            line.clear();
            if (&mut reader).take(MAX_REQUEST).read_until(b'\n', &mut line).await? == 0 {
                return Ok(());
            }
            if line.last() != Some(&b'\n') {
                return Err(io::Error::other("incomplete MCP request"));
            }
            let Ok(message) = serde_json::from_slice::<Value>(&line) else {
                return Err(io::Error::other("invalid MCP request"));
            };
            if requests.send(message).await.is_err() {
                return Ok(());
            }
        }
    };
    let send = async {
        while let Some(Reply { value, ack }) = replies.recv().await {
            if ack.as_ref().is_some_and(oneshot::Sender::is_closed) {
                continue;
            }
            let mut line = serde_json::to_vec(&value)?;
            line.push(b'\n');
            write.write_all(&line).await?;
            if let Some(ack) = ack {
                let _ = ack.send(Delivery::Delivered);
            }
        }
        Ok(())
    };
    tokio::select! {
        result = receive => result,
        result = send => result,
    }
}

#[derive(Default)]
pub(super) struct Calls {
    pending: std::collections::HashMap<String, (Value, bool)>,
    approved: Vec<Approved>,
}

struct Approved {
    tool: &'static str,
    id: String,
    input: Value,
}

fn without_summary(mut input: Value) -> Value {
    if let Some(fields) = input.as_object_mut() {
        fields.remove("toolSummary");
    }
    input
}

impl Calls {
    pub fn approve(&mut self, tool: &'static str, id: String, input: Value) -> bool {
        if self.approved.iter().any(|call| call.id == id) || self.pending.contains_key(&id) {
            return false;
        }
        if self.approved.len() >= MAX_PENDING {
            self.approved.remove(0);
        }
        self.approved.push(Approved { tool, id, input: without_summary(input) });
        true
    }

    pub fn observed_result(&mut self, id: &str) {
        self.approved.retain(|call| call.id != id);
    }

    fn take_approval(&mut self, tool: &str, input: &Value) -> Option<String> {
        let same_args = without_summary(input.clone());
        let exact = self.approved.iter().position(|call| call.tool == tool && call.input == same_args);
        let at = exact.or_else(|| {
            let mut matching = self.approved.iter().enumerate().filter(|(_, call)| call.tool == tool);
            let (at, call) = matching.next()?;
            let unknown = call.input.is_null() || call.input.as_object().is_some_and(serde_json::Map::is_empty);
            (unknown && matching.next().is_none()).then_some(at)
        })?;
        Some(self.approved.remove(at).id)
    }

    pub fn receive(&mut self, mut request: Value, tools: Tools, active: bool, sink: &crate::claude::EventSink) -> Option<Reply> {
        if request["id"].is_null() {
            return None;
        }
        // Only Claude's trusted transport supplies its own tool-use identifier.
        if let Some(params) = request.get_mut("params").and_then(Value::as_object_mut) {
            params.remove("_meta");
        }
        match helper::mcp(&request, tools) {
            Mcp::Answer(mut value) => {
                if request["method"] == "initialize" && value["result"]["protocolVersion"].is_null() {
                    value["result"]["protocolVersion"] = json!("2025-03-26");
                }
                Some(Reply { value, ack: None })
            }
            Mcp::Call { rpc, .. } if !active || self.pending.len() >= MAX_PENDING => Some(Reply {
                value: helper::mcp_result(&rpc, false, "The turn is inactive or has too many pending calls.", &[]),
                ack: None,
            }),
            Mcp::Call { tool, id, rpc, input } => {
                let approved = self.take_approval(tool, &input);
                let separate_step = approved.is_none();
                let id = approved.unwrap_or(id);
                self.pending.insert(id.clone(), (rpc, separate_step));
                // ACP can show only a summary. Use the real arguments for labels and child-work links.
                if let Some(name) = helper::tool_label(&format!("mcp__faber__{tool}")) {
                    sink(crate::events::EngineEvent::Tool { id: id.clone(), name: name.into(), input: input.clone(), parent: None });
                }
                sink(helper::tool_event(tool, id, input));
                None
            }
        }
    }

    pub fn finish(
        &mut self,
        id: &str,
        ok: bool,
        text: &str,
        pictures: &[String],
        ack: Option<oneshot::Sender<Delivery>>,
        sink: &crate::claude::EventSink,
    ) -> Option<Reply> {
        let (rpc, separate_step) = self.pending.remove(id)?;
        if separate_step {
            sink(crate::events::EngineEvent::ToolResult { id: id.into(), ok, declined: false, exit_code: None, text: text.into() });
        }
        Some(Reply { value: helper::mcp_result(&rpc, ok, text, pictures), ack })
    }

    pub fn cancel(&mut self, sink: &crate::claude::EventSink) -> impl Iterator<Item = Reply> {
        self.approved.clear();
        for (id, (_, separate_step)) in &self.pending {
            if *separate_step {
                sink(crate::events::EngineEvent::ToolResult {
                    id: id.clone(),
                    ok: false,
                    declined: false,
                    exit_code: None,
                    text: "The turn has ended.".into(),
                });
            }
        }
        self.pending.drain().map(|(_, (rpc, _))| Reply { value: helper::mcp_result(&rpc, false, "The turn has ended.", &[]), ack: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn authentication_does_not_consume_the_first_mcp_message() {
        let (mut client, mut engine) = tokio::io::duplex(128);
        let secret = [b'a'; 64];
        client.write_all(&secret).await.unwrap();
        client.write_all(b"{}\n").await.unwrap();
        assert!(authenticate(&mut engine, &secret).await);
        let mut first = [0; 3];
        engine.read_exact(&mut first).await.unwrap();
        assert_eq!(&first, b"{}\n");
    }

    #[test]
    fn a_helper_call_uses_the_approved_step_id_instead_of_client_metadata() {
        let mut calls = Calls::default();
        let input = json!({"title":"Check the result","brief":"Read the output."});
        assert!(calls.approve(helper::NAME, "observed-step".into(), input.clone()));
        let request = json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"helper","arguments":input,
            "_meta":{"claudecode/toolUseId":"forged-step"}}});
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sink: crate::claude::EventSink = std::sync::Arc::new(move |event| {
            let _ = tx.send(event);
        });
        assert!(calls.receive(request, Tools { helper: true, ..Tools::default() }, true, &sink).is_none());
        assert!(matches!(rx.try_recv().unwrap(), crate::events::EngineEvent::Tool { id, name, input, .. }
            if id == "observed-step" && name == "Helper" && input["title"] == "Check the result"));
        assert!(matches!(rx.try_recv().unwrap(), crate::events::EngineEvent::HelperCall { id, .. } if id == "observed-step"));
        assert!(calls.finish("forged-step", true, "wrong", &[], None, &sink).is_none());
        assert_eq!(calls.finish("observed-step", true, "done", &[], None, &sink).unwrap().value["id"], 7);
    }
}
