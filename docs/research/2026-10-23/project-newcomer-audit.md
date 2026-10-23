# Prepared project conversation and Settings newcomer probes

Date: 2026-10-23. Partial evidence for task 996, not completion of its full matrix or stable-version qualification.

## Controlled boundary

The default first-user driver has no AI and cannot use the native folder picker. Its Projects task only visits the
area. The opt-in `--task conversa-projeto` now prepares the existing sandboxed Demo git fixture and enables the fake
Claude chat recording. The reviewer sees screenshots and accessible controls, selects Haiku manually, and sends the
recording's exact question: `In one sentence: why is the sky blue?`. This tests finding an already prepared project,
selecting its model, sending and following a recorded answer. It does not test project creation, edits, checks, Auto
routing or a live provider inside Faber. Reviewer calls still consume subscription quota.

The test app was rebuilt before the first project walk. Its executable SHA-256 was
`D5C87B5F37661A7DFB03F324250C4F1702EC4E9F6E7914FBB7AA9603E440594E` before and after the corrected Portuguese run.
Later `--no-build` walks rebuild the driver but reuse this app; they are not claims that unrelated app source changes
were rebuilt. Raw reports, logs and screenshot folders below are local, outside the repository.

## Results checked independently

| Probe | Evidence | What it establishes |
| :--- | :--- | :--- |
| Settings, Haiku, 720x480, pt-BR | `settings-newcomer-haiku-720-pt-20261023.md`; folder `2026-10-23-aSJ0jF`, images 1–5 | Passed first run without an AI, opened the sidebar and Settings, found General > Language. It did not change language. |
| Project, Haiku, requested pt-BR, actual English | `project-newcomer-haiku-720-pt-20261023.md`; folder `2026-10-23-r41UAg`, images 1–11 | Failed preparation: settings were written in English before the requested language. Never count this as Portuguese evidence. The raw report remains unchanged. |
| Language negative control | `project-language-negative-20261023.log`; folder `2026-10-23-P9e8pI` | The new document-language guard failed with `en, expected pt-BR` before a reviewer call, saving `language-mismatch.png`. |
| Project, Haiku, corrected pt-BR, 720x480 | `project-newcomer-haiku-720-pt-corrected-20261023.md`; folder `2026-10-23-dNxzpR`, images 1–12 | Demo opened, Haiku selected, exact question sent, recorded answer ended with Ready and Nothing running. The last image shows only the tail of the answer, not the entire answer at once. |
| Project, Codex, English, 720x480 | `project-newcomer-codex-720-en-20261023.md`; folder `2026-10-23-TBXgmv`, images 1–6 | Stuck after entering Projects: repeatedly chose Add project folder rather than reopening the sidebar to find Demo. No project message was sent. Native folder selection is outside this driver's boundary, so the lack of a picker is not a diagnosed app failure. |

Reports and logs live in `E:/Temp/faber-tools/`. Screenshot folders live under
`E:/Temp/faber-target/catalog-update/faber-test/first-user/`. A fresh read-only reviewer inspected the Settings images,
the corrected project images and the setup code, and found no blocking issue in the test preparation. The language
fix writes the requested settings before the existing fixture's default settings; `Ctx::write_settings` deliberately
does not overwrite a file already created. The driver also rejects a document-language mismatch before asking its
reviewer, instead of saving a mislabeled success.

## Findings kept as evidence, not assumed diagnoses

- At 720 px, entering Projects closes the sidebar. The main screen offers Add project folder, and Demo is found by
  opening the sidebar again. The reviewer continued, but reported uncertainty. The Portuguese walk also clicked Add
  project folder after reusing an earlier control number while describing another target. Control numbers belong to
  each fresh screen, so this observation also involves the reviewer's interpretation.
  The independent Codex walk did not discover the saved project at all. This strengthens the navigation finding,
  while preserving the distinction between not finding Demo and the driver's inability to control a native picker.
- After sending, the header shows Projects, the conversation title and the separate copy's name; it does not show
  Demo. The reviewer could not confirm the project name from that final screen alone. This is not evidence that the
  message went to the wrong project.
- Settings was reachable without connecting an AI. The reviewer's early statements that Settings was absent were
  made during onboarding or with the sidebar closed; the images do not establish a missing control.

## Proof and remaining scope

The two new focused Node cases failed before the task/report changes, then all 16 focused cases passed, including
the earlier native-selector regressions. The example compiled. The initial full local proof passed all seven checks;
the final proof and independent review receipts must accompany the saved change. No new distributed
app work is added: the new fixture starts git only inside a test sandbox. Its incremental CPU and memory were not
measured. No live provider, native folder selection, complete language/size/vendor matrix, or broad stable release is
qualified here.
