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
