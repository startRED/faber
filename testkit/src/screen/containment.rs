//! What the runner promises about processes, checked with real ones (which is why it is a scenario and not a unit test):
//! an app is inside its job from its first moment, everything it starts is in the job, ending the job ends all of it, and
//! a program that is not the app's is left alone. The programs are the fake helper's `--family` and `--idle` modes, started
//! without a shell.

use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::proctree::{descendants, same_process, snapshot};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "containment: what the app starts ends with it, and an unrelated program is left alone", run: containment }]
}

fn containment(ctx: &mut Ctx) -> Result {
    let helper = ctx.paths.fake_cli.clone();
    // A program of its own, started by the runner and owned by no one.
    let mut stranger = Command::new(&helper)
        .arg("--idle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start the unrelated program: {e}"))?;
    let result = check(ctx, &helper, &mut stranger);
    let _ = stranger.kill();
    let _ = stranger.wait();
    result
}

fn check(ctx: &Ctx, helper: &std::path::Path, stranger: &mut std::process::Child) -> Result {
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let (owned, _) = Owned::start(helper, &["--family".to_string()], &env, &ctx.sb.sandbox, helper).map_err(|e| e.to_string())?;
    // The app starts two programs. The tree is looked at first and the job asked after, so everything the tree holds was
    // already in the job when it was asked. Windows also starts a console host for a console program; the system makes
    // it, so it is not the job's to hold.
    let mut members = Vec::new();
    let mut tree = Vec::new();
    eventually("the app to have started its two programs", Duration::from_secs(10), || {
        let Ok(rows) = snapshot() else { return false };
        tree = descendants(&rows, owned.pid()).into_iter().filter(|r| !r.name.eq_ignore_ascii_case("conhost.exe")).collect();
        members = owned.members().unwrap_or_default();
        tree.len() >= 3
    })?;
    ensure(members.contains(&owned.pid()), || format!("the app {} is not in its job {members:?}", owned.pid()))?;
    ensure(tree.iter().all(|r| members.contains(&r.pid)), || {
        format!("something the app started is outside its job {members:?}: {tree:?}")
    })?;
    owned.terminate();
    ensure(owned.members().is_none(), || "the job is still open after it was ended".to_string())?;
    eventually("everything the job held to be gone", Duration::from_secs(10), || {
        snapshot().is_ok_and(|rows| !rows.iter().any(|r| same_process(r, &tree)))
    })?;
    ensure(stranger.try_wait().map_err(|e| e.to_string())?.is_none(), || "ending the job ended a program that is not the app's".to_string())
}
