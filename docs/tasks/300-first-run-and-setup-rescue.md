# First run and setup rescue

Area: Account cards, First run
Files: engine/src/account.rs, engine/src/install.rs, engine/src/cli.rs, engine/src/engine.rs, ui/Accounts.tsx, first-run screen, ui/locales/, engine/tests/account.rs, tests/ui/setup.check.ts, docs/product-map.md
Depends: none
Size: two pull requests: first first-run visibility and setup checks, then account detection

Why: At 720x480, picking an uninstalled AI hides its Install row below the footer (walkthrough shot `reports/reforma/w10/shots/a/a02-claude-picked.png`). A newcomer needs a setup diagnosis, and one missed detection can retire a working AI and its sessions.

Build: Keep the picked AI's install or sign-in row visible at 720x480. Add one “Check my setup” button in Accounts and limits: without sending a message or reading credentials, check each connected AI's executable through the engine resolver with `--version` and no shell, supported version and existing signed-in status, plus git on PATH, data-folder writability and free space. Show one line per check with ok, warning or one action to fix it; engine returns keys and every language supplies the words. Reproduce a missed detection first; check PRs #438 and #442. Add a refresh path retaining a missing detection while its old executable exists, a session is live or a recent reply exists, log the retention, and drop a gone executable only after two consecutive refreshes. Keep intentional `replace_clis` removal unchanged and wire runtime re-detection at the current host boundary.

Leave out: automatic repairs, quota-consuming checks, credential files, account-status wording, the sign-in timeout and quota reading.

Done: A screen check fails today when Install is hidden at 720x480 and proves install and sign-in are visible; another presses the health button and reads its results. Fake-CLI engine tests cover missing, too-old and signed-out CLIs and a read-only data folder with the right keys. A detection test proves a live session survives one miss, the executable is kept while present and dropped after two misses when gone; intentional removal still works. Update the product map and design page, add a what's-new entry in every language and get fresh and coherence reviews. Report the short child processes on button press and any detection cost.
