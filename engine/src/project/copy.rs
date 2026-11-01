//! A project conversation's own copy (R201): a git worktree of the project's repository on a branch of its own,
//! `faber/<slug>`, made from the head the person has checked out when the conversation sends its first message. The
//! agent works there, so two conversations and the person's own checkout never touch the same files. Faber only makes
//! and removes copies; bringing the work back (a commit, a push, a pull request) is the agent's, and nothing merges by
//! itself.
//!
//! Every git call goes through the mission broker's runner ([`Invocation`], [`Executor`]): a fixed argv, no shell, no
//! hooks, no filters. A copy lives in Faber's data folder, `copies/<conversation id>`, so the project's own tools never
//! find a second checkout inside it, and the name says which conversation it belongs to without reading any record.
//!
//! A copy goes when its conversation is deleted, and at start when its work is in the person's branch (or that branch's
//! upstream) and nothing is left uncommitted. Uncommitted changes, or commits no other branch holds, are never deleted
//! without the person saying so ([`unsaved`]).

use super::git::{Executor, Invocation, native_path, valid_oid};
use super::lease::real_path;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where a conversation's copy lives and which branch it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkCopy {
    /// The copy's folder: `<data>/copies/<conversation id>`.
    pub path: String,
    /// Its branch, `faber/<slug>`, never the person's.
    pub branch: String,
    /// The repository's main checkout, where git is asked about the copy.
    pub root: String,
    /// Where the project folder sits inside the repository (`app/` or empty at its top): the agent starts in the same
    /// place of the copy.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sub: String,
    /// The person's branch when the copy was made (`refs/heads/main`), which "merged" is measured against. None when the
    /// checkout was on no branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

impl WorkCopy {
    /// The folder the agent starts in.
    pub fn folder(&self) -> PathBuf {
        let mut dir = PathBuf::from(&self.path);
        dir.extend(self.sub.split('/').filter(|part| !part.is_empty()));
        dir
    }
}

/// The folder that holds every conversation's copy.
pub fn home(data: &Path) -> PathBuf {
    // Git does not take a verbatim path (the extended-length form), so the copy is always named in its ordinary form.
    PathBuf::from(native_path(data)).join("copies")
}

/// A conversation id that can name a folder: a UUID's letters, digits and dashes.
fn valid_conversation(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The branch name for a conversation: `faber/` and up to four words of its first message, folded to plain ASCII, then
/// four characters of its id, so two conversations about the same thing never share a branch.
pub fn branch_for(text: &str, id: &str) -> String {
    let fold = |c: char| match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        'ñ' | 'Ñ' => 'n',
        'ß' => 's',
        c if c.is_ascii_alphanumeric() => c.to_ascii_lowercase(),
        _ => ' ',
    };
    let folded: String = text.chars().map(fold).collect();
    let mut words = String::new();
    for word in folded.split_whitespace().take(4) {
        // A long first word is cut; a later one that does not fit ends the name.
        if words.is_empty() {
            words.push_str(&word[..word.len().min(28)]);
        } else if words.len() + 1 + word.len() <= 28 {
            words.push('-');
            words.push_str(word);
        } else {
            break;
        }
    }
    let tag: String = id.chars().filter(char::is_ascii_alphanumeric).take(4).collect::<String>().to_ascii_lowercase();
    if words.is_empty() { format!("faber/work-{tag}") } else { format!("faber/{words}-{tag}") }
}

/// A branch Faber made for a conversation: `faber/` and lowercase letters, digits and dashes.
fn own_branch(branch: &str) -> bool {
    branch.strip_prefix("faber/").is_some_and(|rest| {
        !rest.is_empty() && rest.len() <= 64 && rest.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    })
}

fn git(exec: &mut dyn Executor, cwd: &Path, args: &[&str]) -> Option<String> {
    let out = exec.execute(&Invocation::new(cwd, args)).ok()?;
    out.success.then(|| String::from_utf8_lossy(&out.stdout).trim_end_matches(['\r', '\n']).to_string())
}

