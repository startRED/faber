# The local build follows landed commits, not missions (R93)

The local build of R93 (slots A and B, a canary, a last good build; `src-tauri/src/local_update.rs`,
`engine/src/project/update.rs`, `engine/src/project/canary.rs`) promotes builds of landed missions, which nobody starts
since missions have no screen. Make it promote a build of each commit that lands on `phase-1-base`, with the same exact
tree proof, canary and rollback, only on the development channel, never touching the installed release in
`E:\Apps\Faber` (R95). The public updater (R156) stays the way the installed app updates. If the canary still needs a
mission, give it the smallest project-conversation form instead.

Done when: a landed commit builds into the inactive slot, passes its canary and is promoted at a safe moment; a failing
one rolls back by itself; the existing update and canary tests pass in the new form; the product map's "Local build"
row says what starts it.
