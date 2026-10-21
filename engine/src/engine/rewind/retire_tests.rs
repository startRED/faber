//! Exercise the shared-server teardown with an owned container, so the no-process early drop cannot hide a cycle.
use super::*;
use crate::model::{ReplyStyle, Settings};
use crate::proc;
use crate::router::Effort;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

async fn read(input: &mut tokio::io::Lines<BufReader<DuplexStream>>, method: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let msg: Value = serde_json::from_str(&input.next_line().await.unwrap().expect("server input closed early")).unwrap();
            if msg["method"] == "skills/list" {
                continue;
            }
            assert_eq!(msg["method"], method);
            return msg;
        }
    })
    .await
    .expect("server did not receive its next request")
}

async fn write(output: &mut DuplexStream, msg: Value) {
    output.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
}

async fn start(input: &mut tokio::io::Lines<BufReader<DuplexStream>>, output: &mut DuplexStream, method: &str, thread: &str, turn: &str) {
    let open = read(input, method).await;
    write(output, json!({"id":open["id"],"result":{"thread":{"id":thread}}})).await;
    let send = read(input, "turn/start").await;
    write(output, json!({"id":send["id"],"result":{"turn":{"id":turn}}})).await;
}

async fn until(ready: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(3), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned shared server stayed alive after its last work");
}

#[tokio::test]
async fn retired_shared_server_closes_after_replacing_a_session_before_settings_change() {
    let data = tempfile::tempdir().unwrap();
    let root = proc::ProcessIdentity { pid: 100, created: 1 };
    let members = Arc::new(Mutex::new(vec![root]));
    let stopped = Arc::new(AtomicBool::new(false));
    let (driver_in, input) = tokio::io::duplex(65536);
    let (mut output, driver_out) = tokio::io::duplex(65536);
    let mut input = BufReader::new(input).lines();
    let query = members.clone();
    let terminated = members.clone();
    let notice = stopped.clone();
    let contained = proc::Containment::scripted_descendants(
        root,
        move || Ok(query.lock().unwrap().clone()),
        move || {
            terminated.lock().unwrap().clear();
            notice.store(true, Ordering::Release);
        },
    );
    let process = Arc::new(Mutex::new(Some(proc::Proc {
        stdin: Box::new(driver_in),
        stdout: Box::new(driver_out),
        stderr: None,
        child: None,
        contained: Some(contained),
    })));
    let app = Engine::new(
        EngineOptions {
            data_dir: data.path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "unused".into(), prefix: vec![] })]),
            spawn: Arc::new(move |_, _, _| Ok(process.lock().unwrap().take().expect("both sessions must share one server"))),
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(|_| {}),
    );
    app.save_settings(Settings { permissions: Some(Permissions::Ask), ..Default::default() }).unwrap();
    let request = |id, effort| SendRequest {
        conversation_id: id,
        area: Area::Chat,
        project_id: None,
        text: "Keep working".into(),
        images: vec![],
        files: vec![],
        choice: Choice { vendor: "codex".into(), model: None, effort: Some(effort) },
    };
    let a = app.send(request(None, Effort::Medium)).unwrap();
    let init = read(&mut input, "initialize").await;
    write(&mut output, json!({"id":init["id"],"result":{}})).await;
    read(&mut input, "initialized").await;
    start(&mut input, &mut output, "thread/start", "a", "a1").await;
    write(&mut output, json!({"method":"turn/completed","params":{"threadId":"a","turn":{"id":"a1","status":"completed"}}})).await;
    until(|| app.open(&a).unwrap().messages.last().unwrap().status == Some(Status::Done)).await;
    let peer = app.send(request(None, Effort::Medium)).unwrap();
    start(&mut input, &mut output, "thread/start", "b", "b1").await;
    let old_sid = app.with(|c| c.live[&a].sid);
    app.send(request(Some(a.clone()), Effort::High)).unwrap();
    start(&mut input, &mut output, "thread/resume", "a", "a2").await;
    until(|| {
        app.with(|c| {
            !c.rewind.retired_sessions.contains_key(&(a.clone(), old_sid)) && !c.rewind.session_writers.contains_key(&(a.clone(), old_sid))
        })
    })
    .await;
    // A drained closed thread now has its own confirmed exit, before the shared process and its answering peer stop.
    assert!(!stopped.load(Ordering::Acquire), "a closed thread cannot tear down its answering peer's container");
    app.save_settings(Settings { reply_style: Some(ReplyStyle::Short), ..Default::default() }).unwrap();
    assert_eq!(app.open(&peer).unwrap().messages.last().unwrap().status, Some(Status::Streaming));
    for (thread, turn) in [("a", "a2"), ("b", "b1")] {
        write(&mut output, json!({"method":"turn/completed","params":{"threadId":thread,"turn":{"id":turn,"status":"completed"}}})).await;
    }
    // proc::stop shuts stdin, then confirms that the owned container is empty. Supply EOF only after that stop.
    until(|| stopped.load(Ordering::Acquire)).await;
    drop(output);
    until(|| app.with(|c| c.rewind.retired_sessions.is_empty())).await;
    assert!(members.lock().unwrap().is_empty());
    app.shutdown();
}