/// The full id of the commit a build names (a crash line's `commit`: 7 to 40 hexadecimal digits) in the repository of `folder`,
/// or `None` when the text is not such a name or the repository has no such commit. The text is checked before git sees it.
pub fn resolve(exec: &mut dyn Executor, folder: &Path, name: &str) -> Option<String> {
    let hex = (7..=40).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_hexdigit());
    if !hex {
        return None;
    }
    // Hex text can also be a branch or tag name, which git prefers to an abbreviated id: only an id that begins with it counts.
    let name = name.to_ascii_lowercase();
    git(exec, folder, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")])
        .filter(|oid| valid_oid(oid) && oid.starts_with(&name))
}

/// The commit a copy is at now.
pub fn head(exec: &mut dyn Executor, copy: &Path) -> Option<String> {
    git(exec, copy, &["rev-parse", "--verify", "--quiet", "HEAD"]).filter(|oid| valid_oid(oid))
}

/// Makes the copy of conversation `id` for the project in `folder`, on `branch`, starting from `from` (another copy's
/// branch, for a conversation branched from it), else from `at` (a commit's full id, such as the build that crashed; one the
/// repository does not have is left out) or the head the person has checked out. A copy already in place for this
/// conversation is used as it is, and one removed earlier comes back at the same folder, so the CLI resumes its session.
/// `Ok(None)`: the folder is not in a git repository with a commit, and the conversation works in the folder itself.
pub fn make(
    exec: &mut dyn Executor,
    folder: &Path,
    data: &Path,
    id: &str,
    branch: &str,
    from: Option<&str>,
    at: Option<&str>,
) -> Result<Option<WorkCopy>, &'static str> {
    if !valid_conversation(id) || !own_branch(branch) || from.is_some_and(|b| !own_branch(b)) || at.is_some_and(|c| !valid_oid(c)) {
        return Err("error.copy.failed");
    }
    let Some(top) = git(exec, folder, &["rev-parse", "--show-toplevel", "--show-prefix"]) else { return Ok(None) };
    let mut lines = top.lines();
    let root = lines.next().unwrap_or_default().trim().to_string();
    let sub = lines.next().unwrap_or_default().trim().trim_end_matches('/').to_string();
    let Some(head) = git(exec, folder, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).filter(|h| valid_oid(h)) else {
        return Ok(None);
    };
    if root.is_empty() || sub.split('/').any(|part| part == ".." || part == ".git") {
        return Err("error.copy.failed");
    }
    // A folder the repository does not hold (an untracked folder inside a home folder kept in git) would be missing from
    // the copy: it works in the folder itself.
    if !sub.is_empty() && git(exec, folder, &["rev-parse", "--verify", "--quiet", &format!("HEAD:{sub}")]).is_none() {
        return Ok(None);
    }
    let base = git(exec, folder, &["symbolic-ref", "--quiet", "HEAD"]).filter(|r| r.starts_with("refs/heads/"));
    let path = home(data).join(id);
    let copy = WorkCopy { path: path.to_string_lossy().into_owned(), branch: branch.to_string(), root: root.clone(), sub, base };
    // No folder on the way may be a link: the copy is made, and later deleted, only inside Faber's own folder.
    real_path(&path).map_err(|_| "error.copy.failed")?;
    if path.exists() {
        // The same conversation's copy, already made: git must see it as a work tree of its own on this branch.
        let on = git(exec, &path, &["symbolic-ref", "--quiet", "HEAD"]);
        return if on.as_deref() == Some(&format!("refs/heads/{branch}")) { Ok(Some(copy)) } else { Err("error.copy.failed") };
    }
    std::fs::create_dir_all(home(data)).map_err(|_| "error.copy.failed")?;
    let target = copy.path.clone();
    let root = Path::new(&root);
    // A copy set aside at start stays registered at this folder until git forgets it, and holds its branch till then.
    let _ = git(exec, root, &["worktree", "prune"]);
    let exists = |exec: &mut dyn Executor, branch: &str| {
        git(exec, root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_some()
    };
    let made = if exists(exec, branch) {
        git(exec, root, &["worktree", "add", "--", &target, branch])
    } else {
        // A source branch removed since (its work merged) leaves the head the person has.
        let held =
            at.filter(|oid| git(exec, root, &["rev-parse", "--verify", "--quiet", &format!("{oid}^{{commit}}")]).as_deref() == Some(*oid));
        let start = from.filter(|from| exists(exec, from)).or(held).unwrap_or(&head);
        git(exec, root, &["worktree", "add", "-b", branch, "--", &target, start])
    };
    made.map(|_| Some(copy)).ok_or("error.copy.failed")
}

