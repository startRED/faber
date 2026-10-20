//! `@` in the composer (parity T4): the files and folders of a conversation's working folder that match what the person typed.
//! In a git repository git lists them, so what it ignores stays out; elsewhere a walk leaves out `.git` and the usual build
//! folders. No shell runs, and the list stays small.
use crate::project::git::{Executor, Invocation};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;

/// The most matches one query returns.
pub const LIMIT: usize = 50;
/// A folder that is not a repository is walked up to this many entries, so one query stays quick.
const MOST_ENTRIES: usize = 20_000;
/// Folders a walk never enters: dependencies and build output, which git usually ignores too.
const SKIP: [&str; 10] = [".git", "node_modules", "target", "dist", "build", "out", "vendor", "coverage", "__pycache__", ".faber"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Mention {
    /// Relative to the working folder, with `/` between its parts; a folder ends with `/`.
    pub path: String,
    pub dir: bool,
}

/// The entries of `root` that match `query`, the best first, at most `LIMIT`.
pub fn find(root: &Path, git: Option<&mut dyn Executor>, query: &str) -> Vec<Mention> {
    let files = match git.and_then(|g| listed(g, root)) {
        Some(files) => files,
        // In a repository git could not list, a walk would offer what git ignores.
        None if root.ancestors().any(|a| a.join(".git").exists()) => return vec![],
        None => walked(root),
    };
    // Only an entry that is there now and reached through no link (which may lead out of the folder) is offered. The best
    // matches are looked at in order until the list is full, so a few disk reads per entry shown.
    let there = |m: &Mention| {
        let mut at = root.to_path_buf();
        let parts: Vec<&str> = m.path.split('/').filter(|p| !p.is_empty()).collect();
        parts.iter().enumerate().all(|(i, part)| {
            at.push(part);
            // Every part before the last is a folder, and the last is what the entry says it is.
            let dir = i + 1 < parts.len() || m.dir;
            std::fs::symlink_metadata(&at).is_ok_and(|d| !d.file_type().is_symlink() && d.is_dir() == dir)
        })
    };
    rank(&files, query).into_iter().filter(there).take(LIMIT).collect()
}

/// A path git reported that stays inside the folder and that the composer can write back: relative, no `..`, no drive, no
/// backslash (git never writes one on Windows; elsewhere it is part of a name that a reader could take for a separator)
/// and no double quote (which ends a quoted path in the message).
fn inside(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains(['\\', ':', '"']) && path.split('/').all(|part| part != "..")
}

/// What git knows of the folder, tracked or new, without what it ignores; None outside a repository.
fn listed(git: &mut dyn Executor, root: &Path) -> Option<Vec<String>> {
    let out = git.execute(&Invocation::new(root, &["ls-files", "-z", "--cached", "--others", "--exclude-standard"])).ok()?;
    if !out.success {
        return None;
    }
    Some(out.stdout.split(|b| *b == 0).map(|p| String::from_utf8_lossy(p).into_owned()).filter(|p| inside(p)).collect())
}

/// The files and folders (ending with `/`) under `root`, never following a link.
fn walked(root: &Path) -> Vec<String> {
    let (mut files, mut dirs, mut seen) = (vec![], vec![String::new()], 0usize);
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&dir)) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MOST_ENTRIES {
                return files;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !inside(&name) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            let path = format!("{dir}{name}");
            if kind.is_dir() && !SKIP.contains(&name.as_str()) {
                dirs.push(format!("{path}/"));
                files.push(format!("{path}/"));
            } else if kind.is_file() {
                files.push(path);
            }
        }
    }
    files
}

