# Evaluations

What the AI writes, measured with the real CLIs.


Tests replay recorded CLIs, so they prove what Faber sends and draws, never what a model writes back. Two opt-in
evaluations ask the real CLIs instead, with the person's own sign-in, and never run in `cargo t` or CI: Auto's router
(`engine/examples/route_eval.rs`) and the words Faber gives both CLIs (`node scripts/persona-eval.mjs`, task 130, an
idea from T3 Code's `evals/`). The second runs synthetic cases the way the owner writes: one per weak spot the
2026-10-29 diagnosis found (English helper titles on a Portuguese screen, technical words let in by a pasted log) or
the owner's language rule (a Portuguese screen and a message of only a pasted
English log gets Portuguese), one for the answer's shape and normal-use controls that both versions must pass (among
them an English screen with a person writing Portuguese, and the reverse: the person's words win). A case with a
screen language gets the words Faber builds for that screen. Silence about what still runs has no case: `claude -p` waits for a
background command before it answers, and one `codex exec` turn cannot start one, so no single headless turn shows it. Each
case runs `--samples` times per CLI with the old words (`scripts/persona-eval/persona-v8.json`, frozen) and the new
ones (`engine/examples/persona_text.rs`), on the same model, each in a fresh empty folder; Codex runs the cases one
`codex exec` turn can hold. The grader is plain functions with unit tests (`scripts/persona-eval.test.mjs`, in
`npm run test:scripts`); the run prints passes per case and side and the cost (Claude Code's figure, both CLIs' tokens),
and keeps every answer in `target/persona-eval/`. Claude Code adds the signed-in account's email to every run, and a name in it can pull an answer
toward another language with either version, so compare the two sides, never one side's count alone. A change to the persona runs it before landing and names the result.
