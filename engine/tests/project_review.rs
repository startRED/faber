//! Structural regressions for the review's two duplication findings.
#[test]
fn native_git_path_spelling_has_one_implementation() {
    let source = include_str!("../src/project/git.rs");
    assert_eq!(source.matches("strip_prefix(\"\\\\\\\\?\\\\UNC\\\\\")").count(), 1);
}

#[test]
fn canonical_test_temp_root_has_one_shared_implementation() {
    for source in [include_str!("book.rs"), include_str!("project_recovery.rs"), include_str!("project_route_v0.rs")] {
        assert!(!source.contains("fn tempdir()"));
        assert!(source.contains("use support::tempdir;"));
    }
}