/// Files and the folders they sit in, matched on the whole path without case. A name that starts with the query comes
/// first, then a name that holds it, then a path that holds it; shorter paths before longer ones.
fn rank(files: &[String], query: &str) -> Vec<Mention> {
    let query = query.trim().replace('\\', "/").to_lowercase();
    let mut folders = BTreeSet::new();
    for file in files {
        if file.ends_with('/') {
            folders.insert(file.clone());
        }
        let mut end = 0;
        while let Some(at) = file[end..].find('/') {
            end += at + 1;
            folders.insert(file[..end].to_string());
        }
    }
    let entries = folders
        .into_iter()
        .map(|path| Mention { path, dir: true })
        .chain(files.iter().filter(|f| !f.ends_with('/')).map(|f| Mention { path: f.clone(), dir: false }));
    let mut scored: Vec<(u8, Mention)> = entries
        .filter_map(|m| {
            let path = m.path.to_lowercase();
            let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string();
            let score = if name.starts_with(&query) {
                0
            } else if name.contains(&query) {
                1
            } else if path.contains(&query) {
                2
            } else {
                return None;
            };
            Some((score, m))
        })
        .collect();
    scored.sort_by(|(a, x), (b, y)| a.cmp(b).then(x.path.len().cmp(&y.path.len())).then(x.path.cmp(&y.path)));
    scored.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::git::{GitError, Output};

    struct Git(Option<&'static str>, Vec<Vec<String>>);
    impl Executor for Git {
        fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
            self.1.push(call.args.clone());
            Ok(Output { success: self.0.is_some(), stdout: self.0.unwrap_or_default().as_bytes().to_vec() })
        }
    }

    #[test]
    fn in_a_repository_git_lists_the_files_without_the_ignored_ones() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        for file in ["src/parser.rs", "src/lexer.rs", "README.md"] {
            std::fs::write(dir.path().join(file), "x").unwrap();
        }
        let mut git = Git(Some("src/parser.rs\0src/lexer.rs\0README.md\0src/parsed-gone.rs\0"), vec![]);
        let found = find(dir.path(), Some(&mut git), "pars");
        assert_eq!(found, [Mention { path: "src/parser.rs".into(), dir: false }], "a file deleted since is not offered");
        std::fs::write(dir.path().join("lib"), "now a file").unwrap();
        let mut moved = Git(Some("lib/a.rs\0"), vec![]);
        assert_eq!(find(dir.path(), Some(&mut moved), "lib"), [], "a folder that became a file is not offered");
        let args = git.1[0].join(" ");
        assert!(args.ends_with("ls-files -z --cached --others --exclude-standard"), "{args}");
        assert!(args.contains("core.hooksPath="), "no hook of the folder runs: {args}");
        let all = find(dir.path(), Some(&mut git), "");
        assert_eq!(all[0], Mention { path: "src/".into(), dir: true }, "a folder is offered too, the shortest first");
    }

    #[test]
    fn outside_a_repository_a_walk_leaves_out_git_dependencies_and_build_output() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["notes.md", "src/app.ts", "node_modules/x/app.ts", "target/debug/app.exe", "dist/app.js"] {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let mut git = Git(None, vec![]);
        let found: Vec<String> = find(dir.path(), Some(&mut git), "app").into_iter().map(|m| m.path).collect();
        assert_eq!(found, ["src/app.ts"]);
        let found: Vec<String> = find(dir.path(), None, "").into_iter().map(|m| m.path).collect();
        assert_eq!(found, ["src/", "notes.md", "src/app.ts"]);
        std::fs::create_dir(dir.path().join("assets")).unwrap();
        let found: Vec<String> = find(dir.path(), None, "assets").into_iter().map(|m| m.path).collect();
        assert_eq!(found, ["assets/"], "an empty folder is offered too");
    }

    #[test]
    fn a_path_that_would_leave_the_folder_is_never_offered_nor_a_walk_inside_a_repository() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ok.txt"), "x").unwrap();
        let mut git = Git(Some("..\\outside.txt\0../up.txt\0/etc/passwd\0C:/x.txt\0a/../b.txt\0ok.txt\0"), vec![]);
        let found: Vec<String> = find(dir.path(), Some(&mut git), "").into_iter().map(|m| m.path).collect();
        assert_eq!(found, ["ok.txt"]);
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("private.txt"), "x").unwrap();
        let mut broken = Git(None, vec![]);
        assert_eq!(find(dir.path(), Some(&mut broken), "private"), [], "a repository git cannot list shows nothing");
    }

    #[cfg(unix)]
    #[test]
    fn a_tracked_path_through_a_linked_folder_is_left_out() {
        let (dir, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        std::fs::write(out.path().join("file.txt"), "x").unwrap();
        std::os::unix::fs::symlink(out.path(), dir.path().join("dir")).unwrap();
        std::fs::write(dir.path().join("own.txt"), "x").unwrap();
        // Many linked entries rank first; they never take the places of the one that is fine.
        let linked: String = (0..60).map(|i| format!("dir/{i}.txt\0")).collect();
        for i in 0..60 {
            std::fs::write(out.path().join(format!("{i}.txt")), "x").unwrap();
        }
        let listing: &'static str = Box::leak(format!("dir/file.txt\0{linked}own.txt\0").into_boxed_str());
        let mut git = Git(Some(listing), vec![]);
        let found: Vec<String> = find(dir.path(), Some(&mut git), "").into_iter().map(|m| m.path).collect();
        assert_eq!(found, ["own.txt"]);
    }

    #[test]
    fn a_name_that_starts_with_the_query_comes_first_and_the_list_stays_small() {
        let dir = tempfile::tempdir().unwrap();
        let mut files: Vec<String> = (0..80).map(|i| format!("deep/tests/case_{i}_main.rs")).collect();
        files.push("main.rs".into());
        files.push("src/lexer.rs".into());
        for file in &files {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let found = find(dir.path(), None, "MAIN");
        assert_eq!(found.len(), LIMIT);
        assert_eq!(found[0].path, "main.rs");
        assert!(found.iter().all(|m| m.path != "src/lexer.rs"));
        assert_eq!(rank(&files, "lex")[0].path, "src/lexer.rs");
    }
}
