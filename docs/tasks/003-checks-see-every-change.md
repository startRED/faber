# Faber's own checks see every change of a reply

The live probe (`engine/examples/selfdev_accept.rs`, R204's checks) found two ways Faber skips its own checks with no
sign. (1) A reply that changes files only through a shell command (`sed -i`, `cargo fmt`, a script) gets no Checks row,
because the trigger is the list of edit tools (`CHANGES_FILES`). Decide by what the folder holds instead: the content of
the copy's changed files (git status and their hashes) before and after the turn, which the verifier already computes.
(2) A confirmed Checks list is dropped and the declared one used whenever the copy's setup commands differ from the
project folder's (for example `cargo fetch` appears once a gitignored `Cargo.lock` is created in the copy), so the row
passes fewer checks than the person confirmed. Keep the confirmation tied to the commands the person saw, not to
setup lines that the folder's state adds.

Done when: a reply that edits only with a shell command gets its Checks row; a project whose lock file is gitignored
keeps its confirmed list in a copy; engine tests cover both; the probe's two notes in its doc comment are removed.
