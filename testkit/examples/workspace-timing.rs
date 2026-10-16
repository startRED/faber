//! Manual: times a project's mission ground twice (sync, setup, proofs) in a fresh data folder, the way a mission's first and
//! second build go. Usage: `workspace-timing REPO DATA_DIR PROOF_COMMAND...` (each proof command is one quoted argument).
//! Reads REPO's HEAD and never changes REPO; everything it builds lives under DATA_DIR.
use faber_engine::project::{git, mission, proof, workspace::Workspace};
use std::path::Path;
use std::time::Instant;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [repo, data, commands @ ..] = args.as_slice() else {
        eprintln!("usage: workspace-timing REPO DATA_DIR PROOF_COMMAND...");
        std::process::exit(2);
    };
    let (repo, data) = (Path::new(repo), Path::new(data));
    let mut process = git::Process::find().expect("git");
    let repository = git::enroll(&mut process, repo, "HEAD").expect("enroll");
    let resolve = mission::resolve_tool;
    let target = data.join("proof-target").join("timing");
    let (proofs, spawn) = mission::definitions(commands, &target, &resolve).expect("proofs");
    let present = |path: &str| repo.join(path).is_file();
    let steps = mission::setup_definitions(&mission::setup_commands(&present), &target, &resolve).expect("setup");
    let space = Workspace::new(data, "timing").expect("workspace");
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime");
    for round in ["first", "second"] {
        let started = Instant::now();
        space.sync(&mut process, Path::new(&repository.identity.root), &repository.base, repo).expect("sync");
        let synced = started.elapsed();
        let mut setup = std::time::Duration::ZERO;
        for step in &steps {
            if space.setup_current(step) {
                continue;
            }
            let at = Instant::now();
            let seen = runtime.block_on(proof::execute(step, space.dir(), &spawn, "setup".into()));
            assert_eq!(seen.exit, Some(0), "setup {} failed: {}", step.id, String::from_utf8_lossy(&seen.stderr));
            space.setup_done(step);
            setup += at.elapsed();
        }
        let mut checks = vec![];
        for definition in &proofs {
            let at = Instant::now();
            let seen = runtime.block_on(proof::execute(definition, space.dir(), &spawn, "proof".into()));
            checks.push(format!("{} exit {:?} in {:.1}s", definition.toolchain, seen.exit, at.elapsed().as_secs_f32()));
        }
        println!(
            "{round}: sync {:.1}s, setup {:.1}s, proofs [{}], total {:.1}s",
            synced.as_secs_f32(),
            setup.as_secs_f32(),
            checks.join("; "),
            started.elapsed().as_secs_f32()
        );
    }
}
