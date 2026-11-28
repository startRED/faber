//! Times opening the file history (`History::open`) on a data folder, stage by stage, so a slow start says which stage
//! is slow. Point it at a copy, never at a live data folder: opening runs the cleanup, which may retire expired points.
//!
//!     node scripts/cargo.mjs run -q --release -p faber-engine --example rewind_open -- <copied data folder>

use faber_engine_projects::rewind::History;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() {
    let Some(data) = std::env::args().nth(1) else {
        eprintln!("usage: rewind_open <data folder holding a rewind/ folder>");
        std::process::exit(2);
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let began = Instant::now();
    let opened = History::open(std::path::Path::new(&data), now);
    let total = began.elapsed().as_millis();
    match opened {
        Ok(history) => {
            let w = history.work();
            let ms = |us: u64| us / 1000;
            println!(
                "open={total}ms load={}ms recover={}ms inspect={}ms cleanup={}ms path_checks={} in {}ms object_reads={} deleted={}",
                ms(w.load_us),
                ms(w.recover_us),
                ms(w.inspect_us),
                ms(w.cleanup_us),
                w.path_checks,
                ms(w.path_check_us),
                w.object_reads,
                w.owned_files_deleted
            );
        }
        Err(e) => println!("open={total}ms error={e}"),
    }
}
