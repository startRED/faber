//! What a check reads, found from the command itself, so a fast verify runs only the checks a change can affect. The
//! project declares commands, not paths (its "Proof commands", R137). A false "unchanged" is the worst bug, so only a
//! tool whose inputs are fixed by its nature selects (a formatter reads its language, a type checker its scripts); a
//! script runs what `package.json` says it runs, and any other command reads every file and always runs.

use std::collections::BTreeMap;

/// The files a check depends on: endings (`.rs`) and file names (`Cargo.toml`), or every file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watch {
    All,
    Files(&'static [&'static str]),
}

/// `cargo fmt` formats the `.rs` files of the members its `Cargo.toml` files list.
const RUST_FORMAT: &[&str] = &[".rs", "Cargo.toml", "rustfmt.toml", ".rustfmt.toml"];
/// A type checker or a script linter reads the script files and its JSON configuration (and JSON a script imports).
const SCRIPT: &[&str] = &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs", ".vue", ".svelte", ".json"];

/// The scripts of a `package.json`, by name.
pub type Scripts = BTreeMap<String, String>;

/// Reads the scripts of `<folder>/package.json`; none when it has none or cannot be read.
pub fn scripts(folder: &std::path::Path) -> Scripts {
    let Ok(text) = std::fs::read_to_string(folder.join("package.json")) else { return Scripts::new() };
    let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    json["scripts"]
        .as_object()
        .map_or_else(Scripts::new, |s| s.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
}

/// The program of a command line, as compared: lower case, without `.exe`.
fn program(words: &[&str]) -> String {
    words.first().map(|p| p.trim_end_matches(".exe").to_ascii_lowercase()).unwrap_or_default()
}

/// What a tool reads by its nature; any other program reads everything. Never a package manager, so it never recurses.
fn tool(words: &[&str]) -> Watch {
    // The first word that is not an option: `cargo +nightly fmt` is `fmt`.
    let sub = words.iter().skip(1).find(|w| !w.starts_with(['-', '+'])).copied().unwrap_or_default();
    match program(words).as_str() {
        "cargo" if sub == "fmt" => Watch::Files(RUST_FORMAT),
        "rustfmt" => Watch::Files(RUST_FORMAT),
        "tsc" | "oxlint" => Watch::Files(SCRIPT),
        _ => Watch::All,
    }
}

/// What a command reads, from its program and first words; `npm run x` reads what the script `x` runs, when it is one
/// plain command of a tool and no `prex` or `postx` script runs with it.
pub fn watch(tokens: &[String], scripts: &Scripts) -> Watch {
    let words: Vec<&str> = tokens.iter().map(String::as_str).collect();
    if !["npm", "pnpm", "yarn", "bun"].contains(&program(&words).as_str()) {
        return tool(&words);
    }
    let mut args = words.iter().skip(1).filter(|w| !w.starts_with('-'));
    // `npm test` and the like are scripts too, but run by rules of their own.
    let name = match args.next() {
        Some(&"run" | &"run-script") => args.next().copied().unwrap_or_default(),
        _ => return Watch::All,
    };
    if [format!("pre{name}"), format!("post{name}")].iter().any(|around| scripts.contains_key(around)) {
        return Watch::All;
    }
    match scripts.get(name) {
        // A script that chains, pipes or redirects reads anything.
        Some(line) if !line.contains(['&', '|', ';', '<', '>', '$', '`', '(', ')']) => tool(&line.split_whitespace().collect::<Vec<_>>()),
        _ => Watch::All,
    }
}

impl Watch {
    /// Whether a changed path (relative, with `/`) is one the check reads.
    pub fn reads(self, path: &str) -> bool {
        match self {
            Watch::All => true,
            Watch::Files(endings) => {
                let name = path.rsplit('/').next().unwrap_or(path);
                endings.iter().any(|e| if e.starts_with('.') && !e[1..].contains('.') { name.ends_with(e) } else { name == *e })
            }
        }
    }
}

/// The manifests of the Rust packages a change touches, relative to `folder` with `/`: for each changed `.rs` or
/// `Cargo.toml` inside `folder`, the nearest folder at or above the file, and not above `folder`, whose `Cargo.toml` has
/// a `[package]`. Git's paths are from the repository's top, which may be above `folder` (a project opened in a
/// subfolder; on Windows a folder's case may differ from git's). A package is named by its manifest, never by its name,
/// which a nested workspace may share; true marks a manifest that is a workspace too, whose `default-members` may leave
/// its own package out. Sorted, each once.
pub fn rust_manifests(folder: &std::path::Path, changed: &[(String, Option<String>)]) -> Vec<(String, bool)> {
    let top = folder.ancestors().find(|d| d.join(".git").exists()).unwrap_or(folder);
    let Ok(below) = folder.strip_prefix(top) else { return vec![] };
    let prefix: Vec<String> = below.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let mut manifests = std::collections::BTreeSet::new();
    for (path, _) in changed.iter().filter(|(p, _)| p.ends_with(".rs") || p.rsplit('/').next() == Some("Cargo.toml")) {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() <= prefix.len() || parts.contains(&"..") || parts.iter().zip(&prefix).any(|(part, own)| !same(part, own)) {
            continue;
        }
        let mut dirs = parts[prefix.len()..parts.len() - 1].to_vec();
        loop {
            let manifest = dirs.iter().map(|d| format!("{d}/")).collect::<String>() + "Cargo.toml";
            let text = std::fs::read_to_string(folder.join(&manifest)).unwrap_or_default();
            if text.lines().any(|l| header(l) == "[package]") {
                if !manifest.chars().any(char::is_control) {
                    let workspace = text.lines().any(|l| header(l) == "[workspace]");
                    manifests.insert((manifest, workspace));
                }
                break;
            }
            if dirs.pop().is_none() {
                break;
            }
        }
    }
    manifests.into_iter().collect()
}

/// The packages that depend on a touched one, at any depth, read from `cargo metadata --no-deps` of its workspace (the
/// graph Cargo itself builds from): a change to a library's API breaks them, not it. Each manifest relative to `folder`,
/// inside it, with its workspace mark, as `rust_manifests` gives them; the touched ones are left out.
pub fn rust_dependents(folder: &std::path::Path, metadata: &str, touched: &[(String, bool)]) -> Vec<(String, bool)> {
    let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata) else { return vec![] };
    // A path as `/`-separated text with no `\\?\` prefix, and the same folded where folders ignore case (ASCII only, so
    // lengths stay).
    let text = |p: &str| {
        let p = p.replace('\\', "/");
        p.strip_prefix("//?/").unwrap_or(&p).trim_end_matches('/').to_string()
    };
    let fold = |p: &str| if cfg!(windows) { p.to_ascii_lowercase() } else { p.to_string() };
    // Each package: its manifest, its folder (folded) and the folders (folded) of the packages it depends on by path.
    let packages: Vec<(String, String, Vec<String>)> = metadata["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let manifest = text(p["manifest_path"].as_str()?);
            let dir = fold(manifest.rsplit_once('/')?.0);
            let deps = p["dependencies"].as_array().into_iter().flatten().filter_map(|d| Some(fold(&text(d["path"].as_str()?))));
            Some((manifest, dir, deps.collect()))
        })
        .collect();
    let root = text(&folder.to_string_lossy());
    let within = |manifest: &str| {
        let head = manifest.get(..root.len()).filter(|head| fold(head) == fold(&root))?;
        manifest[head.len()..].strip_prefix('/').map(String::from)
    };
    let mut seen: std::collections::BTreeSet<String> = packages
        .iter()
        .filter(|(manifest, ..)| within(manifest).is_some_and(|rel| touched.iter().any(|(t, _)| fold(t) == fold(&rel))))
        .map(|(_, dir, _)| dir.clone())
        .collect();
    let mut todo: Vec<String> = seen.iter().cloned().collect();
    while let Some(dir) = todo.pop() {
        for (_, dependent, deps) in &packages {
            if deps.contains(&dir) && seen.insert(dependent.clone()) {
                todo.push(dependent.clone());
            }
        }
    }
    let mut found = std::collections::BTreeSet::new();
    for (manifest, dir, _) in &packages {
        let Some(rel) = within(manifest).filter(|_| seen.contains(dir)) else { continue };
        if touched.iter().any(|(t, _)| fold(t) == fold(&rel)) || rel.split('/').any(|p| p == "..") || rel.chars().any(char::is_control) {
            continue;
        }
        let workspace = std::fs::read_to_string(folder.join(&rel)).unwrap_or_default().lines().any(|l| header(l) == "[workspace]");
        found.insert((rel, workspace));
    }
    found.into_iter().collect()
}

