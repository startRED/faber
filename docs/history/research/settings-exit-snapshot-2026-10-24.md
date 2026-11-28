# Settings exit snapshot, 2026-10-24

Status: locally reproduced and corrected; focused native and paired visual proofs complete. Final independent code/docs review and integration remain outstanding. This is historical evidence, not a replacement for the living Settings design.

## Reproduction and cause

At 720×480, pt-BR, Oficina/full, the General→Privacy exit copy showed “Igual ao sistema”, while initial and final live General showed “Português (Brasil)”. The baseline did not retain select state and did not establish any persisted preference change.

The focused native Settings regression records the live select, detached native clone, detached original after navigation and actual shadow exit copy. RED confirmed live value `pt-BR`/index2/Português (Brasil), native clone empty/index0/Igual ao sistema, detached live still `pt-BR`/index2, and exit copy empty/index0. No save callback ran. The failure is native cloning losing current selectedness, not disposal or a saved-language mutation.

The shared `ui/motion.ts` snapshot now copies each descendant native select's selectedIndex into the corresponding clone before shadow insertion. All current UI selects are single-select, and animateList callers clone containing elements, not a select root. No generalized form-state copier was added. The inherited `.leaving` exclusion, IDs/autofocus stripping, inert/aria-hidden/pointer-none behavior, animation timing and cleanup remain unchanged.

## Focused proof receipts

External root: `E:/Documentos/ProjetosIA/faber-evidence/section-snapshot-fix-20261024/`.

- RED f72cca→0e0cd1:0 passed/1 failed at the actual value assertion;185.2s total,182.6s build,2.4s UI. `negative-run.log` was preserved before GREEN. The negative app/fake were separately frozen; payload receipt5623 records that identity.
- The first GREEN attempt63c8ca failed before lease/child because the wrapper compared CRLF and LF status formatting. It was a preparation failure, not a product regression.
- Reviewed GREEN02 4ca33c→d58712:1 passed/0 failed;29.6s total,24.8s build,2.1s UI. `positive-run.log` was preserved before related runs. Its provenance records exit0 and unchanged source/original artifacts/old leases.
- Related bfb91c→baa187:the existing section cross-fade check passed1/0 in24.6s (build18.8s/UI2.2s), and the literal `motion:` filter passed14/0 in9.8s (build3.4s/UI2.8s). Both provenance records say unchanged. These are focused proofs, not a complete screen suite.

Each native run used the normal resource guard and B runChild explicit lease in the existing exclusive nested target `build-3/section-snapshot-red-20261024`. No cache1/2 lease was deleted or bypassed, and original cache3 app/driver/fake/probe identities were retained.

## Paired visual evidence and identity

Baseline:`settings-section-motion-20261024/run-01`; after:`section-snapshot-after-20261024/run-01`, both under the external evidence root. Each has five original PNGs:initial General, Privacy transition, Privacy settled, General transition, General settled. Baseline C af853 used app4b54472d; after J ecf21cc1 plus the captured two-file diff13cc1148 used app06a7e42e38925ffe8f1b3b1fb6054194722f6757f98cf8ceff7072be556d3849 and fake476e08e2 with the absolute original cache3 driver. They are different executables.

GREEN02 appd35ab4fd was not frozen outside its target before related builds replaced it. The reason for executable byte variation was not investigated. Its result remains evidence for that invocation, not retroactive identity of app06. The actual after app/fake were frozen in `positive-payload-after-related`, with matching hashes independently checked. Later documentation edits do not invalidate the equal two-file source snapshots captured before/after the native observation; those snapshots describe that historical run.

AFTER receipt745a2c records normal guard, natural exit0/CLEAN. The independent result reviewer inspected all10 original images and all107 recorded language-bearing General ghost samples; every new sample retained `pt-BR`/index2/Português (Brasil). The new transition label matches its live endpoints; the old transition visibly reset. Both navigation endpoints preserve exact destination navigation focus, expected heading/current state, scroll0, zero ghosts/exit hosts/active animations, and unchanged permission. Every recorded exit clone remained inert, aria-hidden and pointer-none.

The three stable image pairs scored40→40 under the review's eight-line rubric; transient images were not scored as final states. These equal stable scores do not erase the baseline's temporal defect. The final independent report is `section-snapshot-after-20261024/independent-result-review.txt`, SHA256 `e3fde907eb4d07659f58797764de0b2d515327063215caca1b6c8fe47a4d86a8`; its manifest pins all10 PNGs. This writer does not approve its own change.

## Limits and cost

The observation is sequential,720×480,pt-BR,Oficina/full with CSS playback0.25; JavaScript removal timers are unchanged. Images and frame/mutation reads are not atomic and establish neither exact percentages nor a complete slowed fade. No normal-speed performance, full task997, integration, interruption/rapid-reversal, other dimensions/languages/themes/zoom, reduced/none or screen-reader claim follows from this capture. Existing focused checks provide their own narrower coverage. Only native select state was reproduced; other form-control properties were not generalized.

The patch adds no DOM, timer or process. It performs two descendant queries and one selectedIndex assignment per select on an existing bounded exit copy. Actual memory/CPU cost is unmeasured; zero cost or improvement is not claimed. The future lane remains stacked on six prerequisites (five C changes and H's palette fix); seven commits including this change are not an integrated result. No commit, push or landing is authorized by this record.
