//! Small, dependency-free maintenance helper, compiled once and reused by `cargo.mjs`.
#![allow(dead_code)]

#[path = "../testkit/src/sweep.rs"]
mod sweep;

use std::{env, fs, path::PathBuf, process::Command, time::SystemTime};

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("missing main checkout")?);
    let own = PathBuf::from(args.next().ok_or("missing protected cache")?);
    if args.next().is_some() || !root.is_absolute() || !own.is_absolute() {
        return Err("storage maintenance needs absolute paths".into());
    }
    let mut git = Command::new("git");
    git.current_dir(&root).args(["worktree", "list", "--porcelain"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        git.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = git.output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("cannot list checkouts for storage maintenance".into());
    }
    let worktrees = sweep::worktrees(&String::from_utf8_lossy(&output.stdout));
    if worktrees.first().and_then(|p| p.canonicalize().ok()) != root.canonicalize().ok() {
        return Err("storage maintenance could not confirm the main checkout".into());
    }
    let now = SystemTime::now();
    let shared = root.join("target");
    for target in [sweep::loose_root(), shared.clone()] {
        let meta = match fs::symlink_metadata(&target) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        if !meta.is_dir() || meta.is_symlink() {
            return Err("storage maintenance refuses a linked build folder".into());
        }
        if !sweep::due(&target, now) {
            continue;
        }
        let report = if target == shared {
            sweep::sweep_worktrees(&target, &own, &worktrees, now, sweep::CAP)
        } else {
            sweep::sweep(&target, &own, now, sweep::CAP)
        };
        let gb = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
        if report.after < report.before {
            println!("Build storage: {:.1} GB -> {:.1} GB", gb(report.before), gb(report.after));
        }
        if report.after > sweep::CAP {
            println!(
                "Build storage: {:.1} GB exceeds the {:.0} GB budget; active and protected caches were kept",
                gb(report.after),
                gb(sweep::CAP)
            );
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Storage maintenance failed: {error}");
        std::process::exit(1);
    }
}
