# Task 13: offscreen task rendering stays within the memory bar

The required Windows suite first failed only the 200-task Projects memory
scenario: **271.0 MB**, above the unchanged 250 MB ceiling. The other 1,391
checks passed, including owned process-tree retirement and full 300-second
scripted update probation. This failure was retained rather than averaged away.

Running the same Projects scenario alone reproduced the failure at
**282.8 MB**. The board kept all 200 cards participating in rendering while
cycling six looks and five widths, including offscreen cards. Native CSS
`content-visibility: auto` now defers their rendering, with an intrinsic block
size until a card is measured. The DOM remains bounded at 200 tasks and native
focus/search can reveal offscreen content; no JavaScript virtual list or
dependency was added.

The unchanged scenario then passed at **213.0 MB**, including scrolling to the
last card, opening its lazily rendered evidence, answering a decision and
reloading the saved state. This is a **69.8 MB** reduction against the isolated
reproduction, not a real mission RAM measurement. The limit remains 250 MB.

The full seven-command rerun then passed: **1,392 tests, zero failures, zero
not run and zero skipped**. Projects peaked at **215.3 MB** in that run.

Private evidence root:
`C:/Users/Erick/AppData/Local/Faber/t13-exposure-20261014/`.

| Evidence | Result |
| --- | --- |
| `validation-r120-bounds/proof-07.log` | 1,391 pass, one memory failure at 271.0 MB |
| `r120-projects-alone.log` | Reproduced at 282.8 MB |
| `r120-projects-contained.log` | Same scenario passes at 213.0 MB |
| `validation-r120-memory/proofs.json` | Full required proof rerun; completed exit codes are authoritative |

The real mission/update lifecycle remains separate and unproved. No owner app
was stopped, no visible test window was opened, and the screenshots and raw
failed observations remain private.
