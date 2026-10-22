mod chime;

/// Runs `git` directly (no shell) in the repository and returns its one line, or nothing when git or the repository is missing.
fn git(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git").args(args).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string()).filter(|s| !s.is_empty())
}

/// The commit this build comes from (`FABER_COMMIT`, 12 hex digits), or "unknown" when git is not there, as in a source archive.
/// A new commit or a branch switch builds the shell again; the paths come from git, so a worktree's `.git` file is no trouble.
/// Only files that exist are watched: Cargo reruns this script at every build for a watched file that is missing.
fn embed_commit() {
    let watch = |name: &str| {
        let path = git(&["rev-parse", "--git-path", name]).filter(|path| std::path::Path::new(path).exists());
        if let Some(path) = &path {
            println!("cargo:rerun-if-changed={path}");
        }
        path.is_some()
    };
    watch("HEAD");
    watch("packed-refs");
    // A branch whose ref is only packed has no file yet and its next commit creates one in refs/heads, so that folder is watched
    // until then (shared by every worktree, hence not always).
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"])
        && !watch(&branch)
    {
        watch("refs/heads");
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).filter(|c| c.chars().all(|ch| ch.is_ascii_hexdigit()));
    println!("cargo:rustc-env=FABER_COMMIT={}", commit.as_deref().unwrap_or("unknown"));
}

fn main() {
    embed_commit();
    println!("cargo:rerun-if-changed=chime.rs");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("attention.wav"), chime::wav()).expect("write Faber's chime");
    // The screen is embedded from ../dist, built by `npm run build`. A fresh checkout gets an empty folder so the
    // Rust side still compiles and tests on its own.
    std::fs::create_dir_all("../dist").expect("create ../dist");
    tauri_build::build();
    // tauri-build links its Windows resource (the manifest asking for Common Controls v6) into binaries only; the
    // spike host is an example, and without it Windows refuses to start it.
    if std::env::var_os("CARGO_FEATURE_PANE_SPIKE").is_some() && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-examples={}", out.join("resource.lib").display());
    }
}
