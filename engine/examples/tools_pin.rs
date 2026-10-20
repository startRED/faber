//! Pins a tool for `engine/src/tools.json`: downloads `<owner/repo>` at `<commit>` from GitHub and prints the digest of
//! `<path>`, or downloads an npm package at a version and prints the digest of the whole package, the way Faber checks it
//! when it installs. `cargo run -p faber-engine --example tools_pin -- <repo> <commit> <path>` or `-- npm <package> <version>`

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [repo, commit, path] = args.as_slice() else {
        eprintln!("usage: tools_pin <owner/repo> <commit> <path> | npm <package> <version>");
        std::process::exit(2);
    };
    let npm = repo == "npm";
    let file = std::env::temp_dir().join(format!("faber-pin-{}", uuid::Uuid::new_v4()));
    let url = if npm {
        let name = commit.rsplit('/').next().unwrap_or(commit);
        format!("https://registry.npmjs.org/{commit}/-/{name}-{path}.tgz")
    } else {
        format!("https://github.com/{repo}/archive/{commit}.zip")
    };
    if let Err(e) = faber_engine::install::default_download(&url, &file, None).await {
        eprintln!("download failed: {e}");
        std::process::exit(1);
    }
    let digest = if npm { faber_engine::tools::package_digest_of(&file) } else { faber_engine::tools::digest_of(&file, path) };
    let _ = std::fs::remove_file(&file);
    match digest {
        Ok(d) => println!("{d}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
