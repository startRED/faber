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

The focused Chat walk is being run first with the Codex newcomer reviewer, pt-BR, 720x480, on the hidden isolated
desktop. Its result and screenshots are not yet evidence of success. The complete matrix and human inspection of the
resulting screenshots remain unfinished.
