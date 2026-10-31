# A "Check my setup" button that finds and says what is wrong

Area: Account cards
Files: engine/src/account.rs, engine/src/install.rs, engine/src/cli.rs, ui/Accounts.tsx, ui/api.ts, ui/locales/, tests/ui/setup.check.ts, engine/tests/account.rs, docs/product-map.md
Depends: none
Size: about 350 changed lines

Why: `docs/product/parity.md` row K3: both CLIs have `/doctor`; Faber only has the account cards. A newcomer on Windows whose CLI "does not start" sees a status word and has no next step.

Build: one button in Accounts and limits ("Check my setup") that runs, per connected AI and without sending a message, fixed checks in order and shows one plain line each with ok, a warning or a fix: the executable is found and starts (`--version`, through the engine's own resolver, never a shell), its version is one Faber knows, it is signed in (the existing status reading), `git` is on the PATH (Projects need it), the data folder is writable and has room. Each failure names the one action that fixes it (Install, Sign in, a link to the vendor's own instructions). The engine returns keys (`reason.*`), the screen writes the words in every language. Nothing is read from the CLI's credential files; only whether the CLI says it is signed in.

Leave out: repairs that run by themselves, and any check that costs quota.

Done: engine tests that fail today with a fake CLI that is missing, too old, signed out, and a read-only data folder: each gives its key. A screen check presses the button and sees the lines. Extends "Accounts and limits". Cost: runs only on a press, a few short child processes.
