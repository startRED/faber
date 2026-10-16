//! One persistent working copy per project, where a mission's setup and proofs run. A mission's own lease worktree is
//! made and removed for every mission, so a build that lives in it (node_modules, an incremental build) starts from zero
//! each time. This copy stays: each mission resets it to the mission's base commit, removes what is untracked but keeps
//! what the project's `.gitignore` ignores (the installed packages and build output), lays the candidate's changes over
//! it, and runs the setup and the proofs there. The lease worktree stays the only place the Writer writes and the only
//! source of what lands; this copy never lands anything.
//!
//! It is a second worktree of the same repository (`git worktree add`), so no object is fetched or copied and no
//! transport is used. Its folder is Faber's own, under the engine's data folder.

use super::git::{Executor, Invocation, valid_oid};
use super::proof::Definition;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    dir: PathBuf,
}

/// What a mission needs of the working copy: where it is, which repository it belongs to, and the setup that fills it.
#[derive(Clone)]
pub struct Stage {
    pub workspace: Workspace,
    /// The repository's own checkout: the copy is a worktree of it.
    pub repo_root: PathBuf,
    /// The install steps, run in the copy before the proofs (see `mission::setup_commands`).
    pub setup: Vec<Definition>,
}

fn valid_project(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A path the lease reports as changed, as a clean relative path that stays inside the copy and is not git's own folder.
fn clean(path: &str) -> bool {
    let p = Path::new(path);
    !path.is_empty()
        && p.is_relative()
        && p.components().all(|c| matches!(c, std::path::Component::Normal(_)))
        && !path.split('/').any(|part| part.eq_ignore_ascii_case(".git"))
        && !path.contains(['\\', ':', '\0'])
}

/// The paths of `git status --porcelain=v1 -z --no-renames`: `XY path\0` each.
fn changed(stdout: &[u8]) -> Result<Vec<String>, &'static str> {
    stdout
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let text = std::str::from_utf8(entry).map_err(|_| "reason.workspace.unsafe")?;
            let path = text.get(3..).ok_or("reason.workspace.unsafe")?;
            if clean(path) { Ok(path.to_string()) } else { Err("reason.workspace.unsafe") }
        })
        .collect()
}

impl Workspace {
    pub fn new(data: &Path, project: &str) -> Result<Self, &'static str> {
        if !valid_project(project) {
            return Err("reason.workspace.unsafe");
        }
        // Git does not accept a verbatim path (the extended-length form) in --work-tree, so the copy is always named in its
        // ordinary form.
        let plain = data.to_string_lossy();
        let data =
            plain.strip_prefix(r"\\?\").filter(|rest| rest.as_bytes().get(1) == Some(&b':')).map_or(data.to_path_buf(), PathBuf::from);
        let root = data.join("workspaces").join(project);
        Ok(Self { dir: root.join("tree"), root })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn git(exec: &mut impl Executor, cwd: &Path, args: &[&str]) -> Result<Vec<u8>, &'static str> {
        let out = exec.execute(&Invocation::new(cwd, args)).map_err(|_| "reason.workspace.git")?;
        if out.success { Ok(out.stdout) } else { Err("reason.workspace.git") }
    }

    /// Where the copy's git folder is, recorded outside the copy. A proof runs at Low integrity and may rewrite the copy's
    /// own `.git` file to point git elsewhere, so Faber never lets git find the repository from inside the copy.
    fn gitdir_file(&self) -> PathBuf {
        self.root.join("gitdir")
    }

