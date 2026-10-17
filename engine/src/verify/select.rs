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
}
