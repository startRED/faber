# Live manifest validation and recovery

Status: native offline document validation passed 19 focused checks locally and two independent static security
reviews after reproducing and fixing object-shape and collection-allocation failures. The seven required local
proofs also passed (script suite: 120 passed, one skipped; five existing UI warnings and an existing bundle warning).
Native signature verification, publication tooling, durable cache, recovery and installation integration remain
unbuilt. Extends the [offline signature boundary](2026-10-24-cli-tools-live-manifest.md), not its authorization scope.

## One implementation

Implement the document rules once in the engine, without network, filesystem, processes or wall-clock reads.
The caller supplies authenticated UTF-8 bytes, the current UTC time and its app version. The validator reads
supported catalog identities and protocol variants from the engine's compiled definitions, never from the caller.
A publication check invokes this same implementation; do not duplicate catalog validation in JavaScript.
The existing Node signature primitive remains an independent publication check, never the app's runtime verifier.
Neither interface accepts a caller assertion that a signature is valid as installation authority.
The native signature boundary must authenticate bytes itself before a production caller can validate them.

Separate three results: a parsed document, a candidate trust-state transition, and durable installation authority.
The first two are offline validation results. Only a consumer that successfully authenticates and persists both
the document and trust state can establish the third. None of this slice changes installation or launch behavior.
An absent previous revision in this pure API means only that the offline analysis has no revision floor. It is
never proof of first use, a recovery decision or permission to install; make this explicit in its public comment.

## Document

The envelope retains its 1 MiB bound. Reject invalid UTF-8, duplicate JSON member names at every depth, nesting
deeper than 16, unknown fields and unsupported schema versions. Apply bounds before allocating unbounded lists.
Use one explicit snake_case wire format with no defaulted security fields.

Required top-level fields:

| Field | Rule |
| :-- | :-- |
| `schema` | Integer `1` |
| `revision` | Positive integer no greater than `9007199254740991` |
| `issued_at` | UTC `YYYY-MM-DDTHH:MM:SSZ`, real calendar date, no leap second |
| `expires_at` | Same format; strictly after emission |
| `minimum_app_version` | Canonical three-component nonnegative numeric version |
| `catalog` | At most 512 entries referencing already compiled catalog identities |
| `compatibility` | At most 512 exact-version/platform qualification records |
| `policies` | Explicit policy for each compiled provider, no duplicates or omissions |
| `drivers` | At most 64 descriptors selecting compiled protocol variants only |

Require `issued_at <= now < expires_at` with no future-time allowance. Compare app versions numerically, not
lexicographically. An unsupported app version cannot authorize new work. No maximum document lifetime is selected
here: expiration is mandatory, and the publisher's operational cadence remains separate from parsing rules.

Numeric app-version components are canonical decimal integers between zero and `4294967295`, with no leading
zeros except zero itself. Exact tool versions are canonical SemVer strings of at most 128 ASCII characters,
with the same numeric-component bounds and valid prerelease/build suffixes. No leading `v`, range or mutable
channel alias such as `latest` is accepted. They are exact identities, never a request to find a newer release.
Evidence identities are lowercase 64-character SHA-256 hex strings. Reasons are 1–512 Unicode scalar values,
with no control characters. Identifiers are 1–64 lowercase ASCII letters, digits or dashes, matching existing
catalog identifiers. All objects reject additional fields, including nested evidence objects.
The caller's app version accepts SemVer prerelease and build metadata, since Faber distributes previews. A preview
is below the stable version with the same numeric triple, but above stable versions with a lower triple. Thus
`0.2.0-preview.1` fails minimum `0.2.0`, passes minimum `0.1.9`, and `0.2.0+build.1` meets minimum `0.2.0`.
Reject malformed suffixes rather than stripping them; build metadata does not alter version ordering.

Exact collection entry shapes:

- `catalog`: `{id, commit, digest}`. Each id must match exactly one compiled catalog item; commit and digest must
  equal that item's existing pins byte for byte. Missing entries are unavailable for new managed installs, never
  an instruction to uninstall or disable existing copies. No remote executable metadata is accepted in schema 1.
- `compatibility`: `{provider, version, operating_system, architecture, artifact_digest, driver, outcome, evidence, reason}`.
  Provider is a compiled provider identity. Platform is `windows`, `macos` or `linux`; architecture is `x86_64`
  or `aarch64`; the pair must also be supported by the compiled app. `driver` references one descriptor in the
  same document. `artifact_digest` is the SHA-256 of the tested official binary, or null for an unqualified row
  lacking an artifact. A qualified row requires this digest. A future installation must match it in addition to
  existing official-source checksum validation; reusing a release version for different bytes cannot qualify them.
  Outcome is `qualified` or `unqualified`. Evidence has exactly `startup`, `turn`, `cancellation`
  and `cleanup`, each a digest or null; all four must be digests for `qualified`. Reason is always required.
