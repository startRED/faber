use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok().filter(|output| output.status.success())?;
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}

fn main() {
    println!("cargo:rerun-if-env-changed=FABER_TELEMETRY_URL");
    println!("cargo:rerun-if-env-changed=FABER_RELEASE_CHANNEL");
    for name in ["HEAD", "packed-refs"] {
        if let Some(path) = git(&["rev-parse", "--git-path", name]).filter(|path| std::path::Path::new(path).exists()) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&["rev-parse", "--git-path", &branch]).filter(|path| std::path::Path::new(path).exists())
    {
        println!("cargo:rerun-if-changed={path}");
    }
    let commit = git(&["rev-parse", "--short=12", "HEAD"]).filter(|value| value.bytes().all(|byte| byte.is_ascii_hexdigit()));
    println!("cargo:rustc-env=FABER_COMMIT={}", commit.as_deref().unwrap_or("unknown"));
}
