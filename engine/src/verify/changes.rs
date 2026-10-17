//! What changed in a folder: the files that differ from where its branch left its base, committed or not, plus the new
//! ones git does not ignore, each with a hash of its content. Asked of git through the engine's own runner (fixed argv, no
//! shell, no hooks); no file watcher in V1, so a verify asks git each time.

use crate::project::git::{Executor, Invocation, valid_oid};
use std::path::Path;

/// The changed files of a folder.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changed {
    /// The commit the changes are counted from (the merge base with the base branch, or the head).
    pub base: String,
    /// Each changed path, relative to the repository's top with `/`, and the hash of its content: `None` when the file is
    /// gone or was not hashed (too big, or too many files), which no early cutoff ever trusts.
    pub files: Vec<(String, Option<String>)>,
}

/// The most files hashed and the largest one: past either, the file counts as changed with no hash.
const MAX_HASHED: usize = 2_000;
const MAX_BYTES: u64 = 16 * 1024 * 1024;

fn git(exec: &mut dyn Executor, cwd: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = exec.execute(&Invocation::new(cwd, args)).ok()?;
    out.success.then_some(out.stdout)
}

fn line(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).trim().to_string()
}

/// Paths of a `-z` list.
fn paths(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned())
}

/// Faber's own run logs (`.faber/verify/`), which a verify writes and must never count as a change.
pub fn own_log(path: &str) -> bool {
    path.starts_with(".faber/verify/") || path.contains("/.faber/verify/")
}

/// The changed files of `folder`, counted from its merge base with `base` (the person's branch a copy came from), or else
/// with `origin/HEAD`, its upstream, `main` or `master`. `None` (nothing is known, so every check runs) when the folder is
/// not in a git repository with a commit, or when no base tells what its commits changed.
pub fn changed(exec: &mut dyn Executor, folder: &Path, base: Option<&str>) -> Option<Changed> {
    let top = line(git(exec, folder, &["rev-parse", "--show-toplevel"])?);
    let head = line(git(exec, folder, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?);
    if top.is_empty() || !valid_oid(&head) {
        return None;
    }
    let candidates = base
        .into_iter()
        .map(|b| (b, true))
        .chain(["refs/remotes/origin/HEAD", "@{upstream}", "refs/heads/main", "refs/heads/master"].into_iter().map(|b| (b, false)));
    let mut from = None;
    for (candidate, hint) in candidates {
        if let Some(found) = git(exec, folder, &["merge-base", "HEAD", candidate]).map(line).filter(|b| valid_oid(b)) {
            from = Some((found, hint));
            break;
        }
    }
    // Without the branch a copy came from, a base that is HEAD itself (work committed on `main`, no remote) says nothing
    // of what the commits changed: nothing is known, so every check runs and nothing is cut off.
    let from = match from {
        Some((found, hint)) if hint || found != head => found,
        _ => return None,
    };
    let diff = git(exec, folder, &["diff", "--name-only", "-z", "--no-renames", &from, "--"])?;
    let others = git(exec, folder, &["ls-files", "--others", "--exclude-standard", "-z", "--full-name"]).unwrap_or_default();
    let mut names: Vec<String> = paths(&diff).chain(paths(&others)).filter(|p| !own_log(p)).collect();
    names.sort();
    names.dedup();
    let top = Path::new(&top);
    let files = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let hash = (index < MAX_HASHED).then(|| hash(&top.join(&name))).flatten();
            (name, hash)
        })
        .collect();
    Some(Changed { base: from, files })
}

/// The hash of a file's content, when it is a file small enough to read.
fn hash(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    std::fs::read(path).ok().map(|bytes| crate::project::kit::digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::git::{GitError, Output};

    /// Git as a table: each call's arguments (without the fixed `-c` pairs) and what it prints.
    struct Table(Vec<(&'static [&'static str], &'static [u8])>, Vec<Vec<String>>);
    impl Executor for Table {
        fn execute(&mut self, call: &Invocation) -> Result<Output, GitError> {
            let args: Vec<String> = call.args.iter().skip_while(|a| *a == "-c" || a.contains('=')).cloned().collect();
            self.1.push(args.clone());
            let found = self.0.iter().find(|(want, _)| want.iter().map(|w| w.to_string()).eq(args.iter().cloned()));
            Ok(Output { success: found.is_some(), stdout: found.map(|(_, out)| out.to_vec()).unwrap_or_default() })
        }
    }

    const FORK: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn the_changes_count_from_where_the_branch_left_its_base_and_leave_faber_s_own_logs_out() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ui")).unwrap();
        std::fs::write(dir.path().join("ui/App.tsx"), "a").unwrap();
        let top: &'static str = Box::leak(format!("{}\n", dir.path().display()).into_boxed_str());
        let mut git = Table(
            vec![
                (&["rev-parse", "--show-toplevel"], top.as_bytes()),
                (&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"], b"1111111111111111111111111111111111111111\n"),
                (&["merge-base", "HEAD", "refs/heads/main"], b"2222222222222222222222222222222222222222\n"),
                (&["diff", "--name-only", "-z", "--no-renames", FORK, "--"], b"ui/App.tsx\0gone.rs\0"),
                (
                    &["ls-files", "--others", "--exclude-standard", "-z", "--full-name"],
                    b"new.md\0.faber/verify/1-fast.log\0app/.faber/verify/x.log\0",
                ),
            ],
            vec![],
        );
        let found = changed(&mut git, dir.path(), Some("refs/heads/main")).unwrap();
        assert_eq!(found.base, FORK);
        let names: Vec<&str> = found.files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(names, ["gone.rs", "new.md", "ui/App.tsx"]);
        assert_eq!(found.files[0].1, None, "a deleted file has no hash");
        assert_eq!(found.files[2].1.as_deref(), Some(crate::project::kit::digest(b"a").as_str()));
        assert!(git.1.iter().all(|args| args.iter().all(|a| !a.contains("verify/"))), "git is never asked about the logs");
    }

    #[test]
    fn a_folder_outside_git_has_no_changes_to_select_from() {
        let mut git = Table(vec![], vec![]);
        assert_eq!(changed(&mut git, Path::new("."), None), None);
    }

    #[test]
    fn work_committed_on_main_itself_is_not_known_so_nothing_is_cut_off() {
        const HEAD: &[u8] = b"1111111111111111111111111111111111111111\n";
        let table = |base: &'static [u8]| {
            Table(
                vec![
                    (&["rev-parse", "--show-toplevel"], b"/repo\n"),
                    (&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"], HEAD),
                    (&["merge-base", "HEAD", "refs/heads/main"], base),
                    (&["diff", "--name-only", "-z", "--no-renames", "1111111111111111111111111111111111111111", "--"], b""),
                    (&["ls-files", "--others", "--exclude-standard", "-z", "--full-name"], b""),
                ],
                vec![],
            )
        };
        assert_eq!(changed(&mut table(HEAD), Path::new("."), None), None, "on main, main is no base");
        // The branch a copy came from is: a copy with no commit of its own has only what is not committed.
        let copy = changed(&mut table(HEAD), Path::new("."), Some("refs/heads/main")).unwrap();
        assert!(copy.files.is_empty());
    }
}
