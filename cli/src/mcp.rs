//! MCP over standard input and output, using the caller's channel identity.
use crate::{args::Args, connect, out};
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};

const MAX_REQUEST: usize = 1024 * 1024;

fn tools(commands: &Value) -> Vec<Value> {
    commands
        .as_array()
        .into_iter()
        .flatten()
        .map(|command| {
            json!({"name":command["name"], "description":command["about"].as_str().unwrap_or("Faber command"),
            "inputSchema":command["inputSchema"]})
        })
        .collect()
}

fn reply(id: &Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "result":result})
}
fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0", "id":id, "error":{"code":code,"message":message}})
}

pub async fn run(args: &Args) -> i32 {
    let token = std::env::var("FABER_AGENT_TOKEN").ok().filter(|s| !s.is_empty());
    let conversation = std::env::var("FABER_CONVERSATION").ok().filter(|s| !s.is_empty());
    let Some(data) = connect::data_dir(args) else { return out::UNREACHED };
    let mut hello = connect::hello("faber mcp");
    hello.token = token.clone();
    if let Some(id) = &conversation {
        hello.client.push_str(&format!(" conversation={id}"));
    }
    hello.agent_depth = hello.agent_depth.max(1);
    let Ok(client) = faber_channel::Client::connect(&data, hello).await else { return out::UNREACHED };
    if client.offer["caller"]["kind"] != "agent"
        || token.is_some() && conversation.as_ref().is_some_and(|id| client.offer["caller"]["conversation"] != *id)
    {
        return out::UNREACHED;
    }
    let definitions = tools(&client.offer["commands"]);
    let mut input = BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = faber_channel::wire::read_line(&mut input, &mut buffer, MAX_REQUEST).await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                let value = error(&Value::Null, -32700, "Parse error");
                if stdout.write_all(format!("{value}\n").as_bytes()).await.is_err() {
                    break;
                }
                continue;
            }
        };
        let Some(id) = request.get("id") else { continue };
        let response = match request["method"].as_str() {
            Some("initialize") => {
                let requested = request["params"]["protocolVersion"].as_str().unwrap_or("");
                let version = if ["2024-11-05", "2025-03-26", "2025-06-18"].contains(&requested) { requested } else { "2025-06-18" };
                reply(
                    id,
                    json!({"protocolVersion":version, "capabilities":{"tools":{}}, "serverInfo":{"name":"faber","version":env!("CARGO_PKG_VERSION")}}),
                )
            }
            Some("ping") => reply(id, json!({})),
            Some("tools/list") => reply(id, json!({"tools":definitions})),
            Some("tools/call") => {
                let name = request["params"]["name"].as_str().unwrap_or("");
                let arguments = request["params"].get("arguments").cloned().unwrap_or(json!({}));
                let (value, failed) = if !definitions.iter().any(|tool| tool["name"] == name) {
                    (json!({"key":"error.host.unknownCommand"}), true)
                } else if !arguments.is_object() {
                    (json!({"key":"error.host.badArgs"}), true)
                } else {
                    match client.call_detailed(name, arguments).await {
                        Ok(Ok(value)) => (value, false),
                        Ok(Err(err)) => (json!({"key":err.key,"data":err.data}), true),
                        Err(_) => (json!({"key":"error.channel.closed"}), true),
                    }
                };
                reply(id, json!({"content":[{"type":"text","text":value.to_string()}],"isError":failed}))
            }
            _ => error(id, -32601, "Method not found"),
        };
        if stdout.write_all(format!("{response}\n").as_bytes()).await.is_err() {
            break;
        }
    }
    out::DONE
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mcp_tools_cover_the_command_table() {
        let commands: Value = serde_json::from_str(include_str!("../commands.json")).unwrap();
        let definitions = tools(&commands);
        assert_eq!(definitions.len(), commands.as_array().unwrap().len());
        for command in commands.as_array().unwrap() {
            assert!(definitions.iter().any(|tool| tool["name"] == command["name"]));
        }
        assert!(definitions.iter().all(|tool| tool["inputSchema"]["type"] == "object"));
    }

    #[tokio::test]
    async fn mcp_rejects_oversized_input() {
        let bytes = vec![b'x'; MAX_REQUEST + 1];
        let mut input = BufReader::new(bytes.as_slice());
        assert!(faber_channel::wire::read_line(&mut input, &mut Vec::new(), MAX_REQUEST).await.is_err());
    }
}
