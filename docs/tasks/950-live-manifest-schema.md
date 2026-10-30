# Check the live manifest's payload offline: schema, revision and expiry

Area: none
Files: scripts/live-manifest.mjs, scripts/live-manifest.test.mjs, docs/superpowers/specs/2026-10-24-cli-tools-live-manifest.md
Depends: none
Size: about 350 changed lines

Why: slice 1 of `docs/superpowers/specs/2026-10-24-cli-tools-live-manifest.md` (command-line tools and the live
manifest; the product map lists both as not built). `scripts/live-manifest.mjs` already verifies the Ed25519 envelope
(`verifyEnvelope`, at most 1 MiB, pinned key only) and returns authenticated UTF-8 bytes; "schema, cache and freshness
checks below remain unimplemented". This task writes the schema check on those bytes, offline, with no network, no
filesystem and no app integration, and changes no install behavior.

Build:

- `parsePayload(bytes, now, appVersion)` in the same script: parse after the signature check, then require one schema
  version, a monotonically increasing integer revision, issued and expiry times, and a minimum supported app version;
  reject unknown fields, duplicate identities, oversized documents and invalid values.
- Catalog metadata and compatibility records stay in separate lists. A compatibility record binds provider, exact CLI
  version, operating system, architecture, outcome and an immutable evidence identity; a green record requires evidence
  for protocol startup, one recorded turn, cancellation and process cleanup. No entry may carry a permission or choose
  a model.
- Driver descriptors may select only named protocol variants; a field that looks like code, a command, a URL or an
  endpoint is rejected. Provider policy switches carry plain-words text and evidence and can only suspend.
- Fixtures cover the spec's list: changed bytes, wrong key, malformed base64, excessive input, unknown schema,
  duplicates, missing evidence, invalid dates, expired authorization and an unsupported protocol variant. Use ephemeral
  in-memory test keys only; never real signing material.

Leave out: the cache and the accepted-revision state (task live-manifest-trust-state), any fetch, production signing,
publishing, and every change to `engine/src/install.rs`.

Done: tests in `scripts/live-manifest.test.mjs` that fail today for each fixture above (run by `npm run test:scripts`);
the spec's status line updated. Extends: the Catalog and CLI installation entries of the product map (no change to the
app yet, so no map row changes). The commit body says "no memory or processor cost: script and tests only". One review
by the other vendor; the spec asks two independent security reviews before any of this connects to an install, so the
second may wait for task live-manifest-trust-state (R146).
