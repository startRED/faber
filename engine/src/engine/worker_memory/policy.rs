//! Admission policy for new automatic work; active processes are never capped or stopped here.

pub(super) fn fits(used: u64, total: u64, available: u64) -> bool {
    total > 0 && available <= total && available >= 2 << 30 && used < total / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    const GIB: u64 = 1 << 30;

    #[test]
    fn worker_memory_preserves_two_gib_for_the_person() {
        assert!(!fits(GIB, 16 * GIB, 2 * GIB - 1));
        assert!(fits(GIB, 16 * GIB, 2 * GIB));
    }

    #[test]
    fn worker_memory_combines_all_owned_trees_against_half_physical_memory() {
        assert!(fits(8 * GIB - 1, 16 * GIB, 4 * GIB));
        assert!(!fits(8 * GIB, 16 * GIB, 4 * GIB));
        assert!(!fits(9 * GIB, 16 * GIB, 4 * GIB));
    }

    #[test]
    fn worker_memory_invalid_system_readings_never_admit() {
        assert!(!fits(0, 0, 0));
        assert!(!fits(0, GIB, 2 * GIB));
        assert!(!fits(0, 8 * GIB, 9 * GIB));
    }
}
