# The draft attachments check leaves no fake CLI behind

Area: none
Files: testkit/src/screen/draft_attachments.rs (and, only if the cause is there, the app's close path)
Depends: none
Size: about 60 changed lines

Why: `draft attachments: edited inline file metadata is removed when the draft reopens` failed twice in the last 40 red
runs, both on pull requests that do not touch drafts or attachments: `queue/pr-474` run 41053322464 (12:30, screen 7/8,
scripts only) and `queue/pr-448` run 41039738430 (10:28, screen 7/8). Both times the scenario's own steps passed and
the end-of-scenario audit (`testkit/src/app.rs`, "a process of the app or of a CLI is still running") found a
`fake-cli.exe` whose parent is the first app the scenario launched, born about 2 s after that app and just before the
second launch (`edited_inline` closes the first app with `ctx.close(app.id)` and launches a second one at once). So the
first app started its CLI (the warm one of a Claude chat) while it was being closed, and that CLI outlived it.

Build: first find out on which side the race is. If Faber can start a CLI after it was asked to close, or closes
without ending a CLI that is still starting, that is a real leak a person would also hit: fix it in the app (the close
path ends every CLI it started, including one still starting) and say so in the pull request. If the app is right and
only the check closes too early (before the warm start settled), make the scenario wait for the state the other
scenarios that relaunch wait for before `ctx.close`, never a fixed sleep.

Done: the scenario passes 20 times in a row under `cargo t --filter "draft attachments"` with the machine loaded (the
other scenarios of screen part 7 running alongside); the pull request names the two red runs above and says which side
was wrong. If it was the app, a check fails without the fix, and the what's-new rule applies.
