//! Lessons row 5: a mission's roles and proofs run at idle priority inside their own job, so a game never stutters for
//! them, and what a role leaves running when its lease ends is ended and named in the report.
#![cfg(windows)]
use faber_engine::proc::{self, OwnedTree, Proc};
use std::time::{Duration, Instant};

const IDLE: u32 = 0x40;

fn cmd() -> String {
    std::path::Path::new(&std::env::var("SystemRoot").unwrap()).join("System32").join("cmd.exe").to_string_lossy().into_owned()
}

/// `cmd /d /c <line>`: the test's own fixed lines, never a model's.
fn run(spawn: &proc::Spawner, line: &str, cwd: &std::path::Path) -> (Proc, OwnedTree) {
    let p = spawn(&cmd(), &["/d".into(), "/c".into(), line.into()], cwd).unwrap();
    let tree = p.contained.as_ref().and_then(|c| c.owned_tree()).expect("a job owns the tree");
    (p, tree)
}

fn is_ping(pid: u32) -> bool {
    faber_winproc::image_path(pid).is_ok_and(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("ping.exe")))
}

/// Waits until the job runs at least `n` pings and returns every member (the root, the pings and Windows' console host).
async fn with_pings(tree: &OwnedTree, n: usize) -> Vec<u32> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let found = tree.members().unwrap();
        if found.iter().filter(|p| is_ping(**p)).count() >= n || Instant::now() >= until {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn ended(pid: u32) {
    let until = Instant::now() + Duration::from_secs(10);
    while !faber_winproc::process_ended(pid) && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn lower(names: &[String]) -> Vec<String> {
    names.iter().map(|n| n.to_ascii_lowercase()).collect()
}

/// `start /b /normal` asks for normal priority on purpose: the job holds every member at idle all the same.
const TWO_PINGS: &str = "start /b /normal ping -n 30 127.0.0.1 >nul & ping -n 30 127.0.0.1 >nul";

#[tokio::test]
async fn a_role_and_everything_it_starts_run_at_idle_priority_even_when_one_asks_for_normal() {
    let dir = tempfile::tempdir().unwrap();
    let (_p, tree) = run(&proc::idle_spawner(proc::real_spawner()), TWO_PINGS, dir.path());
    let found = with_pings(&tree, 2).await;
    assert!(found.iter().filter(|p| is_ping(**p)).count() == 2 && found.contains(&tree.identity().pid), "cmd and its two pings");
    for pid in &found {
        assert_eq!(faber_winproc::priority_class(*pid).unwrap(), IDLE, "process {pid} of the role");
    }
    tree.retire().await.unwrap();
}

#[tokio::test]
async fn a_proof_and_everything_it_starts_run_at_idle_priority() {
    let dir = tempfile::tempdir().unwrap();
    let environment = ["SystemRoot", "PATH"].iter().filter_map(|n| Some((n.to_string(), std::env::var(n).ok()?))).collect();
    let (_p, tree) = run(&proc::isolated_spawner(environment, vec![]), TWO_PINGS, dir.path());
    let found = with_pings(&tree, 2).await;
    assert!(found.iter().filter(|p| is_ping(**p)).count() == 2 && found.contains(&tree.identity().pid), "cmd and its two pings");
    for pid in &found {
        assert_eq!(faber_winproc::priority_class(*pid).unwrap(), IDLE, "process {pid} of the proof");
    }
    tree.retire().await.unwrap();
}

/// A chat CLI gets no class of its own: it runs as Faber does (a test runner that is itself idle passes it on).
#[tokio::test]
async fn a_chat_cli_keeps_the_priority_faber_runs_at() {
    let dir = tempfile::tempdir().unwrap();
    let (_p, tree) = run(&proc::real_spawner(), "start /b /normal ping -n 30 127.0.0.1 >nul", dir.path());
    let own = faber_winproc::priority_class(std::process::id()).unwrap();
    assert_eq!(faber_winproc::priority_class(tree.identity().pid).unwrap(), own);
    if own != IDLE {
        let ping = with_pings(&tree, 1).await.into_iter().find(|p| is_ping(*p)).unwrap();
        assert_ne!(faber_winproc::priority_class(ping).unwrap(), IDLE, "nothing holds a chat's children at idle");
    }
    tree.retire().await.unwrap();
}

/// The leftover a lease's end finds is named (file names only, never a command line) and is gone once the tree is retired.
#[tokio::test]
async fn what_a_role_left_running_when_its_lease_ends_is_named_and_ended() {
    let dir = tempfile::tempdir().unwrap();
    let (_p, tree) = run(&proc::idle_spawner(proc::real_spawner()), TWO_PINGS, dir.path());
    let found = with_pings(&tree, 2).await;
    let retired = tree.retire().await.unwrap();
    assert_eq!(lower(retired.leftovers()), ["ping.exe", "ping.exe"], "both pings; not the role itself, nor its console host");
    for pid in found {
        assert!(faber_winproc::process_ended(pid), "process {pid} outlived its lease");
    }
    // Asking again answers the same.
    assert_eq!(tree.retire().await.unwrap().leftovers().len(), 2);
}

/// A role that quits by itself and leaves a child: the job closes with its last holder, and the child is still counted.
#[tokio::test]
async fn a_child_left_by_a_role_that_quit_on_its_own_is_still_counted() {
    let dir = tempfile::tempdir().unwrap();
    let (p, tree) = run(&proc::idle_spawner(proc::real_spawner()), "start /b ping -n 30 127.0.0.1 >nul", dir.path());
    let found = with_pings(&tree, 1).await;
    ended(tree.identity().pid).await;
    drop(p);
    let retired = tree.retire().await.unwrap();
    assert_eq!(lower(retired.leftovers()), ["ping.exe"]);
    for pid in found {
        assert!(faber_winproc::process_ended(pid), "process {pid} outlived its lease");
    }
}

#[tokio::test]
async fn a_role_that_cleaned_up_after_itself_has_no_leftover() {
    let dir = tempfile::tempdir().unwrap();
    let (_p, tree) = run(&proc::idle_spawner(proc::real_spawner()), "echo done", dir.path());
    ended(tree.identity().pid).await;
    assert_eq!(tree.retire().await.unwrap().leftovers(), [] as [String; 0]);
}
