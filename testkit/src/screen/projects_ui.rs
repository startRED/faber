//! Real Projects IPC and geometry on the isolated desktop, using temporary records and no provider session.
use super::{Result, Scenario, ensure, same, start_settings};
use crate::app::Ctx;
use crate::page::Page;
use crate::sandbox::Launch;
use serde_json::{Value, json};
use std::path::Path;

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "projects screen: private checkpoints release mission power and restart never reacquires it", run: power },
        Scenario { name: "projects screen: fixed IPC rejects invalid identities, stale grants and fake execution", run: commands },
        Scenario { name: "projects T5: mode, answer and Stop use the authoritative runner", run: controls },
        Scenario {
            name: "projects screen: R92 mission receipts survive restart and all delivery policies stay windowless",
            run: notifications,
        },
    ]
}

fn power(ctx: &mut Ctx) -> Result {
    let launch = fixture(ctx)?;
    let mut app = ctx.launch(launch.clone())?;
    for state in ["done", "paused", "blocked"] {
        let running = invoke(&mut app.page, "probe_mission_power", json!({"missionId":"screen_mission", "state":"running"}))?;
        ensure(running["ok"] == true && running["value"]["awake"] == true, || format!("Running did not hold power: {running}"))?;
        let stopped = invoke(&mut app.page, "probe_mission_power", json!({"missionId":"screen_mission", "state":state}))?;
        ensure(stopped["ok"] == true && stopped["value"]["awake"] == false, || format!("{state} leaked power: {stopped}"))?;
    }
    invoke(&mut app.page, "probe_mission_power", json!({"missionId":"screen_mission", "state":"running"}))?;
    ctx.close(app.id)?;
    let mut reopened = ctx.launch(launch)?;
    let restored = invoke(&mut reopened.page, "probe_mission_power", json!({"missionId":"screen_mission"}))?;
    ensure(restored["ok"] == true && restored["value"]["awake"] == false && restored["value"]["state"] == "paused", || {
        format!("restart reacquired interrupted power: {restored}")
    })?;
    Ok(())
}

pub(super) fn fixture(ctx: &mut Ctx) -> Result<Launch> {
    start_settings(ctx, "en")?;
    super::projects::run_probe(ctx, "--project-screen-fixture")?;
    let git = faber_engine::cli::find_cli("git").ok_or("git is unavailable")?;
    Ok(Launch { git_bin: Some(Path::new(&git.command).parent().ok_or("git has no directory")?.into()), ..Default::default() })
}

fn invoke(page: &mut Page, command: &str, args: Value) -> Result<Value> {
    page.eval(&format!("(async () => {{ try {{ return {{ ok: true, value: await window.__TAURI_INTERNALS__.invoke({}, {}) }}; }} catch (error) {{ return {{ ok: false, error: String(error) }}; }} }})()", json!(command), args))
}

fn refused(page: &mut Page, command: &str, args: Value, key: &str) -> Result {
    let result = invoke(page, command, args)?;
    same(result["ok"].as_bool(), Some(false), command)?;
    same(result["error"].as_str(), Some(key), command)
}

fn commands(ctx: &mut Ctx) -> Result {
    let launch = fixture(ctx)?;
    let mut app = ctx.launch(launch)?;
    for id in ["../escape", "C:/escape", "bad;command", ""] {
        refused(&mut app.page, "mission_state", json!({"missionId": id}), "error.journal.badId")?;
    }
    refused(&mut app.page, "mission_state", json!({"missionId": "unknown"}), "error.journal.mismatch")?;
    ensure(!ctx.sb.sandbox.join("repo/.faber/missions/unknown").exists(), || "a read of an unknown mission created a journal".into())?;
    std::fs::create_dir_all(ctx.sb.sandbox.join("repo/.faber/missions/empty")).map_err(|e| e.to_string())?;
    refused(&mut app.page, "mission_state", json!({"missionId":"empty"}), "error.journal.mismatch")?;
    ensure(!ctx.sb.data.join("missions/empty").exists(), || "an empty public folder created private authority".into())?;
    refused(
        &mut app.page,
        "project_grant",
        json!({"projectId":"demo","mode":"seguro","selfUpdate":true,"revision":1}),
        "reason.grant.stale",
    )?;
    refused(
        &mut app.page,
        "project_grant",
        json!({"projectId":"demo","mode":"equilibrado","selfUpdate":false,"revision":2}),
        "reason.grant.mode",
    )?;
    refused(&mut app.page, "mission_start", json!({"projectId":"demo","goal":""}), "error.wire.malformed")?;
    refused(&mut app.page, "mission_start", json!({"projectId":"demo","goal":"A real goal"}), "reason.proof.unconfirmed")?;
    refused(&mut app.page, "mission_resume", json!({"missionId":"screen_mission"}), "reason.proof.unconfirmed")?;
    refused(
        &mut app.page,
        "mission_answer",
        json!({"missionId":"screen_mission","decisionId":"../choice","answer":"Keep"}),
        "error.journal.badId",
    )?;
    ensure(ctx.records()?.is_empty(), || "a refused Projects command reached a CLI".into())?;
    ensure(invoke(&mut app.page, "project_revoke", json!({"projectId":"demo"}))?["ok"] == true, || "revoke failed".into())?;
    refused(&mut app.page, "mission_resume", json!({"missionId":"screen_mission"}), "reason.grant.revoked")?;
    let granted = invoke(&mut app.page, "project_grant", json!({"projectId":"demo","mode":"seguro","selfUpdate":false,"revision":2}))?;
    ensure(granted["ok"] == true && granted["value"]["revision"] == 2, || format!("fresh Seguro grant: {granted}"))?;
    refused(&mut app.page, "mission_resume", json!({"missionId":"screen_mission"}), "reason.grant.stale")?;
    let plan = ctx.sb.sandbox.join("repo/.faber/missions/screen_mission/plan.json");
    let before = std::fs::read(&plan).map_err(|e| e.to_string())?;
    std::fs::write(&plan, b"{").map_err(|e| e.to_string())?;
    refused(&mut app.page, "mission_state", json!({"missionId":"screen_mission"}), "error.store.corrupt")?;
    std::fs::write(&plan, before).map_err(|e| e.to_string())?;
    ctx.notes.push("fixed commands; no journal on unknown read; stale/revoked grants refused; no worker or owner-checkout fallback".into());
    Ok(())
}

