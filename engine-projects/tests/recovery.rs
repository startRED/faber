//! Real Git in temporary linked worktrees. No shell, global Git settings, live conversations, or network.
use faber_engine_projects::changes::recovery_snapshot;
use faber_engine_projects::project::copy::WorkCopy;
use faber_engine_projects::project::git::{Executor, Invocation, Process, native_path};
use serde_json::{Value, json};
use std::path::Path;
use std::time::{Duration, Instant};

fn git(exec: &mut Process, folder: &Path, args: &[&str], succeeds: bool) {
    let out = exec.execute_bounded(&Invocation::new(folder, args).before(Instant::now() + Duration::from_secs(10)), 4096).unwrap();
    assert_eq!(out.success, succeeds, "Git {args:?}: {}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn detects_real_conflicted_merge_and_rebase_in_a_linked_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let folder = temp.path().join("copy");
    let scope = "app[1]";
    let file = "ação.txt";
    let mut exec = Process::find().expect("Git must be installed to verify linked worktrees");
    std::fs::create_dir_all(repo.join(scope)).unwrap();
    git(&mut exec, &repo, &["init", "--initial-branch=main"], true);
    git(&mut exec, &repo, &["config", "user.name", "Faber test"], true);
    git(&mut exec, &repo, &["config", "user.email", "test@example.invalid"], true);
    let write_and_commit = |exec: &mut Process, root: &Path, text: &str| {
        std::fs::write(root.join(scope).join(file), format!("{text}\n")).unwrap();
        std::fs::write(root.join("outside-scope.txt"), format!("{text}\n")).unwrap();
        git(exec, root, &["add", "--all"], true);
        git(exec, root, &["commit", "-m", text], true);
    };
    write_and_commit(&mut exec, &repo, "common");
    let copy =
        WorkCopy { path: native_path(&folder), branch: "faber/recovery".into(), root: native_path(&repo), sub: scope.into(), base: None };
    git(&mut exec, &repo, &["worktree", "add", "-b", &copy.branch, &copy.path], true);
    let read = |exec: &mut Process| -> Value { serde_json::to_value(recovery_snapshot(exec, &copy)).unwrap() };
    assert_eq!(read(&mut exec)["state"], "clean");
    std::fs::write(folder.join("outside.txt"), "out of scope").unwrap();
    assert_eq!(read(&mut exec)["state"], "clean");
    std::fs::remove_file(folder.join("outside.txt")).unwrap();
    write_and_commit(&mut exec, &folder, "copy version");
    write_and_commit(&mut exec, &repo, "main version");

    let expected = json!([format!("{scope}/{file}")]);
    git(&mut exec, &folder, &["merge", "--no-edit", "main"], false);
    let merge = read(&mut exec);
    assert_eq!(merge["state"], "changed");
    assert_eq!(merge["mergeInProgress"], true);
    assert_eq!(merge["rebaseInProgress"], false);
    assert_eq!(merge["conflicts"], expected);
    assert_eq!(merge["scope"], scope);
    assert_eq!(merge["conflictsIncomplete"], true);
    git(&mut exec, &folder, &["merge", "--abort"], true);

    git(&mut exec, &folder, &["rebase", "main"], false);
    let rebase = read(&mut exec);
    assert_eq!(rebase["state"], "changed");
    assert_eq!(rebase["mergeInProgress"], false);
    assert_eq!(rebase["rebaseInProgress"], true);
    assert_eq!(rebase["conflicts"], expected);
    git(&mut exec, &folder, &["rebase", "--abort"], true);
    assert_eq!(read(&mut exec)["state"], "clean");
}
