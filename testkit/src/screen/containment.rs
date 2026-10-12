//! What the runner promises about processes, checked with real ones (which is why it is a scenario and not a unit test):
//! an app is inside its job from its first moment, everything it starts is in the job, ending the job ends all of it, and
//! a program that is not the app's is left alone. The programs are the fake helper's `--family` and `--idle` modes, started
//! without a shell.

use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::desktop;
use crate::proctree::{descendants, same_process, snapshot};
use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "containment: what the app starts ends with it, and an unrelated program is left alone", run: containment },
        Scenario { name: "hidden desktop: the app starts on a desktop that the person's does not show", run: hidden_desktop },
    ]
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

/// The proof that the mechanism works, before Faber itself is ever launched: the fake helper's `--desktop-probe` is
/// started like the app is and lists the top-level windows its desktop has. It makes no window, so nothing can be seen
/// whatever happens. On the person's desktop it would list the windows listed here; on the hidden one it lists none
/// of them.
fn hidden_desktop(ctx: &mut Ctx) -> Result {
    let helper = ctx.paths.fake_cli.clone();
    let mine: Vec<usize> = desktop::windows()?.into_iter().map(|(handle, ..)| handle).collect();
    ensure(!mine.is_empty(), || "the runner's own desktop lists no windows, so there is nothing to compare with".to_string())?;
    let env: BTreeMap<String, String> = std::env::vars().collect();
    let (owned, stderr) =
        Owned::start(&helper, &["--desktop-probe".to_string()], &env, &ctx.sb.sandbox, &helper).map_err(|e| e.to_string())?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = stderr {
            let _ = std::io::Read::read_to_string(&mut pipe, &mut text);
        }
        let _ = sender.send(text);
    });
    let said = receiver.recv_timeout(Duration::from_secs(10)).map_err(|_| "the probe said nothing".to_string());
    owned.terminate();
    let said = said?;
    let Some(list) = said.trim().strip_prefix("windows") else {
        return Err(format!("the probe did not list its windows: {}", said.trim()));
    };
    let theirs: Vec<usize> = list.split(',').filter_map(|h| h.trim().parse().ok()).collect();
    let shared: Vec<&usize> = theirs.iter().filter(|h| mine.contains(h)).collect();
    ensure(shared.is_empty(), || format!("the probe sees windows of the desktop in use: {shared:?}; it is not on a hidden desktop"))?;
    ctx.notes.push(format!("the probe sees {} windows, the desktop in use {}", theirs.len(), mine.len()));
    Ok(())
}
