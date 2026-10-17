# Writing a what's-new entry

How to write the line a person reads on the "what's new" screen after an update (R142) and in the release body. Where the
entry goes and how it gets its id is in [workflow.md](workflow.md#numbers); this page is only about the words.

A person reads the screen once, in a few seconds, between an update and their work. Each line has to answer two
questions on its own: what changed for me, and what do I do differently now (often: nothing).

## The rules

1. **Lead with what the person gets, then where it shows.** "You can see which sites the agent opened" before "under the
   status line". A sentence that only says where a thing appears ("A subagent shows on the right…") is not done. Most
   people scan, so the first four or five words carry the line: put the thing they know (the agent, a game, Settings)
   there.
2. **Say exactly what stops and what keeps going.** When a change pauses, hides, waits or skips something, name both
   sides, because a reader fills the gap with the worst guess. "Faber goes still while a game is in front" was read as
   "the agent stops working". Write: "The agent keeps working; only the screen stops moving."
3. **A number when there is one, and only a measured one.** A `perf` change, or any change that lowers memory, processor
   or waiting, says by how much, from the measurement in its commit's Cost section or a CI run: "uses about 40 MB less
   with a long conversation open". No measurement, no number, and no "faster" or "lighter" either: name the symptom that
   went away ("long answers no longer stutter while they scroll", "stops redrawing the screen") and let the reader judge.
4. **A change of habit says before and after.** When something a person relied on now works differently, the line says
   what it did and what it does now. A removal, or a thing that now needs a step it did not, is said plainly and never
   hidden in a `better` line.
5. **The screen's own words, nothing else.** Use only words a person sees in Faber (`docs/product-map.md`, the locale
   files). No file names, commands, setting keys, R-numbers, "IPC", "CLI", "render", "repaint", "DOM", "fixture". When the
   screen's word is new, say once what it is in passing: "a helper (a second AI the agent starts for one part)".
6. **One idea per line, under 25 words.** Two facts that a reader needs together stay in one sentence; two unrelated
   ones are two entries. A line that needs "and also" twice is two entries.
7. **Changes to one thing become one line.** Entries waiting in the same release that touch the same screen or habit
   (three additions to the status line) are folded into one when the release is cut, keeping the newest id.
8. **"You", present tense, active.** "You can write to a helper while it works", not "Helpers can now be written to".
   "Now" only when the old behaviour was something the person relied on.
9. **A fix says what went wrong, in the person's terms.** "Answers no longer stop at 'I'll wait' when helpers run in the
   background", never "Fixed a bug in…" and never "bug fixes and improvements".
10. **Behind the scenes stays out**, unless a person feels it: faster CI, refactors and test changes have no entry. "Updates
   now arrive more often" is fine once it is true.
11. **Plain and calm.** No exclamation marks, no "exciting", "powerful", "seamless", "we're thrilled", no emoji, no dashes
    (`—`, `–`); a colon or a full stop instead. No "not X but Y" that answers a doubt nobody had.
12. **Each language is written, not translated.** The same facts in the words a native speaker would use; a sentence may
    change shape between languages. Rule 2's "what keeps going" must survive in every one.

## Kinds

| Kind | For | Example |
| :--- | :--- | :--- |
| `new` | Something a person can do that they could not before | You can write to a helper while it works. |
| `better` | Something they already did, now easier, clearer or lighter | While the agent reads the web, you see the sites it opened. |
| `fixed` | Something that went wrong and no longer does | Answers no longer stop at "I'll wait" when helpers run in the background. |

## Before and after

| Before | After |
| :--- | :--- |
| Faber goes still while a game is in front: nothing moves or repaints, and what happened meanwhile shows when you come back. | Playing a game? The agent keeps working, but Faber's window stops moving and redrawing until you come back, and then shows what happened. |
| A subagent shows on the right what it did: the files it changed, the checks it ran and any file outside the request. | When a helper finishes, you see on the right the files it changed and the tests it ran, and any file it touched that you did not ask for. |
| Agent clicks and types on pages; logins and payments wait for you. | The agent can now click and fill in the pages it opens. It never signs in or pays: it stops and waits for you. |

## Check before you commit

Read the line as someone who has used Faber for a week and has never seen this pull request:

- Can they say what is different for them, without guessing?
- If something stops, waits or hides, do they know what keeps going?
- Is every number from a measurement named in the commit?
- Is every word one they have seen on Faber's screen?

## Where these come from

Published changelogs read in October 2026: Linear and GitHub name the effect a person sees and the symptom a fix
removed; Stripe and Slack say plainly what was removed and how output changed before and after; Chrome gives memory
figures only when it measured them. Usability research on how people read the web (Nielsen Norman Group) is why the first
words matter most.
