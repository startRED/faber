# Offline signed-manifest persistence

Status: task 998 offline persistence implemented; 15 focused failure scenarios and all seven required local proofs
passed, with two fresh independent security reviews. Not connected to the app, a feed or installation authority.
Extends the native signature and metadata boundaries. No app, installer, feed or production signer is connected.

## State and scope

Keep three files in an app-owned directory: an initialization marker, a revision/digest trust record and the exact
signed envelope. Only absence of all three is an uninitialized baseline. A missing marker with either other file,
an invalid marker, an unreadable file or an invalid trust record is blocked, never first use. A trust record binds
the exact authenticated payload hash, not reserialized metadata. Require closed typed fields and bounded reads.
The marker is the exact version token `1\n` (read limit 16 bytes); the floor is a named JSON object with only
`revision` (1 through 9007199254740991) and `digest` (64 lowercase hexadecimal characters), bounded to 4096 bytes.
The envelope read limit is 1 MiB. Read at most each limit plus one byte before rejecting oversized input.

The API receives public pins, explicit time and app version on every operation. It never retains an authorization
in memory across calls. Loading reauthenticates the cached envelope, rechecks current expiry and app support, and
requires exactly the persisted revision/digest. A newer cache cannot silently advance an older trust record.
All errors are fail-closed. No method grants permission to install or launch anything.

## Serialized publication

Use a stable dedicated file lock following the existing store's budget lock pattern. Own it across reading state,
validating the candidate against the current floor and both replacements. Fail busy instead of starting a second
writer. The lock also prevents the existing replacement helper's fixed temporary name from colliding across
instances or processes. No polling, background thread or persistent engine task is introduced.
Readers acquire the same lock. Its dedicated file is never replaced or removed by this API. Report busy separately
from corrupt or unreadable state. All validation and publication happen within that ownership lifetime.

Reuse `store::Disk` and `store::replace_with`, including injected failures. After authenticating and validating the
candidate, initialize the marker if needed, replace the trust record, then replace the envelope. Return success
only when all three required files are published. Never roll the trust record back after a later failure.
Revalidate under the lock so an older delayed refresh cannot replace a newer revision. Equal revision accepts only
the identical payload digest; a different valid signature envelope for identical bytes remains harmless.
A higher authenticated revision can change policy, including lifting a suspension; suspension itself is not a
permanent local flag. No lower revision or equal-revision changed payload can do so.

An interrupted initialization with a marker but no floor remains blocked. A valid floor with missing, expired or
damaged cache can recover only through an authenticated current candidate at or above that floor. A missing or
damaged floor after initialization cannot be repaired by this API, because no surviving revision can constrain
recovery. That requires a separate trusted recovery contract. An old valid cache is never evidence of a lower floor.

## Durability limits

The existing helper syncs file content before rename, but does not establish parent-directory metadata durability
against sudden power loss on every supported filesystem. This slice proves orderly failed writes and interrupted
publication through injected states; it must not claim power-loss durability without platform-specific evidence.
Likewise, local state cannot distinguish first use from an attacker deleting every marker, floor and cache file.
No resistance to such complete local deletion or local file replacement is claimed. The future app integration must
establish its initialization/recovery authority before the baseline can authorize production installations.
The caller must supply trustworthy time and a writable local app-owned directory with working file locks. Reads
also open that lock for writing. Lock acquisition failures currently share `Busy`; consumers must not assume a
retry will resolve them or add an unbounded retry loop. Cross-process/network-filesystem semantics are not proved
by the same-process contention fixtures. A future production integration must qualify its supported local stores.

First initialization deliberately remains blocked after a published marker loses its floor, even if the original
failure was transient. There is no trusted reset in this API. Replacements currently use the non-patient disk helper,
so a transient Windows rename refusal is also a failed publication. Establish trusted initialization/recovery and
evaluate bounded busy retries before enabling this boundary as production installation authority.

No production caller, dependency, resident worker, timer or retained document is added. Disk operations and signature
validation happen only on explicit calls; each file read is bounded. App memory/processor cost has not been measured
because this boundary is not connected to the app. Local test execution is not an app cost measurement.

## Required negative fixtures

- All files absent, partial initialization, missing marker with surviving state and invalid marker.
- Missing, malformed, oversized or unreadable trust state; missing, corrupt, oversized or truncated cache.
- Valid signatures with expired metadata, revision rollback or equal-revision changed bytes.
- Cache/floor mismatch in either direction, including a suspension followed by damage to either file.
- Injected marker, floor and envelope write/rename failures, with a fresh reader after each failure window.
- Recovery using identical current bytes or a newer candidate against a healthy floor; no recovery with a lost floor.
- Contending cache instances and a delayed older candidate after a newer accepted revision.

Fixtures use only synthetic public signature data and disposable directories. Capture an actual negative before
implementation, then focused passing proofs, all required local checks and two independent security reviews.
Native test execution must be coordinated with the other active lane; no fullscreen or resource guard is bypassed.
