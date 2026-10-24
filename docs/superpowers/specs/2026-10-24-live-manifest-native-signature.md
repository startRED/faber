# Native manifest authentication

Status: native authentication implemented for task 998; nine focused checks passed after an actual missing-module
negative, all seven local proofs passed, and two independent security reviews found no blockers. Queued checks remain
pending. The implementation is disconnected from app installation, network and persistent cache.
Extends the [document validator](2026-10-24-live-manifest-policy.md) and the
[publication signature boundary](2026-10-24-cli-tools-live-manifest.md). No app installation or network path changes.

## One checked entry point

`validate_signed_document` accepts an envelope, a caller's pinned public Ed25519 keys, explicit UTC time, app version
and an optional previous revision/hash. The pins must come from app-compiled trusted material, never from the feed.
Tests supply disposable public fixtures; no production signer, key custody or publication origin is selected.

Bound the entire envelope to 1 MiB before parsing it. Require one object with exactly `key_id`, `payload` and
`signature`, each a string; reject duplicate members, arrays and unknown fields. Key ids use 1–64 lowercase ASCII
letters/digits/dashes with an alphanumeric first character. Payload and signature use nonempty canonical padded
standard base64. The signing key is exactly 32 public bytes, and the signature exactly 64 bytes.

Reuse the installed Ring-backed Ed25519 verification algorithm exposed by Rustls's public verification mapping.
Its public interface verifies a raw message against raw public-key bytes; it does not add a TLS transcript or
prehash the message. Select only the compiled Ed25519 mapping, never an algorithm named by remote metadata. Do not
initialize TLS, read a key file, use the network or add a dependency for this boundary. Failure to find or verify
that algorithm fails closed.

Verify the exact decoded payload bytes first, then require valid UTF-8, then invoke the existing document validator.
Never parse unauthenticated payload metadata. A correct signature does not override schema, current time, app support
or revision/hash checks. Return a `VerifiedDocument` with private fields and read-only access to both the original
bytes and parsed document, so consumers cannot silently replace authenticated content with reserialized JSON.

This result is an offline analysis, not durable installation authority. An absent previous revision does not prove
first use, and no verified result repairs a missing trust floor. Cache initialization, atomic publication, expiry
on reuse, provider switches at launch and installer gates remain separate unbuilt work. No active work is changed.

## Proofs

The fixture generator lives outside the repository. It signs with a disposable in-memory key, exports only public
key/payload/signatures, and cross-checks those bytes using the independent Node signature verifier. The committed
fixture labels itself synthetic and cannot establish real binary qualification.

Tests cover exact Unicode bytes and a suspended policy; altered bytes/signatures and untrusted keys; signature
before malformed JSON/invalid UTF-8; authentic but invalid/expired/rolled-back metadata; closed envelope objects and
duplicate members; forbidden feed-supplied keys; key-id/base64 normalization; and the exact 1 MiB boundary.
Capture an actual negative before implementation, then passing native checks, the required local proofs and two
fresh independent security reviews. Full-app memory/processor measurements and queued checks remain required
before any app integration or stable-version claim.
