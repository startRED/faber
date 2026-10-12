//! Real process proofs use the fake CLI on the runner's hidden desktop, never a provider session.

use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::proctree::{descendants, same_process, snapshot};
use serde_json::{Value, json};
use std::time::Duration;

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario {
        name: "projects: engine jobs end grandchildren on stop, drop and owner crash; stale PID records cannot kill a peer",
        run: trees,
    }]
}

fn trees(ctx: &mut Ctx) -> Result {
    for mode in ["stop", "drop", "crash"] {
        let ready = ctx.sb.sandbox.join(format!("tree-{mode}.json"));
        let control = ctx.sb.sandbox.join(format!("tree-{mode}-control.json"));
        let env = crate::sandbox::child_env(&std::env::vars().collect::<Vec<_>>(), &ctx.sb, &Default::default(), &ctx.paths.fake_cli);
        let args = vec!["--engine-tree-host".into(), ready.to_string_lossy().into_owned(), control.to_string_lossy().into_owned()];
        let (host, _, _) =
            Owned::start(&ctx.paths.fake_cli, &args, &env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map_err(|e| e.to_string())?;
        let mut root = 0;
        let mut original = vec![];
        eventually("the fake CLI's grandchild", Duration::from_secs(10), || {
            let Some(state) = std::fs::read(&ready).ok().and_then(|v| serde_json::from_slice::<Value>(&v).ok()) else { return false };
            root = state["root"].as_u64().unwrap_or(0) as u32;
            original = snapshot().map(|rows| descendants(&rows, root)).unwrap_or_default();
            original.retain(|p| !p.name.eq_ignore_ascii_case("conhost.exe"));
            original.len() >= 4
        })?;
        if mode == "crash" {
            host.stop_app();
        } else {
            std::fs::write(&control, json!({"action":mode}).to_string()).map_err(|e| e.to_string())?;
        }
        let dead = eventually("the owned CLI tree to end", Duration::from_secs(3), || {
            snapshot().is_ok_and(|rows| !rows.iter().any(|r| same_process(r, &original)))
        });
        if let Err(e) = dead {
            host.terminate();
            return Err(e);
        }
        if mode == "stop" {
            let mut peer = 0;
            eventually("a peer owned by another engine job", Duration::from_secs(3), || {
                peer = std::fs::read(&ready)
                    .ok()
                    .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
                    .and_then(|s| s["peer"].as_u64())
                    .unwrap_or(0) as u32;
                peer != 0 && snapshot().is_ok_and(|rows| rows.iter().any(|r| r.pid == peer))
            })?;
            let rows = snapshot()?;
            let live = rows.iter().find(|p| p.pid == peer).ok_or("peer gone")?;
            let mut stale = live.clone();
            stale.born = live.born.saturating_sub(1);
            ensure(!same_process(&stale, &rows), || "a reused PID was accepted as the same process".into())?;
            std::fs::write(&control, json!({"action":"stop-again","recordedPid":peer,"recordedBorn":stale.born}).to_string())
                .map_err(|e| e.to_string())?;
            eventually("the old job to be stopped again", Duration::from_secs(3), || {
                std::fs::read_to_string(&ready).is_ok_and(|s| s.contains("stoppedAgain"))
            })?;
            ensure(snapshot()?.iter().any(|p| same_process(p, std::slice::from_ref(live))), || {
                "a stale PID record killed the peer".into()
            })?;
        }
        host.terminate();
        ctx.notes.push(format!("{mode}: {} owned processes gone", original.len()));
    }
    Ok(())
}
