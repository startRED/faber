# Skill-only plugin lifecycle proof

Status: focused synthetic runtime proof passed. No catalog admission or production change.

## Scope

The existing positive plugin tests use a bundle containing a skill, command, agent and npm server. This additional
case checks the smaller supported bundle: one synthetic skill, a minimal manifest with author and license metadata,
and license notices. All fixture text is original test data; no third-party skill is copied or installed.

The store receives no Node configuration and no server packages. The check covers trusted repeated installation,
one parent ledger entry, the parent plus its one skill in resource inventory, selection and verified launch delivery
for both supported vendors, no server delivery, switching off/on, removal, Undo preserving each switch state, and
reinstallation after calling the existing Undo-expiry operation directly; no application restart is exercised. Synchronous kit preparation follows the existing
tests before store transitions so a background snapshot cannot turn a sequential lifecycle check into a timing race.

## Initial execution

The focused Rust run exited 1: the new check expected one installed resource, but received two. This was an incorrect
test expectation, not evidence of a product defect. `installed_resources()` starts with the installed parent entries
and then adds their declared resources. The corrected assertion checks the exact parent-and-skill map, including
their pinned revision, instead of only checking a count. The underlying installation ledger must still have one entry.

The initial run took 592.3 seconds including the normal full-screen hold; its Rust step took 154.6 seconds. The log
is retained outside disposable build storage in `faber-evidence/plugin-skill-proof-20261024/initial-count-failure.log`.
The revised run exited 0: the one selected plugin test passed, plus the runner aggregate (2 reported passes, not two distinct plugin scenarios). Total time was 705.6 seconds including the normal full-screen hold; the Rust step took 41.5 seconds, including 39.7 seconds compiling. The retained `retry.log` and `retry-run.log` identify the exact selected check.

## Review and limits

A fresh read-only review found no blocker in the synthetic lifecycle. Its two suggestions, checking that verified
launch retains the skill and awaiting preparation between state transitions, were applied and independently reviewed.
The seven lightweight checks passed; formatting and lane claims were checked again after correcting the resource expectation, and documentation was rechecked after recording the outcome. The focused runtime proof passed on the revised test. Integrated checks remain pending.

This does not validate any real candidate archive, original-source download, candidate-specific alias compatibility,
catalog card, or permission to admit or install an external plugin. Existing toolbox tests cover duplicate refusal at
the user-facing operation; the trusted `install_sources()` API intentionally treats a healthy repeated install as a
successful no-op. No product map, translated entry or new delivery API is needed for this test-only change.

Memory and processor cost: no production code or shipped data changes, so no product runtime cost is introduced.
No runtime memory or processor measurement was performed here.
