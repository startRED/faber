# UX branch review verification

Review target: `55d593a`. The external report lists sixteen findings, although the request describes fifteen.
Reproductions run in Faber's hidden Windows test build; no real CLI, account files or visible desktop is used.

| Finding | Verdict and evidence |
| --- | --- |
| 1 | Real. `review 1` failed because the first answer block disappeared. Text-block boundaries are no longer answer boundaries. The engine records the first explicit final phase, or the last tool's text offset when phases are unavailable; every subsequent answer block stays prominent. Old records, unknown offsets and tool-terminated turns keep their whole text. |
| 6 | Real. `review 6` failed on missing `role=status`. The current activity is now a polite atomic live region; only the answer text has `aria-busy`. |
| 7 | Real. `review 7` copied only “All done.” instead of the explanation and code. Copy now writes the complete original message. |
| 14 | False as stated. The existing engine fixture test already exercises real Break/Text events, non-BMP UTF-16 offsets and persistence, and the existing screen check covers absent offsets. Added replay cases cover multi-block final phases, a tool after an explicit final phase, empty terminal blocks and tool-terminated turns. |
| 15 | Real. `review 15` failed with “1 steps · 1 commands”. Separate `activity.steps` and `activity.commands` use the existing plural helper in all five hand-written locales. |

The initial hidden UI run reproduced findings 1, 6, 7 and 15 before their fixes. The added engine replay failed on missing boundary metadata before implementation.
