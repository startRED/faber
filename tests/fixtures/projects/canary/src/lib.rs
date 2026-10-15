//! Frozen self-update goal: fix normalization without changing the proof.
pub fn normalize(value: &str) -> String {
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_and_lowercases() {
        assert_eq!(normalize("  FaBeR  "), "faber");
    }
}
