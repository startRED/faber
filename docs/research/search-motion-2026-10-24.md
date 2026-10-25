# One exit for Search and commands

Status: local correction and paired visual proof checked; integration remains unfinished. This does not finish task 997.

## Reproduction and cause

Independent inspection of the slowed palette-and-Find recording found a second fading palette after Escape.
A separate normal-speed diagnostic recorded the actual removed palette and the shadow exit surface. After the live
palette had completed its own exit, the application list observer copied that already-leaving element into the exit
surface. Its child CSS exit restarted while the generic copy also faded. The copy was inert and hidden from accessibility
tools, but remained visible over the conversation. It appeared about 89 ms after the diagnostic collector started
and disappeared around 241.5 ms in that recording. These are collector samples, not exact durations since Escape
or a speed benchmark.

The normal-speed diagnostic retained four images and native records under
`E:/Documentos/ProjetosIA/faber-evidence/palette-find-diagnosis-20261024/`; launch receipts `1704c7`/`925d31` ended naturally
with code 0 and CLEAN. Fresh read-only reviewer `/root/observation_evidence_review` inspected all four images and the
records and confirmed the second copy. Source and executable identities matched before and after.

## Correction

The shared list exit observer excludes an element already marked `.leaving`, alongside its existing exclusions.
The original owner still performs its exit and focus restoration. Ordinary list removals retain their generic exit.
No setting, command, dependency, process, observer or timer is added. Avoiding an unnecessary clone removes work on this
path; no memory or processor saving has been measured yet.

The new focused check mounts the actual palette inside the application list observer at full motion. It marks the real
leaving element and latches any corresponding shadow copy until the live owner disconnects and two frames pass. This
checks that a transient duplicate cannot disappear before the assertion and produce a false pass.

## Executable evidence and limits

The check failed before correction (`cdfeba`, 0 passed, 1 failed: the completed palette exit restarted in the generic
exit surface). The negative runner log was overwritten before a copy was saved; that failure remains in the execution
receipt and is not described as a retained log. After correction it passed (`bf6ccf`, 1 passed, 0 failed; 27.9 s total,
13.3 s build, 1.9 s UI). The positive log is retained beside this draft. These durations are proof costs, not a product
performance comparison. These direct native runs did not establish the Node storage wrapper's shared lease.

Fresh read-only code reviewer `/root/menu_exit_code_review` found no code blocker; its raw report is retained beside
the positive log. The first after attempt
never started its guard or app because an existing cache reservation was younger than the reclamation threshold.
Only that attempt's own waiting process was stopped; the reservation and historical executables were preserved.

## Paired native observation

The original nine-image baseline completed at `palette-own-exit-proof-20261024/before/run-01` (`d59a90`/`70bf13`).
A newly reviewed after-only recipe copied the already-built correction and fake CLI exclusively into a separate
runtime folder inside the existing third cache. The normal shared lease and resource guard were acquired before
any copy; source and destination hashes matched. No app build, original-cache overwrite or stale-reservation removal
was needed. Preparation review and zero-child checks passed before the one after execution (`9df839`/`99fd89`).

That run retained nine images at 720x480, 1280x800 and 1920x1080, Portuguese, full motion and explicit normal playback.
The baseline latched the extra palette in 28, 30 and 29 observations; the correction latched none. Its collectors
recorded 46, 49 and 51 frames over about 249, 268 and 273 ms. All three widths restored the exact same empty composer
and had no live or copied palette at the final sample. Both runs exited naturally with code 0 and CLEAN, and their
captured source/executable identities matched before and after. Later documentation edits are outside those captures.

Fresh read-only reviewer `/root/observation_evidence_review` inspected all eighteen images and the records. Each
baseline live-removal sample visibly retained the copy, and each corrected sample was clear. Open surfaces and final
closed states showed no evident scoped regression. Parent inspection also confirmed the clear 720 px sample.
After evidence: `E:/Documentos/ProjetosIA/faber-evidence/palette-own-exit-after-payload-20261024/after/run-01/`.

Raw memory observations in MiB, before opening to fully closed, were 164.71 to 172.64, 171.58 to 182.85 and 183.21 to
196.46 for the baseline; 163.59 to 173.27, 170.06 to 184.16 and 182.22 to 200.51 for the correction, in increasing width
order. CPU readings are cumulative process-tree observations with timestamps in the receipts. The samples include
capture/observer activity and are not isolated benchmark trials; they establish no memory or processor saving, leak
conclusion, idle-memory guarantee or speed improvement.

This evidence does not establish long-language layout, forced colors, screen-reader behavior, all motion surfaces,
normal-speed performance improvements or integration. The separate Find clearance and count correction has its own proof.
