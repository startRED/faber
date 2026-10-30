# Make the Keep going lines say what they measure and what comes next

Area: Keep going
Files: ui/Thread.tsx, ui/foot.ts, ui/Details.tsx, ui/Running.tsx, engine/src/engine/keep_going.rs, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/foot.check.ts, tests/ui/keepgoing.check.ts, engine/tests/keep_going.rs, docs/design/keep-going.md
Depends: none
Size: about 300 changed lines

Why: the live look of 2026-10-29 (`docs/research/2026-10-29-faber-diagnosis/README.md`, "Live look"). Two faults in the
same screen, both found in the code.

- "Enquanto você estava fora (2 h)" (`away.title`, `AwayLine` in `ui/Thread.tsx` near line 910) measures the time since
  Continue-alone was turned on: `away_of` in `engine/src/engine/keep_going.rs` (near line 209) uses `KeepGoing.since`
  (`engine/src/model.rs` line 499) and `AWAY_MS` (30 min). It is not time away. It sits next to the folded run's own
  "1 h · 7 turnos" (`run.block.title`, `ui/Run.tsx` line 185), which measures the first prompt to the last reply. Two
  hours and one hour for the same night, with different turn counts. Say what it measures: "Continuar sozinho: ligado
  há 2 h, 7 turnos de trabalho", and keep one of the two numbers (this brief proposes the engine's, with the run block
  showing only its turns).
- Between two turns, with Continue-alone on, the foot said "Pronto · atualizado há 8 min" (`footText`, `ui/foot.ts`) over
  "Procurando alterações pra verificar" (`details.weighing`, `ui/Details.tsx` line 105 in `DetailsLine`; `checks.weighing`
  in `ui/Thread.tsx`), and nothing visible changed in 18 s. Faber was checking the files before the next turn
  (`engine/src/engine/verifying.rs`) or resting (`KeepWait` with `why` and `until`, shown in the running row by
  `run.kg.wait.*`, `ui/Running.tsx` line 110). While the conversation is weighing, checking or resting, the foot must not
  say "Pronto": say "Verificando antes de continuar" or "Próximo passo às 14:32" when `keepWait.until` is known (the
  running item already carries it, `ui/api.ts` line 546).

What to build: the two sentences above, in the five languages by hand; `footText` (`ui/foot.ts`) does not know `weighing` or `keepWait` today, though `Answer` receives `props.weighing`; pass
them in, and the foot is not "done" while Faber still has a step to take.

Leave out: the single live state (tasks 121 to 123 move these lines into details), the engine's pacing of rests.

Done: screen checks in `tests/ui/foot.check.ts` and `tests/ui/keepgoing.check.ts` fail today by showing "Pronto" while
`weighing` is true and by naming two different durations; an engine test in `engine/tests/keep_going.rs` pins the
`Away` fields. Extends: product-map "Keep going". Update `docs/design/keep-going.md`. What's-new entry in every
language. Walk it at 720 px and in the longest language. No memory or processor cost. Review by the other vendor (R146)
and the coherence review.
