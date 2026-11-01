# Installing an AI's CLI without internet or with a blocked download says what happened

Area: Setup
Files: engine/src/install.rs, ui/Setup.tsx, ui/Accounts.tsx, ui/locales/, tests/ui/setup.check.ts, engine/tests/install.rs, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: first run asks a newcomer to install the CLIs. On a PC with no network, behind a proxy, with antivirus holding the download or a full disk, the install can fail, and the person sees a generic failure. `engine/src/install.rs` already separates write failures (`is_write_failure`); look at what it returns for a network failure, a hash mismatch and a timeout, and keep what already holds.

Build: sort an install failure into a few `reason.*` keys (no connection, the download was refused or blocked, the file did not match its checksum, no space or no permission to write, took too long), each with one plain line and the one next step (Try again, free space, check the connection, install it yourself with the vendor's page). The "Try again" button restarts cleanly from nothing left over (`clean_temp_dir`). Words by hand in every language, shown in the first-run step and in the account cards.

Leave out: a proxy settings screen and any new download source.

Done: engine tests that fail today with a loopback server that refuses, hangs, serves a wrong hash and a fake full disk: each gives its key and leaves no temp folder; a screen check shows each line and that Try again works. Extends "First run" and "Accounts and limits". Cost: none outside installs.
