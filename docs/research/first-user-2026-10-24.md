# First-user round for the stable-version work

Status: in progress. Task 996 remains open until both reviewers, both languages and both window sizes have completed
the requested walks, confirmed findings have fixes, and the second pass has no blocking findings.

## Harness correction

The old default walk launched with `--first-run --no-ai`, then asked the reviewer to send a Chat message and receive
an answer in that same disconnected app. The focused `--task conversa` inherited the same flags. Neither could prove
a working Chat turn, and the arbitrary question did not match the existing fake CLI recording.

The walk now closes the disconnected onboarding/Catalog app before Chat and opens a fresh isolated app with the
existing recorded Claude response. Chat explicitly chooses Haiku and sends the recording's exact question. Projects
and Settings continue in that second app. The focused Chat task uses that replay directly; the separate Demo project
task retains its existing setup. Reports identify recorded answers and do not call the model's own `done` verified.

Regression evidence: the new Chat test failed before the change because its launch still contained `--no-ai` and
`--first-run`. Afterwards all 17 first-user script tests passed. The check covers default task order, the launch
flags on each side of the restart, exact replay question/model guidance and the focused report's context.

A fresh read-only `gpt-6.1-sol` reviewer found nothing blocking, independently ran the 17 tests, and checked the
driver lifecycle and existing replay. It did not run the native app or spend model quota.

The repository's seven lightweight proof commands passed on this change. The screen-source checker reports five
existing warnings; the frontend compiler reports its existing large-chunk warning. Neither command fails. No
production code, installed tool, permanent process, timer or desktop-app memory allocation changes. The default
developer walk adds one sequential hidden-app launch, never two overlapping apps.

## Native evidence

The focused Chat walk passed with the Codex newcomer reviewer, pt-BR, 720x480, on the hidden isolated desktop. It
selected Claude Haiku, entered the exact question, sent it, and saw the recorded answer in five steps with no reported
confusion. The sandbox and cleanup passed. The final screenshot was independently inspected in the main session and
shows the full expected answer, idle status and an available composer. The detailed report is
`first-user-2026-10-24-wePfFO.md` beside this file; this is one controlled replay, not real-provider compatibility.

The five screenshots and the report were copied outside the disposable build caches to
`E:/Documentos/ProjetosIA/faber-evidence/newcomer-chat-20261024-c590/`, with `sha256.json` recording each file's digest.

The broader matrix began, but its first walk ended after three onboarding actions with exit 1 and no completed report.
That attempt does not prove onboarding. The later first-pass configurations and screenshot classifications are retained below; confirmation remains unfinished.
The harness correction is saved and sent for integration; the required landing command failed at the GitHub API
connection, before queueing. The connector could open the review request, but does not replace that landing command.

## Follow-up walks and classification

All sixteen bounded default/project configurations completed their first-pass runs across Haiku/Codex, pt-BR/English
and 720x480/1280x800. This does not approve every reported goal: some tasks reached their step limit or retained
subjective hesitation. The [follow-up evidence](first-user-2026-10-24-follow-up.md) records the later walks, the
reviewer-context limitation and the pending isolated confirmation pass. The focused Codex Chat smoke above is
additional evidence, not another completed matrix configuration.

| Walk | Program evidence | What it establishes and what remains |
| :--- | :--- | :--- |
| Codex, focused first run, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-tPs6KS.md`; five images retained | Image 3 shows the main disconnected screen. The reviewer nevertheless declared itself stuck looking for another continuation step. The goal was reached visually; its confusion remains recorded, not relabeled as a flawless UX pass. |
| Haiku, default walk, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-claude-pt-BR-720x480-default.md`; report and images copied with hashes | First run, Catalog, Chat and Settings were declared complete; the core Chat answer was visible. Projects was declared stuck with the narrow navigation drawer still open. The complete findings remain in the generated report. |
| Haiku, focused project, same language/size, retry | Exit 0; `first-user-2026-10-24-claude-pt-BR-720x480-project.md`; nine images, source/copy report hashes equal | Final image shows a reply and Done in Demo, with the start of the reply above the viewport. This proves a completed recorded turn, not that the complete answer was inspected or real-provider compatibility. |
| Codex, default walk, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-codex-pt-BR-720x480-default.md`; 24 images and report retained; 29 artifact digests verified | Chat's complete recorded answer and idle state were independently inspected. Catalog and Settings were declared complete. First run reached the disconnected main screen, but the reviewer asked how to continue without AI and declared itself stuck. Projects again stopped with the narrow drawer open; that does not establish a missing page. |
| Codex, focused project, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-codex-pt-BR-720x480-project.md`; nine images, 14 retained artifact digests verified | Demo was found and the recorded turn completed. The final image shows the reply's end and idle status, with its beginning above the viewport; full-answer inspection is not established. The drawer initially confused the reviewer, which continued by choosing Demo. |

