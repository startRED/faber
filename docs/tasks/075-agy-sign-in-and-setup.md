# Connecting Gemini: Faber finds Antigravity's CLI and shows whether it is signed in

Area: Accounts
Files: engine/src/cli.rs, engine/src/account.rs, engine/src/engine.rs, src-tauri/src/setup.rs, ui/FirstRun.tsx, ui/Accounts.tsx, ui/locales/, tests/ui/, docs/product-map.md
Depends: gemini-vendor-variant, agy-recorded-runs
Size: about 380 changed lines

Why: until Faber finds `agy` and knows its sign-in state, Gemini cannot be "connected", so nothing routes to it. How the other two
are handled: `Cli::find_in` / `find_cli` (`engine/src/cli.rs`) look on PATH and in the official installers' folders (here `agy`
sits in `%LOCALAPPDATA%\agy\bin`, which an app started from the desktop may not have on PATH); `account::login_args` and
`status_args` (`engine/src/account.rs`) hold each CLI's own sign-in commands, run in a terminal the person sees; the first-run
step "First run: accounts" and the later "Manage accounts" cards (`ui/FirstRun.tsx`, `ui/Accounts.tsx`, product map "Accounts" and
"Account cards, later") offer Claude, Codex and "None for now"; `src-tauri/src/setup.rs` detects and installs. AGENTS.md: users sign
in through each CLI's own flow, Faber never reads or stores credentials, and nothing third-party is bundled (downloads come from the
original source).

Build, from the facts in `docs/dev/antigravity-cli.md` (task `agy-recorded-runs`): discovery of `agy` including its installer's
folder; a sign-in state without a model call and without reading the CLI's files (the recorded status command, or the verdict of a
cheapest-model one-word turn only at the person's request if nothing else exists, and say so in the page); `login_args` for the
terminal sign-in; Gemini as a third card in the first-run step and in Manage accounts, written like the others (a line naming the
plan it runs on, "Connect"), hidden in the first run until `agy` is found so a newcomer's three-way choice does not grow
(extending the existing step, not a new one). Install: the card shows where the official installer is, and Faber does not
download or run it in this task (see the owner question in the report). Five languages by hand.

Leave out: installing `agy` for the person, quota, routing, marks on the answers (their tasks).

Done: Rust tests for discovery in the installer folder and for the status verdicts over recorded outputs; a screen check where the
accounts step shows a Gemini card only when `agy` is found, in the longest language at 720 px; a newcomer walk (no AI connected,
`agy` absent) unchanged. Extends: "First run: accounts" and "Account cards, later" in `docs/product-map.md`. What's-new entry in
every language ("You can connect Gemini through Antigravity"). Security: one review by the other vendor and a second (a sign-in
flow). Cost: none at rest; one short status process only when the cards open.
