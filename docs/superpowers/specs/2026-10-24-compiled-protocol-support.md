# One compiled protocol identity per provider

Status: implemented; three focused scenarios passed (four runner checks including the command aggregate), all seven
required local proofs passed, with two fresh independent security reviews and a final source/receipt review.

The offline manifest validator currently repeats the provider/protocol pairs already chosen by the real launch
argument builders. Task 998 requires one compiled support definition before a manifest can select a driver.

Add `Vendor::protocol() -> &'static str` beside the existing provider identities. Its exhaustive enum match is the
single compiled mapping: Claude uses `stream-json`, Codex uses `app-server`. Reuse it in both real argument builders
and in the manifest descriptor check through `Vendor::ALL`. No remote string becomes an executable or argument.
Do not add a registry object, dynamic registration, new protocol, dependency or alternate launch path.
This boundary covers the managed conversation driver argument builders. Separate account probes retain their
existing protocol literals; no claim is made that all short-lived probes use this mapping.

Preserve complete argv, permission modes and provider selection. Protocol identity does not qualify an installed
CLI version or authorize installation/launch. Signed manifest, local enablement and session-boundary integration
remain separate unfinished work. No production signer or feed is selected here.

Before implementation, prepare proofs that bind each current provider to its exact known protocol, compare real
argument-builder protocol fields to that compiled identity, and exercise accepted/rejected driver descriptors.
Capture the expected missing-method negative, then prove both the new cases and existing launch-argument behavior.
Read the actual recorded outcomes; a compile error is only a missing-interface negative, not a failed assertion.
The queued checks still qualify the complete change. Fresh security review must check that no other argv changes.

No resident memory, task, allocation or process is introduced by the mapping itself. Builders retain their existing
argv allocations. App memory/processor measurements are not inferred from these source-level observations.

The actual negative recorded only the missing method, not a behavioral assertion failure. The positive executed
the real argument builders for both areas and all six permission modes, including Windows/hooks combinations for
Codex, and accepted/refused descriptor pairs. The minimal diff preserves other arguments by inspection; this slice
does not claim a complete-vector runtime comparison or qualification of an installed real CLI binary.
