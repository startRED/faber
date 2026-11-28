# Frontend-design pinned-source runtime qualification

Status: the actual preserved five-file candidate passed an isolated Linux store lifecycle and driver-delivery preparation probe. No catalog admission, personal installation or live AI session.

## Source and boundary

The candidate is `anthropics/claude-plugins-official` at `58267944f92187b7d6a1c822165c96c81c864806`, scope `plugins/frontend-design`. Its root tree digest is `ce5bda41d00150cd52bea577bc91f738dd86a68ac75efc3a6d512727ba82bd6c`; its sole skill tree digest is `83485963700bcf11d5bf78118facb301676b69d918fee2e45a95e7f1a2840f6c`.

The preparer reused the previously preserved exact source. All five file byte counts, raw SHA-256 and Git blob SHA-1 identities matched that manifest, and the complete file inventory matched. It created a GitHub-shaped local archive rather than fetching a new remote archive. No upstream instructions, code or license text were added to the repository.

The standalone Rust probe links the real engine from clean source `af5c280bf953e254a19a22a91c2c5ab7011cac3f`. The engine's tool-store and kit source files are unchanged from the current base. It calls the existing `Store::with_plugins`, `install_sources`, `kit_for`, `verified_for_launch`, on/off, Remove, Undo and expiry APIs below a fresh external probe directory. The fixed-source definition belongs only to that local test store. No production installer, catalog definition, personal CLI directory or configuration changes.

## Executed evidence

| Check | Result |
| :--- | :--- |
| Original root/skill digests through the real Rust digest implementation | Match both fixed digests |
| Exact original plugin through the existing installer | Pass |
| Healthy repeated trusted installation | One parent ledger entry and the exact parent-plus-skill resource map |
| Matching project task, both supported vendors | The namespaced skill is selected exactly once; the prepared and launch-verified skill bytes match the original; zero servers |
| Off, On, Remove and Undo in both states | Saved state, exact restored inventory and both vendors' prepared/verified presence or absence all match |
| Remove, expire Undo, reinstall | Exact resource map and both vendors' verified delivery restored |
| Changed skill bytes under the original pin | Refused for source digest mismatch; empty installed-resource map |
| Hook, undeclared command, executable, permission grant | Each specific structural refusal occurs, even after recomputing mutated root/part digests |
| Traversal path, case collision, ZIP symlink | Each specific extraction refusal occurs; empty installed-resource map |

Preparation, compilation and execution exited zero. The successful run used Ubuntu-24.04 through WSL. The retained runtime receipt records the executable and harness hashes, source identity and every output line. The source tree remained clean and on the same revision before and after.

An initial invocation failed because the Windows Python alias had no interpreter and direct compiler linking could not resolve the engine dependency. The corrected runner uses existing WSL Python and Cargo's offline dependency resolution. No project or production fix was required.

## Independent review and limits

A fresh read-only reviewer found incomplete state-transition assertions in the first probe. The final probe checks both vendors after each transition, exact resource inventory after Undo/reinstall and the specific corruption error. Follow-up review found no blocker in those corrections or in the seven structural refusals. The reviewer separately inspected the original five files and found no concrete security blocker within this source-and-skill scope.

Empty ledger assertions are not a proof that no temporary extraction file ever existed. The traversal marker checks one expected outside destination, not every possible write. Existing store containment checks remain necessary.

This result does not prove a renewed original-source download, native Windows/macOS behavior, a real AI conversation, CLI-native plugin behavior, runtime memory/CPU or visible catalog interactions. It is compatible-source qualification, not an acceptance decision for a new catalog item. Adding a second frontend-design resource still needs a distinct benefit relative to the existing design skills, plus the ordinary admission and product checks. Current shipped inventory stays unchanged.

## Retained artifacts

The evidence is outside disposable build storage in `faber-recovery/c77e-20261024`: `frontend-plugin-runtime-receipt.json`, `frontend-plugin-runtime.log`, `frontend-plugin-build.log`, `frontend-plugin-prepare.log`, `frontend-plugin-independent-review.md`, `frontend-plugin-probe.rs`, `frontend-plugin-prepare.py` and `frontend-plugin-run.mjs`. Original preserved-source provenance is in `faber-evidence/frontend-design-source-20261024/manifest.json`.

Memory and processor cost: no production source or shipped data changes; no product runtime measurement was made.
