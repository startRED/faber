//! Commands exposed to terminal and program callers.
use crate::{
    args::Args,
    connect,
    out::{self, Out},
};
use faber_channel::Client;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Read,
    time::{Duration, Instant},
};

const VERBS: &[&str] = &[
    "help",
    "status",
    "list",
    "read",
    "send",
    "stop",
    "continue",
    "archive",
    "unarchive",
    "pin",
    "unpin",
    "rename",
    "projects",
    "quota",
    "settings",
    "watch",
    "wait",
    "call",
];
type ResultValue = Result<Value, i32>;
async fn ask(client: &Client, out: &Out, command: &str, args: Value) -> ResultValue {
    match client.call_detailed(command, args).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(error)) => {
            let code = if matches!(error.key.as_str(), "error.reach.ask" | "error.reach.person" | "error.reach.screen") {
                out::PERSON
            } else {
                out::REFUSED
            };
            Err(out.fail_detail(&error.key, error.detail.as_deref(), code))
        }
        Err(_) => Err(out.fail("cli.error.channel", out::UNREACHED)),
    }
}
fn input(text: &str) -> Option<String> {
    if text != "-" {
        return Some(text.into());
    }
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).ok()?;
    Some(s)
}
fn num(args: &Args, name: &str, default: u64) -> Option<u64> {
    args.value(name).map_or(Some(default), |v| v.parse().ok())
}
fn usage(out: &Out) -> i32 {
    out.fail("cli.error.usage", out::USAGE)
}
fn valid(args: &Args, verb: &str) -> bool {
    let count = args.words.len();
    let (min, max) = match verb {
        "help" => (0, 2),
        "read" | "stop" | "continue" | "archive" | "unarchive" | "pin" | "unpin" | "wait" | "send" => (2, 2),
        "rename" => (3, 3),
        "call" => (2, 3),
        "status" | "list" | "projects" | "quota" | "settings" | "watch" => (1, 1),
        _ => return false,
    };
    if !(min..=max).contains(&count) || args.on("archived") && args.on("all") {
        return false;
    }
    let allowed = match verb {
        "list" => &["state", "archived", "all", "project", "family", "query", "limit"][..],
        "read" => &["last"],
        "send" => &["to", "project", "vendor", "model", "effort", "wait", "timeout"],
        "watch" => &["id", "family", "project"],
        "wait" => &["timeout"],
        _ => &[],
    };
    if !args.only(allowed) {
        return false;
    }
    if ["limit", "last", "timeout"].iter().any(|k| args.value(k).is_some_and(|v| v.parse::<u64>().is_err())) {
        return false;
    }
    if args.value("vendor").is_some_and(|v| !["auto", "claude", "codex"].contains(&v))
        || args.value("effort").is_some_and(|v| !["low", "medium", "high"].contains(&v))
    {
        return false;
    }
    true
}
pub async fn run(args: Args, out: Out) -> i32 {
    let verb = if args.on("help") { "help" } else { args.word(0).unwrap_or("help") };
    if !valid(&args, verb) {
        return usage(&out);
    }
    if verb == "help" {
        let live = if let Some(data) = connect::data_dir(&args) {
            Client::connect(&data, connect::hello("faber")).await.ok().map(|c| c.offer["commands"].clone())
        } else {
            None
        };
        let commands: Value =
            live.filter(Value::is_array).unwrap_or_else(|| serde_json::from_str(include_str!("../commands.json")).unwrap_or(json!([])));
        let target = args.word(1);
        let verbs: Vec<Value> = VERBS
            .iter()
            .filter(|v| target.is_none_or(|t| t == **v))
            .map(|v| {
                let key = format!("cli.help.{v}");
                json!({"name":v,"description":if out.json { key.clone() } else { out.words.get(&key) }})
            })
            .collect();
        let commands: Vec<Value> =
            commands.as_array().into_iter().flatten().filter(|c| target.is_none_or(|t| c["name"] == t)).cloned().collect();
        if target.is_some() && verbs.is_empty() && commands.is_empty() {
            return usage(&out);
        }
        if out.json {
            out.line(&json!({"verbs":verbs,"commands":commands}));
        } else {
            for v in verbs {
                println!("{}  {}", v["name"].as_str().unwrap_or(""), v["description"].as_str().unwrap_or(""));
            }
            for c in commands {
                println!(
                    "{} {} [{} / {}] {}",
                    c["name"].as_str().unwrap_or(""),
                    c["args"],
                    c["kind"].as_str().unwrap_or(""),
                    c["reach"].as_str().unwrap_or(""),
                    c["about"].as_str().unwrap_or("")
                );
            }
        }
        return out::DONE;
    }
    let client = match connect::connect(&args, "faber").await {
        Ok(c) => c,
        Err(e) => return out.fail(e.key(), out::UNREACHED),
    };
    let id = args.word(1).unwrap_or("");
    let result = match verb {
        "status" => match ask(&client, &out, "conversations", json!({"state":["running"]})).await {
            Ok(rows) => Ok(
                json!({"version":client.offer["version"],"protocol":client.offer["protocol"],"caller":client.offer["caller"],"running":rows}),
            ),
            Err(e) => Err(e),
        },
        "list" => {
            let mut v = json!({"limit":num(&args,"limit",50).unwrap()});
            for (flag, key) in [("project", "projectId"), ("family", "family"), ("query", "query")] {
                if let Some(x) = args.value(flag) {
                    v[key] = json!(x);
                }
            }
            if let Some(s) = args.value("state") {
                v["state"] = json!(s.split(',').collect::<Vec<_>>());
            }
            if args.on("archived") {
                v["archived"] = json!(true);
            } else if args.on("all") {
                v["archived"] = json!(false);
            }
            ask(&client, &out, "conversations", v).await
        }
        "read" => ask(&client, &out, "conversation", json!({"id":id,"last":num(&args,"last",20).unwrap()})).await,
        "send" => {
            let Some(text) = input(id) else { return usage(&out) };
            let mut choice = json!({"vendor":args.value("vendor").unwrap_or("auto")});
            for k in ["model", "effort"] {
                if let Some(v) = args.value(k) {
                    choice[k] = json!(v);
                }
            }
            let mut req = json!({"text":text,"area":if args.value("project").is_some(){"project"}else{"chat"},"images":[],"files":[],"choice":choice});
            if let Some(v) = args.value("to") {
                req["conversationId"] = json!(v);
            }
            if let Some(v) = args.value("project") {
                req["projectId"] = json!(v);
            }
            match ask(&client, &out, "send", json!({"req":req})).await {
                Ok(v) if args.on("wait") => {
                    out.result(&v);
                    match v.as_str() {
                        Some(id) => return wait(&client, &out, id, num(&args, "timeout", 300).unwrap()).await,
                        None => Err(out.fail("cli.error.channel", out::UNREACHED)),
                    }
                }
                other => other,
            }
        }
        "stop" => ask(&client, &out, "interrupt", json!({"id":id})).await,
        "continue" => match ask(&client, &out, "conversation", json!({"id":id,"last":1})).await {
            Ok(v) => match v["interruptedTurn"].as_str() {
                Some(turn) => ask(&client, &out, "continue_closed", json!({"id":id,"turnId":turn})).await,
                None => Err(out.fail("cli.error.noTurn", out::REFUSED)),
            },
            Err(e) => Err(e),
        },
        "archive" | "unarchive" => ask(&client, &out, "archive", json!({"id":id,"on":verb=="archive"})).await,
        "pin" | "unpin" => ask(&client, &out, "pin", json!({"id":id,"on":verb=="pin"})).await,
        "rename" => ask(&client, &out, "rename", json!({"id":id,"title":args.word(2)})).await,
        "projects" | "quota" | "settings" => ask(&client, &out, verb, json!({})).await,
        "wait" => return wait(&client, &out, id, num(&args, "timeout", 300).unwrap()).await,
        "watch" => return watch(&client, &out, &args).await,
        "call" => {
            let value = match args.word(2) {
                Some(s) => match input(s).and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
                    Some(v) if v.is_object() => v,
                    _ => return usage(&out),
                },
                None => json!({}),
            };
            ask(&client, &out, id, value).await
        }
        _ => unreachable!(),
    };
    match result {
        Ok(v) => {
            out.result(&v);
            out::DONE
        }
        Err(e) => e,
    }
}
async fn wait(client: &Client, out: &Out, id: &str, timeout: u64) -> i32 {
    let start = Instant::now();
    loop {
        let v = match ask(client, out, "conversation", json!({"id":id,"last":1})).await {
            Ok(v) => v,
            Err(e) => return e,
        };
        if v["state"] != "working" {
            out.result(&v);
            return out::DONE;
        }
        if timeout > 0 && start.elapsed() >= Duration::from_secs(timeout) {
            out.result(&v);
            return out::TIMEOUT;
        }
        tokio::select! { _=tokio::signal::ctrl_c()=>return out::STOPPED, _=tokio::time::sleep(Duration::from_millis(250))=>{} }
    }
}
async fn watch(client: &Client, out: &Out, args: &Args) -> i32 {
    let mut events = match client.subscribe().await {
        Ok(v) => v,
        Err(_) => return out.fail("cli.error.channel", out::UNREACHED),
    };
    let mut previous: HashMap<String, String> = HashMap::new();
    let mut initial = true;
    let mut last: Option<Instant> = None;
    loop {
        if let Some(at) = last {
            let remaining = Duration::from_millis(250).saturating_sub(at.elapsed());
            if !remaining.is_zero() {
                tokio::time::sleep(remaining).await;
            }
        }
        let mut filter = json!({"archived":false});
        for (flag, key) in [("family", "family"), ("project", "projectId")] {
            if let Some(v) = args.value(flag) {
                filter[key] = json!(v);
            }
        }
        let rows = match ask(client, out, "conversations", filter).await {
            Ok(v) => v,
            Err(e) => return e,
        };
        last = Some(Instant::now());
        for row in rows.as_array().into_iter().flatten() {
            let Some(id) = row["id"].as_str() else { continue };
            if args.value("id").is_some_and(|wanted| wanted != id) {
                continue;
            }
            let state = row["state"].as_str().unwrap_or("");
            let was = previous.get(id).cloned();
            if initial && !["working", "waiting"].contains(&state) {
                previous.insert(id.into(), state.into());
                continue;
            }
            if was.as_deref() == Some(state) {
                continue;
            }
            let mut line = json!({"type":"state","id":id,"title":row["title"],"state":state,"was":was,"family":row["family"]});
            if state == "idle" && was.as_deref().is_some_and(|s| s == "working" || s == "waiting") {
                let detail = match ask(client, out, "conversation", json!({"id":id,"last":0})).await {
                    Ok(v) => v,
                    Err(e) => return e,
                };
                line["state"] = detail["state"].clone();
                if !detail["conclusion"].is_null() {
                    line["conclusion"] = detail["conclusion"].clone();
                }
            }
            out.result(&line);
            previous.insert(id.into(), state.into());
        }
        initial = false;
        tokio::select! { _=tokio::signal::ctrl_c()=>return out::STOPPED, event=events.recv()=>if event.is_none() {return out.fail("cli.error.channel",out::UNREACHED)} }
    }
}
