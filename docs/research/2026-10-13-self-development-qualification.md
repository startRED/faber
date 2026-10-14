# Task 13: qualification stopped

Measured on 2026-10-13 (America/Fortaleza). This is a failed preflight, not a qualification pass.
The repository JSON is an informational copy. Authoritative diagnostic evidence is in
`%LOCALAPPDATA%/Faber/t13-acceptance-2026-10-13/`:
`qualification-preflight-error.log` and `qualification-stopped.json`.

## What ran

The manual Faber helper in `testkit/examples/project-qualification.rs` starts its worker through
the existing hidden desktop and owned Windows job. It uses Faber's native CLI resolver and process
guard, hashes installed executable/script bytes, and only requests `--version` and official account
status. It never requests a model turn or sends a project prompt. Account output is reduced through
the existing status parser; raw account data is not retained.

The initial worker failed with the exact diagnostic `protected resolved path`. A second no-model
attempt retained that diagnostic after adding bounded worker error capture. Fingerprinting refuses
protected directory components before opening files, including after canonicalization. **The rejected
binary was not opened.** No workaround, credential copy, alternate installation, or protected-folder
read followed the failure.

The initial error did not retain the failing vendor identity or preceding version/account results.
Those values and the number of CLI status processes remain unknown; no successful per-vendor probe
is claimed. The helper now persists a stopped receipt with completed observations and the failing
vendor before returning from a fingerprint failure. That receipt behavior is unit-tested; the installed
CLI preflight was not replayed after the stop. Four helper tests cover bounded version data, byte-bound
fingerprints, protected paths, permitted hardlinks, and non-overwritable stopped records.

## Stop and retained gates

Runbook stop rule 2 applies before live model qualification: no verified per-invocation cost bound or
measured stop-cost reserve is available. The failed binary fingerprint also prevents a version/hash-bound
qualification receipt under the owner's no-read rule. CLI lifetime deadlines do not prove a dollar cap.

The requested native-tool/canary and complete `system/init` exposure probes were **not performed**.
There is no fresh passing receipt for either vendor. Tools, MCP servers, plugins and hooks remain
unobserved in this run. No planted canary or real credential file was read. No paid credit or subscription
model turn was requested. No private admission receipt was fabricated from help flags, account status,
a caller Boolean, fixture receipts, or these informational repository files.

`engine/src/project/policy.rs::admit` remains fail-closed and non-configurable. Freshness checks,
binary-change requalification and successful admission are not implemented by this partial preflight.
The full qualification producer/consumer requested for Phase B remains unfinished.

Additional source blockers, independently visible before any mission dispatch:

- `engine/src/project/budget.rs::Budget::can_admit` requires a positive verified monetary exposure;
  `Invocation::exposure_micros` alone is a caller value, not installed-driver proof.
- `engine/src/project/kit/delivery.rs::attest` does not observe hooks and can return `BestEffort` for
  the second vendor. It is not a complete installed-session loadout qualification.
- `engine/src/engine/project_sessions.rs::dispatch_mission_worker` still refuses writer roles with
  `reason.project.processTreeUnproven`, even if the earlier policy gate were opened.
- `src-tauri/src/local_update.rs::probe_canary` returns `CanaryVerdict::NotRun`. A startup handshake
  cannot authorize promotion without the private miniature writer/proof/other-vendor mission.

Earlier captures are described in `docs/research/2026-10-12-project-policy-probes.md` and in the owner's
`faber/docs/security-r97-review.md`. The latter file is absent from this branch. Its trimmed captures
prove selected earlier operations and do not replace current version/hash-bound qualification.

## Remaining Phase B work

A permitted installed binary identity, independently observed native-tool and planted-canary denials,
complete tool/MCP/plugin/hook attestation, a defensible invocation bound and safe stopping measurement
must precede any successful qualification receipt. Only that private, fresh, matching evidence may
open admission. No live Phase C mission, build promotion or successful Resume is authorized by this report.