- `policies`: `{provider, install, launch, reason, evidence}`. Install and launch are independently `allow` or
  `suspend`. Reason and one evidence digest are required for both states, making an explicit restoration
  attributable too. Exactly one policy per compiled provider is required. A newer valid policy may lift a remote
  suspension, but cannot lift any local permission or disabled-item restriction.
- `drivers`: `{id, provider, variant}`. The id is unique; the provider/variant pair must exist in compiled support.
  Every descriptor must be referenced by at least one compatibility record, and a record's provider must match
  its descriptor. A descriptor cannot add or redefine compiled support. No default descriptor is inferred.

Remote catalog entries may select only identities and original-source pins already supported by this app. They
cannot supply launch arguments, environment variables, hooks, shell commands, paths, endpoints, permissions or
models. A new executable delivery capability requires its own checked app change. Existing store digest, path,
license, consent and disabled-state checks remain mandatory independently of this document.

Compatibility identity is `(provider, exact_version, operating_system, architecture)`. Reject duplicate identities,
unknown providers/platforms and unsupported descriptors. An approved row requires four separate immutable evidence
identities: protocol startup, a recorded turn, cancellation and process cleanup. A populated identity only links
evidence; publication must separately verify that the evidence qualifies the real binary. Synthetic examples and
signature checks do not qualify a binary.
One immutable evidence artifact may contain several of those observations, so equal evidence digests are allowed.
The publisher must independently establish each observation; four populated fields do not establish four passes.

Policies can only restrict new managed installations or new managed launches at a session boundary. They cannot
interrupt active work or override a locally disabled item. Each restrictive policy must carry a bounded reason
and an immutable evidence identity. A descriptor contains only a known provider and compiled variant identity.
Keep provider/variant support in one compiled table shared with the existing driver selection, not a remote table.
Before connection, replace this slice's two matching protocol literals with that shared registry and exercise the
real launch boundary. The offline validator alone does not establish that integration or qualify a real binary.

## Revision and recovery

Persist both the highest accepted revision and SHA-256 of its exact authenticated payload bytes. Equal revisions
with different bytes are conflicts even if both signatures are valid. Equal revision and identical bytes permit
recovery only while the document remains current and supported. Lower revisions never replace accepted state.
Higher revisions replace it only after signature, schema, support and freshness checks all succeed.

Represent never-initialized separately from initialized-but-missing or damaged state. Only never-initialized may
use the bundled baseline's own qualified exact versions. Once initialized, a missing, expired or corrupt cache,
missing trust record or inconsistent pair blocks new managed installs. Descriptive catalog browsing may continue;
it conveys no recovered authority and cannot clear a suspension.

Missing or damaged trust state loses the trusted revision floor. A correctly signed current feed alone cannot
repair that loss. Keep it blocked; external recovery requires a separate reviewed contract. Only an intact trust
record permits recovery from a damaged cache using the identical revision/digest pair or a newer valid revision.
Never infer first use solely from absent files. A durable initialization marker must precede the first accepted
trust record; interrupted initialization is a recovery state. Until a protected first-use/initialization contract
is implemented and checked, this offline slice cannot establish runtime authority from any missing-file state.

Serialize refresh publication and recheck the accepted revision inside that critical section. Persist the new
trust record before making the candidate cache available; if either replacement fails, return an error and block
new managed installs until the matching pair is recovered. Never restore an older trust record to make a failed
cache write appear successful. A restart must reauthenticate the cache and compare its revision and digest with
the persisted record before using it. Every crash window between replacements needs a deterministic fixture.
The previous document also loses in-memory authority as soon as the persisted pair becomes inconsistent; return
success only after both replacements are durable. No caller may continue using a previously cached authorization
after publication reports a failed replacement.

This protects against feed replay and interrupted writes, not a local attacker able to replace every protected
state file. Stronger local rollback protection needs a separately reviewed platform storage contract.

## Proof before connection

Write shared valid/invalid byte fixtures for the engine and publication check. Cover unknown and duplicate fields,
invalid dates, integer boundaries, future/expired documents, app-version ordering, unknown identities, absent
qualification evidence, forbidden executable fields, equal-revision equivocation and lower-revision replay.
For persistence, cover restart, concurrent refresh completion in reverse order, each failed replacement and a
suspension followed by damaged cache or trust state. No case touches a real account or personal installation.

Native validation tests must first fail for the intended missing behavior. Run them only after the approval
recovery lane releases its native proof window; never start a second app or bypass its fullscreen protection.
Require two fresh security reviews on the final source and exact local proof receipts before queuing the slice.
Production signer identity, private-key custody, feed origin and publication remain the owner's unresolved choices;
this contract neither invents them nor needs private signing material.
