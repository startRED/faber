# When git is not installed, Projects say so and offer the way to get it

Area: Projects
Files: engine/src/project/git.rs, engine/src/project/copy.rs, engine/src/engine/copies.rs, ui/Projects.tsx, ui/locales/, tests/ui/projects.check.ts, engine/tests/project_git.rs, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: a Windows newcomer often has no git. Projects depend on it (the separate copy, Changes, the branch crumb), and the failure today is whatever the git call returns. First check what happens now (search `project/git.rs` for how a missing executable is reported) and keep what already holds.

Build: one engine check "git is available" (the real executable found the way `engine/src/cli.rs` resolves programs, no shell, cached for the run, rechecked after the window regains focus). When missing, a project conversation still starts in the folder itself (no separate copy, no Changes) and a single quiet card in the project home says: "Git is not installed, so each conversation works in your folder." with one button that opens the official download page in the browser. The key comes from the engine (`reason.git_missing`); words by hand in every language.

Leave out: installing git for the person (Faber never bundles tools) and any change for folders that already work.

Done: an engine test that fails today with git missing from the PATH: a project turn starts in place with no error and reports the reason key. A screen check shows the card. Extends "Separate copy" and "Conversation changes". Cost: one cached lookup.