/// A manifest line as a table header reads: no comment, no spaces (`[ package ] # x` is `[package]`).
fn header(line: &str) -> String {
    line.split('#').next().unwrap_or_default().chars().filter(|c| !c.is_whitespace()).collect()
}

/// Whether a folder name from git is the folder's own: Windows folders ignore case.
fn same(part: &str, own: &str) -> bool {
    if cfg!(windows) { part.eq_ignore_ascii_case(own) } else { part == own }
}

/// Whether a check is heavy: a Cargo build, of which one runs at a time across every folder.
pub fn heavy(tokens: &[String]) -> bool {
    tokens.first().is_some_and(|p| p.trim_end_matches(".exe").eq_ignore_ascii_case("cargo"))
        && !tokens.iter().skip(1).find(|w| !w.starts_with(['-', '+'])).is_some_and(|sub| sub == "fmt")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn only_a_tool_whose_inputs_are_fixed_selects_and_any_other_command_reads_everything() {
        let scripts: Scripts = [
            ("typecheck", "tsc --noEmit"),
            ("lint", "oxlint ui tests/ui"),
            ("build", "vite build"),
            ("docs", "node scripts/docs.mjs"),
            ("both", "tsc --noEmit && node scripts/docs.mjs"),
            ("again", "npm run typecheck"),
            ("loop", "NPM run loop"),
            ("check", "tsc --noEmit"),
            ("precheck", "node check-docs.mjs"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let reads = |line: &str, path: &str| watch(&words(line), &scripts).reads(path);
        assert!(reads("cargo fmt --all --check", "engine/src/verify.rs") && reads("cargo fmt --all --check", "engine/Cargo.toml"));
        assert!(!reads("cargo fmt --all --check", "ui/App.tsx") && !reads("cargo fmt --all --check", "docs/a.md"));
        assert!(reads("npm run typecheck", "ui/App.tsx") && reads("npm run typecheck", "ui/data.json"));
        assert!(!reads("npm run typecheck", "engine/src/lib.rs") && !reads("npm run lint", "docs/a.md"));
        // A Cargo build reads data files (`include_str!`, fixtures), a bundler and a docs script anything: they always run.
        for every in [
            "cargo test",
            "cargo clippy --workspace",
            "cargo t --claims",
            "npm run build",
            "npm run docs",
            "npm run both",
            "npm run again",
            "npm run loop",
            "npm run check",
            "npm run missing",
            "npm test",
            "eslint .",
            "make check",
        ] {
            assert_eq!(watch(&words(every), &scripts), Watch::All, "{every}");
        }
        assert!(!Watch::Files(RUST_FORMAT).reads("docs/rs"), "an ending is matched on the file name");
        assert!(Watch::Files(RUST_FORMAT).reads("Cargo.toml") && !Watch::Files(RUST_FORMAT).reads("other-Cargo.toml"));
    }

    #[test]
    fn only_a_cargo_build_is_heavy() {
        assert!(heavy(&words("cargo clippy --workspace")) && heavy(&words("cargo t --claims")));
        assert!(!heavy(&words("cargo fmt --all --check")) && !heavy(&words("npm run build")));
    }

    #[test]
    fn the_packages_that_depend_on_a_touched_one_at_any_depth_are_checked_too() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("ws");
        std::fs::create_dir_all(root.join("app")).unwrap();
        std::fs::write(root.join("app/Cargo.toml"), "[package]\nname = \"app\"\n[workspace]\n").unwrap();
        let at = |p: &str| root.join(p).to_string_lossy().into_owned();
        let package = |manifest: &str, deps: &[String]| {
            serde_json::json!({
                "manifest_path": at(manifest),
                "dependencies": deps.iter().map(|d| serde_json::json!({"name": "x", "path": d})).chain([serde_json::json!({"name": "serde"})]).collect::<Vec<_>>(),
            })
        };
        let metadata = serde_json::json!({"packages": [
            package("core/Cargo.toml", &[]),
            package("app/Cargo.toml", &[at("core")]),
            package("tool/Cargo.toml", &[at("app")]),
            package("other/Cargo.toml", &[]),
            // Outside the project's folder: never named.
            serde_json::json!({"manifest_path": dir.path().join("away/Cargo.toml").to_string_lossy(), "dependencies": [{"name": "x", "path": at("core")}]}),
        ]})
        .to_string();
        let touched = [("core/Cargo.toml".to_string(), false)];
        let found = rust_dependents(&root, &metadata, &touched);
        assert_eq!(found, [("app/Cargo.toml".to_string(), true), ("tool/Cargo.toml".to_string(), false)]);
        assert!(rust_dependents(&root, &metadata, &[("other/Cargo.toml".into(), false)]).is_empty(), "nothing depends on it");
        assert!(rust_dependents(&root, "not json", &touched).is_empty());
        if cfg!(windows) {
            let upper = std::path::PathBuf::from(root.to_string_lossy().to_uppercase());
            assert_eq!(rust_dependents(&upper, &metadata, &touched).len(), 2, "a folder whose case differs from Cargo's");
            let verbatim = std::path::PathBuf::from(format!(r"\\?\{}", root.display()));
            assert_eq!(rust_dependents(&verbatim, &metadata, &touched).len(), 2, r"a folder written with the \\?\ prefix");
        }
    }

    #[test]
    fn a_rust_change_names_the_manifest_of_the_package_that_holds_it() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path();
        std::fs::create_dir_all(top.join(".git")).unwrap();
        // The project is opened in a subfolder of the repository: git's paths start with `app/`.
        let root = top.join("app");
        let write = |path: &str, text: &str| {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        write("Cargo.toml", "[package]\nname = \"app\"\n\n[workspace]\nmembers = [\"engine\"]\ndefault-members = [\"engine\"]\n");
        write("engine/Cargo.toml", "[dependencies]\nname = 'x'\n\n[ package ] # the engine\nname = 'engine'\n");
        write("vendor/engine/Cargo.toml", "[package]\nname = \"engine\"\n");
        write("odd dir/Cargo.toml", "[package] # spaces in its folder\nname = \"odd\"\n");
        let changed = |paths: &[&str]| paths.iter().map(|p| (p.to_string(), None)).collect::<Vec<_>>();
        let manifest = |path: &str, workspace: bool| (path.to_string(), workspace);
        assert_eq!(
            rust_manifests(&root, &changed(&["app/engine/src/verify/select.rs", "app/engine/Cargo.toml", "app/engine/src/lib.rs"])),
            [manifest("engine/Cargo.toml", false)]
        );
        // A nested workspace of its own, with a package of the same name, is named by its path.
        assert_eq!(rust_manifests(&root, &changed(&["app/vendor/engine/src/lib.rs"])), [manifest("vendor/engine/Cargo.toml", false)]);
        // The root is a package and a workspace whose default members leave it out: it is checked as a workspace.
        assert_eq!(rust_manifests(&root, &changed(&["app/build.rs"])), [manifest("Cargo.toml", true)]);
        assert_eq!(rust_manifests(&root, &changed(&["app/odd dir/src/lib.rs"])), [manifest("odd dir/Cargo.toml", false)]);
        assert!(rust_manifests(&root, &changed(&["app/ui/App.tsx", "app/docs/a.md"])).is_empty(), "no Rust file, no package");
        assert!(rust_manifests(&root, &changed(&["other/src/lib.rs", "app/../x/src/lib.rs"])).is_empty(), "outside the folder");
        if cfg!(windows) {
            assert_eq!(rust_manifests(&root, &changed(&["APP/engine/src/lib.rs"])), [manifest("engine/Cargo.toml", false)], "case");
        }
    }
}
