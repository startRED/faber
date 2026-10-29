# Helpers and answers in the person's words, proven by an evaluation

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 4. Answers got about ten times plainer after the
persona rule of 2026-10-20, but 1,144 of 1,155 helper titles the owner saw were English (from the CLI's `description`
or the helper `title`, shown untranslated by `ui/worklog.ts`), and the rule has two weak spots in
`engine/src/persona.rs`: technical words are allowed once the person used one (the owner pastes long prompts, so the door
is usually open), and "never write that you are waiting on them" silences the agent when the person needs to know what
waits.

- Persona: every helper or subagent `description`/`title` is short and in the person's language; open the door only for
  words the person used about the same thing, or asked for; replace the waiting ban with "say what is waiting and what
  happens next, once, only when it changed". Answer shape: the conclusion first, short sections, no repeated status.
  Bump `VERSION`.
- Screen: when a title is not in the person's language or is empty, show "Ajudante 1, 2…" and keep the original in
  details; "Dizer algo…" becomes "Falar com a IA principal sobre este ajudante" with an empty composer and a structured
  target (direct helpers keep their own action). "pronto" stays only when the helper's result was checked.
- Evaluation (idea from T3's `evals/`): a small opt-in suite outside CI that runs both CLIs on authored, synthetic cases:
  one case per weak spot that fails with the old persona and passes with the new, plus normal-use controls; several
  samples each against the same model; the grader is a plain function with unit tests. It reports cost.

Extends: product-map entries for the persona and the work log. Review by the other vendor. What's-new entry.