The first project attempt was interrupted without a report when its coordinating runner received SIGINT. Three partial
images and an explicit interrupted status were preserved. It is not a success. The abandoned cache reservation was
later moved into evidence only after independent read-only recovery review, path/link checks, exact owner matching,
absence of known children, and a process snapshot showing no build writers. No temporary data or other processes were
deleted or ended. The normal reservation mechanism then admitted the next focused check. The interrupted audit is
not retroactively called clean.

Evidence is outside disposable caches at
`E:/Documentos/ProjetosIA/faber-evidence/newcomer-matrix-20261024-c590/`. The successful project retry has its own
`retry-after-interruption` folder, preserving the earlier partial images. Its executable digest remained unchanged;
it was the hidden UI build from the independently developed motion corrections, not an assertion that this worktree's
unbuilt UI was exercised.

A separate read-only native diagnostic reproduced Projects at 720x480. With the drawer open, the main stage is inert
and blurred, as documented. Closing the scrim exposes Open a project, its explanation and Add project folder. Both
images and `projects-diagnostics.json` are retained in the same evidence folder; that diagnostic's sandbox audit and
cleanup passed. The reported empty main page is therefore not confirmed. The empty sidebar caption exists at opacity
1, but sits low in the scroll area; the add button intentionally appears on pointer or keyboard focus. Discovery in
the small drawer remains a usability finding to compare with remaining reviewers, not a fabricated missing page.

Source inspection also confirms that the first-run appearance step contains three cards, stacked and scrollable in
the small window. Seeing only one without scrolling does not establish missing choices. That discovery complaint,
Catalog detail scroll position, model-selection wording and language-field ambiguity remain recorded for confirmation
in the outstanding walks. No speculative product change was made from those statements alone.

The Codex default walk retains all evidence under `codex-pt-BR-720x480-default/run-01` in the external matrix folder.
Before/after digests separately identify the actual app (`build-3/faber-test/debug/faber.exe`), the UX driver and
the fake CLI; all stayed unchanged. The driver digest alone does not identify the app. The UI source was the
independently developed motion/notes worktree at `1618816e`, not this harness worktree. The controlled replay and
disconnected first-run apps both ended naturally. Repeated drawer confusion now has evidence from both reviewers;
the documented behavior keeps navigation open when switching areas, so changing that behavior needs a separate
product decision rather than silently treating the drawer as a missing Projects screen.
The Codex project walk has its own `codex-pt-BR-720x480-project/run-01` evidence folder with the same unchanged app,
driver, fake CLI and UI source identities. Its final image was inspected separately from the reviewer's success claim.

Remaining work: confirmation of subjective findings, regression checks for confirmed fixes and an isolated second
pass without blocking findings. Task 996 remains open; no stable-release claim.

## English narrow default walk and executable provenance

The Codex en/720x480 default walk ended naturally with exit 0, using the corrected sidebar app at `af853a33`.
Its report is preserved verbatim in `first-user-2026-10-24-codex-en-720x480-default.md`. External evidence under
`codex-en-720x480-default/run-01` retains 33 images and 39 verified artifact digests. A fresh reviewer checked eleven
key images and all digests. This is the fifth completed first-pass configuration, not a second-pass result.

- First run: `primeira-abertura-4.png` shows the disconnected main screen, reaching the stated goal. Later difficulty
  finding how to continue remains a discovery complaint, not proof the goal was unreachable.
- Catalog: `catalogo-7.png` shows the entire When the agent uses it explanation. `catalogo-9.png` shows Back to the list;
  the walk had already returned previously. Both claims of missing information are contradicted. The final task was
  incomplete through the reviewer's interpretation; that does not prove a product defect.
