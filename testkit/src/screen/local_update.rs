//! Live lifecycle proofs use only temporary development slots and the hidden test build.
use super::{Result, Scenario, ensure, eventually};
use crate::app::Ctx;
use crate::contain::Owned;
use crate::sandbox::{Launch, child_env};
use faber_engine::project::update::{CanaryVerdict, ExactTree, Slot, SlotLayout, SwapCheck, UpdateManager, UpdateState};
use faber_engine::project::{GitMode, Grant, RepoIdentity, SCHEMA};
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn scenarios() -> Vec<Scenario> {
    vec![Scenario { name: "local_update: independent helper restores an early crash headlessly without changing saved grants", run: crash }]
}

fn crash(ctx: &mut Ctx) -> Result {
    let root = ctx.sb.sandbox.join("development");
    let release = ctx.sb.sandbox.join("release");
    let stable_data = ctx.sb.sandbox.join("stable-data");
    let manager = UpdateManager::new(SlotLayout::new(&root, &stable_data, &release).map_err(|e| e.key())?).map_err(|e| e.key())?;
    let payload = fs::read(&ctx.paths.app).map_err(|e| e.to_string())?;
    let tree = ExactTree::new(&"1".repeat(40), &"2".repeat(40), &ExactTree::compute_build_hash(&payload), SCHEMA).map_err(|e| e.key())?;
    for slot in [Slot::A, Slot::B] {
        fs::write(manager.layout().slot_dir(slot).join("faber.exe"), &payload).map_err(|e| e.to_string())?;
    }
    let time = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    manager.adopt_baseline(Slot::A, tree.clone(), &payload, time).map_err(|e| e.key())?;
    let last_good = manager.manifest().map_err(|e| e.key())?.ok_or("missing manifest")?.last_good;
    let repo = RepoIdentity::new("repo", "checkout");
    let mut grant = Grant::new("repo", GitMode::Seguro, 1, time);
    grant.self_update = true;
    manager.prepare_next(tree.clone(), &grant, &repo, 1, &[], time).map_err(|e| e.key())?;
    manager
        .record_canary_verdict(
            CanaryVerdict::Pass {
                verified_at: time,
                receipt_id: "fixture-canary".into(),
                tree,
                proof_receipt: "fixture-proof".into(),
                compatible_schemas: vec![SCHEMA],
            },
            &payload,
            time,
        )
        .map_err(|e| e.key())?;
    manager.enter_quiescence(false, false).map_err(|e| e.key())?;
    manager
        .begin_swap(&grant, &repo, 1, SwapCheck { payload: &payload, active_window: false, active_operations: false }, time)
        .map_err(|e| e.key())?;
    let data = root.join("data");
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let saved = b"{\"schema\":1,\"spent\":7,\"consumed\":true,\"extension\":\"keep\"}";
    fs::write(data.join("mission.json"), saved).map_err(|e| e.to_string())?;
    fs::write(data.join("test-crash-next"), b"once").map_err(|e| e.to_string())?;
    let mut env = child_env(&std::env::vars().collect::<Vec<_>>(), &ctx.sb, &Launch::default(), &ctx.paths.fake_cli);
    for (key, value) in [
        ("FABER_CHANNEL", "development".into()),
        ("FABER_DEV_ROOT", root.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_DATA", stable_data.to_string_lossy().into_owned()),
        ("FABER_INSTALLED_RELEASE", release.to_string_lossy().into_owned()),
    ] {
        env.insert(key.into(), value);
    }
    let helper = ctx.paths.target.join("debug/faber-local.exe");
    for path in [&helper, &ctx.paths.app] {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let offset =
            bytes.get(60..64).and_then(|bytes| <[u8; 4]>::try_from(bytes).ok()).map(u32::from_le_bytes).ok_or("missing PE header")?
                as usize;
        let subsystem =
            bytes.get(offset + 24 + 68..offset + 24 + 70).and_then(|bytes| <[u8; 2]>::try_from(bytes).ok()).map(u16::from_le_bytes);
        ensure(subsystem == Some(2), || format!("{} has a visible console subsystem", path.display()))?;
    }
    let (owned, _, _) =
        Owned::start(&helper, &["--transition".into()], &env, &ctx.sb.sandbox, &ctx.paths.fake_cli).map_err(|e| e.to_string())?;
    let result = (|| {
        eventually("independent rollback", Duration::from_secs(30), || {
            manager.manifest().is_ok_and(|m| m.is_some_and(|m| matches!(m.state, UpdateState::RolledBack { .. })))
        })?;
        let manifest = manager.manifest().map_err(|e| e.key())?.ok_or("missing rollback")?;
        ensure(manifest.active_slot == Slot::A && manifest.rollback_count == 1 && manifest.last_good == last_good, || {
            "rollback changed last good".into()
        })?;
        eventually("bad slot cleanup", Duration::from_secs(10), || !root.join("slots/b/faber.exe").exists())?;
        eventually("restored slot running headlessly", Duration::from_secs(30), || {
            let path = root.join("slots/a/running.lock");
            std::fs::OpenOptions::new().read(true).write(true).open(path).is_ok_and(|file| file.try_lock().is_err())
        })?;
        ensure(fs::read(data.join("mission.json")).map_err(|e| e.to_string())? == saved, || {
            "rollback reset spend or dropped fields".into()
        })?;
        ensure(fs::read(root.join("slots/a/faber.exe")).map_err(|e| e.to_string())? == payload, || "last-good executable changed".into())?;
        // The helper has no Tauri runtime. Its restored engine has no WebView or console, even on the default CI desktop.
        if let Some(ids) = owned.members() {
            let windows = crate::desktop::windows()?;
            ensure(!windows.iter().any(|(_, pid, visible)| *visible && ids.contains(pid)), || {
                "local transition opened a visible window".into()
            })?;
        }
        let (probe, _, stderr) =
            Owned::start(&ctx.paths.fake_cli, &["--desktop-visible-probe".into()], &env, &ctx.sb.sandbox, &ctx.paths.fake_cli)
                .map_err(|e| e.to_string())?;
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut stderr) = stderr {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
            }
            let _ = send.send(text);
        });
        let windows = receive.recv_timeout(Duration::from_secs(10));
        probe.terminate();
        let windows = windows.map_err(|_| "visible desktop probe timed out")?;
        let members = owned.members().ok_or("missing owned transition job")?;
        let rows = windows.lines().next().and_then(|line| line.strip_prefix("windows")).ok_or("invalid desktop probe")?;
        let visible = rows
            .split(',')
            .filter(|row| !row.trim().is_empty())
            .map(|row| row.trim().split_once(':').and_then(|(_, pid)| pid.parse::<u32>().ok()).ok_or("invalid visible-window owner"))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure(!visible.iter().any(|pid| members.contains(pid)), || {
            format!(
                "transition created visible windows on its own desktop: {}; owned {:?}; processes {:?}",
                windows.trim(),
                owned.members(),
                crate::proctree::snapshot()
                    .map(|rows| rows.into_iter().filter(|row| owned.members().unwrap_or_default().contains(&row.pid)).collect::<Vec<_>>())
            )
        })?;
        ctx.notes.push("temporary slots; hidden test build; one rollback; immutable payload; saved spend/grants/extensions unchanged; no installer or visible window".into());
        Ok(())
    })();
    owned.terminate();
    result
}
