# Frontend skills and attachment reselection: review record

Date: 2026-10-22. Worktree: frontend-excellence-2764. This records independent reviews and local evidence, not a
release, competitor win or proof of higher generated-design quality.

## Independent reviews

- `/root/frontend_review`, fresh read-only gpt-6.1-sol: inspected the delivery changes, callers, permissions, skills,
  product-map and documentation. No code blocker. The initial documentation gap was updated and the delta reviewed
  without new findings. Six source-study scenario plans stayed within scope; no sample interface was built.
- `/root/frontend_security`, a separate fresh read-only gpt-6.1-sol: no security blocker. Confirmed exact Read/Skill
  names rather than a folder-wide read grant, unchanged write denial, new folder whitelists, linked-directory
  rejection, isolated-request exclusion and no extra authority in fallback or skill text. No tests run by this reviewer.
- `/root/frontend_review` also inspected the attachment change authored by `/root/attachment_reselect`: `add` copies
  the FileList before its first await, so clearing the native input does not discard file bytes. No finding.

These were independent read-only reviews. The writer did not approve their own implementation. Native-model behavior,
screen-reader use, whole-app resource use and generated visual quality were not measured by these reviews.

## Visual critique

`/root/frontend_visual`, a fresh read-only gpt-6.1-sol, inspected all eight before photos, then compared the six
affected before/after pairs at 1920x1080 and 1280x800. Each of the eight criteria scored 5 for each affected screen:

| Screen, both sizes | Before | After | Findings |
| :--- | :--- | :--- | :--- |
| Chat home | 40/40 | 40/40 | None |
| Project home | 40/40 | 40/40 | None |
| Conversation | 40/40 | 40/40 | None |

Visual critique: before 40/40, after 40/40, by /root/frontend_visual. Cards checked: C005 and C011 ok.
Settings was 40/40 in the before pass and was not affected, so no after score is claimed for it. C006 is maintained
by the hidden harness and no new window creation; static photos do not prove focus or motion behavior.

The native attachment handler was temporarily restored to the original behavior for before capture and then the exact
proven fix was reapplied. The before pass retained all four screens because that temporary source state had no UI
diff; the after pass retained only the three screens affected by Composer. Both composition scenarios passed with
zero findings. A concurrent production-build command invalidated one intermediate after attempt; the serialized
retry passed. No failed attempt is counted as evidence.

Photos: `target/faber-test/critique/before/` and `after/`, with indexes. The hidden component interaction check,
not the photographs, proves file reselection and preserved bytes.

## Program evidence and limits

- File reselection check failed before the fix: native input still held `C:\fakepath\same.pdf` instead of empty.
  After the fix the complete UI suite passed: 687 passed, 0 failed, 0 not run, 0 skipped, 190.1 s including build.
- Expanded guide integration initially found two old index assumptions (400-byte cap and language at the end).
  The updated bounded index keeps the language suffix; all 14 guide-related tests then passed. This covers both
  vendors/areas, fallback, old plugin support, resume/compaction, guarded modes, isolated requests and Windows junctions.
- Before composition: 1 passed, 0 findings, 94.9 s including build. Serialized after composition: 1 passed,
  0 findings, 59.3 s including build. These are local scenario durations, not interaction latency measurements.
- The 243-ID audit is static source evidence: 13 existing, 186 partial, 11 missing, 33 unknown. These counts do not
  mean 230 features are broken; partial includes unmeasured criteria. Each source/test reference retains that limit.
- Skill bodies total 15,655 UTF-8 bytes with two discovery descriptions at 151 and 152 ASCII bytes. This is measured
  text size, not model token usage or memory. The change adds no dependency, resident process or recurring timer.
- No native RAM/CPU comparison, screen-reader session, crash/restart durability test, paired frontend generation
  benchmark or comparison against commercial desktop apps was performed. Those remain unfinished as recorded in the
  [implementation plan](../superpowers/plans/2026-10-22-frontend-excellence.md).

The tool log holds the UI receipt; screenshot runs reused the working copy's normal run.log. Guide and script receipts
are retained in `target/frontend-guide-cargo.log` and `target/frontend-scripts-proof.log`. The normal queue must still
verify its exact commit before it can reach the development base.

## Durable draft attachments continuation

