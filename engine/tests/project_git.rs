//! Git argument construction: pure functions, no processes or real repositories.
use faber_engine::project::git::{GitError, config_overrides, valid_oid};

#[test]
fn null_object_ids_are_not_ids() {
    for width in [40, 64] {
        assert!(!valid_oid(&"0".repeat(width)));
        assert!(valid_oid(&"a".repeat(width)));
    }
    assert!(!valid_oid(&"a".repeat(41)));
}

#[test]
fn command_config_overrides_are_pure_and_fail_closed() {
    let args =
        config_overrides(b"filter.trap.process\nfilter.trap.required\nhook.lint.command\nhook.lint.event\nhook.ref.enabled\n").unwrap();
    assert_eq!(
        args,
        ["-c", "filter.trap.process=", "-c", "filter.trap.required=false", "-c", "hook.lint.enabled=false", "-c", "hook.ref.enabled=false"]
    );
    for bad in ["hook.unsafe name.command", "hook..command", "hook.x.command\nremote.x.url", "filter.bad;command.clean"] {
        assert_eq!(config_overrides(bad.as_bytes()), Err(GitError::Command));
    }
}