/// Whether everything `tip` holds is also on a ref other than `own`: another local branch or a remote one.
fn held(exec: &mut dyn Executor, root: &Path, tip: &str, own: &str) -> bool {
    git(exec, root, &["for-each-ref", "--contains", tip, "--format=%(refname)", "refs/heads/", "refs/remotes/"])
        .is_some_and(|holders| holders.lines().any(|r| !r.is_empty() && r != own))
}

/// The copy holds work that exists nowhere else: changes not committed, or commits that no other branch, local or remote,
/// holds, on its branch or wherever its HEAD is now (the agent may have detached it or moved to a branch of its own). A
/// question git cannot answer counts as unsaved.
pub fn unsaved(exec: &mut dyn Executor, copy: &WorkCopy) -> bool {
    let (path, root) = (Path::new(&copy.path), Path::new(&copy.root));
    let own = format!("refs/heads/{}", copy.branch);
    let mut tips = vec![];
    if path.is_dir() {
        match git(exec, path, &["status", "--porcelain", "-z"]) {
            Some(changes) if changes.is_empty() => {}
            _ => return true,
        }
        match git(exec, path, &["rev-parse", "--verify", "--quiet", "HEAD"]) {
            Some(head) => tips.push(head),
            None => return true,
        }
    }
    match git(exec, root, &["rev-parse", "--verify", "--quiet", &own]) {
        Some(tip) => tips.push(tip),
        // No branch and no copy: nothing was committed that could be lost, unless the repository itself cannot be read.
        None if !root.is_dir() => return true,
        None => {}
    }
    tips.dedup();
    !tips.iter().all(|tip| held(exec, root, tip, &own))
}

