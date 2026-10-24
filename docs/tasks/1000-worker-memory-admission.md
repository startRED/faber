# Parallel workers wait when their RAM budget is full

Selected by the lead from the direction check of 2026-10-24, after the owner's queued tasks.

Extend automatic helper and agent-started conversation admission with measured CLI-tree memory and available system memory. Count each owned process identity once, including the root; shared Codex threads count their server once. Preserve existing quota, permission, install and cancellation checks. Hold new automatic starts when capacity is unavailable; release capacity resumes them. Stop removes waiting starts. Do not kill active work or claim a hard limit on CLI-controlled descendants or process growth.

Before production changes, record a failing regression for pressure, release and cancellation. Use fake process readings for unit checks; no real processes in unit tests. Obtain an independent review and run all required proofs on this exact lane. Real parallel memory/processor measurements require the shared native-test window to be released by its owner; report them as unverified until then.

The macOS follow-up uses resident bytes with exact creation-time checks before and after each libproc query. Linux reads approximate resident pages from `/proc/<pid>/statm`, multiplies by the native page size with checked arithmetic, and checks the owned process's exact start-time identity before and after reading. Both can count shared pages per process. Linux parser and admission regressions use fake readings on Windows; compilation checks do not prove real Linux runtime behavior. Real macOS/Linux runtime sampling and parallel RAM/processor measurements remain unfinished acceptance items; do not close this task until they are verified.
