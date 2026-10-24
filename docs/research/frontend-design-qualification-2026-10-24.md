# Frontend-design plugin source qualification — 2026-10-24

Status: potentially eligible on source inspection only; not approved, admitted, installed or executed. Task 995 remains open. The original source investigation changed no repository, catalog, installed tool, personal CLI folder or configuration; this follow-up adds only its research record. R247/R253/R362/R389 delivery rules were not relaxed.

## Exact source and scope

- Repository: anthropics/claude-plugins-official.
- Commit: `58267944f92187b7d6a1c822165c96c81c864806`.
- Original folder: [plugins/frontend-design](https://github.com/anthropics/claude-plugins-official/tree/58267944f92187b7d6a1c822165c96c81c864806/plugins/frontend-design).
- Root directory listing: [exact-ref contents](https://api.github.com/repos/anthropics/claude-plugins-official/contents/plugins/frontend-design?ref=58267944f92187b7d6a1c822165c96c81c864806).
- Complete recursive skills tree: [b215f7d4a65ca021ee0440c58d22de7b644f5d71](https://api.github.com/repos/anthropics/claude-plugins-official/git/trees/b215f7d4a65ca021ee0440c58d22de7b644f5d71?recursive=1), `truncated:false`.
- Complete recursive metadata tree: [d5f5fde87bc3813f9117c7bc6d6423681c6db114](https://api.github.com/repos/anthropics/claude-plugins-official/git/trees/d5f5fde87bc3813f9117c7bc6d6423681c6db114?recursive=1), `truncated:false`.

The root listing has only .claude-plugin/, LICENSE, README.md and skills/. Both directories were recursively enumerated. All five files were read, including hidden metadata and the skill-local license. Every enumerated nested entry is a plain Git blob with mode 100644; directories are trees. No symlink or submodule was listed. This is complete coverage of the pinned plugin folder, not approval of the entire marketplace repository or files outside this scope.

Source access used the connected GitHub read tool after direct network reads failed. No archive was extracted and no upstream code ran. The returned UTF-8 contents were independently converted back to bytes and verified against every listed Git blob SHA-1 before SHA-256 calculations.

## Complete file inventory and byte identities

| Relative file | Bytes | Verified Git blob SHA-1 | Raw-byte SHA-256 |
| :--- | ---: | :--- | :--- |
| `.claude-plugin/plugin.json` | 180 | `6a1426c78e5ed0cca07858597c277b4a59b8989a` | `fd9a7db11f5dec771d9e29ad1d6a3298c12f04c81c3a6457099b81fe477d835d` |
| `LICENSE` | 11358 | `d645695673349e3947e8e5ae42332d0ac3164cd7` | `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30` |
| `README.md` | 977 | `00cd435c88bc23614422a12494488bd5b3bd255b` | `5351137ecb2c5adfcdbadcdbbeda1eab16409bae98eb53f62f945514552d6706` |
| `skills/frontend-design/LICENSE.txt` | 10174 | `f433b1a53f5b830a205fd2df78e2b34974656c7b` | `0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594` |
| `skills/frontend-design/SKILL.md` | 9390 | `a5333457c414d20d625f307df945842c0952ecc3` | `d91970639e9f5c37682ac7ab60094d35f1c7c1f38d731bd56396563aee10c1d3` |

Total: 32079 bytes in five files.

Canonical Faber digests, using byte-preserving UTF-8 contents and lexicographically sorted relative paths with `path NUL byte-length NUL bytes`:

- Whole original plugin folder: `ce5bda41d00150cd52bea577bc91f738dd86a68ac75efc3a6d512727ba82bd6c`.
- Skill folder `skills/frontend-design`: `83485963700bcf11d5bf78118facb301676b69d918fee2e45a95e7f1a2840f6c`.

These are source qualification digests, not a receipt from Faber's installer. The plugin has no version field; the original commit and tree digest identify this candidate.

## Declared and implicit capabilities

[.claude-plugin/plugin.json](https://github.com/anthropics/claude-plugins-official/blob/58267944f92187b7d6a1c822165c96c81c864806/plugins/frontend-design/.claude-plugin/plugin.json) contains name, description and author only. It declares no executable hook, tool grant, permission, model override, MCP server or external part path.

The sole implicit native part is [skills/frontend-design/SKILL.md](https://github.com/anthropics/claude-plugins-official/blob/58267944f92187b7d6a1c822165c96c81c864806/plugins/frontend-design/skills/frontend-design/SKILL.md), with name `frontend-design`. Its simple front matter has only name, description and license. The complete folder has:

- One skill, covering its SKILL.md and LICENSE.txt.
- Zero commands, agents, server definitions, hooks, scripts, package manifests, lifecycle scripts, binaries, monitors, output styles, workflows, settings files or language-server definitions.
- No `allowed-tools`, model/permission overrides, plugin-root expansion, inline shell execution marker or outside-project @path read.
- No operational instruction to install/fetch a package, executable, model or other skill; no account, paid key, sign-in or credential storage requirement.

The body supplies design planning, typography, layout, copy, accessibility, motion restraint and self-critique guidance. Screenshots and note-taking are conditional on an existing environment. Its fictional client/payment framing is prose, not authority to spend, contact someone, change permissions or override the person's brief.

The README's sole external operational-looking link is explicitly further reading: a mutable main-branch cookbook URL. It is a citation rather than an instruction to fetch or execute it. That notebook was not opened or approved. No mutable reference is required to deliver the five-file plugin.

All source instructions were treated as data during this audit. In later use, the person's request, project rules and Faber's permission boundaries remain authoritative. The skill's self-critique never substitutes for Faber's required independent review.

## License within the actual scope

[Plugin LICENSE](https://github.com/anthropics/claude-plugins-official/blob/58267944f92187b7d6a1c822165c96c81c864806/plugins/frontend-design/LICENSE) is Apache License 2.0. The skill's own front matter explicitly points to [its LICENSE.txt](https://github.com/anthropics/claude-plugins-official/blob/58267944f92187b7d6a1c822165c96c81c864806/plugins/frontend-design/skills/frontend-design/LICENSE.txt), also Apache License 2.0. The skill-local text is the identical 10,174-byte prefix of the root license; the root additionally carries the standard appendix. No proprietary or contradictory local terms were found in the five-file scope.

This reports checked license text, not legal advice. Preserve original licenses/notices and download from the original source; bundle nothing into Faber.

## Comparison against the current local catalog and prior exclusions

Reviewed `sidebar-close-oct24` sources:
- `engine/src/tools.json`: no frontend-design item; no admitted plugin definitions. It already contains other frontend design skills, including design-taste-frontend, which is a different source and name.
- `docs/research/2026-10-21/catalog-list.md:314–335,390–422`: twenty historical candidate bundles, all excluded. Frontend-design is not one of those twenty. A candidate from the same marketplace is not the same source folder.
- `docs/tasks/995-catalog-servers-and-plugins.md`: curated additions must use the existing checked delivery and require lifecycle and selection proof.

The twenty excluded candidates are not twenty shipped plugins. No earlier exclusion is reversed here. Other conversations' external inventories, unpublished curation or changes outside this local copy were not available for verification; do not admit a duplicate before reconciling that inventory with its owner.

## Existing delivery comparison

The current preflight section at catalog-list.md:357–386 supersedes its historical blocker. The inspected source appears compatible with:
- Definition and exact part validation in `engine/src/tools/plugins.rs:64–127`.
- Prompt metadata, native execution and path restrictions at `:151–224`.
- Whole-source checking, manifest allowlist, undeclared-part checks and executable refusal at `:263–406`.
- Private checked skill delivery, without personal skill-root writes or new installer/ledger/selector.

This was a manual source comparison. No Rust validator, extractor, installer or session kit was executed; no positive pass from those systems is claimed.

## Missing admission evidence

Before any admission, reconcile the external curation and then prove the candidate through existing mechanisms rather than create another API:

1. Run the existing whole-plugin validator against the exact five-file source and one declared skill part, checking original alias, license metadata and the two canonical digests above.
2. With a fake original-source archive, prove install, duplicate refusal, on/off, remove, Undo and reinstallation through the same parent transaction/ledger. A skill-only plugin starts no server process; verify delivery of the skill instead of inventing a server-start check.
3. Plant modified bytes, a symlink, traversal/case collision, an undeclared command/server/hook and executable or widening metadata; verify refusal without publishing partial files or writing outside the store.
4. Prove supported project session kits select the namespaced skill for a relevant frontend task and do not duplicate an already available frontend-design alias. Check launch-time whole-folder revalidation, revocation rules and CLI-specific delivery with the existing session machinery.
5. Obtain a fresh independent source/coherence review, the repository's required checks and any integration/visible-catalog evidence required by AGENTS.md. No catalog card, translations, what's-new entry or publication is authorized by this qualification alone.

No fake-install, uninstall, containment execution, live CLI use, selection runtime, memory/processor measurement or independent approval was performed in this investigation. The result is a potentially compatible pinned source, not a new installed or approved plugin.

## Task coordination and retained evidence

The active security conversation confirmed that the earlier storage conversation had received only the candidate's
metadata, without acknowledging qualification. The storage conversation is archived and cannot receive a message;
its latest available answer concerns the completed build-storage work. The active security conversation was sent
this completed qualification and its digest, so it need not repeat the source audit. This does not claim access to
unpublished work or permission to admit the candidate. Admission, installation and tool execution remain out of scope.

The original external report is retained verbatim with SHA-256
`709f196ed5c52c66b143b5ed528d41243351a13bf0c564ad716cece028ef6027`. This repository copy contextualizes the original status and adds this coordination
note. A separate synthetic skill-only lifecycle test is being prepared; its initial inventory assertion was incorrect
and its revised execution remains pending. It is not a successful validation of the real source described here.