/// Reclaims only ignored, untracked build folders in an old, stopped copy. Returns freed bytes and entries inspected.
pub fn cleanup(
    exec: &mut dyn Executor,
    data: &Path,
    id: &str,
    copy: &WorkCopy,
    idle: std::time::Duration,
    running: bool,
    mut while_idle: impl FnMut(&mut dyn FnMut()),
) -> (u64, u64) {
    if running || idle < std::time::Duration::from_secs(7 * 24 * 60 * 60) {
        return (0, 0);
    }
    let path = home(data).join(id);
    if !valid_conversation(id) || Path::new(&copy.path) != path || !own_branch(&copy.branch) || !plain_path(&path) {
        return (0, 0);
    }
    let Ok(root) = path.canonicalize() else { return (0, 0) };
    let Ok(source) = Path::new(&copy.root).canonicalize() else { return (0, 0) };
    if root == source || source.starts_with(&root) {
        return (0, 0);
    }
    let Some(tracked) = git(exec, &path, &["ls-files", "--cached", "-z"]) else { return (0, 0) };
    let files: Vec<_> = tracked.split('\0').filter(|p| !p.is_empty()).collect();
    let mut projects = vec![PathBuf::new()];
    for file in &files {
        let file = Path::new(file);
        if matches!(
            file.file_name().and_then(|n| n.to_str()),
            Some("Cargo.toml" | "package.json" | "pyproject.toml" | "setup.py" | "requirements.txt")
        ) && let Some(parent) = file.parent()
            && (1..=2).contains(&parent.components().count())
            && parent.components().all(|c| matches!(c, std::path::Component::Normal(_)))
            && !projects.iter().any(|p| p == parent)
        {
            projects.push(parent.to_path_buf());
        }
    }
    let (mut freed, mut inspected) = (0, 0);
    for project in projects {
        for name in ["target", "node_modules", ".next", "dist", "__pycache__"] {
            let relative = project.join(name);
            let candidate = path.join(&relative);
            let relative = relative.to_string_lossy().replace('\\', "/");
            let prefix = format!("{relative}/");
            if files
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&relative) || f.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(&prefix)))
                || safe_metadata(&candidate, &root).is_none_or(|m| !m.is_dir())
                || git(exec, &path, &["check-ignore", "--quiet", "--", &prefix]).is_none()
            {
                continue;
            }
            let trash = candidate.with_file_name(format!(".faber-clean-{name}"));
            let aside = trash.join(name);
            let mut moved = false;
            // The private scratch folder ignores itself, so a new reply cannot commit build output during deletion.
            while_idle(&mut || {
                if safe_metadata(&candidate, &root).is_some_and(|m| m.is_dir()) && std::fs::create_dir(&trash).is_ok() {
                    use std::io::Write;
                    if safe_metadata(&trash, &root).is_some()
                        && std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(trash.join(".gitignore"))
                            .and_then(|mut file| file.write_all(b"*\n"))
                            .is_ok()
                    {
                        moved = std::fs::rename(&candidate, &aside).is_ok();
                    }
                    if !moved {
                        if safe_metadata(&trash.join(".gitignore"), &root).is_some_and(|m| m.is_file()) {
                            let _ = std::fs::remove_file(trash.join(".gitignore"));
                        }
                        if safe_metadata(&trash, &root).is_some_and(|m| m.is_dir()) {
                            let _ = std::fs::remove_dir(&trash);
                        }
                    }
                }
            });
            if moved {
                freed += cleanup_entries(&aside, &root, &mut inspected);
                if std::fs::symlink_metadata(&aside).is_ok() {
                    // Skipped links and unreadable files stay ignored; never restore a partially cleaned build folder.
                    eprintln!("faber: copy {id} cleanup retained {}", aside.display());
                } else {
                    if safe_metadata(&trash.join(".gitignore"), &root).is_some_and(|m| m.is_file()) {
                        let _ = std::fs::remove_file(trash.join(".gitignore"));
                    }
                    if safe_metadata(&trash, &root).is_some_and(|m| m.is_dir()) {
                        let _ = std::fs::remove_dir(&trash);
                    }
                }
            }
        }
    }
    eprintln!("faber: copy {id} cleanup freed {freed} logical bytes, inspected {inspected} entries");
    (freed, inspected)
}

fn safe_metadata(path: &Path, root: &Path) -> Option<std::fs::Metadata> {
    if !plain_path(path) {
        return None;
    }
    let meta = std::fs::symlink_metadata(path).ok()?;
    let canonical = path.canonicalize().ok()?;
    (canonical != root && canonical.starts_with(root)).then_some(meta)
}

fn plain_path(path: &Path) -> bool {
    path.ancestors().all(|ancestor| {
        let Ok(meta) = std::fs::symlink_metadata(ancestor) else { return false };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return false;
            }
        }
        !meta.file_type().is_symlink()
    })
}

fn cleanup_entries(path: &Path, root: &Path, inspected: &mut u64) -> u64 {
    *inspected += 1;
    let Some(meta) = safe_metadata(path, root) else { return 0 };
    let mut freed = 0;
    if meta.is_dir() {
        let Ok(children) = std::fs::read_dir(path) else { return 0 };
        for child in children {
            let Ok(child) = child else { return freed };
            freed += cleanup_entries(&child.path(), root, inspected);
        }
    } else if !meta.is_file() {
        return 0;
    }
    let Some(current) = safe_metadata(path, root) else { return freed };
    if current.is_dir() != meta.is_dir() {
        return freed;
    }
    if current.is_dir() {
        let _ = std::fs::remove_dir(path);
    } else if std::fs::remove_file(path).is_ok() {
        freed += current.len();
    }
    freed
}

