//! The red base (docs/design.md#red-base): before its suites, a run outside CI says when phase-1-base's last CI run
//! failed and what failed in it, and installs the git hook that refuses a push onto it. Both are `scripts/base.mjs`; this
//! only calls it, prints its lines and keeps the hook in place. Offline, or without Node or `gh`, it says nothing.

use std::path::Path;
use std::process::Command;

/// Marks the hook as Faber's: one with another first line belongs to the person and is never touched.
const MARK: &str = "# faber: red base guard";

/// The pre-push hook. Git runs it at the worktree's root; a worktree whose branch predates the script, or a PC without
/// Node, pushes as before.
pub const HOOK: &str = "#!/bin/sh\n\
# faber: red base guard\n\
# Written by `cargo t` (testkit/src/base.rs). Refuses a push onto a red phase-1-base, or one that carries a number\n\
# placeholder (docs/design.md#red-base). `FABER_PUSH_ON_RED=1 git push` pushes the fix.\n\
[ -f scripts/base.mjs ] && command -v node >/dev/null 2>&1 || exit 0\n\
exec node scripts/base.mjs pre-push \"$@\"\n";

/// Whether the hook file should be (re)written: it is missing, or it is an older copy of Faber's.
pub fn should_write(existing: Option<&str>) -> bool {
    existing.is_none_or(|text| text != HOOK && text.lines().nth(1) == Some(MARK))
}

fn git_path(root: &Path, what: &str) -> Option<String> {
    let output = Command::new("git").current_dir(root).args(["rev-parse", "--git-path", what]).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn install(root: &Path) {
    let Some(hooks) = git_path(root, "hooks") else { return };
    let hooks = root.join(hooks);
    let file = hooks.join("pre-push");
    if !should_write(std::fs::read_to_string(&file).ok().as_deref()) || std::fs::create_dir_all(&hooks).is_err() {
        return;
    }
    if std::fs::write(&file, HOOK).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755));
        }
    }
}

/// Installs the hook and prints the red base, if it is red. CI checks the commit itself and skips both.
pub fn warn(root: &Path) {
    if std::env::var_os("CI").is_some() {
        return;
    }
    install(root);
    let Ok(output) = Command::new("node").current_dir(root).arg("scripts/base.mjs").output() else { return };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        println!("BASE  {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hook_is_written_when_missing_or_old_and_a_persons_own_hook_is_left_alone() {
        assert!(should_write(None));
        assert!(!should_write(Some(HOOK)));
        assert!(should_write(Some("#!/bin/sh\n# faber: red base guard\nexec node old.mjs\n")));
        assert!(!should_write(Some("#!/bin/sh\nnpx lint-staged\n")));
        assert_eq!(HOOK.lines().nth(1), Some(MARK));
    }

    #[test]
    fn the_hook_never_stops_a_push_it_cannot_judge() {
        assert!(HOOK.contains("[ -f scripts/base.mjs ] && command -v node >/dev/null 2>&1 || exit 0"));
        assert!(HOOK.contains("exec node scripts/base.mjs pre-push"));
    }
}
