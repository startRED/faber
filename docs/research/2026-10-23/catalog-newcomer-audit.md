# Catalog newcomer audit, 2026-10-23

Status: partial evidence, not task 996 completion or stable-version approval.

## What ran

The existing native hidden test build, with a fresh sandbox and no AI connected inside Faber, ran a focused Catalog
walk. The external reviewer saw screenshots and numbered reachable controls, had no shell/apps/browser tools, and
was asked to find a debugging tool, read its purpose and use explanation, then return without installing or changing
anything. All three processes exited 0; the driver requires sandbox cleanup before accepting that exit.

| Reviewer | Window and language | Observed result |
| :--- | :--- | :--- |
| Codex gpt-6-luna | 720x480, pt-BR | Opened Catalog, opened Systematic debugging and returned. Purpose was visible; the use explanation was below the viewport. Reading that explanation is not proved. |
| Claude Haiku | 1280x800, English | Opened Catalog, opened Systematic debugging twice and returned. Purpose and the use explanation were visible and legible in the detail screenshot. |
| Codex gpt-6-luna | 1280x800, pt-BR | Found Investigação de erros in the starting list, read its purpose and conditional agent-use explanation, and returned. Fresh image review confirmed these three states and that it remained uninstalled. This did not exercise text search. |

The smaller walk also went back and forth between subscription and appearance onboarding before advancing. The
larger walk reported uncertainty about whether Catalog needed an AI connection, then accessed it successfully.
These observations need confirmation before changing product copy or layout; they do not establish an interface bug.

Fresh independent read-only review inspected the screenshots and confirmed the bounded findings above. None of these walks
tests installing, removing, on/off, plugin/server execution, a Chat response or a project conversation. The full
two-reviewer, two-language, two-size matrix was incomplete at this initial stage; the subsequent Catalog-only round is below.

## A false source of confidence fixed

The first walk received `document.body.innerText`, including paragraphs below the viewport, alongside its screenshot.
It claimed to have read the use explanation without scrolling. That walk is retained as evidence of the observer's
flaw, not as proof a newcomer read the page. The observer now sends the screenshot and reachable control names only;
the reviewer reads page text from the image. A regression check rejects an offscreen-text sentinel in the prompt.

The report also used to label the reviewer's `done` as success. It now says completion was declared by the AI and
requires inspecting the images. This is deliberately not an automatic acceptance check. The original 720px Portuguese
and 1280px English observer walks ran before this report-label wording changed; their raw reports retain the earlier label and must
be read with the limitations recorded here.

Catalog is now in the default task list. `--task catalogo` permits a focused walk; missing and unknown selections fail
before creating output folders or launching anything. The prompt also names the actual interface language instead
of always claiming Portuguese. Each focused run still begins with the normal first-run screen.

## Local evidence

Artifacts are local, under `E:/Temp/faber-tools/`:

- Initial flawed observer: `catalog-newcomer-720.md` and `.log`.
- Screenshot-only walks: `catalog-newcomer-720-screen-only.md` and `catalog-newcomer-1280-en-screen-only.md`, with matching logs.
- Screenshots: `E:/Temp/faber-target/catalog-update/faber-test/first-user/2026-10-23-mcflGU/` and `2026-10-23-n2qSqD/`.
- Additional Portuguese wide-window walk: `catalog-newcomer-1280-pt-screen-only.md` and matching log; screenshots
  `E:/Temp/faber-target/catalog-update/faber-test/first-user/2026-10-23-4PojXR/`, independently inspected at steps 4–6.
- Focused script checks: 11 passed, 0 failed. Missing selection, language, offscreen-text and declared-completion checks
  were observed failing before their respective fixes.

This changes development review tooling and documentation only, with no shipped app memory/CPU change. It does not
measure idle app resources or resolve the separate retained-memory failure. Read-only instructions to the reviewer
are not a technical action sandbox; the observed action traces contain browsing and navigation, not installation.

## Native choices in the review controller

The controlled conversation walk exposed a controller limitation: a native selector opened correctly in the image,
but numbered controls did not provide an action to choose its options. The controller now exposes accessible option
labels for reachable native selectors and validates an exact, unique enabled choice. It rechecks the same connected,
reachable element, option value and disabled state before dispatching the existing input/change behavior. Model text
is JSON-serialized, never evaluated as code. Fourteen focused checks passed, including a regression observed failing
before implementation and execution checks for stale, disabled and changed choices. A fresh read-only review found
no blocking issue. The execution visibility check does not recheck opacity changes after observation.

