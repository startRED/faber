# Frequent message interactions: bounded motion

Status: locally checked and independently reviewed; integration and the complete task 997 motion inventory remain
unfinished. This is a narrow correction, not a claim that every animation has been visually reviewed.

## Defect and correction

Message actions translated upward by 4 px and entered over the longer interface duration. Expanding attachments used
the 400 ms arrival duration with an unbounded per-file stagger. Both contradicted the existing Motion rules in
`docs/design/looks.md`: frequent controls only fade within 150 ms; chrome finishes within 300 ms including stagger.

Actions now only fade, using the existing feedback duration with a shared 150 ms ceiling. Attachments retain arrival
motion with shared ceilings of 240 ms for arrival and 60 ms for the entire stagger. Reduced and no motion retain
their existing disabled paths. Chat and Projects share these rules; no new component, dependency, listener, timer,
background process or DOM node was added. Runtime memory and processor cost were not measured locally; their full
budget checks remain required on the integrated change. No claim of zero measured cost is made.

## Executable evidence

The two new hidden native UI checks failed before the CSS correction: an attachment took 400 ms and an action had
`matrix(1, 0, 0, 1, 0, -4)`. The run reported 3 passed and 2 failed (tool session 87487). After correction the same
five checks passed, then passed again after moving the ceilings into shared timing tokens (session 82850: 37.2 s
total, 24.8 s build, 1.7 s UI). They cover 30 attachments, full/reduced/none and pointer/keyboard rule timing.

All seven lightweight repository proof commands passed. Source lint retains five existing warnings; the frontend
build retains its existing large-chunk warning. The final CSS received another lint/build/claims pass. The native
composition scenario passed before (79.6 s, 0 findings) and after (77.5 s, 0 findings), taking eight images per run.
These durations describe the proof runs, not an app performance improvement.

Evidence is retained outside disposable caches at
`E:/Documentos/ProjetosIA/faber-evidence/motion-frequent-input-20261024/`: sixteen PNGs, two screenshot indexes,
negative-control and corrected composition logs, SHA-256 digests and the corrected source/executable identities.

## Independent review

The fresh read-only `gpt-6.1-sol` reviewer `/root/review_motion_frequent` checked code and coherence. It first blocked
literal durations under R171; replacing them with shared timing tokens resolved that finding. No other blocker.
Cards checked: C003, C007, C008 and C011; Chat and Projects share the same controls and CSS.

Visual critique: before 40/40, after 40/40, by the same independent reviewer. It inspected all eight before/after pairs:
Chat home, Project home, Conversation and Settings, each at 1920x1080 and 1280x800. Every screen scored 5/5 on all
eight rubric lines, with no deductions. Cards checked: C005 and C011. Evidence names are
`critique-{chat-home,project-home,conversation,settings}-{1920x1080,1280x800}.png` in `before` and `after`.

The composition images do not expose hovered message actions or expanded attachments. Static images cannot prove
animation timing or reduced-motion behavior; those claims rely on the native checks above. A slowed visual walk of
all moving surfaces and their individual scores is still unfinished under task 997.

## Focused slow capture: attachments and the message bar

The first successful focused capture is retained in the evidence root above under
`attachments-720-full-fixture04/run-04/`: four PNGs, before/after provenance and a detailed receipt. Three earlier
attempts failed and remain retained: missing Git before launch, an incomplete clock sample, then a timeout awaiting
attachment disclosure and a completed response. Independent diagnosis identified an incompatible replay input;
that failed capture did not establish whether submission occurred. None produced successful screenshots or counts
as visual approval.

The successful run used the existing pinned app, driver and fake CLI, without compilation. A detached runtime
worktree at `af853a33` changed only the replay's first input to seven exact 1x1 PNG blocks and the original question,
following the existing attachment-check pattern. This is synthetic content, not a live provider response.
Before/after source and executable identities matched; the runtime fixture was intentionally modified and recorded.

At 720x480, full motion and 0.25x playback, the four images show hidden message actions, revealed hover actions,
attachment expansion in progress and all seven attachments expanded. Submission waited for Send to become enabled,
clicked the composer and pressed Enter; the field cleared. The normal resource gate was acquired. The clock passed
on sample two of the bounded three-sample policy (184 frames, median gap about 5.3 ms); 235 expansion observations
include intermediate opacity and all three newly revealed attachments reaching opacity 1. Exit was natural with
code 0 and the CLEAN marker. Parent tool receipts: launch `476e8d`, identity/self-check/dry-run `84d26c`.

Fresh read-only reviewer `/root/fixture04_review` (`gpt-6.1-sol`) inspected all four images and receipts. It found no
evident visual defect in this scope: the hover bar did not move content and the expanded thumbnails stayed aligned
without visible overlap. Its preparation and result reports are retained verbatim beside the capture folder.
This is a focused observation, not a replacement for the scored composition critique or the full task-997 walk.
The 1x1 images do not cover real-photo quality, decoding load or varied aspect ratios. Slowed playback does not
measure normal-speed smoothness, memory or processor cost. Integration remains unverified.
