# Faber installs Antigravity's CLI for Gemini, the way it installs the other two

Area: First run: accounts
Files: src-tauri/src/setup.rs, engine/src/cli.rs, ui/FirstRun.tsx, ui/Accounts.tsx, ui/setup.ts, ui/locales/, tests/ui/, docs/product-map.md
Depends: agy-sign-in-and-setup
Size: about 300 changed lines

Why: the owner chose (2026-10-30) that Faber installs `agy` when it is missing, like it installs Claude Code and Codex
(`src-tauri/src/setup.rs` detects and installs; the "First run: accounts" row of the product map offers Install, and a
failed install offers the official install page). Task `agy-sign-in-and-setup` only shows where the installer is.

Build: Install on the Gemini card in the first run and in Manage accounts runs Antigravity's official installer from its
original source (AGENTS.md: nothing third-party is bundled; downloads come from the original source), with no shell
(AGENTS.md: child processes never go through a shell; check `engine/src/cli.rs` for how the others resolve the real
executable), hidden (never a visible window), then checks again and finds `agy` in its installer's folder even when PATH
was not refreshed. A failed install says to try again and offers the official install page, as the others do. Read the
installer's real URL and arguments from Antigravity's own docs and record them in `docs/dev/antigravity-cli.md`.

Leave out: sign-in (task `agy-sign-in-and-setup`), quota, routing.

Done: Rust tests over the install command (official host only, no shell, refuses shell characters, like
`c3_1_login_uses_official_args_in_user_dir_and_refuses_shell_characters`); a screen check where Install on the Gemini card
reaches the recorded installer and then shows Connect; the newcomer walk with `agy` absent shows Install. Extends: "First
run: accounts" and "Account cards, later". What's-new entry in every language. Security: two reviews, one by the other
vendor (a download that runs a program).