The first focused implementation required DOM focus. A hidden native replay refuted that assumption: the image
showed the open menu and Claude Haiku, but the observed selector did not own DOM focus. Its correct selection was
rejected. Those failed runs remain in `connected-chat-probe/select-report.md` and `select-diagnostic-report.md`.
Options now use accessible metadata without that focus assumption. `select-accessible-report.md` and its raw log
show the reviewer choosing Claude Haiku, typing and sending. The programmatic selection can leave an already-open
native popup visible until the next ordinary click; this is a controller artifact, not evidence of a product bug.

That conversation did not produce a response because its arbitrary question differed from the recorded input.
`testkit/src/replay.rs` advances only on matching input, and `claude/chat.jsonl` expects the exact sky-blue question.
This result does not establish a provider or app failure. Each probe keeps a separate preparation recipe, source and
probe digests, log, report and screenshot folder. No real provider account was connected inside the app.

A final controlled replay, `connected-chat-probe/select-recorded-report.md`, asked for the exact recorded question.
The reviewer selected Claude Haiku, typed the sky-blue question and sent it. Screenshot
`E:/Temp/faber-target/catalog-update/faber-test/first-user/2026-10-23-Gb1UQs/conversa-5.png` shows the complete recorded
answer, Ready and nothing running. This proves the bounded manual-model conversation path with the existing fake
recording, not Auto routing, real-provider compatibility or a current rebuilt app binary. App sources were unchanged.

## Subsequent Catalog-only matrix

Each of the eight reviewer/language/window combinations has now been attempted with screenshot-only page prose.
These are bounded walks of the existing hidden test build, not eight successful product acceptance results. The
first Codex English wide-window attempt returned an empty reviewer answer at step 3; it is preserved separately and
was retried once. This is a reviewer failure, not evidence of an app failure. All reports remain unedited.

| Reviewer | Window | Language | Observed result |
| :--- | :--- | :--- | :--- |
| Codex | 720x480 | pt-BR | Returned to the list; reading the use explanation was not proved in the initial walk. |
| Haiku | 720x480 | pt-BR | Purpose and use explanation appeared; then looped through the sidebar without returning and tried Alt+Left. |
| Codex | 720x480 | English | Purpose and use explanation appeared; the reviewer failed to scroll fully back to the return control. |
| Haiku | 720x480 | English | Purpose and use explanation appeared; sidebar navigation retained the detail and the reviewer did not return. |
| Codex | 1280x800 | pt-BR | Purpose, conditional use explanation and return to list confirmed by independent image review. |
| Haiku | 1280x800 | pt-BR | Purpose, conditional use explanation and return to list confirmed by independent image review. |
| Codex | 1280x800 | English | Retry purpose, conditional use and return confirmed by image review; initial empty answer remains separate. |
| Haiku | 1280x800 | English | Purpose and conditional use explanation legible, returned to the list in the initial walk. |

The two English small-window reviewers incorrectly reported that the return control did not exist. Their earlier
detail screenshot shows it at the top; after scrolling it is above the viewport. Both also received an image with
the full conditional-use explanation, contradicting the Codex summary that it never appeared. Those raw summaries
are findings to inspect, not facts to reproduce in product documentation.
Fresh independent read-only review inspected all five additional completed walks and the directed diagnostic,
confirming the bounded table outcomes. It also rejected the Portuguese small-window summary's implication that
cut-off text was unavailable: the next image shows the full explanation after scrolling.

A separate deterministic diagnostic, with no AI reviewer, opens the same detail at 720x480, scrolls to the bottom,
checks that Back is not among reachable controls, scrolls fully to the top, checks Back has reappeared, clicks it and
checks the card is back in the list. It passed and exited 0. This demonstrates that scrolling back works in this
test build; the newcomer failures show a discovery problem, not an impossible return. The raw diagnostic reuses a
generic report template: its introductory newcomer wording is inapplicable, as its explicit reviewer label states.

Additional local evidence under `E:/Temp/faber-tools/`: `catalog-newcomer-720-en-screen-only`,
`catalog-newcomer-codex-720-en`, `catalog-newcomer-haiku-1280-pt`, `catalog-newcomer-haiku-720-pt`,
`catalog-newcomer-codex-1280-en` and `catalog-newcomer-codex-1280-en-retry` each have separate `.md` and `.log` files.
`catalog-return-directed-report.md`, `catalog-return-directed.log`, `prepare-catalog-return-probe.mjs` and
`catalog-return-probe-receipt.json` retain the directed recipe and pinned source/probe hashes. Its screenshots are
`2026-10-23-cedSvs` under the existing first-user evidence root. No installation or external navigation was performed.

Task 996 still needs its other screens and conversations, fixes and a repeat that clears confirmed blockers. This
matrix does not prove tool installation, switching, execution, real providers, idle resources or stable qualification.