/// The copy can go by itself: nothing uncommitted, its HEAD still on its own branch, and every file that branch changed is
/// the same in the person's branch (or its upstream) as on the branch. That holds after a merge, a fast-forward and a
/// squash alike; a branch the person's side changed further since is not merged by this measure, and stays.
pub fn merged(exec: &mut dyn Executor, copy: &WorkCopy) -> bool {
    let path = Path::new(&copy.path);
    if !path.is_dir() || git(exec, path, &["status", "--porcelain", "-z"]).is_none_or(|changes| !changes.is_empty()) {
        return false;
    }
    let own = format!("refs/heads/{}", copy.branch);
    if git(exec, path, &["symbolic-ref", "--quiet", "HEAD"]).as_deref() != Some(own.as_str()) {
        return false;
    }
    let (root, Some(base)) = (Path::new(&copy.root), copy.base.as_deref()) else { return false };
    let Some(tip) = git(exec, root, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{}", copy.branch)]) else { return false };
    let upstream = git(exec, root, &["for-each-ref", "--format=%(upstream)", base]).filter(|u| !u.is_empty());
    [Some(base.to_string()), upstream].into_iter().flatten().any(|target| {
        if git(exec, root, &["merge-base", "--is-ancestor", &tip, &target]).is_some() {
            return true;
        }
        let Some(fork) = git(exec, root, &["merge-base", &tip, &target]) else { return false };
        let Some(names) = git(exec, root, &["diff", "--name-only", "-z", "--no-renames", &fork, &tip]) else { return false };
        let files: Vec<&str> = names.split('\0').filter(|n| !n.is_empty()).collect();
        if files.len() > 200 {
            return false;
        }
        let mut args = vec!["diff", "--quiet", "--no-renames", &target, &tip, "--"];
        args.extend(files);
        git(exec, root, &args).is_some()
    })
}

/// Removes conversation `id`'s copy and its branch. Only the folder `copies/<id>` of Faber's own data folder is ever
/// deleted, whatever the record says; links on the way stop it. Callers decide first that nothing unsaved is lost.
pub fn remove(exec: &mut dyn Executor, data: &Path, id: &str, copy: &WorkCopy) -> Result<(), &'static str> {
    let path = home(data).join(id);
    if !valid_conversation(id) || Path::new(&copy.path) != path || !own_branch(&copy.branch) {
        return Err("error.copy.failed");
    }
    real_path(&path).map_err(|_| "error.copy.failed")?;
    let root = Path::new(&copy.root);
    let target = copy.path.clone();
    if path.exists() && root.is_dir() {
        let _ = git(exec, root, &["worktree", "remove", "--force", "--", &target]);
    }
    if path.exists() {
        // The repository is gone or git refused: the folder is Faber's own, and goes as files.
        std::fs::remove_dir_all(&path).map_err(|_| "error.copy.busy")?;
    }
    if root.is_dir() {
        let _ = git(exec, root, &["worktree", "prune"]);
        let _ = git(exec, root, &["branch", "-D", &copy.branch]);
    }
    Ok(())
}

/// Where a copy goes while it is deleted at start: a name no conversation has.
fn aside(data: &Path, id: &str) -> PathBuf {
    home(data).join(format!(".gone-{id}"))
}

/// Moves conversation `id`'s copy out of its place in one rename, so a message sent at that moment makes a fresh copy
/// instead of starting its agent in one being deleted. The engine calls it under its lock, with nothing running in the copy;
/// a folder a process still holds open cannot be renamed on Windows, and stays.
pub fn set_aside(data: &Path, id: &str) -> bool {
    let (from, to) = (home(data).join(id), aside(data, id));
    valid_conversation(id) && real_path(&from).is_ok() && real_path(&to).is_ok() && !to.exists() && std::fs::rename(&from, &to).is_ok()
}

/// Deletes a copy [`set_aside`]: the folder goes, git forgets the work tree, and the branch goes too unless a new copy
/// already has it checked out again (git refuses that delete, and the branch stays with its new copy).
pub fn retire(exec: &mut dyn Executor, data: &Path, id: &str, copy: &WorkCopy) -> Result<(), &'static str> {
    let gone = aside(data, id);
    if !valid_conversation(id) || !own_branch(&copy.branch) {
        return Err("error.copy.failed");
    }
    real_path(&gone).map_err(|_| "error.copy.failed")?;
    if gone.exists() {
        std::fs::remove_dir_all(&gone).map_err(|_| "error.copy.busy")?;
    }
    let root = Path::new(&copy.root);
    if root.is_dir() {
        let _ = git(exec, root, &["worktree", "prune"]);
        let _ = git(exec, root, &["branch", "-D", &copy.branch]);
    }
    Ok(())
}

