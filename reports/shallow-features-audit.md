# Shallow-features audit

Audited the current checkout read-only. The comparison notes in `E:/Dev/re-corpus/deepseek-notes/` were treated as leads and checked against this checkout; several of their reported gaps describe an older state. This is a source and documentation audit, not a runtime or memory measurement.

## Inventory and status

| Area | Status | Evidence and assessment |
| --- | --- | --- |
| User settings | **Used** | All 22 `Settings` fields are represented by the inventory check and map (`engine/tests/product_map.rs:17-44,149-184`; `docs/product-map.md:249-270`). The per-key stored domains, readers and callers are enumerated below. `Settings` schema is `engine/src/model.rs:1514-1585`. `grantsEpoch`, `permissionsRevision` and `beforePlan` are internal safety state, read/written by the permission and plan paths (`engine/src/engine.rs:2502-2550,3187-3197,3248`). No unused setting field found; current saved values were not inspected. |
| Persisted state | **Used, with explicit legacy leftovers** | Active records include conversation JSON and attachments/drafts; project and separate-copy records; chat groups; recents/environments; quota and Auto-learning ledgers; catalog/tool state; kept decisions and project memory; local update, crash, self-change and activity records; browser tabs and cookies; CLI channel state; private Chat workspace relocation; and OS autostart. Their writers/readers are named in `docs/product-map.md:268-336`; sampled call sites include `ui/App.tsx:111-112,254,366-372,408-428,515-528,1322-1408`, `ui/unsent.ts:107-257`, `engine/src/workspace/unix_chat.rs:9-97`, `engine/src/workspace/relocation.rs:7-125`, `engine/src/engine/kits.rs:208`, and `src-tauri/src/update.rs:115-153`. |
| Obsolete persisted state | **Dead, retained or cleaned up** | `faber.project.goal.<id>` and `faber.project.mission.<id>` have no current writes or reads; project removal only deletes them (`ui/projects.ts:7-8`). Old mission journals/workspaces and `qualification/` are explicitly left alone and never read or deleted (`docs/product-map.md:294,296`; mission data is also retired by R215). Their on-disk size is unknown; no runtime memory cost was measured. |
| Shell and host commands | **Used; no dangling legacy command found** | The map inventory check now includes both `src-tauri/src/main.rs` and `host/src/table.rs`, alongside settings, UI screens and browser keys (`engine/tests/product_map.rs:47-75,78-126,149-204`). Static comparison found 174 host commands and 157 UI wrappers; the 17 without UI wrappers are host/CLI reads and agent actions, test hooks, or the internal visual-document path (for example `conversation(s)`, `projects`, `quota`, `settings`, `start_conversation`, `message_conversation`, `show_conversation`, `report_checks`, `checks_progress`, `visual_document`, and three test probes). Every UI wrapper matched a host command. Current project tools are copy, goals, memory and proof commands (`host/src/table.rs:773-837`); `project_revoke`, `local_update_state`, and mission start/state/mode/stop/resume/answer commands have no current source matches. |
| Main product surfaces | **Used by the map; behavior not exhaustively exercised** | The current map gives each live surface a concrete UI or host owner (`docs/product-map.md:64-133`). It covers the shell and setup (titlebar, first run, tutorial, account cards, Sidebar, topbar, context meter, Settings, Catalog, tray, update and what's-new); conversation work (home, no-AI state, thread, composer, permission/question cards, subagents/helpers, review rounds, changes, side questions, agent-started conversations, delivery, archives, pull-request following, Keep going, plan/closing box, result notes, running column, file-at-line, visual answers/pieces, browser and sign-ins); and project work (project home/new/past conversation, memory, goals, checks, map, search preferences, AI limits, separate copies). Rows also cover right-click menus, back confirmation, search, in-thread find, keyboard shortcuts, image viewing, error toasts, outside windows and running commands. I did not walk each surface in the app. |
| Attachment size/error handling | **Implemented and checked; engine admission follow-up remains** | `ui/attachments.ts:2-14` rejects read errors/aborts and synchronous failures with the file name. `Composer.add` reserves pending bytes before reads, caps attachments at 8 MiB total per message, catches failures and renders an accessible retryable error (`ui/Composer.tsx:107-135,537-574,617-619`; `ui/locales/en.ts:500-501`). Four native attachment checks passed, including restored oversized drafts, retry and clearing stale errors; the existing thumbnail/removal check passed separately. Data URLs temporarily duplicate file bytes; peak memory remains unmeasured. |
| Sidebar keyboard navigation | **Implemented and checked** | `ui/Sidebar.tsx:460-486` handles ArrowUp/Down, Home/End and project/group ArrowLeft/Right, limits focus targets to visible enabled rows, and leaves modified shortcuts and search input alone. The native check passed project/group focus and parent/child behavior (`tests/ui/sidebar-navigation.check.ts`). Existing Alt+Up/Down still switches conversations globally (`ui/keys.ts:50-51`; `ui/App.tsx:1190-1198`). |
| Visual/idle polish | **Used and intentionally protected** | Current full-file counts: `ui/Hole.tsx` 511, `ui/Ambient.tsx` 206, `ui/Ghost.tsx` 27, `ui/idle.ts` 48 and `ui/motion.ts` 347 (1,139 total, including comments). These are live surfaces, not orphan code. C005 requires equal quality across area homes; C008 requires quiet motion and no hidden-window CPU/GPU work (`docs/product-rules.md:71,74`). The idle and home checks are listed at `docs/product-map.md:527`; no per-component memory/CPU measurement was found. |
| Explicitly not built | **Planned/absent, not dead** | The current map calls out remaining command-line tools/live manifest, enforced review gate, upfront proof contract, rendered visual judges, step snapshots/undo API, agent-driven settings beyond web-search preference, autonomous mode/unattended fence, on-screen git modes, remaining visual answers and full benchmark (`docs/product-map.md:539-543`). They should not be described as working features; no memory cost can be attributed to them from this checkout. |

## Per-item inventory

“Used” means a current source reader, writer or UI handler was found; “dead” means only deletion/legacy references remain; “half-built” means persisted or engine machinery exists but a promised visible connection is absent. Map line references identify the exact inventory row; the source references are the code evidence. No data folder contents were opened.

### Settings

Each of the 22 keys below is used. Model definition and defaults: `engine/src/model.rs:1514-1585`; generated product inventory: `engine/tests/product_map.rs:17-44,149-184`; screen controls: `ui/Settings.tsx:124-330` and first run `ui/App.tsx:261-263`.

| Used setting and persisted domain | Live reader / caller evidence |
| --- | --- |
| `appearance`: named theme fields `base`, `accent`, `ground`, fonts, size, density, corners, motion, effects, colors | `host/src/look.rs`; UI theme reader `ui/appearance.ts`; first-run and Settings via App/Settings. |
| `language`: system-following empty value, `en`, `pt-BR`, `es`, `fr`, `de` | `ui/i18n.ts`; `ui/App.tsx` calls `setShellTexts` at 1248; `engine/src/persona.rs` receives `Tuning::language`. |
| `userName`: optional string, max 40 characters | `ui/App.tsx` greeting/sidebar footer and new-conversation prompt; `engine/src/persona.rs` copies into conversation input. |
| `permissions`: `ask`, `acceptEdits`, `plan`, `auto`, `dontAsk` | Turn enforcement `engine/src/engine.rs:2502-2550`; chip/card in `ui/Composer.tsx` and `ui/Thread.tsx`. |
| `permissionsRevision`: fresh token per explicit pick; `beforePlan`: optional previous mode; `grantsEpoch`: random invalidation token | Approval enforcement `engine/src/engine.rs:3187-3197,3248`; a plan card carries mode revision. |
| `webSearch`: `off`, `cached` (Codex), `live` | `engine/src/engine.rs` / `engine/src/engine/toolbox.rs`; `ui/VisualAnswer.tsx` snapshots policy; Settings switch. |
| `reasoningSummary`: `default`, `auto`, `concise`, `detailed`, `none` | Codex turn setup in `engine/src/engine.rs`; Settings control. |
| `replyStyle`: `default`, `short`, `explained`, `own` | Both CLI drivers via `engine/src/persona.rs` and `engine/src/engine.rs`; Settings. |
| `replyStyleText`: trimmed, cleaned text up to 400 Unicode characters; empty allowed | Passed to persona only for `replyStyle=own`; Settings editor. |
| `userHooks`: boolean, default false | `engine/src/engine.rs` and CLI launchers; Settings hook preview/control. |
| `notifications`, `notificationSound`: booleans, default true | Tray readers `src-tauri/src/tray.rs`; switches in Settings. |
| `faberTrust`: boolean, default false | `engine/src/engine/toolbox.rs:288,296`, `engine/src/engine/toolbox_replace.rs:102,112`; Settings. |
| `previews`: boolean, default false | `src-tauri/src/update.rs:538`; Settings. |
| `keepAwake`: boolean, default true; `autostart`: boolean, default false | `src-tauri/src/awake.rs`, `src-tauri/src/autostart.rs`; Settings and actual OS keeper. |
| `resumeInterrupted`: boolean, default true | `engine/src/engine/resume_cut.rs`; Settings. |
| `setup`: `done`, `skipped` or absent; `experience`: `beginner`, `experienced` or absent; `tutorialDone`: boolean or absent | `ui/setup.ts`, `ui/App.tsx`, tutorial component and Settings replay action. |

### Persisted values

All current map rows (`docs/product-map.md:287-340`) are listed. Items that share a reader are grouped, with every stored name still enumerated. `Dead` entries have no source reader/writer beyond cleanup; `Half-built` is explicit in the map.

| Status | Persisted value(s) | Reader/caller evidence |
| --- | --- | --- |
| Used | `child-transcripts/<conversation>/<hashed-call>/*.input.json`; `Message.acts`; `Message.review`; `creative-reviews/<uuid>.json`; `rewind/<folder-id>/`; `rewindNotice`, `rewindSummary`, message `rewind`; `pieceValues` | `engine/src/transcript.rs`, `engine/src/act.rs`, `engine/src/review.rs`, `engine/src/engine/creative_judges.rs`, `engine/src/rewind.rs`, `engine/src/engine/rewind.rs`, `engine/src/pieces.rs`; UI `Thread.tsx`, `Rewind.tsx`, `VisualPiece.tsx` (map 287-293; Review rounds map 15,89). |
| Used | Conversation `browser`; `side-chats/<id>.json`; `conversations/<id>.json`; `chat-groups.json`, `chat-groups/<group>/<file>`, `chat-group-move.json` | `engine/src/engine/browse.rs`, `engine/src/engine/aside.rs`, `engine/src/store.rs`, `engine/src/groups.rs`; `ui/App.tsx`, `ui/Site.tsx` (map 293-296). |
| Used | `projects.json`; `copies/<conversation id>/`; `recents.json` | `engine/src/project.rs`, `engine/src/project/copy.rs`, `engine/src/store.rs`, recents UI/API; host copy API `host/src/table.rs:772-775` (map 297,299,301). |
| Dead | `<project>/.faber/missions/<id>/`, `missions/<id>/`, `workspaces/<project>/`, `proof-target/<project>/`; `qualification/` | Map explicitly says left alone, never read or deleted (`docs/product-map.md:298,300`); current source search found no runtime readers. R215 retires the owning feature. Disk bytes not inspected. |
| Used | `auto-learned.json`; `decisions-kept.json`; `budget.json` | `engine/src/auto_learned.rs`, `engine/src/project/memory.rs`, `engine/src/store.rs`, `engine/src/ledger.rs`; Forget called at `ui/Accounts.tsx:70` (map 302-305). |
| Used | `notes-seen`; `update.log`, `startup.log`, `update-tried`; `catalog-hint-seen`; `shell-texts.json` | Host `host/src/table.rs:768-769,306-307`; `src-tauri/src/update.rs`, `src-tauri/src/main.rs`; catalog hint `engine/src/engine/kits.rs:208`; tray `src-tauri/src/tray.rs` (map 306-309). |
| Used | `cache-health.jsonl`; `self-changes.jsonl`; `look.jsonl`; `method/<project>.jsonl`; `crashes.jsonl`; `running.marker` | `engine/src/cache.rs`, `engine/src/toolbox.rs`, `engine/src/look_log.rs`, `engine/src/method_log.rs`, `engine/src/crash.rs`; shell crash lifecycle `src-tauri/src/main.rs` (map 310-315). |
| Used | `resume-after-update.json`; `channel/secret`; `channel/faber.sock`; `neutral/`; `rules-sources.json` | `engine/src/engine/install_gate.rs`, `channel/src/secret.rs`, `channel/src/server.rs`, `engine/src/proc.rs`, `engine/src/store.rs` (map 316-319). |
| Used | `tools/plugin/skills/<id>/`; `tools/servers/<id>/`; `tools/plugins/<id>/source/`; `tools/installed.json`; `tools/selection.json`; `tools/kits/<uuid>/` | Host tool commands `host/src/table.rs:848-900`, `engine/src/tools.rs`, `engine/src/store.rs`, engine session kit assembly (map 321). |
| Half-built, protected by read-only boundary | `faber-trust/adoptions.json` | Read/approval paths exist in `engine/src/store.rs`, `engine/src/project/memory.rs`, `engine/src/engine.rs`; map says screen/card loading and reconciliation are unfinished, and no adopted-source write API exists (`docs/product-map.md:322`). R544 says adopted task folders remain read-only until an approved writer exists (`docs/decisions.md:546`); R35/R332 establish project memory’s existing-file and bounded-read rules (`:41,:335`). |
| Used | `chat-workspace-unix.json`; `chat-workspace.json`; external Chat roots; `attachments/<id>/` | `engine/src/workspace/unix_chat.rs`, `engine/src/workspace/relocation.rs`, `engine/src/store.rs` (map 323-325). |
| Used | Project instruction/learning files; `sessionStartCards`; internal start-card formatter; `<folder>/.faber/verify/`; `<project>/.faber/verify.local.json` | `engine/src/project/memory.rs`, `engine/src/project/rules.rs`, `engine/src/engine/kits.rs`, `engine/src/engine/verifying.rs`, `host/src/table.rs:806-847`; map 295,326-329. |
| Used | `faber.place`; `faber.layout`; `faber.column`; `faber.reading`; in-memory Catalog filter/search/page/selection and last Settings section | `ui/App.tsx:111-112,254,366-372,408-428,515-528,803,917`; Sidebar layout read/write `ui/Sidebar.tsx:130-142`; `ui/Catalog.tsx`, `ui/Settings.tsx` (map 330-334). |
| Used | `faber.choice.<conversationId>` | `ui/App.tsx:981,1322,1366,1394,1408,1493`; falls back to the conversation record (map 335). |
| Dead, with useful cleanup | `faber.project.goal.<projectId>`; `faber.project.mission.<projectId>` | Only current source hits are `localStorage.removeItem` in `ui/projects.ts:7-8`; no matching write/read. Those calls are real cleanup when a project is removed. Keep them: deleting the lines would leave obsolete values behind and would not remove any live feature. The owner explicitly authorized deleting dead commands and unread stored values; these two browser keys already get cleaned up at the relevant removal point (map 336-337). |
| Used | `faber.unsent.<field>`; `piece-edit.<identity-and-definition>`; `faber.unsent-attachments.<field>`; IndexedDB `faber.unsent` / `attachments` | `ui/unsent.ts:29-257`, `ui/piece-edits.ts`, `ui/Composer.tsx:537-574,617-637`, `ui/App.tsx`; screen durability checks (map 338-339). |
| Used | Windows Run key; CLI executables in `~/.local/bin` and user PATH | `src-tauri/src/autostart.rs`; `engine/src/install.rs`; setup UI checks binaries (map 340). |

### Host commands without a UI API wrapper

The static inventory is `host/src/table.rs`; UI mirror is `ui/api.ts`; inventory checker compares both at `engine/tests/product_map.rs:47-75`. These host commands intentionally serve the CLI, internal shell/sidecar, test harness, or no-window behavior. Each name is enumerated so none is hidden behind the count:

| Names and status | Caller evidence |
| --- | --- |
| Used by CLI: `conversation`, `conversation_markdown`, `conversations`, `mark_waiting`, `message_conversation`, `projects`, `quota`, `resume_conversation`, `show_conversation`, `start_conversation` | `cli/src/verbs.rs:17-40,173-281,335-371` maps status/read/export/list/open/session messages to the host channel. |
| Used by host startup: `settings` | Host initialization supplies Settings to the first screen state (`host/src/lib.rs`, `src-tauri/src/main.rs`); later writes use the separately named `save_settings` UI command. |
| Used by hidden test harness only: `checks_progress`, `report_checks`, `test_host_size`, `test_site_probe`, `test_site_refuse_once` | `host/src/table.rs:1024-1054`, hidden-test Tauri handlers `src-tauri/src/main.rs:619-640,924-930`; release builds compile out `test-hooks` (`src-tauri/src/main.rs:6`). |
| Used by internal visual page delivery: `visual_document` | Sidecar implementation `host/src/sidecar.rs:445`; served once through its `faber-visual` scheme; Tauri table `host/src/table.rs:1011-1012`. |

Separate reader check: UI callers found for `versions` (`ui/App.tsx:184`), `forget_auto_learned` (`ui/Accounts.tsx:70`), and `set_shell_texts` (`ui/App.tsx:1248`). `visual_policy` is test-only (`host/src/table.rs:1019-1020`). `background_commands` is both wrapped in `ui/api.ts:1006` and implemented (`host/src/table.rs:476-477`, `engine/src/engine.rs:1057`), but no production UI/CLI caller was found: **dead exposed wrapper / review candidate**, without a removal recommendation here. `asks` is a normal UI wrapper called at `ui/App.tsx:133`.

No current host or UI symbols were found for `project_revoke`, `local_update_state`, `RunMode`, or mission start/state/mode/stop/resume/answer commands. `visual_document` is not dead despite lacking a visible UI invocation: it is called by the internal visual scheme.

### Surface entry points

The map inventory names each surface at `docs/product-map.md:64-133`; component ownership was checked in source. Status is **used** unless marked.

| Surface names | Source mount, callback, or handler evidence |
| --- | --- |
| Titlebar; First run: accounts; First run: look; Tutorial; Account cards; later; Sidebar; Topbar; Context meter; Settings; Catalog; Tray; Update opening screen; What's new | `ui/App.tsx` mounts app shell and routes; `ui/Settings.tsx`, `ui/Catalog.tsx`, `ui/Accounts.tsx`, `ui/Sidebar.tsx`, `ui/Topbar.tsx`, `ui/Tutorial.tsx`; tray `src-tauri/src/tray.rs`, updater `src-tauri/src/update.rs`; connected API calls through `ui/api.ts`. |
| Chat home; No AI; Chat conversation; Composer; Question card; Subagent step; Review rounds; Helper from other AI; Conversation changes; Quick question on side; Conversation started by agent; Turn-end delivery; Conversations talking to each other; Archived conversation; Plan and closing box; Result note; Project results; Running column; File at a line | `ui/App.tsx` selects chat/home/thread; `ui/Thread.tsx`, `ui/Composer.tsx`, `ui/Question.tsx`, `ui/Results.tsx`, `ui/Changes.tsx`, `ui/Aside.tsx`; host callbacks in `host/src/table.rs:283-498` and engine `engine/src/engine/*.rs`. |
| Project home; Projects new; Past project conversation; Project memory; Project goals; Faber's checks; Faber's project map; Faber's web-search preference; Faber's AI limits; separate copies | `ui/Projects.tsx`, `ui/ProjectHome.tsx`, `ui/ProjectSettings.tsx`, `ui/Memory.tsx`; host project APIs `host/src/table.rs:772-847`, engine `engine/src/project/*.rs`. |
| Chat group page; Chat group row; A conversation row; A project row; Projects: open a project; Projects: new conversation; Recent folder or environment | `ui/Sidebar.tsx` builds rows and dispatches open/select/remove; `ui/App.tsx` changes active area/conversation/project; group/project mutations route through `ui/api.ts` to `host/src/table.rs:167-185,637-654`. |
| Pull-request following; Keep going run; Visual answers; Built-in visual pieces; Page in Faber browser; Sign-ins from Firefox; Image viewer | `ui/Thread.tsx`, `ui/VisualAnswer.tsx`, `ui/Site.tsx`, `ui/ImageViewer.tsx`; internal browser/visual host handlers `host/src/table.rs:930-1012`. |
| Faber's catalog search and install; Faber's project memory search; Your message; An empty conversation after going back; An answer; Selected text; A text field; Step; Window opens | `ui/Catalog.tsx` search/install callbacks; `ui/Memory.tsx` search/edit; `ui/Composer.tsx` and `ui/Thread.tsx` render messages and empty state; `ui/Selection.tsx` selected-text action; `ui/Palette.tsx` text field and command steps; `ui/App.tsx` handles open-state recovery. |
| Right-click menus; Go back confirmation; Search and commands; Find in conversation; Keyboard shortcuts; Save-error toast; Outside windows; Running commands; File links | `ui/App.tsx`, `ui/keys.ts`, `ui/Sidebar.tsx`, `ui/Thread.tsx`, `ui/CommandMenu.tsx`, `ui/SaveError.tsx`; shell/host handlers `src-tauri/src/main.rs`, `host/src/table.rs`. |
| Independent creative comparisons; Offer bigger ask; Results note | `ui/Thread.tsx` renders saved reply markers/results; agent/engine path `engine/src/engine/creative_judges.rs`, `engine/src/engine/plan.rs`; receipts map row 289 and conversation row 295. |
| Approved-source start card | **Half-built**: backend and formatter exist (`engine/src/project/rules.rs`, `engine/src/project/memory.rs`), but exact approved taste/start-card connection and reconciliation are expressly unfinished (`docs/product-map.md:322,327`). |

## Missions, run mode, and protected choices

The shallow-features notes' “unreachable mission engine” finding is stale here. The current map says the mission machinery is gone from the engine and marks missions removed (`docs/product-map.md:21,529`); the old mission folders are only retained on disk (`:294`). The final owner decision is R215 (`docs/decisions.md:221`), which retires the mission and local-build system. R160 (`:166`) and R171 (`:177`) explain the earlier product direction and removal of mission screens, but their temporary “missions remain in the engine” state is superseded by R215. There are no current `RunMode`, mission command, `project_revoke` or `local_update_state` definitions in source. Nothing in the current decisions requires preserving run mode or reviving mission commands.

Preserve the replacement project workflow and consent boundaries:

- R201 (`docs/decisions.md:207`) protects per-conversation project copies; the agent works on its branch and Faber never merges.
- R422, R425-R427 (`:425,428-430`) protect standing goals as ordinary project conversations, with a per-project switch, spending limits and the bounded local crash source.
- R344 (`:347`) authorizes the dedicated Catalog; R249 (`:255`) defines its curated list and excludes Impeccable pending a telemetry check.
- R408 (`:411`) keeps Auto learning local and provides the Forget control; R417 (`:420`) reserves tool installation for consent or the separately chosen setup-trust switch.
- R421 (`:424`) says crash records remain local. R153 (`:159`) permits Impeccable's own telemetry-off contact only when that skill is installed; because R249 excludes it from the catalog, this is not a current default path.
- R169 (`:175`) sets the Settings sections and layout. Keep choices there instead of adding another work-screen setting.

### Protected choices: source footprint and recommendation

Line counts below are whole current source-file counts, including tests/comments only where the named file contains them; they are an upper-bound maintenance footprint, not feature-only added lines. Shared files make precise attribution impossible without a diff-based history study. None of these entries has a measured per-feature memory figure.

| Decision | Countable source footprint | Memory | Recommendation to owner |
| --- | --- | --- | --- |
| R215 mission/local-build retirement (`docs/decisions.md:221`) | 0 live mission-engine lines found; old data paths are only named in map row 298. | Unknown disk bytes; no active reader/process found. | Keep retired; preserve old files as the map says. No restoration or deletion recommendation. |
| R201 separate project copies (`:207`) | 955 lines across `engine/src/project/copy.rs` (568) and `engine/src/engine/copies.rs` (387); shared Settings UI adds a portion of `ui/ProjectSettings.tsx` (251). | Runtime memory unmeasured. | Keep copy workflow; no extra owner decision needed. |
| R422, R425-R427 standing goals/crash goal (`:425,428-430`) | 1,388 lines in `engine/src/project/goals.rs` (700) + `engine/src/engine/goals.rs` (688), plus shared `ui/ProjectSettings.tsx` (251). | Per-goal/task memory unmeasured. | Keep the bounded, opt-in workflow and budget; no new decision proposed. |
| R344 Catalog and R249 curated list/Impeccable exclusion (`:347,:255`) | 405 lines in `ui/Catalog.tsx`; engine catalog readers span shared tool inventory/installer files, so no feature-only LOC split. | Memory for installed tools/kit leases unmeasured here. | Keep curated Catalog; retain Impeccable exclusion until telemetry check is explicitly resolved. |
| R408 local Auto learning / Forget (`:411`) | 1,146 lines in `engine/src/auto_learned.rs` (226) + shared `engine/src/engine/auto.rs` (920); Forget UI at `ui/Accounts.tsx:70`. | File is described as a few hundred bytes; runtime heap not measured. | Keep local-only storage and visible Forget action. |
| R417 installation consent/trust boundary (`:420`) | 1,273 lines in `engine/src/toolbox.rs` (757) + `engine/src/engine/toolbox.rs` (516), not counting shared Settings/UI. | Tool and cache memory unmeasured. | Keep per-action consent, bounded trust switch, and local audit record. |
| R421 local crash records (`:424`) | 1,553 lines in `engine/src/crash.rs` (506) + `src-tauri/src/main.rs` (1,047); main file is shared host setup and not feature-only. | Log capped at 100 KB; runtime memory unmeasured. | Keep local bounded record and its stated crash-goal use. |
| R153 conditional Impeccable telemetry contact (`:159`) | 0 current default-product lines: its package is excluded by R249; no live adoption/contact caller was found. | No current default-path cost measured; installed skill’s own cost not measured. | Do not revive through the Catalog until its telemetry state is checked. |
| R169 Settings placement (`:175`) | 360 lines in `ui/Settings.tsx`, plus settings-specific behavior in shared engine/UI files. | Settings view memory unmeasured. | Keep Settings as the home for general controls; no second work-screen setting proposed. |
| R35, R332, R544 read-only adopted project sources (`docs/decisions.md:41,335,546`) | 217 directly countable implementation lines: `engine/src/project/memory.rs:487-629` (143), `engine/src/store.rs:964-1020` (57), and `engine/src/engine.rs:1452-1468` (17); tests in the same files are not included. | No feature memory measurement. Readers cap source bytes at 512 KiB and task enumeration at 256 Markdown files; transient heap cost remains unknown. | Keep person-only approval records and bounded read-only readers. Complete UI/card reconciliation only with an approved writer design; R544 keeps task folders read-only meanwhile. |

No additional owner-decision entry is proposed: these recommendations restate the current owner decisions and the already recorded R215 removal. No unresolved product choice was created by this audit.

## Recommendations and limits

1. Do not restore mission commands, the old mission UI, `Run mode`, `project_revoke` or the local build. R215 is the final decision; current project copies and standing goals are the supported paths.
2. Attachment and Sidebar checks passed after implementation, as recorded below. Keep the separate engine admission follow-up explicit; these UI checks do not establish protection of programmatic host/CLI requests.
3. Keep the two obsolete browser keys' existing project-removal cleanup (`ui/projects.ts:7-8`): without it, deleted projects leave unread legacy values in browser storage, while removing the cleanup code would not remove any live functionality. The old mission data and qualification receipts remain explicitly retained by the map; they were not inspected, so their stored size and contents are unknown and no active reader was found.
4. Keep the home/idle polish under C005/C008. Its cost has not been measured per component, so avoid claiming either a memory win or a regression from line counts alone.

## Audit limits

The inventory above began as a static `rg` and source/map inspection; runtime checks were run afterward and are recorded below. No data folders were inspected, memory/CPU measured, or exhaustive network-destination audit performed. `engine/tests/product_map.rs` verifies that inventory names appear in the map; it does not itself prove each command has a runtime user. The command cross-check used the host table and UI wrappers and found no unexplained mismatch. Individual screen behavior was not exhaustively reviewed.

## Implementation and verification follow-up

The implementation extends the existing Composer, shared attachment reader and Sidebar; it adds no dependency,
resident process, interval, user setting or alternate command. Image previews, individual attachment removal and
the central shortcut list already existed on the current base and were retained. Browser reads reserve the same
8 MiB aggregate budget while in flight; sending checks older restored drafts too. Refusals identify the file and
limit, preserve the draft and clear when switching drafts. The shared file writer refuses oversized file batches.

The lead explicitly split the engine turn-admission wiring out when `fix-515a` claimed `engine/src/engine.rs`
exclusively. That file and its integration test are absent from this change. The ready follow-up is preserved at
`E:/Dev/reports/shallow-a139/engine-attachment-admission.patch`; it adds the import, the first guard in `send_task`
and the admission regression test. Until it lands, the combined image/file limit is enforced in the UI only;
programmatic host/CLI requests do not yet have that admission guarantee. The shared validator and its unit checks
remain, without claiming full engine enforcement.

The initial 20/40 MiB proposal failed a new cross-layer regression check because Electron and the host keep a
16 MiB JSON boundary. The final single 8 MiB aggregate budget passes that check after base64 encoding, with no
transport widening. The UI error is hand-written in all five languages and names the file that exhausts the budget.

| Evidence | Result |
| --- | --- |
| Final required local proofs through `verify full` | All seven passed on the final app code in 99 seconds. Receipt: `.faber/verify/1793803017767-full.log`, preserved as `E:/Dev/reports/shallow-a139/verify-full-final.log`. An earlier fast run added extra Clippy checks; seven failed with Windows error 112 (disk full), not source diagnostics. |
| `node --test scripts/sidebar-navigation.test.mjs` | Three passed, including the real row event handler with controlled button fixtures. |
| Hidden native UI, `sidebar: arrows` | One passed, including Chat groups, collapsed/exit rows and project parent/child assertions; final sequence session 7945 exited 0. |
| Hidden native UI, `composer: attachment` | Four passed, including restored oversized drafts, removal clearing stale errors, and long-filename glyph bounds/alignment at 720 px in English and German; final sequence session 7945 exited 0. |
| Hidden native UI, existing image preview/removal | One passed; session 70115 exited 0. |
| Native composition sentinel | Main-screen scenario passed at both screenshot sizes; final sequence session 7945 exited 0. |
| Shared validator exact-byte unit check | One real Rust case passed at exact boundaries with padding and combined images/files; session 87283 exited 0 (the runner also prints a suite aggregate). |
| Transport budget regression | Passed: 8 MiB aggregate stays within the existing 16 MiB serialized transport boundary. |
| Engine admission follow-up prototype | Its admission regression passed before the requested split. Historical receipt: `E:/Dev/reports/shallow-a139/attachment-engine-initial.log`; this is not proof of admission wiring in the queued UI-only piece. |
| Independent code/coherence review | Completed; the concrete CSS-specificity finding was fixed and the native alignment check then passed. Cards C006, C007, C009 and C011 hold. Independent visual comparison is recorded separately below. |

Resource cost: validation reads base64 lengths/padding without decoding or allocating a copy. Browser accounting
runs only when adding attachments; sidebar focus scans rows only on a key press. The 8 MiB decoded payload can
occupy about 10.67 MiB of base64 ASCII before transient copies. This is a representation bound, not measured RAM;
image pixel decoding and total application memory remain unmeasured. No per-feature CPU or memory saving is claimed.
The first verifier attempt filled its newly generated local verification cache. A later full run passed all seven
required proofs on the final 8 MiB/split-scope code. Per the lead's instruction, only
`env.CARGO_TARGET_DIR` of the main project's untracked `.faber/verify.local.json` was changed to the real folder
`E:/Dev/targets/cleanup`; no other local setting was changed and that file is not committed. Evidence and baseline
photos were copied to `E:/Dev/reports/shallow-a139`. Deleting this copy's own C: `target` was refused by automatic
tool policy even after explicit authorization; native Cargo clean also refused its missing cache tag. No deletion
or successful cache cleanup is claimed. No owner data was deleted.

## Independent visual comparison

Visual critique: before 30/40, after 30/40, by council codex, claude; no jester and no screen dropped.
Both fresh reviewers inspected all sixteen photos and validated their own JSON with the unchanged exported
`parseVerdict`. The unchanged `aggregate` and `report` exports produced the council receipts. The original
verdicts, photos and reports remain in `E:/Dev/reports/shallow-a139/`; final council reports are in
`before_final/council.md` and `after_final/council.md`, with `visual-comparison.json` recording the comparison.

| Screen | Size | Before | After |
| --- | --- | --- | --- |
| Chat home | 1280x800 | 38 | 38 |
| Chat home | 1920x1080 | 37 | 37 |
| Conversation | 1280x800 | 37 | 37 |
| Conversation | 1920x1080 | 37 | 37 |
| Project home | 1280x800 | 30 | 30 |
| Project home | 1920x1080 | 33 | 33 |
| Settings | 1280x800 | 34 | 34 |
| Settings | 1920x1080 | 36 | 36 |

Pre-existing defect confirmed by both reviewers: on **Project home at 1280x800**, the topbar clips the first
heading line, visible in both baseline and final photos. The composition sentinel passed despite this crop.
The lead explicitly accepted the existing-below-32/no-drop reading of the visual rule and requested a separate
scheduled fix; this change does not redesign that screen. The optional polish `climb` returns `revert` because
nothing rose; this is a functional change, not a visual-polish climb. That result is preserved, not relabeled.
Other deductions are single-reviewer notes, retained as unconfirmed in the council reports.

The photos do not verify movement or native screen-reader use; neither is claimed. The requested broader
per-feature CPU/RAM measurement remains unavailable. The engine admission follow-up, the pre-existing crop,
and the policy-blocked cache cleanup are still unfinished.

## Queue-preparation correction

The first queue preparation stopped before queueing: Clippy rejected the new `attachment_limits` test module
because production items followed it (`items_after_test_module`). This was introduced by this change, not a
baseline failure. The test module was moved to the end of `engine/src/workspace.rs`; its contents and every
other source token were preserved (an assert-based comparison passed). No warning suppression or skipped
verification was added. Fresh proof receipts accompany the retry; earlier native/visual receipts remain
evidence for the unchanged UI and attachment logic, not for completion of the queued commit's gate.