The existing text draft keys now also own image/file Blob records in IndexedDB and a small discovery/removal marker.
No new dependency, background process, timer or screen was added. Live composers and pending reads share byte snapshots;
confirmed caches release after the last consumer, while pending/failed writes keep the existing update-restart protection.
Typing writes text only. This is draft persistence, not an outgoing journal or idempotent CLI dispatch.

Independent storage/privacy reviews found and drove repairs to late reads after remount, two overlapping late appends,
and durable deletion retry. Two fresh final read-only security reviews (`draft_final_security_one` and `_two`) then found
no blocker in the corrected store, startup retry or confirmed-delete cleanup. Both also reviewed the subsequent inline
metadata-pruning/preserveReads delta and found no new blocker. They did not execute the program proofs themselves.

Real regression receipts, not just source assertions:

- Initial remount regression: expected three restored attachments, got zero before implementation.
- Late double-append regression: expected two attachments, got one before ordering/subscription repair.
- Integration before startup/late-delete repairs: 699 passed, three failed. After repair: 702 passed, no failures/skips,
  196.0 s including build. This preceded the final inline metadata cleanup and is not its final UI receipt.
- Final metadata-race regression failed under a one-line mutation removing `preserveReads`: the pending new PDF
  disappeared. The exact original source bytes were restored in a finally block and compared with the backup.
  The complete UI suite then passed: 703 passed, zero failures/skips, 193.0 s including build (UI 150.8 s).
- A real restart after editing away an inline file initially left its private removal block in IndexedDB: native RED
  timed out after 15 s waiting for deletion. Hydration now prunes known obsolete metadata without cancelling new readers.
- Final native run: two passed, no failures/skips, 86.2 s including build (49.7 s); scenarios themselves 16.3 s.
  The first ends and relaunches actual app processes three times with the same isolated profile, proves saved PNG/PDF
  bytes and names, failure/retry, removed-file exclusion, one saved user message and no automatic resend after restart.
  The second proves obsolete inline metadata really leaves the store after editing and restarting.
- The native fixture initially timed out awaiting a response because the generic chat recording expected different
  user content without an image and did not answer Auto routing. Its request is now adapted to the exact synthetic
  words/image and uses an explicit model. No product behavior was changed to hide that test mismatch.

Native cost, tiny synthetic PNG/PDF at 720 px in German: before 121.5 MB, committed 135.4 MB, restored after real restart
123.0 MB. All were below 400 MB and included the WebView tree. The failed-save/retry flow recorded 500 ms aggregate
process-tree CPU over 288 ms wall time, including existing startup work and a screenshot. This is not isolated marginal
CPU cost, idle usage or representative large-attachment performance. No power-loss guarantee, quota exhaustion,
screen-reader session, all-theme equivalence or cross-OS durability is established by these fixtures.

A fresh visual reviewer inspected all three native German/720 photos. It rejected the initial failure hint because
the border crossed its first line and the retry link touched the sentence. The warning now has scoped content padding
and logical inline button spacing; the independent recheck confirmed the repair in the new captures. Before/after restart captures
otherwise kept text, attachment controls and the contained long file name consistent. That is visual evidence only;
actual byte integrity comes from the native scenario.

Final composition scenario: one passed, zero findings, 53.9 s screen time (404.7 s total including slot/preparation;
build 47.0 s). A fresh read-only reviewer inspected eight baseline and eight final main-screen photos plus all three
German attachment states. Lowest score: 39/40 before, 39/40 after; Chat and Settings 40/40, project/conversation 39/40
at both sizes. The unchanged one-point deduction was duplicate naming in sidebar and breadcrumb, two occurrences;
the sentinel's hard repeated-name condition uses three. No screen dropped and no new visual blocker remained.
Cards checked: C005/C007/C011 ok within the inspected screenshots. Baselines explicitly reuse six prior-package after
photos and two unchanged Settings before photos; this is not a fresh same-build baseline comparison.

Required local source, types, documentation, production bundle, formatting, claims and script proofs passed. Five
pre-existing source warnings and the existing large-bundle warning remain. No unrelated warning was silently fixed.
Native receipt: `target/draft-native-green.log`; UI: `target/draft-ui-green.log`; composition:
`target/draft-composition-green.log`; final independent critique: `target/draft-final-visual-review.md`.
The queued exact-tree integration remains separate from these local proofs.
