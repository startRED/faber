//! Pins a skill for `engine/src/tools.json`: downloads `<owner/repo>` at `<commit>` from GitHub and prints the digest of
//! `<path>`, the way Faber checks it when it installs. `cargo run -p faber-engine --example tools_pin -- <repo> <commit> <path>`

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [repo, commit, path] = args.as_slice() else {
        eprintln!("usage: tools_pin <owner/repo> <commit> <path>");
        std::process::exit(2);
    };
    let zip = std::env::temp_dir().join(format!("faber-pin-{}.zip", commit));
    let url = format!("https://github.com/{repo}/archive/{commit}.zip");
    if let Err(e) = faber_engine::install::default_download(&url, &zip, None).await {
        eprintln!("download failed: {e}");
        std::process::exit(1);
    }
    let digest = faber_engine::tools::digest_of(&zip, path);
    let _ = std::fs::remove_file(&zip);
    match digest {
        Ok(d) => println!("{d}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