- Chat: `conversa-5.png` shows the complete recorded answer, Done and Nothing running.
- Projects: `projetos-3.png` includes the new visible close control. `projetos-5.png` explains working inside an added
  folder, satisfying the purpose-discovery goal. After add-folder clicks, `projetos-8.png` shows no native selector;
  the hidden environment cannot establish how a normal picker behaves, and adding a folder exceeds that task's goal.
- Settings: `configuracoes-4.png` shows Language. The final Portuguese image shows System, so the specific language
  selection claimed by the reviewer is not confirmed.

The app remained `4b54472d85da2f6ca68a2d844c0b3a8519b2e90469844e0cac5c009a83483cf0`; the fake CLI and A/C source
snapshots also stayed unchanged. The driver changed from `660f6e3fbc0f737c33fe4a113da13da4c4bbdc5b737cf67025ade39c1d8b15c2`
to `efdfc87e399e64670b38c587326550590895780e7a91259bcaa82e0883bc494e`. Source inspection confirms the harness builds
that driver before launch even with `--no-build`, which skips only the app build. No driver hash was captured at the
exact spawn boundary in this walk; the endpoints do not prove an unchanged running driver.

The harness now streams executable hashes into `execution-identities.json` before/after the driver build, immediately
before each driver start, after readiness and after exit. Missing pre-build files are explicit null values; other
read errors stop the review. The new identity test first failed because the helper did not exist, then all eighteen
first-user checks passed. No desktop-app code or behavior changes. This developer-only evidence work reads files
sequentially in small stream chunks rather than retaining whole executables. Its native smoke is recorded below.
## English narrow project walk and identity smoke

The sixth first-pass configuration, Codex en/720x480 with the seeded Demo conversation, ended naturally with exit 0.
The raw report is `first-user-2026-10-24-codex-en-720x480-project.md`; external `codex-en-720x480-project/run-01` retains
its report, log, twelve screenshots, executable identities, provenance and lease receipt. Nineteen artifact digests
were verified. The reviewer first mistook the account footer for conversation navigation, returned from Accounts,
then chose Demo and completed the recorded turn. Its final image shows the complete Rayleigh answer and idle state;
this is replay evidence, not a real-provider connection or a confusion-free walk.

The new identity file has all five expected stages in order: before-driver-build, after-driver-build,
batch-0-before-driver-start, batch-0-ready, batch-0-after-driver-exit. App and fake CLI each have one digest throughout;
the driver has one digest across after-build, before-start, ready and exit. A's uncommitted source hashes/status and
C's clean source hashes/status stayed unchanged during the run. This confirms the new logging in native use without
claiming that an endpoint file hash alone attests to a running process. The complete first-pass matrix, a second pass
and integrated gates remain unfinished.
## Portuguese wide default walk

The seventh first-pass configuration, Codex pt-BR/1280x800, ended naturally with exit 0. The raw report is
`first-user-2026-10-24-codex-pt-BR-1280x800-default.md`. External `codex-pt-BR-1280x800-default/run-01` retains
21 screenshots and 28 verified artifact digests, including the report, log, receipt, provenance and identity file.
First run reaches the disconnected main screen. Catalog details include When the agent uses it and Back to the list;
the recorded Chat answer is fully visible, and Settings identifies Portuguese (Brazil).

Projects explains that Faber works inside an added folder, satisfying the stated purpose-discovery goal. The reviewer
continued beyond that goal, clicked Add project folder repeatedly and declared itself stuck when no picker appeared
in the hidden harness. Normal native-picker behavior remains unverified; this is not evidence of a missing Projects
page or an established picker defect.

All eight identity stages are present: two around the driver build and before-start, ready and after-exit for each
of the two isolated app batches. App, fake CLI and post-build driver hashes stayed equal across their recorded
boundaries; A and C source identities also stayed unchanged. These are file observations at recorded boundaries,
not loaded-process attestation. This is a completed first-pass walk, not approval of every goal or a second pass.
A fresh read-only reviewer independently verified all 28 digests and eight key images, confirming the classifications
above and no newly established blocking defect. The remaining thirteen images were not independently inspected.

## Portuguese wide project walk

The eighth first-pass configuration, Codex pt-BR/1280x800 with seeded Demo, ended naturally with exit 0.
External `codex-pt-BR-1280x800-project/run-01` retains eight images and fifteen verified artifact digests. A fresh
read-only reviewer independently verified all fifteen digests, the five ordered identity stages, unchanged clean
A/C sources and the natural receipt. It inspected `conversa-projeto-3.png` and `conversa-projeto-8.png`: Demo is open,
and the final image shows the exact question, Claude Haiku, the complete Rayleigh reply, Done and Nothing running.
The reviewer reported no confusion. This proves the controlled seeded conversation, not project creation, file edits
or a real-provider connection. The remaining six images were not independently inspected.