/// The conversations that have a folder under `copies/`, by id. A copy set aside by an earlier start and never finished
/// goes now: it was clean and merged then, and git forgets it at its repository's next prune.
pub fn found(data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(home(data)) else { return vec![] };
    let names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    for id in names.iter().filter_map(|name| name.strip_prefix(".gone-")).filter(|id| valid_conversation(id)) {
        let gone = aside(data, id);
        if real_path(&gone).is_ok() {
            let _ = std::fs::remove_dir_all(gone);
        }
    }
    names.into_iter().filter(|id| valid_conversation(id)).collect()
}

/// What git says about a copy folder whose conversation is gone: its branch and its repository, enough to remove it.
pub fn orphan(exec: &mut dyn Executor, data: &Path, id: &str) -> Option<WorkCopy> {
    let path = home(data).join(id);
    let branch = git(exec, &path, &["symbolic-ref", "--quiet", "--short", "HEAD"]).filter(|b| own_branch(b))?;
    let common = git(exec, &path, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    let root = Path::new(&common).parent()?.to_string_lossy().into_owned();
    Some(WorkCopy { path: path.to_string_lossy().into_owned(), branch, root, sub: String::new(), base: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_is_a_few_plain_words_of_the_first_message_and_a_piece_of_the_id() {
        let id = "3F2A9c1d-0000-4000-8000-000000000000";
        assert_eq!(branch_for("Fix the login page, please", id), "faber/fix-the-login-page-3f2a");
        assert_eq!(branch_for("Corrigir a função de busca", id), "faber/corrigir-a-funcao-de-3f2a");
        assert_eq!(branch_for("  ¿?¡! ", id), "faber/work-3f2a");
        assert_eq!(branch_for("supercalifragilisticexpialidocious and more", id), "faber/supercalifragilisticexpialid-3f2a");
        for text in ["a b c", "x/../y", "--force", "refs/heads/main", "名前"] {
            assert!(own_branch(&branch_for(text, id)), "{text}");
        }
        for bad in ["main", "faber/", "faber/A", "faber/a b", "faber/../x", "faber/a/b", "-faber/x"] {
            assert!(!own_branch(bad), "{bad}");
        }
    }

    #[test]
    fn the_agent_starts_in_the_project_folder_of_its_copy() {
        let copy =
            WorkCopy { path: "C:/d/copies/c1".into(), branch: "faber/x-c1".into(), root: "C:/r".into(), sub: "app/web".into(), base: None };
        assert_eq!(copy.folder(), PathBuf::from("C:/d/copies/c1").join("app").join("web"));
        assert_eq!(WorkCopy { sub: String::new(), ..copy }.folder(), PathBuf::from("C:/d/copies/c1"));
    }

    /// Answers git by what it was asked, and keeps every call.
    #[derive(Default)]
    struct Scripted {
        has: Vec<String>,
        /// Names git resolves by something other than a commit id (a branch or a tag) to the commit they point at.
        named: Vec<(String, String)>,
        calls: Vec<Vec<String>>,
    }

    impl Executor for Scripted {
        fn execute(&mut self, call: &Invocation) -> Result<super::super::git::Output, super::super::git::GitError> {
            let mut args = call.args.as_slice();
            while args.first().is_some_and(|a| a == "-c") {
                args = &args[2..];
            }
            self.calls.push(args.to_vec());
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let say = |text: &str| Ok(super::super::git::Output { success: true, stdout: text.as_bytes().to_vec() });
            let no = || Ok(super::super::git::Output { success: false, stdout: vec![] });
            match args.as_slice() {
                ["rev-parse", "--show-toplevel", "--show-prefix"] => say("/repo\n\n"),
                ["rev-parse", "--verify", "--quiet", "HEAD^{commit}"] => say(&format!("{HEAD}\n")),
                ["rev-parse", "--verify", "--quiet", r] if r.ends_with("^{commit}") => {
                    let name = r.trim_end_matches("^{commit}");
                    let by_name = self.named.iter().find(|(n, _)| n == name).map(|(_, oid)| oid);
                    by_name.or_else(|| self.has.iter().find(|oid| oid.starts_with(name))).map_or_else(no, |oid| say(&format!("{oid}\n")))
                }
                ["symbolic-ref", "--quiet", "HEAD"] => say("refs/heads/main\n"),
                ["rev-parse", "--verify", "--quiet", r] if r.starts_with("refs/heads/") => no(),
                ["worktree", ..] => say(""),
                other => panic!("unexpected git command {other:?}"),
            }
        }
    }

    const HEAD: &str = "1111111111111111111111111111111111111111";
    const BUILD: &str = "abcdef0123456789abcdef0123456789abcdef01";

    #[test]
    fn a_build_commit_is_resolved_only_when_it_is_a_hash_the_repository_has() {
        let mut git = Scripted { has: vec![BUILD.into()], ..Scripted::default() };
        let repo = Path::new("/repo");
        assert_eq!(resolve(&mut git, repo, "abcdef012345").as_deref(), Some(BUILD));
        assert_eq!(resolve(&mut git, repo, BUILD).as_deref(), Some(BUILD));
        assert_eq!(resolve(&mut git, repo, "ABCDEF012345").as_deref(), Some(BUILD), "upper case is the same hash");
        assert_eq!(resolve(&mut git, repo, "0123456789ab"), None, "a hash this repository does not have");
        // A branch or tag named like a hash wins over the abbreviated id in git: what it points at is not the build.
        git.named.push(("deadbeef0123".into(), HEAD.into()));
        assert_eq!(resolve(&mut git, repo, "deadbeef0123"), None, "a name that resolves to a commit with another id");
        let asked = git.calls.len();
        for bad in ["", "unknown", "abc", "--upload-pack=x", "abcdef01234g", "abcdef012345 ", "HEAD", &"a".repeat(41), "-abcdef0123"] {
            assert_eq!(resolve(&mut git, repo, bad), None, "{bad:?}");
        }
        assert_eq!(git.calls.len(), asked, "text that is not a hash never reaches git");
    }

    #[test]
    fn a_copy_starts_at_the_commit_it_is_given_when_the_repository_has_it_and_refuses_what_is_not_a_hash() {
        let temp = tempfile::tempdir().unwrap();
        // The real path: macOS keeps temporary folders behind a link, and a copy is never made through one.
        let data = temp.path().canonicalize().unwrap();
        let id = "11111111-2222-3333-4444-555555555555";
        let start = |git: &mut Scripted, at: Option<&str>| {
            let made = make(git, Path::new("/repo"), &data, id, "faber/fix-1111", None, at);
            let add = git.calls.iter().find(|c| c.starts_with(&["worktree".to_string(), "add".to_string()])).cloned();
            (made.map(|m| m.is_some()), add.and_then(|c| c.last().cloned()))
        };
        assert_eq!(start(&mut Scripted { has: vec![BUILD.into()], ..Scripted::default() }, Some(BUILD)), (Ok(true), Some(BUILD.into())));
        let (made, added) = start(&mut Scripted::default(), Some(BUILD));
        assert_eq!((made, added), (Ok(true), Some(HEAD.into())), "a commit the repository lacks leaves the head");
        for bad in ["abcdef012345", "--detach", "HEAD", "zzzz"] {
            let mut git = Scripted::default();
            assert_eq!(make(&mut git, Path::new("/repo"), &data, id, "faber/fix-1111", None, Some(bad)), Err("error.copy.failed"));
            assert!(git.calls.is_empty(), "{bad}: nothing is asked of git");
        }
        assert_eq!(start(&mut Scripted::default(), None), (Ok(true), Some(HEAD.into())), "no commit: as before");
    }
}
