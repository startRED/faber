# Faber folders safe and small

Area: Separate copy, Permissions
Files: engine/src/engine/copies.rs, engine/src/project/copy.rs, engine/src/workspace.rs, engine/src/permissions.rs, engine/src/claude.rs, engine/src/codex.rs, engine/src/store.rs, engine/tests/conversation_copy.rs, engine/tests/project_security.rs, ui/locales/, docs/product-map.md
Depends: none
Size: two pull requests: first safe copy cleanup, then data-folder protection

Why: Rebuildable folders in finished project copies filled the owner's disk. Agents must not write Faber's own conversations and settings in sandboxed modes, even when a project sits inside its data folder.

Build: Extend the copy sweep for idle copies with no running reply, after a set idle period such as seven days. Remove only git-ignored folders in a fixed list: `target`, `node_modules`, `.next`, `dist`, `__pycache__`. Never follow links or remove tracked folders, source, whole copies or the user's project; log freed size and keep the copy openable. Inspect both existing sandboxes first. Use the Store's actual data path, including `FABER_DATA_DIR`, for each CLI's write denial; the Codex profile must deny it even when the workspace is inside it. Preserve allowed reads. Extend the existing best-effort command text check to refuse paths in that folder before a card for both AIs. Full access stays unchanged; explain in every language that it permits such access without weakening warnings.

Leave out: a cleanup setting, new permission modes, storage changes and the already-protected CLI folders.

Done: Tests fail today unless an idle ignored target is removed while tracked folders, running copies and junction targets remain untouched. Security tests attempt writes from both CLIs in all sandboxed modes, including a project inside the data folder; they are refused while ordinary project writes work. Full access and its explanation are checked. Update the map, design and what's-new in every language. Obtain two security reviews, one from the other vendor. Report the background-priority directory walk and its measured cost.
