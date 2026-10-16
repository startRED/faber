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
use super::lease::{real_path, reparse};
use super::proof::Definition;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    dir: PathBuf,
    /// The proofs' own folder (build output, cargo home, npm cache, temp), see `mission::setup`.
    own: PathBuf,
}

/// Deletes a link itself and never what it points to: a junction or directory link by `remove_dir`, a file link by
/// `remove_file`.
fn unlink(path: &Path) -> Result<(), &'static str> {
    std::fs::remove_dir(path).or_else(|_| std::fs::remove_file(path)).map_err(|_| "reason.workspace.unsafe")
}

/// Walks `dir` without following anything and deletes every link (symbolic link or junction) it holds. `true` when it
/// removed any (npm makes some itself, for workspace packages and `file:` dependencies, so the install is not trusted then).
fn strip_links(dir: &Path) -> Result<bool, &'static str> {
    let mut removed = false;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let entries = match std::fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err("reason.workspace.unsafe"),
        };
        for entry in entries {
            let entry = entry.map_err(|_| "reason.workspace.unsafe")?;
            let meta = entry.metadata().map_err(|_| "reason.workspace.unsafe")?;
            if reparse(&meta) {
                unlink(&entry.path())?;
                removed = true;
            } else if meta.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(removed)
}

/// Empties a folder but keeps the folder itself (it carries the Low label the proofs need), links first.
fn empty(dir: &Path) -> Result<(), &'static str> {
    strip_links(dir)?;
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in entries.flatten() {
        let path = entry.path();
        let gone = if entry.file_type().is_ok_and(|t| t.is_dir()) { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        gone.map_err(|_| "reason.workspace.unsafe")?;
    }
    Ok(())
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

/// The version of what this module keeps beside the copy (the marks). A copy without it, or with another, is wiped once.
const SCHEMA: &str = "1";

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
        let own = data.join("proof-target").join(project);
        Ok(Self { dir: root.join("tree"), root, own })
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

    /// The copy is where a Low-integrity proof ran, so before Faber touches it at its own integrity the copy must really be
    /// the folder it names: neither it nor any folder above it is a link (a proof can empty it and make it a junction to the
    /// owner's checkout). A link at the copy itself is deleted, never followed, and the copy is made fresh. Then every link
    /// inside the copy goes too. Nothing of the proof may still run: its job closed when `proof::execute` returned.
    fn guard(&self) -> Result<bool, &'static str> {
        if real_path(&self.dir).is_err() {
            if std::fs::symlink_metadata(&self.dir).is_ok_and(|meta| reparse(&meta)) {
                unlink(&self.dir)?;
            }
            real_path(&self.dir).map_err(|_| "reason.workspace.unsafe")?;
        }
        strip_links(&self.dir)
    }

    /// The proofs ran on a candidate that has not landed: what they left in the copy and in the proofs' own folder may be
    /// poisoned, so the next sync wipes it. `release` (the candidate landed) lifts it. The mark is written and flushed to disk
    /// before anything runs, and a mission that cannot write it does not run.
    pub fn taint(&self) -> Result<(), &'static str> {
        use std::io::Write;
        let mut mark = std::fs::File::create(self.root.join("tainted")).map_err(|_| "reason.workspace.unsafe")?;
        mark.write_all(b"1").and_then(|()| mark.sync_all()).map_err(|_| "reason.workspace.unsafe")
    }

    /// The candidate landed as `head`: what its proofs left is trusted again, for a base that contains `head`. If the mark
    /// of that head cannot be written, the taint stays and the next sync wipes.
    pub fn release(&self, head: &str) {
        if valid_oid(head) && std::fs::write(self.root.join("landed"), head).is_ok() {
            let _ = std::fs::remove_file(self.root.join("tainted"));
        }
    }

    pub fn tainted(&self) -> bool {
        self.root.join("tainted").exists()
    }

    /// The head the kept state was last trusted for, if the last candidate landed.
    pub fn landed(&self) -> Option<String> {
        std::fs::read_to_string(self.root.join("landed")).ok().map(|text| text.trim().to_string())
    }

    /// The kept state cannot be trusted: a candidate that did not land ran here, or the one that landed is not part of this
    /// base, or the copy was made by older code that kept no marks (exactly one wipe).
    fn stale(&self, exec: &mut impl Executor, repo_root: &Path, base: &str) -> bool {
        if self.tainted() || std::fs::read_to_string(self.root.join("schema")).ok().as_deref() != Some(SCHEMA) {
            return true;
        }
        self.landed()
            .is_some_and(|head| !(valid_oid(&head) && Self::git(exec, repo_root, &["merge-base", "--is-ancestor", &head, base]).is_ok()))
    }

    fn forget_setup(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.root) {
            for entry in entries.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "stamp")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// Bring the copy to `base` plus the candidate's changes in `from` (the lease worktree). Ignored files stay, unless the
    /// last candidate proven here did not land.
    pub fn sync(&self, exec: &mut impl Executor, repo_root: &Path, base: &str, from: &Path) -> Result<(), &'static str> {
        if !valid_oid(base) {
            return Err("reason.workspace.git");
        }
        // Nothing from an earlier proof may still be running when Faber touches the copy.
        if !crate::proc::jobs_quiet() {
            return Err("reason.workspace.busy");
        }
        let stripped = self.guard()?;
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
        let stale = self.stale(exec, repo_root, base);
        if stripped || stale {
            // An install that held a link is not trusted, and neither is a wiped one: the setup runs again.
            self.forget_setup();
        }
        if stale {
            // Everything ignored goes too (installed packages, build output, nested repositories), the proofs' own folder
            // is emptied, and the marks start over.
            self.tree_git(exec, &["clean", "-ffdx"])?;
            empty(&self.own)?;
            let _ = std::fs::remove_file(self.root.join("tainted"));
            let _ = std::fs::remove_file(self.root.join("landed"));
            std::fs::write(self.root.join("schema"), SCHEMA).map_err(|_| "reason.workspace.unsafe")?;
        } else {
            // Untracked files go; ignored ones (no -x) stay: that is the point of keeping the copy.
            self.tree_git(exec, &["clean", "-fd"])?;
        }
        let status = Self::git(exec, from, &["status", "--porcelain=v1", "-z", "-uall", "--no-renames"])?;
        for path in changed(&status)? {
            let (source, target) = (from.join(&path), self.dir.join(&path));
            // No folder on the way to the target may be a link, so nothing is written or deleted outside the copy.
            real_path(&target).map_err(|_| "reason.workspace.unsafe")?;
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
    fn a_taint_that_cannot_be_written_stops_the_mission() {
        let data = std::env::temp_dir().join(format!("faber-taint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let space = Workspace::new(&data, "p1").unwrap();
        std::fs::create_dir_all(&space.root).unwrap();
        assert!(space.taint().is_ok() && space.tainted());
        space.release(&"a".repeat(40));
        assert!(!space.tainted() && space.landed().as_deref() == Some("a".repeat(40).as_str()));
        // Something that is not a file sits where the mark goes: the write fails, and so does the start.
        std::fs::create_dir(space.root.join("tainted")).unwrap();
        assert_eq!(space.taint(), Err("reason.workspace.unsafe"));
        let _ = std::fs::remove_dir_all(&data);
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
