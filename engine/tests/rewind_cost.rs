//! Run alone: cargo test -p faber-engine --test rewind_cost -- --nocapture
//! The process CPU includes measurement overhead; RSS is a sampled peak, not an allocation or native peak counter.
use faber_engine::model::{Conversation, Message, Role};
use faber_engine::project::git::{Executor, GitError, Invocation, Output};
use faber_engine::rewind::{History, Mode, RETENTION, RestoreRequest, Work};
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MIB: u64 = 1024 * 1024;
const FILES: usize = 1000;
const FILE_BYTES: usize = 16 * 1024;
const CONVERSATION: &str = "3611dcb7-6387-47c6-8b25-1b6f20903171";

struct Git {
    names: Vec<u8>,
    calls: usize,
}

impl Executor for Git {
    fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
        self.calls += 1;
        assert!(call.deadline.is_some());
        let command = call.args.iter().position(|arg| arg == "ls-files").expect("only name listing is allowed");
        let args = &call.args[command..];
        let ignored = args.iter().any(|arg| arg == "--ignored");
        assert_eq!(
            args,
            if ignored {
                vec!["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z", "--"]
            } else {
                vec!["ls-files", "--cached", "--others", "--exclude-standard", "-z", "--"]
            }
        );
        Ok(Output { success: true, stdout: if ignored { vec![] } else { self.names.clone() } })
    }
}

fn reading(system: &mut System, cpu: bool) -> (u64, u64) {
    let pid = Pid::from_u32(std::process::id());
    let refresh = ProcessRefreshKind::nothing().with_memory();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, if cpu { refresh.with_cpu() } else { refresh });
    let process = system.process(pid).expect("the test's own process is listed");
    (process.memory(), process.accumulated_cpu_time())
}

fn measured<T>(name: &str, overall_peak: &mut u64, operation: impl FnOnce() -> T) -> (T, Duration) {
    let mut system = System::new();
    let (baseline, cpu_before) = reading(&mut system, true);
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = stop.clone();
    let sampler = std::thread::spawn(move || {
        let mut system = System::new();
        let mut peak = baseline;
        while !thread_stop.load(Ordering::Relaxed) {
            peak = peak.max(reading(&mut system, false).0);
            std::thread::sleep(Duration::from_millis(1));
        }
        peak.max(reading(&mut system, false).0)
    });
    let start = Instant::now();
    let result = operation();
    let wall = start.elapsed();
    stop.store(true, Ordering::Relaxed);
    let peak = sampler.join().unwrap();
    let (after, cpu_after) = reading(&mut system, true);
    let peak = peak.max(after);
    *overall_peak = (*overall_peak).max(peak);
    let growth = peak.saturating_sub(baseline);
    println!(
        "rewind_cost {name}: wall_ms={:.3} process_cpu_ms={} baseline_rss_mib={:.3} sampled_peak_rss_mib={:.3} sampled_growth_mib={:.3} final_rss_mib={:.3}",
        wall.as_secs_f64() * 1000.0,
        cpu_after.saturating_sub(cpu_before),
        baseline as f64 / MIB as f64,
        peak as f64 / MIB as f64,
        growth as f64 / MIB as f64,
        after as f64 / MIB as f64,
    );
    assert!(growth <= 16 * MIB, "{name} added more than 16 MiB resident memory");
    assert!(peak < 400 * MIB, "{name} exceeded the 400 MiB process budget");
    (result, wall)
}

fn payload(seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..FILE_BYTES)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn report_work(name: &str, history: &History) -> Work {
    let work = history.work();
    println!("rewind_cost {name} work={work:?}");
    work
}

