//! `faber rpc`: the channel itself on stdin and stdout, for a program that drives Faber as a harness. The lines are the
//! channel's (`{"id","cmd","args"}` in, `{"id","ok"|"err"}` and `{"event"}` out); `hello` is answered with what Faber said
//! to this proxy, which spoke first for it, and `subscribe` starts the events.

use crate::args::Args;
use crate::connect;
use crate::out::{DONE, UNREACHED, USAGE};
use faber_channel::wire::{self, Request};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

pub async fn run(args: &Args) -> i32 {
    let client = match connect::connect(args, "faber rpc").await {
        Ok(client) => Arc::new(client),
        Err(why) => {
            eprintln!("{}", why.key());
            return UNREACHED;
        }
    };
    let (tx, mut rx) = mpsc::channel::<String>(faber_channel::client::EVENTS_KEPT);
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(line) = rx.recv().await {
            if stdout.write_all(line.as_bytes()).await.is_err() || stdout.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = stdout.flush().await;
        }
    });
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut buffer = Vec::new();
    let mut running = tokio::task::JoinSet::new();
    // Past this many requests in flight, stdin waits: a stalled stdout cannot pile up tasks without end.
    let in_flight = Arc::new(tokio::sync::Semaphore::new(faber_channel::server::MAX_IN_FLIGHT));
    let mut events_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut code = DONE;
    while let Ok(Some(line)) = wire::read_line(&mut reader, &mut buffer).await {
        // Answers already written leave the set, so a long session keeps only what is in flight.
        while running.try_join_next().is_some() {}
        if line.trim().is_empty() {
            continue;
        }
        let Ok(Request { id, cmd, args }) = serde_json::from_str::<Request>(&line) else {
            eprintln!("cli.error.json");
            code = USAGE;
            break;
        };
        let (client, tx) = (client.clone(), tx.clone());
        match cmd.as_str() {
            "hello" => {
                let _ = tx.send(wire::reply(id, &Ok(client.offer.clone()))).await;
            }
            "subscribe" => {
                events_task.get_or_insert(tokio::spawn(async move {
                    match client.subscribe().await {
                        Ok(mut events) => {
                            let _ = tx.send(wire::reply(id, &Ok(Value::Bool(true)))).await;
                            while let Some(event) = events.recv().await {
                                if tx.send(wire::event(&event)).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx
                                .send(wire::reply(
                                    id,
                                    &Err(if e.kind() == std::io::ErrorKind::AlreadyExists {
                                        "cli.error.subscribed"
                                    } else {
                                        "cli.error.channel"
                                    }
                                    .into()),
                                ))
                                .await;
                        }
                    }
                }));
            }
            _ => {
                let Ok(place) = in_flight.clone().acquire_owned().await else { break };
                running.spawn(async move {
                    let result = client.call(&cmd, args).await.unwrap_or_else(|_| Err("cli.error.channel".into()));
                    let _ = tx.send(wire::reply(id, &result)).await;
                    drop(place);
                });
            }
        }
    }
    // The answers still on their way are written before leaving; the events stop with stdin.
    while running.join_next().await.is_some() {}
    if let Some(events) = events_task {
        events.abort();
    }
    drop(tx);
    let _ = writer.await;
    code
}
