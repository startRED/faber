//! Structural regressions for the review's duplication finding.
#[test]
fn native_git_path_spelling_has_one_implementation() {
    let source = include_str!("../src/project/git.rs");
    assert_eq!(source.matches("strip_prefix(\"\\\\\\\\?\\\\UNC\\\\\")").count(), 1);
}
