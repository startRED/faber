//! Runs alone so another test's allocations cannot change its process-memory measurement.
//! RSS is sampled, and process CPU includes fake-wire parsing and measurement overhead.
mod support;

use faber_engine::background::{MAX_BYTES, MAX_COMMANDS};
use faber_engine::cli::Cli;
use faber_engine::model::{Area, Vendor};
use faber_engine::router::Choice;
use faber_engine::{AppEvent, Engine, EngineOptions, SendRequest};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

fn reading(system: &mut System) -> (u64, u64) {
    let pid = Pid::from_u32(std::process::id());
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory().with_cpu());
    let process = system.process(pid).unwrap();
    (process.memory(), process.accumulated_cpu_time())
}

#[tokio::test]
async fn background_cost_at_thirty_two_full_output_caps() {
    let (spawn, mut input, mut output) = support::controlled();
    let folder = support::DataFolder::new();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let engine = Engine::new(
        EngineOptions {
            data_dir: folder.path().into(),
            #[cfg(windows)]
            chat_dir: folder.chat_path().into(),
            clis: BTreeMap::from([(Vendor::Codex, Cli { command: "fake".into(), prefix: vec![] })]),
            spawn,
            windows: false,
            claude_env_billed: false,
            max_warm: 3,
            idle: Duration::from_secs(120),
            runtime: tokio::runtime::Handle::current(),
            ask: None,
        },
        Arc::new(move |event| drop(tx.send(event))),
    )
    .unwrap();
    engine
        .send(SendRequest {
            conversation_id: None,
            area: Area::Chat,
            project_id: None,
            text: "Measure background output".into(),
            images: vec![],
            files: vec![],
            choice: Choice { vendor: "codex".into(), model: None, effort: None },
        })
        .unwrap();
    let init = support::expect_method(&mut input, "initialize").await;
    support::emit(&mut output, json!({"id":init["id"],"result":{}})).await;
    support::expect_method(&mut input, "initialized").await;
    let thread = support::expect_method(&mut input, "thread/start").await;
    support::emit(&mut output, json!({"id":thread["id"],"result":{"thread":{"id":"cost-thread","model":"test"}}})).await;
    let turn = support::expect_method(&mut input, "turn/start").await;
    support::emit(&mut output, json!({"id":turn["id"],"result":{"turn":{"id":"cost-turn","status":"inProgress"}}})).await;
    support::emit(&mut output, json!({"method":"turn/started","params":{"threadId":"cost-thread","turn":{"id":"cost-turn"}}})).await;
    let mut system = System::new();
    let (baseline, before_cpu) = reading(&mut system);
    let mut peak = baseline;
    let began = Instant::now();
    for index in 0..MAX_COMMANDS {
        let id = format!("cost-{index}");
        support::emit(&mut output, json!({"method":"item/started","params":{"threadId":"cost-thread","turnId":"cost-turn","item":{"type":"commandExecution","id":id,"command":"node long-task.js","status":"inProgress","processId":index.to_string()}}})).await;
        support::emit(&mut output, json!({"method":"item/commandExecution/outputDelta","params":{"threadId":"cost-thread","turnId":"cost-turn","itemId":id,"delta":"x".repeat(MAX_BYTES)}})).await;
        tokio::time::timeout(crate::support::DEADLINE, async {
            loop {
                if let Some(AppEvent::Background { items, .. }) = rx.recv().await
                    && items.len() == index + 1
                    && items.last().unwrap().output.len() == MAX_BYTES
                {
                    break;
                }
            }
        })
        .await
        .expect("background output did not reach its cap");
        peak = peak.max(reading(&mut system).0);
    }
    let fill_ms = began.elapsed().as_secs_f64() * 1000.0;
    let fill_cpu_ms = reading(&mut system).1.saturating_sub(before_cpu);
    let rows = engine.background_commands();
    assert_eq!(rows.items.len(), MAX_COMMANDS);
    let bytes: usize = rows.items.iter().map(|row| row.output.len()).sum();
    assert_eq!(bytes, MAX_COMMANDS * MAX_BYTES);
    let snapshot_start = Instant::now();
    let mut wire_bytes = 0;
    for _ in 0..100 {
        wire_bytes = serde_json::to_vec(&engine.background_commands()).unwrap().len();
    }
    let per_snapshot_ms = snapshot_start.elapsed().as_secs_f64() * 10.0;
    let (final_rss, after_cpu) = reading(&mut system);
    peak = peak.max(final_rss);
    let growth = peak.saturating_sub(baseline);
    println!(
        "background_cost: commands={MAX_COMMANDS} retained_output_bytes={bytes} serialized_snapshot_bytes={wire_bytes} fill_wall_ms={fill_ms:.3} fill_process_cpu_ms={fill_cpu_ms} snapshot_average_ms={per_snapshot_ms:.3} total_process_cpu_ms={} baseline_rss_bytes={baseline} sampled_peak_rss_bytes={peak} sampled_growth_bytes={growth}",
        after_cpu.saturating_sub(before_cpu)
    );
    assert!(growth < 16 * 1024 * 1024, "32 capped command rows added more than 16 MiB of resident memory");
    assert!(peak < 400 * 1024 * 1024, "background command measurement exceeded the process budget");
    engine.shutdown();
}
