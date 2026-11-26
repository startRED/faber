use std::process::Command;

fn main() {
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_owned())
    };
    for name in ["HEAD", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]).filter(|p| std::path::Path::new(p).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        let path = git(&["rev-parse", "--git-path", &branch])
            .filter(|p| std::path::Path::new(p).exists())
            .or_else(|| git(&["rev-parse", "--git-path", "refs/heads"]).filter(|p| std::path::Path::new(p).exists()));
        if let Some(path) = path {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit()));
    println!("cargo:rustc-env=FABER_REPORT_COMMIT={}", commit.as_deref().unwrap_or("unknown"));
}