    /// A git command on the copy, with the repository and work tree given explicitly from the recorded location.
    fn tree_git(&self, exec: &mut impl Executor, args: &[&str]) -> Result<Vec<u8>, &'static str> {
        let gitdir = std::fs::read_to_string(self.gitdir_file()).map_err(|_| "reason.workspace.git")?;
        let gitdir = gitdir.trim();
        if gitdir.is_empty() {
            return Err("reason.workspace.git");
        }
        let (git_dir, work_tree) = (format!("--git-dir={gitdir}"), format!("--work-tree={}", self.dir.display()));
        let all: Vec<&str> = [git_dir.as_str(), work_tree.as_str()].into_iter().chain(args.iter().copied()).collect();
        Self::git(exec, &self.dir, &all)
    }

    /// Bring the copy to `base` plus the candidate's changes in `from` (the lease worktree). Ignored files stay.
    pub fn sync(&self, exec: &mut impl Executor, repo_root: &Path, base: &str, from: &Path) -> Result<(), &'static str> {
        if !valid_oid(base) {
            return Err("reason.workspace.git");
        }
        let dir = self.dir.to_string_lossy().into_owned();
        let usable = self.dir.is_dir() && self.tree_git(exec, &["rev-parse", "--is-inside-work-tree"]).is_ok();
        if !usable {
            // The folder is Faber's own: whatever a failed earlier attempt left there goes.
            let _ = std::fs::remove_file(self.gitdir_file());
            if self.dir.exists() {
                std::fs::remove_dir_all(&self.dir).map_err(|_| "reason.workspace.git")?;
            }
            std::fs::create_dir_all(self.dir.parent().ok_or("reason.workspace.git")?).map_err(|_| "reason.workspace.git")?;
            Self::git(exec, repo_root, &["worktree", "prune"])?;
            Self::git(exec, repo_root, &["worktree", "add", "--detach", "--force", &dir, base])?;
            // Recorded now, before any proof has run in the copy.
            let found = Self::git(exec, &self.dir, &["rev-parse", "--absolute-git-dir"])?;
            let found = String::from_utf8_lossy(&found).trim().to_string();
            if found.is_empty() {
                return Err("reason.workspace.git");
            }
            std::fs::write(self.gitdir_file(), found).map_err(|_| "reason.workspace.git")?;
        }
        self.tree_git(exec, &["checkout", "--detach", "--force", base])?;
        self.tree_git(exec, &["reset", "--hard", base])?;
        // Untracked files go; ignored ones (no -x) stay: that is the point of keeping the copy.
        self.tree_git(exec, &["clean", "-fd"])?;
        let status = Self::git(exec, from, &["status", "--porcelain=v1", "-z", "-uall", "--no-renames"])?;
        for path in changed(&status)? {
            let (source, target) = (from.join(&path), self.dir.join(&path));
            match std::fs::symlink_metadata(&source) {
                Ok(meta) if meta.is_file() => {
                    if target.is_dir() {
                        std::fs::remove_dir_all(&target).map_err(|_| "reason.workspace.unsafe")?;
                    }
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(|_| "reason.workspace.unsafe")?;
                    }
                    std::fs::copy(&source, &target).map_err(|_| "reason.workspace.unsafe")?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if target.is_dir() {
                        std::fs::remove_dir_all(&target).map_err(|_| "reason.workspace.unsafe")?;
                    } else if target.exists() {
                        std::fs::remove_file(&target).map_err(|_| "reason.workspace.unsafe")?;
                    }
                }
                _ => return Err("reason.workspace.unsafe"),
            }
        }
        Ok(())
    }

    /// What a setup step stands on: its command, the lock file it installs from, and whether its result is in the copy.
    /// The same key before and after a run means there is nothing to install again.
    pub fn setup_key(&self, definition: &Definition) -> String {
        let (lock, result) = match definition.toolchain.as_str() {
            "npm" => ("package-lock.json", self.dir.join("node_modules").is_dir()),
            "cargo" => ("Cargo.lock", true),
            _ => ("", true),
        };
        let digest = Sha256::digest(std::fs::read(self.dir.join(lock)).unwrap_or_default());
        format!("{} {} {digest:x} {result}", definition.id, definition.argv.join(" "))
    }

    fn stamp(&self, definition: &Definition) -> PathBuf {
        self.root.join(format!("{}.stamp", definition.id))
    }

    /// The step ran for exactly this key before: nothing to install again.
    pub fn setup_current(&self, definition: &Definition) -> bool {
        std::fs::read_to_string(self.stamp(definition)).is_ok_and(|saved| saved == self.setup_key(definition))
    }

    pub fn setup_done(&self, definition: &Definition) {
        let _ = std::fs::write(self.stamp(definition), self.setup_key(definition));
    }

    pub fn setup_forget(&self, definition: &Definition) {
        let _ = std::fs::remove_file(self.stamp(definition));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_clean_relative_paths_inside_the_copy_are_laid_over_it() {
        for good in ["a.rs", "src/a.rs", "dir/sub/file.txt"] {
            assert!(clean(good), "{good}");
        }
        for bad in ["", "/etc/x", "../x", "a/../x", ".git/config", "a/.GIT/x", "C:/x", "a\\b", "a:b", "./a"] {
            assert!(!clean(bad), "{bad}");
        }
        assert_eq!(changed(b" M src/a.rs\0?? new.txt\0 D gone.txt\0").unwrap(), ["src/a.rs", "new.txt", "gone.txt"]);
        assert_eq!(changed(b" M ../escape\0"), Err("reason.workspace.unsafe"));
        assert_eq!(changed(b"M\0"), Err("reason.workspace.unsafe"));
    }

    #[test]
    fn a_workspace_is_named_by_a_plain_project_id_only() {
        let data = Path::new("data");
        assert!(Workspace::new(data, "project_1-a").is_ok());
        for bad in ["", "../x", "a/b", "a b", "a\\b"] {
            assert!(Workspace::new(data, bad).is_err(), "{bad}");
        }
        assert_eq!(Workspace::new(data, "p1").unwrap().dir(), Path::new("data").join("workspaces").join("p1").join("tree"));
        let verbatim = Workspace::new(Path::new(r"\\?\E:\data"), "p1").unwrap();
        assert!(!verbatim.dir().to_string_lossy().starts_with(r"\\?\"), "{:?}", verbatim.dir());
    }
}
