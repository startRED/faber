# Find why the slow attachment restore check failed on a change that does not touch it

Area: none
Files: tests/ui/composer.check.ts, ui/unsent.ts, ui/Composer.tsx (only if the product is at fault)
Depends: none
Size: small; evidence first, then the smallest fix the evidence supports

Why: the queue run of PR #482 (run 41134816135, job `ui`, 2026-10-30 22:14 UTC) failed on one screen check, "composer:
slow attachment restoration preserves new typing and typing never rewrites bytes", after 33.0 s, with "the stored image
read is held in flight" (`tests/ui/composer.check.ts:351`); the other 782 passed. PR #482 changes only `scripts/land.mjs`,
`scripts/land.test.mjs`, `docs/dev/workflow.md` and a notes file, so the change cannot have caused it. The check replaces
`FileReader.prototype.readAsDataURL` and wraps `IDBFactory.prototype.open`, renders the Composer, and polls up to 3 s for
the stored image read to start; why the whole check took 33 s and the read never started is not known.

Build:
1. Collect evidence, assuming no cause: search recent `ui` job logs for this check (`gh run list`, `gh run view
   --log-failed`, the `log-ui` artifact's `run.log` and screenshot when kept) and count how often it failed and on which
   system; then run it alone on the PC many times (`cargo t --filter "slow attachment"`, also with the PC busy) and record
   the rate. Note where the 33 s went: the check's own 3 s poll, `keepAttachments` before it, or the screen runner's limit.
2. From that evidence only, fix what is wrong: the check (it waits on time instead of the event it needs, or a stub from
   an earlier check is still in place) or the product (the stored read can start late or never, which a person would see
   as an attachment that does not come back). Say which, with the log lines.
3. If no failure reproduces and the logs give nothing more, add the timing the check is missing to its failure message
   (time spent in `keepAttachments`, number of IndexedDB opens, whether the Composer mounted) so the next failure names
   its step, and leave the task's finding in the pull request.

Leave out: other composer checks, the screen runner's limits (testkit), ci.yml.

Done: the pull request states the measured failure rate before and after (for example 0 failures in 50 runs alone and
under load) and the evidence for the chosen cause; a fix to the product has a check that fails without it. Extends:
nothing in `docs/product-map.md` unless the product changed. What's-new: none unless the product changed. Review: a fresh
read-only session. Cost: none in the app.
