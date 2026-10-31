//! Reads stored conversations (read-only) and prints what each delivered and cost, and how long the tally took. It is how the
//! watchdog's thresholds were checked against real use (R-next, results ledger). Never part of `cargo t`.
//!
//!     cargo run -p faber-engine --example results_measure -- <conversations folder> [--dry-over <turns>]

use faber_engine::model::Conversation;
use faber_engine::results::tally;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("the conversations folder");
    let dry_over = match (args.next().as_deref(), args.next()) {
        (Some("--dry-over"), Some(n)) => n.parse::<u32>().unwrap_or(12),
        _ => 12,
    };
    let (mut read, mut counted) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    let mut rows = vec![];
    for entry in std::fs::read_dir(&dir).expect("readable folder").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let started = Instant::now();
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(c) = serde_json::from_slice::<Conversation>(&bytes) else { continue };
        read += started.elapsed();
        let started = Instant::now();
        let r = tally(&c);
        counted += started.elapsed();
        rows.push((c.id.chars().take(8).collect::<String>(), c.title.chars().take(30).collect::<String>(), r));
    }
    rows.sort_by(|a, b| b.2.cost.usd.total_cmp(&a.2.cost.usd));
    println!("{} conversations; reading and parsing {:?}, tallying {:?} in all", rows.len(), read, counted);
    for (id, title, r) in &rows {
        let (d, c) = (&r.delivered, &r.cost);
        let stuck = r.dry.turns >= dry_over && r.dry.since.zip(r.last_delivery.or(r.dry.since)).is_some();
        println!(
            "{id} {title:<30} turns {:>4} work {:>5.1} h  ${:>7.2}  files {:>3} edits {:>3} checks {}/{} commits {:>2} prs {:>2} answers {:>3} concluded {:<5} dry {:>3}{}",
            c.turns,
            c.work_ms as f64 / 3.6e6,
            c.usd,
            d.files,
            d.edits,
            d.checks_passed,
            d.checks_run,
            d.commits,
            d.prs,
            d.answers,
            d.concluded,
            r.dry.turns,
            if stuck { "  <- dry" } else { "" }
        );
    }
}
