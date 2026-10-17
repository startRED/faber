//! A payload goes to disk as it arrives, so the memory of the process does not grow with its size. This test sits alone
//! in its own file, hence in its own process: no other test shares the memory it reads. The workspace forbids `unsafe`
//! code, so it cannot count allocations; it reads the resident memory of its own process instead.

mod loopback;

use faber_engine::install::default_download;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 64 * 1024;

fn resident(system: &mut System, pid: Pid) -> u64 {
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), true, ProcessRefreshKind::nothing().with_memory());
    system.process(pid).expect("the test's own process is listed").memory()
}

/// Reads the memory of this process every 10 ms until it is stopped, and keeps the highest reading.
struct Sampler {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<u64>,
}

impl Sampler {
    fn start() -> Sampler {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::spawn(move || {
            let (mut system, pid) = (System::new(), Pid::from_u32(std::process::id()));
            let mut highest = resident(&mut system, pid);
            while !thread_stop.load(Ordering::Relaxed) {
                highest = highest.max(resident(&mut system, pid));
                std::thread::sleep(Duration::from_millis(10));
            }
            highest.max(resident(&mut system, pid))
        });
        Sampler { stop, thread }
    }

    fn finish(self) -> u64 {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap()
    }
}

#[tokio::test]
async fn the_memory_of_a_download_does_not_grow_with_the_payload() {
    // The server writes 64 KiB at a time from one reused buffer and hashes as it goes; it never holds the body.
    let hashes = Arc::new(Mutex::new(BTreeMap::<usize, String>::new()));
    let server = loopback::Server::start({
        let hashes = hashes.clone();
        move |_, path, stream| {
            let mib: usize = path.trim_start_matches('/').parse().unwrap();
            loopback::head(stream, Some((mib * MIB) as u64))?;
            let (mut block, mut hasher) = (vec![0u8; BLOCK], Sha256::new());
            for index in 0..mib * MIB / BLOCK {
                block.fill(index as u8);
                hasher.update(&block);
                stream.write_all(&block)?;
            }
            hashes.lock().unwrap().insert(mib, faber_engine::hex(&hasher.finalize()));
            Ok(())
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let mut peaks = Vec::new();
    for mib in [32, 160] {
        let (part, mut system, pid) = (dir.path().join(format!("{mib}.part")), System::new(), Pid::from_u32(std::process::id()));
        let baseline = resident(&mut system, pid);
        let sampler = Sampler::start();
        let hash = default_download(&format!("{}/{mib}", server.origin), &part, Some(&server.origin)).await.unwrap();
        let peak = sampler.finish();
        let deadline = Instant::now() + Duration::from_secs(10);
        let sent = loop {
            if let Some(sent) = hashes.lock().unwrap().get(&mib).cloned() {
                break sent;
            }
            assert!(Instant::now() < deadline, "the server never finished the {mib} MiB body");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(hash, sent, "the hash of the {mib} MiB body");
        assert_eq!(std::fs::metadata(&part).unwrap().len(), (mib * MIB) as u64);
        println!(
            "{mib} MiB body: baseline {} MiB, highest {} MiB, growth {} MiB",
            baseline / MIB as u64,
            peak / MIB as u64,
            peak.saturating_sub(baseline) / MIB as u64
        );
        peaks.push((baseline, peak));
        std::fs::remove_file(&part).unwrap();
    }
    let (small, large) = (peaks[0], peaks[1]);
    // A buffered body grows the process by the size of the body; a writer that also keeps every chunk, by 128 MiB between the two runs.
    assert!(large.1.saturating_sub(large.0) <= 24 * MIB as u64, "a 160 MiB download grew the process by more than 24 MiB");
    assert!(large.1 <= small.1 + 16 * MIB as u64, "the highest memory grew by more than 16 MiB from the 32 MiB body to the 160 MiB one");
}
