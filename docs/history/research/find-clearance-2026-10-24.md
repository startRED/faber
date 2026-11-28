# Find results clear of their bar

Status: local production correction checked in native paired observations and seven lightweight commands passed.
Independent image review passed; the final unit rendered-range guard run and integration remain unfinished.
This does not finish task 997.

## Reproduction and cause

The original 720x480 palette-and-Find capture passed its DOM, focus, highlight-release and cleanup assertions, but
independent inspection found a match partly covered by the Find bar. A later normal-speed diagnostic retained the
actual current CSS Highlight range, every client rectangle, the bar and thread rectangles, scroll position and exit
frames. Both runs use a synthetic conversation, not a live provider request.

After the query settled, the current range occupied y124..144 while the bar occupied y98..138: 14 px intersected.
The thread occupied y88..372 and had reached scrollTop 0. The existing centering calculation clamped a negative
destination to zero, leaving no room to bring this first match below the overlay. The immediate query sample did not
intersect; the later settled sample did. This is not an inference from the immediate sample or from slowed playback.

The same normal-speed recording shows Escape changing the count from "1 de 2" to "Nenhum resultado" and widening the
bar from 356.203125 px to 428.203125 px, while it was still leaving. The query still had three characters, but its
highlight ranges had already been released. `hide` cleared `found` immediately while `keepForExit` retained the bar,
so the live count expression rendered an inappropriate no-results state during the exit.

Fresh read-only reviewer `/root/observation_evidence_review` inspected all four diagnostic images and records and
confirmed these facts. Source and executable identities matched before/after; exit was natural with code 0 and CLEAN.
Evidence: `E:/Documentos/ProjetosIA/faber-evidence/palette-find-diagnosis-20261024/`, launch receipts `1704c7`/`925d31`.
The same diagnostic found a separate duplicate palette exit; that correction belongs to its own worktree and proof.

## Correction

While a Find bar is present, including its exit, the thread reserves one existing `--h-lg` control height above its
normal top padding. This keeps the first match clear when scrolling cannot move farther upward. The existing bar,
buttons, shortcuts, query, matching, navigation and centering calculation remain shared by Chat and Projects.

Closing retains only the displayed count string until removal. It still releases the actual text ranges and paints
immediately, so a fading bar keeps its text and width without holding on to message nodes. No setting, command,
dependency, listener, observer or timer is added; one small reactive count string and a conditional CSS rule are added.

## Executable evidence and limits

Two new native checks failed before changing production code: the first highlighted match intersected the bar, and
the exiting count changed from "1 of 4" to "No matches" (receipt `1d1405`, 0 passed, 2 failed). The negative log and
source patch were copied before the next run into `E:/Documentos/ProjetosIA/faber-evidence/find-clearance-20261024/`.

After correction both checks passed (receipt `a50099`, 2 passed, 0 failed; 27.8 s total, 13.3 s build, 1.6 s UI).
These are proof durations, not an app speed improvement. The visibility check was then strengthened to require a
nonempty rendered range before checking intersections; the final run of that guard is still pending.

Fresh read-only code reviewer `/root/find_clearance_code_review` (`gpt-6.1-sol`) found no code blocker and suggested
that rendered-range guard. Its report is retained beside the negative log. Normal-speed performance, long-language
layout, forced colors, screen-reader behavior and the complete motion inventory are not established by these two checks.

## Paired native observation

A fresh independent preparation review preceded the two native runs under the normal shared lease and resource guard.
The baseline (`9af4a5`/`77982c`) and corrected app (`98dddb`/`cb4c2a`) each retained nine images across 720x480,
1280x800 and 1920x1080, Portuguese, full motion and explicit normal playback. They use the same synthetic text chat,
query and keyboard flow. The corrected executable and fake CLI were copied exclusively into a distinct runtime folder
inside the existing third cache after acquiring its lease and guard. Source and copied hashes matched; no original
cache executable, stale reservation or product source changed during capture. Both runs ended naturally with code 0
and CLEAN, and captured source/executable identities matched before and after.

At 720 px and 1280 px the original current range occupied y124..144 and intersected the bar by 14 px. The corrected range occupied
y164..184 and did not intersect. All three corrected widths required a real positive rendered range, one current match
among two, and zero area of intersection for every current-range rectangle. These are separate runtime assertions;
the final strengthened unit-test source was not recompiled or rerun by this capture.

At all three original widths, leaving samples changed from "1 de 2" at width 356.203125 px to "Nenhum resultado" at
428.203125 px. All corrected leaving samples retained "1 de 2" and 356.203125 px. Each run observed actual leaving
elements, immediate absence of both highlight keys, inert pointer-disabled exit and restoration of the exact same
empty composer. The corrected collectors retained 43, 45 and 47 frames. Screenshots are approximate sequential samples,
not proof of an exact animation percentage; the frame/event records establish the transient count and width behavior.

Raw process-tree memory in MiB before opening to fully closed was 166.83 to 173.60, 173.82 to 181.31 and 183.10 to
192.73 for the baseline; 166.05 to 172.32, 178.24 to 182.71 and 188.86 to 198.37 for the correction, in increasing width
order. Cumulative CPU readings and timestamps remain in the records. Capture/observer work is included; these samples
establish no memory or processor saving, leak conclusion, normal-speed improvement or prolonged idle-budget guarantee.

Fresh read-only reviewer `/root/observation_evidence_review` inspected all eighteen images and the records and found
no blocker in the observed scope. Original settled-open scores were 39/40, 39/40 and 40/40, and corrected scores were
40/40 at each width; fully closed states scored 40/40 throughout. Transient baseline exit defects remain explicit and
are not described as clean endpoints. The parent also inspected the corrected 720 px open state.

Evidence: `E:/Documentos/ProjetosIA/faber-evidence/find-clearance-visual-proof-20261024/`. All seven lightweight commands
passed; exact receipts are retained in `find-clearance-20261024/lightproof-receipts.json`. The final unit guard run and
integration remain pending. The earlier direct native checks did not establish the Node wrapper's storage lease.
