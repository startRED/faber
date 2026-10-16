//! Manual: earns the role receipts the way the product does (hidden, contained, a few cents at most) into a private data
//! folder, with the real CLIs, and prints how long it took. Usage: `qualify-live FABER_EXE DATA_DIR`.
//! The data folder must be a new folder of your own: the owner's Faber data is never used here.
use std::path::Path;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [exe, data] = args.as_slice() else {
        eprintln!("usage: qualify-live FABER_EXE DATA_DIR");
        std::process::exit(2);
    };
    let (exe, data) = (Path::new(exe), Path::new(data));
    if ["CI", "GITHUB_ACTIONS"].iter().any(|name| std::env::var(name).is_ok_and(|v| v.eq_ignore_ascii_case("true"))) {
        eprintln!("manual live qualification never runs in CI");
        std::process::exit(2);
    }
    std::fs::create_dir_all(data).expect("data folder");
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let clis = [(faber_engine::model::Vendor::Claude, "claude"), (faber_engine::model::Vendor::Codex, "codex")]
        .into_iter()
        .filter_map(|(vendor, name)| faber_engine::cli::find_cli(name).map(|cli| (vendor, cli)))
        .collect();
    // The engine fixes the receipt folder from its data folder; nothing else is started.
    let engine = faber_engine::engine::Engine::new(
        faber_engine::engine::EngineOptions {
            data_dir: data.to_path_buf(),
            clis,
            spawn: faber_engine::proc::real_spawner(),
            windows: true,
            claude_env_billed: false,
            max_warm: 1,
            idle: Duration::from_secs(30),
            runtime: runtime.handle().clone(),
            ask: None,
        },
        std::sync::Arc::new(|_| {}),
    );
    let started = Instant::now();
    println!("ready before: {:?}", faber_engine::project::qualify::ready());
    let progress = |step: usize, total: usize, vendor: faber_engine::model::Vendor| {
        println!("[{:>5.1}s] step {step} of {total}: {vendor:?}", started.elapsed().as_secs_f32());
    };
    let result = runtime.block_on(faber_engine::project::qualify::earn(data, exe, &progress));
    println!("result: {result:?} after {:.1}s", started.elapsed().as_secs_f32());
    println!("ready after: {:?}", faber_engine::project::qualify::ready());
    engine.shutdown();
    std::process::exit(i32::from(result.is_err()));
}