#[test]
#[ignore = "isolated capture, restore and cleanup measurement; run with --ignored --nocapture --test-threads=1"]
fn rewind_capture_restore_and_cleanup_cost() {
    let temporary = std::env::temp_dir().canonicalize().unwrap();
    let folder = tempfile::tempdir_in(&temporary).unwrap();
    let data = tempfile::tempdir_in(&temporary).unwrap();
    let names: Vec<_> = (0..FILES).map(|index| format!("file-{index:04}.bin")).collect();
    for (index, name) in names.iter().enumerate() {
        std::fs::write(folder.path().join(name), payload(index as u64 + 1)).unwrap();
    }
    let cache_generation_supported = {
        let root = faber_winproc::rewind::Root::open(folder.path()).unwrap();
        faber_winproc::rewind::cache_fingerprint(&root.open_file(&names[0]).unwrap()).unwrap().is_some()
    };
    let mut git = Git { names: names.iter().flat_map(|name| name.bytes().chain([0])).collect(), calls: 0 };
    let mut conversation: Conversation = serde_json::from_value(json!({
        "id": CONVERSATION, "title": "Rewind cost", "area": "project", "workingFolder": folder.path(),
        "createdAt": 1, "updatedAt": 1, "messages": []
    }))
    .unwrap();
    conversation.messages.push(Message::new("question".into(), Role::User, "Change eight files".into(), 1));
    let overall_baseline = reading(&mut System::new(), false).0;
    let mut overall_peak = overall_baseline;
    let mut history = measured("open", &mut overall_peak, || History::open(data.path(), 1)).0.unwrap();
    report_work("open", &history);
    println!("rewind_cost workload: files={FILES} bytes_per_file={FILE_BYTES} changed_files=8 RSS_sampling_ms=1");
    println!("rewind_cost cache_generation_supported={cache_generation_supported}; unsupported generations must read all files");
    history.reset_work();
    let (point, cold_time) = measured("cold_capture_durable", &mut overall_peak, || {
        history.capture(&mut git, folder.path(), &conversation, "question", "turn", 1, false)
    });
    let point = point.unwrap();
    let work = report_work("cold_capture_durable", &history);
    assert_eq!((work.files_read, work.files_reused, work.bytes_read), (FILES as u64, 0, (FILES * FILE_BYTES) as u64));
    assert_eq!((work.objects_written, work.object_flushes), (FILES as u64, 1), "cold capture durably batches the baseline");
    assert_eq!(point.coverage.saved, FILES);
    assert!(!point.coverage.incomplete);
    assert_eq!(git.calls, 2, "cold capture only lists visible and ignored names");
    history.reset_work();
    measured("seal_unchanged_durable", &mut overall_peak, || {
        let scan = history.scan_seal(&mut git, folder.path())?;
        history.seal(scan, 2)
    })
    .0
    .unwrap();
    let work = report_work("seal_unchanged_durable", &history);
    assert_eq!((work.files_read, work.files_reused), if cache_generation_supported { (0, FILES as u64) } else { (FILES as u64, 0) });
    assert_eq!((work.objects_written, work.object_flushes), if cache_generation_supported { (0, 0) } else { (FILES as u64, 1) });
    assert_eq!(git.calls, 4, "unchanged sealing only lists visible and ignored names");
    for (index, name) in names.iter().take(8).enumerate() {
        std::fs::write(folder.path().join(name), payload(FILES as u64 + index as u64 + 1)).unwrap();
    }
    conversation.messages.push(Message::new("second".into(), Role::User, "Change the same eight files again".into(), 3));
    history.reset_work();
    let (point, warm_time) = measured("warm_capture_durable", &mut overall_peak, || {
        history.capture(&mut git, folder.path(), &conversation, "second", "second-turn", 3, false)
    });
    let point = point.unwrap();
    let work = report_work("warm_capture_durable", &history);
    assert_eq!((work.files_read, work.files_reused), if cache_generation_supported { (8, FILES as u64 - 8) } else { (FILES as u64, 0) });
    assert_eq!(work.bytes_read, work.files_read * FILE_BYTES as u64, "same-size edits must read their new bytes");
    assert_eq!((work.objects_written, work.object_flushes), (if cache_generation_supported { 8 } else { FILES as u64 }, 1));
    assert_eq!((work.cleanup_us, work.full_cleanup_runs), (0, 0), "warm capture never runs cleanup on the turn admission path");
    assert_eq!(point.coverage.saved, FILES);
    assert!(!point.coverage.incomplete);
    assert_eq!(git.calls, 6, "warm capture only lists visible and ignored names");
    for (index, name) in names.iter().take(8).enumerate() {
        std::fs::write(folder.path().join(name), payload(FILES as u64 + 8 + index as u64 + 1)).unwrap();
    }
    history.reset_work();
    let (sealed, changed_seal_time) = measured("seal_eight_changed_durable", &mut overall_peak, || {
        let scan = history.scan_seal(&mut git, folder.path())?;
        history.seal(scan, 4)
    });
    sealed.unwrap();
    let work = report_work("seal_eight_changed_durable", &history);
    assert_eq!((work.files_read, work.files_reused), if cache_generation_supported { (8, FILES as u64 - 8) } else { (FILES as u64, 0) });
    assert_eq!((work.objects_written, work.object_flushes), (if cache_generation_supported { 8 } else { FILES as u64 }, 1));
    assert_eq!(git.calls, 8, "changed sealing only lists visible and ignored names");
    println!("rewind_cost scripted_git_calls={} real_processes=0 points={}", git.calls, history.list(CONVERSATION).len());
    assert_eq!(history.list(CONVERSATION).len(), 2);
    let request =
        RestoreRequest { conversation_id: CONVERSATION.into(), message_id: "question".into(), mode: Mode::Files, saved_versions: vec![] };
    history.reset_work();
    let result = measured("restore_eight_durable", &mut overall_peak, || history.restore(&request, &mut conversation, 5)).0.unwrap();
    let work = report_work("restore_eight_durable", &history);
    assert_eq!((work.objects_written, work.object_flushes), (0, 0), "restore verifies and reuses the durable postimages it retains");
    assert!(!result.partial && result.failed.is_empty());
    assert_eq!(result.restored.len(), 8);
    history.acknowledge(&result.id).unwrap();
    for (index, name) in names.iter().take(8).enumerate() {
        assert_eq!(std::fs::read(folder.path().join(name)).unwrap(), payload(index as u64 + 1));
    }
    history.reset_work();
    measured("cleanup_expired", &mut overall_peak, || history.cleanup(RETENTION + 6)).0.unwrap();
    let work = report_work("cleanup_expired", &history);
    assert!(work.owned_files_deleted > 0, "expiry cleanup must actually delete validated history files");
    assert!(history.list(CONVERSATION).is_empty(), "expired, unpinned points must actually be removed");
    assert!(history.redo_root(&result.id).is_none(), "the expired redo journal must actually be removed");
    for folder in std::fs::read_dir(data.path().join("rewind")).unwrap() {
        for prefix in std::fs::read_dir(folder.unwrap().path().join("objects")).unwrap() {
            assert_eq!(
                std::fs::read_dir(prefix.unwrap().path()).unwrap().count(),
                0,
                "expired, unreferenced blobs must actually be removed"
            );
        }
    }
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), FILES, "cleanup never removes project files");
    let overall_growth = overall_peak.saturating_sub(overall_baseline);
    println!(
        "rewind_cost overall: baseline_rss_mib={:.3} sampled_peak_rss_mib={:.3} sampled_growth_mib={:.3}",
        overall_baseline as f64 / MIB as f64,
        overall_peak as f64 / MIB as f64,
        overall_growth as f64 / MIB as f64,
    );
    assert!(overall_growth <= 16 * MIB, "rewind added more than 16 MiB resident memory overall");
    assert!(cold_time <= Duration::from_millis(2000), "cold capture exceeded 2000 ms: {cold_time:?}");
    assert!(warm_time <= Duration::from_millis(250), "warm capture exceeded 250 ms: {warm_time:?}");
    // Sealing runs after the turn, off the message's path: it only has to stay cheap.
    assert!(changed_seal_time <= Duration::from_millis(1000), "eight-file sealing exceeded 1000 ms: {changed_seal_time:?}");
}
