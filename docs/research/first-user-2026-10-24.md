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
That attempt does not prove onboarding. The complete matrix and confirmation of its remaining screenshots are unfinished.
The harness correction is saved and sent for integration; the required landing command failed at the GitHub API
connection, before queueing. The connector could open the review request, but does not replace that landing command.

## Follow-up walks and classification

The matrix is still incomplete: two of its sixteen requested default or project walks completed, both Haiku in
pt-BR at 720x480. The focused Codex Chat smoke above is additional evidence, not a completed default configuration.

| Walk | Program evidence | What it establishes and what remains |
| :--- | :--- | :--- |
| Codex, focused first run, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-tPs6KS.md`; five images retained | Image 3 shows the main disconnected screen. The reviewer nevertheless declared itself stuck looking for another continuation step. The goal was reached visually; its confusion remains recorded, not relabeled as a flawless UX pass. |
| Haiku, default walk, pt-BR, 720x480 | Exit 0; `first-user-2026-10-24-claude-pt-BR-720x480-default.md`; report and images copied with hashes | First run, Catalog, Chat and Settings were declared complete; the core Chat answer was visible. Projects was declared stuck with the narrow navigation drawer still open. The complete findings remain in the generated report. |
| Haiku, focused project, same language/size, retry | Exit 0; `first-user-2026-10-24-claude-pt-BR-720x480-project.md`; nine images, source/copy report hashes equal | Final image shows a reply and Done in Demo, with the start of the reply above the viewport. This proves a completed recorded turn, not that the complete answer was inspected or real-provider compatibility. |

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

Remaining work: both reviewers at both languages and sizes, retained screenshot verification, confirmed findings fixed
with regression checks, and a second pass without blocking findings. Task 996 remains open; no stable-release claim.