## Native-folder observation boundary

A separate read-only source investigation checked the UI worktree at `af853a33`. `add_project` in
`src-tauri/src/main.rs:930` opens a native folder dialog parented to the main window. The UX driver's shot operation
in `testkit/examples/ux_audit.rs:57` delegates to the page screenshot, which uses CDP `Page.captureScreenshot`
(`testkit/src/cdp.rs:468`). These screenshots capture the webview, not the whole desktop or external native dialog.
Their lack of a folder picker therefore cannot prove that no picker opened. The native result and ordinary use
remain unverified. No product correction is justified by that absence alone. The external investigation note has
SHA-256 `e47d232c81c2583ceee649647217656eb0d454a6c2f931484a754fa2fb631b85`.
## English wide default walk and resource-guard correction

The ninth first-pass configuration, Codex en/1280x800 default, ended naturally with exit 0. External
`codex-en-1280x800-default/run-01` retains 21 images and 28 verified artifact digests. A fresh read-only reviewer
verified every digest, the eight ordered identity stages, unchanged clean A/C sources and the natural receipt,
and inspected seven key images. First run reaches the disconnected main screen; Catalog shows its full automatic-use
explanation and returns to the uninstalled list. The Chat reply is complete with Done and Nothing running; Settings
shows General / Language / English. Projects states Add a folder and Faber works inside it, so purpose discovery
succeeded even though the reviewer continued to the native picker and declared itself stuck. The page-only capture
cannot establish the result of that external dialog. This is controlled replay, not a real-provider result.

Investigation after this walk found an important limit in the execution setup. `runChild` reserves build storage
and isolates temporary directories; it does not itself enforce the 8 GB free-memory/full-screen-app gate.
`first-user.mjs` directly builds the driver, and `ux_audit` creates its sandbox without taking the runner's build slot.
`Ctx::guard_desktop` enforces hidden windows, a separate property. The eighth and ninth collector confirmed this
call path; the seventh used the same setup. These walks therefore do not prove those resource guards were applied.
Any earlier coordination note describing them as normally resource-guarded is corrected by this investigation.
Their retained UI observations, source identities and natural receipts remain evidence of those narrower facts.
Native walks stopped at that point until the resource-guard path was resolved as described below; the remaining
matrix and second pass are unfinished.
## English wide project walk and guarded restart

The tenth first-pass configuration, Codex en/1280x800 with seeded Demo, ended naturally with exit 0. The raw report
is preserved verbatim in `first-user-2026-10-24-codex-en-1280x800-project.md`. External
`codex-en-1280x800-project/run-01` retains nine PNGs and one execution-identity file, not ten images: the collector's
`screenshotCount: 10` counts all copied files. A fresh independent reviewer checked images 5 through 9 and verified
the sizes/digests of all fifteen manifest artifacts plus the external full log. The remaining four images were not
independently inspected. Image 5 shows a visible empty message field with its cursor; the two redundant clicks before
typing do not establish a hidden field or a focus defect. Images 7 through 9 establish the exact question, send, then
the complete Rayleigh reply with Done and Nothing running. This is controlled replay, not a real-provider result.

A reviewed external adapter wrapped this run in the existing shared resource gate and held its slot through the
child's lifetime; the normal storage lease wrapped the adapter. Its guard hash matched the reviewed guard manifest and compiled identities.
`RESOURCE_GATE_ACQUIRED` preceded the walk, its single capture directory was retained without overwriting another
run, a lease receipt was captured, and no watcher error was recorded. App, driver, fake CLI and adapter file hashes
stayed equal at the recorded before/after boundaries; A/C sources also stayed unchanged and clean. These file
observations do not attest to loaded process bytes. The gate checks full-screen use and free RAM at startup, with
the existing wait policy; it does not continuously suspend for low RAM after admission. The adapter is an external
operational remedy, not a claim that direct `first-user.mjs` calls enforce that gate. The native-picker screenshot
limitation above remains unchanged. The completed first-pass follow-up and pending second pass are recorded in the [follow-up evidence](first-user-2026-10-24-follow-up.md).
