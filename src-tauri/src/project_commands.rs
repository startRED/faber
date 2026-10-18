//! Validation for the fixed Projects commands; no process or filesystem access.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
