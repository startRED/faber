# Derive one state for the work that is running, in one place

Area: Work log
Files: ui/livework.ts, tests/ui/livework.check.ts, ui/api.ts
Depends: none
Size: about 280 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3. Under one running answer six places say "is it
working?" and each decides for itself from the same message: `footText` in `ui/foot.ts` ("Working", "updated 8 min ago"),
the work log summary `activity.working` and the latest-step lines in `ui/Thread.tsx` (`latest()`, `after()`,
`tally-live`), `DetailsLine` in `ui/Details.tsx` (`details.*`, `background.count`), the run block title `run.block.title`
in `ui/Run.tsx` and the away line `AwayLine` in `Thread.tsx`. They disagree (the live look of 2026-10-29 saw "Pronto"
over "Procurando alterações pra verificar"). The next tasks draw one sentence and one folded line, but both need the
same answer to "what is happening now", decided once.

What to build: a pure module `ui/livework.ts` with `liveWork(input)` returning
`{ kind: 'needs' | 'failed' | 'working' | 'done', answer: string, since?: number, what?: string, helpers: { id, title, state, since? }[] }`.
The input is what the screen already has: the newest reply `Message` (`status`, `finishedAt`, `needsYou`, `permissions`,
`error`, `waitUntil`, `now`, `checks`, `follow`, `tools` with `ok`, `started`, `parent`, `sign`), the conversation's
`weighing` flag and `keepWait` from the running item (`ui/api.ts` line 546). Priority, highest first: needs the person
(a question or an approval) > failed or stopped, scoped to its own answer and never read from a newer turn > working >
done. `what` is the newest running step in the person's words from `stepLine`/`toolLine` of `ui/worklog.ts` and never a
shell line (when no plain description is known it is empty and the caller says "A IA está trabalhando"). No rendering,
no timer, no `t()` call: strings are the next task's.

Leave out: any change of what the screen draws (tasks `one-live-sentence` and `folded-line-and-details`).

Done: `tests/ui/livework.check.ts` fails today (the module does not exist) and checks each priority pair, a stopped old
answer under a newer working turn, a running shell step giving an empty `what`, and a helper list ordered by start. The
product-map "Work log" entry names `ui/livework.ts` as the one place that decides. Review: the lints and one fresh
session of the other vendor (R146); no memory or processor cost beyond a function called on each message update.
