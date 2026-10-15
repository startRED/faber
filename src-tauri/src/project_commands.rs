//! Validation and view revisions for the fixed Projects commands; no process or filesystem access.
use faber_engine::project::{Mission, MissionState};

pub(super) fn id(value: &str) -> Result<(), String> {
    if faber_engine::project::valid_id(value) { Ok(()) } else { Err("error.journal.badId".into()) }
}

pub(super) fn text(value: &str) -> Result<(), String> {
    if value.len() > faber_engine::project::wire::MAX_TEXT {
        return Err("error.wire.tooBig".into());
    }
    if value.trim().is_empty() || value.contains('\0') {
        return Err("error.wire.malformed".into());
    }
    Ok(())
}

pub(super) fn revision(mission: &Mission) -> u64 {
    mission.extra.get("viewRevision").and_then(serde_json::Value::as_u64).unwrap_or(mission.updated_at)
}

#[cfg(test)]
pub(super) fn changed(mission: &mut Mission, now: u64) -> Result<(), String> {
    let next = revision(mission).checked_add(1).ok_or("error.wire.stale")?;
    mission.extra.insert("viewRevision".into(), next.into());
    mission.updated_at = now;
    Ok(())
}

pub(super) fn editable(mission: &Mission) -> Result<(), String> {
    if mission.state == MissionState::Done { Err("error.wire.stale".into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission() -> Mission {
        Mission::new("m1", "p1", "repo", "Goal", "base", 1, 100)
    }

    #[test]
    fn project_commands_reject_injected_ids_and_empty_or_oversized_text() {
        for bad in ["", "../m1", "C:/m1", "a\\b", "a b", "a\0b", "a;b"] {
            assert!(id(bad).is_err(), "{bad:?}");
        }
        assert!(id(&"a".repeat(65)).is_err());
        assert!(id("m_1-valid").is_ok());
        for bad in ["", "   ", "a\0b"] {
            assert!(text(bad).is_err(), "{bad:?}");
        }
        assert!(text(&"a".repeat(faber_engine::project::wire::MAX_TEXT + 1)).is_err());
        assert!(text("A goal with\nwrapped text").is_ok());
    }

    #[test]
    fn project_commands_revisions_survive_same_second_clock_reversal_and_reload() {
        let mut m = mission();
        changed(&mut m, 100).unwrap();
        assert_eq!(revision(&m), 101);
        let mut m: Mission = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        changed(&mut m, 90).unwrap();
        assert_eq!(revision(&m), 102);
        assert_eq!(m.updated_at, 90, "timestamp remains the actual time");
    }

    #[test]
    fn project_commands_refuse_reopening_or_editing_a_finished_mission() {
        let mut m = mission();
        m.state = MissionState::Done;
        assert_eq!(editable(&m), Err("error.wire.stale".into()));
    }
}