fn controls(ctx: &mut Ctx) -> Result {
    use faber_engine::project::{MissionState, RunMode, run::Runner, store::Journal};
    let launch = fixture(ctx)?;
    let mut app = ctx.launch(launch)?;
    let args = json!({"missionId":"screen_mission"});
    let state = invoke(&mut app.page, "mission_state", args.clone())?;
    ensure(state["ok"] == true, || format!("state: {state}"))?;
    let mode = invoke(&mut app.page, "mission_mode", json!({"missionId":"screen_mission","mode":"autonomous"}))?;
    ensure(mode["ok"] == true, || format!("mode: {mode}"))?;
    let j = Journal::open(&ctx.sb.sandbox.join("repo"), &ctx.sb.data, "screen_mission").map_err(|e| e.key().to_string())?;
    same(Runner::load(&j).map_err(str::to_string)?.mission.mode, RunMode::Autonomous, "runner mode")?;
    let answer = invoke(&mut app.page, "mission_answer", json!({"missionId":"screen_mission","decisionId":"choice","answer":"Keep"}))?;
    ensure(answer["ok"] == true, || format!("answer: {answer}"))?;
    ensure(answer["value"]["revision"].as_u64() > mode["value"]["revision"].as_u64(), || {
        "answer did not advance the runner revision".into()
    })?;
    let runner = Runner::load(&j).map_err(str::to_string)?;
    same(runner.mission.extra.get("viewRevision").and_then(Value::as_u64), answer["value"]["revision"].as_u64(), "runner answer revision")?;
    let stopped = invoke(&mut app.page, "mission_stop", args)?;
    ensure(stopped["ok"] == true, || format!("stop: {stopped}"))?;
    same(Runner::load(&j).map_err(str::to_string)?.mission.state, MissionState::Paused, "runner stop")?;
    let ledger: faber_engine::ledger::Ledger =
        serde_json::from_slice(&std::fs::read(ctx.sb.data.join("budget.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    ensure(faber_engine::project::budget::Budget::load(&ledger).map_err(str::to_string)?.grants["screen_mission"].paused, || {
        "Stop did not pause the budget".into()
    })?;
    refused(&mut app.page, "mission_resume", json!({"missionId":"screen_mission"}), "reason.proof.unconfirmed")?;
    ensure(ctx.records()?.is_empty(), || "a refused role reached a CLI".into())?;
    Ok(())
}

fn notifications(ctx: &mut Ctx) -> Result {
    use faber_engine::project::store::Journal;
    use faber_engine::project::{Decision, DecisionKind};
    let launch = fixture(ctx)?;
    let mut app = ctx.launch(launch.clone())?;
    let probe = |page: &mut Page, focused: bool, attention: &str| -> Result<Value> {
        let value = invoke(page, "probe_mission_attention", json!({"missionId":"screen_mission","focused":focused,"attention":attention}))?;
        ensure(value["ok"] == true, || format!("attention probe: {value}"))?;
        Ok(value["value"].clone())
    };
    let first = probe(&mut app.page, true, "normal")?;
    same(first["toast"].as_bool(), Some(false), "focused popup")?;
    same(first["sound"].as_bool(), Some(false), "focused sound")?;
    ctx.close(app.id)?;
    let mut app = ctx.launch(launch)?;
    let repeated = probe(&mut app.page, false, "normal")?;
    same(repeated["toast"].as_bool(), Some(false), "seen decision after process restart")?;
    same(repeated["sound"].as_bool(), Some(false), "seen decision after process restart")?;
    let journal = Journal::open(&ctx.sb.sandbox.join("repo"), &ctx.sb.data, "screen_mission").map_err(|e| e.key().to_string())?;
    let mut serial = 0;
    for attention in ["normal", "soundOnly", "uncertain"] {
        for notify in [false, true] {
            for sound in [false, true] {
                serial += 1;
                let d = Decision::new(&format!("policy{serial}"), "task0", "Pick a direction", DecisionKind::Direction);
                journal.save_decision(&d).map_err(|e| e.key().to_string())?;
                let settings = invoke(&mut app.page, "save_settings", json!({"patch":{"notifications":notify,"notificationSound":sound}}))?;
                ensure(settings["ok"] == true, || "settings refused".into())?;
                let result = probe(&mut app.page, false, attention)?;
                same(result["toast"].as_bool(), Some(notify && attention == "normal"), "normal/fullscreen/uncertain popup policy")?;
                same(result["sound"].as_bool(), Some(sound), "independent sound switch")?;
            }
        }
    }
    let ack = ctx.sb.data.join("seen_decisions.json");
    std::fs::write(ack, b"{").map_err(|e| e.to_string())?;
    refused(
        &mut app.page,
        "probe_mission_attention",
        json!({"missionId":"screen_mission","focused":false,"attention":"normal"}),
        "error.store.corrupt",
    )?;
    ctx.notes.push("actual mission records and atomic notification receipts; process restart; 12 switch/attention combinations; no OS popup or audio emitted".into());
    Ok(())
}
