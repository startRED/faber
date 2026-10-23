# Catalog newcomer audit, 2026-10-23

Status: partial evidence, not task 996 completion or stable-version approval.

## What ran

The existing native hidden test build, with a fresh sandbox and no AI connected inside Faber, ran a focused Catalog
walk. The external reviewer saw screenshots and numbered reachable controls, had no shell/apps/browser tools, and
was asked to find a debugging tool, read its purpose and use explanation, then return without installing or changing
anything. Both processes exited 0; the driver requires sandbox cleanup before accepting that exit.

| Reviewer | Window and language | Observed result |
| :--- | :--- | :--- |
| Codex gpt-6-luna | 720x480, pt-BR | Opened Catalog, opened Systematic debugging and returned. Purpose was visible; the use explanation was below the viewport. Reading that explanation is not proved. |
| Claude Haiku | 1280x800, English | Opened Catalog, opened Systematic debugging twice and returned. Purpose and the use explanation were visible and legible in the detail screenshot. |

The smaller walk also went back and forth between subscription and appearance onboarding before advancing. The
larger walk reported uncertainty about whether Catalog needed an AI connection, then accessed it successfully.
These observations need confirmation before changing product copy or layout; they do not establish an interface bug.

Fresh independent read-only review inspected the screenshots and confirmed the bounded findings above. Neither walk
tests installing, removing, on/off, plugin/server execution, a Chat response or a project conversation. The full
two-reviewer, two-language, two-size matrix remains incomplete.

## A false source of confidence fixed

The first walk received `document.body.innerText`, including paragraphs below the viewport, alongside its screenshot.
It claimed to have read the use explanation without scrolling. That walk is retained as evidence of the observer's
flaw, not as proof a newcomer read the page. The observer now sends the screenshot and reachable control names only;
the reviewer reads page text from the image. A regression check rejects an offscreen-text sentinel in the prompt.

The report also used to label the reviewer's `done` as success. It now says completion was declared by the AI and
requires inspecting the images. This is deliberately not an automatic acceptance check. The two corrected observer
walks above ran before this final report-label wording changed; their raw reports retain the earlier label and must
be read with the limitations recorded here.

Catalog is now in the default task list. `--task catalogo` permits a focused walk; missing and unknown selections fail
before creating output folders or launching anything. The prompt also names the actual interface language instead
of always claiming Portuguese. Each focused run still begins with the normal first-run screen.

## Local evidence

Artifacts are local, under `E:/Temp/faber-tools/`:

- Initial flawed observer: `catalog-newcomer-720.md` and `.log`.
- Screenshot-only walks: `catalog-newcomer-720-screen-only.md` and `catalog-newcomer-1280-en-screen-only.md`, with matching logs.
- Screenshots: `E:/Temp/faber-target/catalog-update/faber-test/first-user/2026-10-23-mcflGU/` and `2026-10-23-n2qSqD/`.
- Focused script checks: 11 passed, 0 failed. Missing selection, language, offscreen-text and declared-completion checks
  were observed failing before their respective fixes.

This changes development review tooling and documentation only, with no shipped app memory/CPU change. It does not
measure idle app resources or resolve the separate retained-memory failure. Read-only instructions to the reviewer
are not a technical action sandbox; the observed action traces contain browsing and navigation, not installation.
