//! Manual probe of Faber's checks (R190 V1): runs the project's own checks on a folder, as the `verify` tool would, and
//! prints what the agent reads and how long each level took. It starts real processes (hidden, below normal priority).
//! Run from the worktree root: `cargo run -p faber-engine --example verify_probe -- <folder> [fast|full]...`; the PC's
//! additions come from `<folder>/.faber/verify.local.json` or the `--local <project folder>` given before the levels.
use faber_engine::model::Project;
use faber_engine::project::git::{Executor, Process};
use faber_engine::verify::{self, Level, Local, Request, Verifier};
use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let folder = PathBuf::from(args.first().cloned().unwrap_or_else(|| ".".into())).canonicalize().expect("a folder");
    if !args.is_empty() {
        args.remove(0);
    }
    let mut local_root = folder.clone();
    if args.first().is_some_and(|a| a == "--local") {
        local_root = PathBuf::from(&args[1]);
        args.drain(..2);
    }
    let levels: Vec<Level> = if args.is_empty() {
        vec![Level::Fast]
    } else {
        args.iter().map(|a| if a == "full" { Level::Full } else { Level::Fast }).collect()
    };
    let project = Project::new("probe".into(), "probe".into(), folder.to_string_lossy().into_owned());
    let view = faber_engine::project::mission::view(&project);
    println!("checks ({}): {:?}", view.source, view.commands);
    let verifier = Verifier::real();
    for level in levels {
        let changed = Process::find().ok().and_then(|mut git| {
            let exec: &mut dyn Executor = &mut git;
            verify::changed(exec, &folder, None)
        });
        println!("\n== {level:?}: {} changed files", changed.as_ref().map_or(0, |c| c.files.len()));
        let request = Request {
            folder: folder.clone(),
            level: Some(level),
            commands: view.commands.clone(),
            setup: view.setup.clone(),
            local: Local::read(&local_root),
            changed,
        };
        let began = Instant::now();
        let outcome = verifier.verify(request).await;
        println!("{}\n-- {level:?} took {:.1} s", outcome.text(), began.elapsed().as_secs_f64());
    }
}
