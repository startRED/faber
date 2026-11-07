# Rehearsal

## Plan

- Placement: standalone preview, suitable for a future full-width product hero; 960 by 510 and a distinct 390 by 570 crop.
- Look: Faber Dark Room, its current Archivo font, Curioso mark, icons and tokens. No replacement identity.
- Story: type and send a project request; read/build/check; open the result in the preview; hold the outcome.
- Reference: the upstream `ai-chat` gold's sent question and retained answer, plus `dark-hero` surface hierarchy.
  The implementation is original and uses the downloaded kit; no upstream source is copied into this repository.
- Poster: the ready result with the preview open. Synthetic disclosure remains visible.

## Evidence

Executed on Windows with Node.js 24.16.0, using catalog commit
`299140fafd08df55a9c234e610c3f64af3f45531` and folder SHA-256
`3e3ed3b317a94cab6f7027c199f2a899a562d8abe89e66e5bbad13b007b97444`.

- `node --test scripts/stageshow.test.mjs`: eight tests passed. They cover bounded arguments, catalog pin/digest,
  linked parents, generated-file hard links, unexpected executable configuration, linked build outputs, all five
  locale sets, and the actual finite story. A visible cursor must not enter again from outside the scene.
- `node scripts/stageshow.mjs --build --lang pt-BR`: passed after verifying every tracked Git blob, the catalog
  folder digest and the bounded source/output trees. The standalone output contains the original Archivo font and
  eight Faber/runtime/font notices, with no unrelated example assets.
- Upstream React/TypeScript checking and Vite production compilation passed. Faber's own dependency graph is unchanged.
- The reproducible browser check is [`verify.mjs`](verify.mjs). It exercises real keyboard pause/step, play, slow
  motion, replay, the entire request/read/check/preview flow at 960 and 390, frame-load timeout/recovery, and the
  reduced-motion result without a cursor. The embedded document's zero margin is asserted; a white browser rim is a bug.

The ignored evidence directory is `target/stageshow/<commit>/stageshow/audit/`. Browser receipts and images use
`faber-controls.json`, `faber-viewer-960.png`, `faber-viewer-390.png` and `faber-reduced-390.png`.

## Motion evidence

The upstream film recordings are deliberately retained as **inconclusive**, even though both printed
`rehearsal passed`: wide recorded 2,179 frames with a 65.3 ms largest gap and compact 2,712 frames with a 73.3 ms
largest gap. Their native sampling gaps left unobserved motion; a gate sentence did not waive that evidence.
Files are `faber-wide-motion-final.json/png` and `faber-compact-motion-final.json/png`. These recordings preceded the
independent trace's final motion repairs and are not proof of the final moving scene.

The independent check records 15.5 seconds of native browser frames over the built artifact. Native time and a
native post-frame observer are captured before the iframe installs its playback clock. Native elapsed time proves
capture coverage: no gap may exceed 100 ms, and no gap is skipped. The separate scene clock measures the authored
trajectory's spatial speed; it does not prove constant physical frame rate. Instantaneous native spatial speed is
retained as a diagnostic, including clock catch-up. All four story beats and all five phases must appear in order,
and the final result must stay unchanged for two seconds. Typing must append only the requested characters while
its field keeps its width; character-width growth is expected content. Observed traces survive failed assertions.

This check found and removed two unnecessary movements: re-entering a cursor already visible on the field, and
sliding text people were already reading as the preview opened. The reading column now stays still. Final trace
receipts use `faber-trace-960.json` and `faber-trace-390.json`; they establish only the assertions their samples cover.

The final built-artifact run passed: wide recorded 3,544 frames with a 23.5 ms largest native gap; compact recorded
3,579 frames with a 27.6 ms largest native gap. Maximum measured scene-clock spatial speeds were 2,111.7 and
1,200.3 px/s. Native instantaneous speeds remained diagnostic at 6,459.1 and 3,143.6 px/s. Both widths completed all
four beats, held the result for two seconds and produced no page errors. Keyboard pause/step, resume, slow motion,
replay, reduced motion, failed-frame timeout and replay recovery passed. This is bounded playback evidence, not a
full motion certification.

Paused page JavaScript heaps were 7,233,128 bytes wide and 7,172,144 bytes compact. Browser task time over a 600 ms
paused sample was 0.000547 s and 0.000523 s respectively. No process remains running for this demo in the shipped app.

## Limits

This is an authored illustration, not end-to-end evidence for the desktop application's project flow. Faber's own
screen tests establish that flow. No AI or real planner runs. Screen-reader testing and a whole-browser memory/CPU
benchmark were not performed. The browser receipt's JavaScript heap and 600 ms paused task samples describe that
instrumented page only, not the desktop application's resident memory or sustained CPU use.
