# Retaining the controls received by a newcomer reviewer

Status: locally checked and independently reviewed; native reviewer execution and integration are unverified.

The second narrow Projects walkthrough did not retain its per-step OBSERVE lists. A screenshot showed a close icon,
but that did not establish whether the reviewer received that control in that particular step. No product defect
or successful discovery can be inferred from that missing evidence.

The existing capture sequence now saves the exact OBSERVE object next to each screenshot as
`<task>-<step>.controls.json` before calling the reviewer. It preserves numbering, labels, coordinates, disabled
state and native options without another model call. Exclusive creation refuses to replace an existing record;
capture, observation and write errors propagate. The image and controls remain sequential, not an atomic snapshot.

The regression check first failed because the capture operation was absent (parent receipt `abb0f1`), then passed
with all 20 newcomer script checks (`3d24b7`). It exercises the real capture operation with a synthetic driver seam,
inspects persisted controls before any reviewer runs, and checks refusal to overwrite the evidence. Fresh read-only
review `/root/observation_evidence_review` (`gpt-6.1-sol`) found no blocker and independently passed all 20 checks
(`4624a5`). No native app, driver or actual reviewer was executed for this change.

The seven lightweight proof commands passed; the full script suite reported 143 passed and one existing skip out
of 144. Existing source and frontend bundle warnings remain. This changes only the development review script:
the shipped app gains no memory or processor cost. Review runs write one small JSON file per existing step and
make no additional provider calls. Native flow validation and integrated checks remain unverified.
