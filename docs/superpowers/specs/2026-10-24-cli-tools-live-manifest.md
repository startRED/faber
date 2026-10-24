# Command-line tools and the live manifest

Status: in progress locally; task 998 remains open. Offline signature verification has six passing checks and two independent security reviews. No remote manifest, compatibility gate or command-line catalog item is built.

## Existing contract

Extends the Catalog and CLI installation entries in [the product map](../../product-map.md), the
[catalog architecture](../../product/architecture.md#curated-catalog-and-live-manifest) and
[distribution](../../product/distribution.md). Catalog remains the only browsing and management surface.
The official CLIs and tools are downloaded from their original sources, never bundled. No account profile,
credential, session token or chat website participates in discovery, qualification or publication.

Today the catalog is compiled from `engine/src/tools.json`. The Windows installer in `engine/src/install.rs`
selects the latest official release and checks its published digest. Neither path consumes a signed Faber
compatibility list. Installed-tool integrity checks must continue to apply independently of remote metadata.

## Deliver in three independently checked slices

1. **Publication contract, offline first.** Define one versioned, bounded signed document and validate it without
   network access or app startup. Produce no live feed and change no install behavior in this slice.
2. **Compatibility feed.** Qualify exact official CLI versions, publish evidence-backed entries, then consume the
   verified document at the existing installation boundary. Preserve a last-good copy and the bundled baseline.
   Only a successful exact-version/platform qualification permits a new managed install. Existing personal CLI
   installations remain untouched; an unsupported version is disclosed, never silently replaced.
3. **Command-line catalog items.** Add only individually qualified tools to the existing Catalog lifecycle and
   session selection. Each tool needs original-source pins, license, platform, requirements, archive limits,
   digest verification, reversible removal and an explicit executable delivery contract before its card appears.
   A generic package runner or arbitrary installation command is not a substitute for that contract.

## Signed document boundary

The first offline primitive is `scripts/live-manifest.mjs`, checked by `scripts/live-manifest.test.mjs`.
It uses Node's [built-in signature verifier](https://nodejs.org/api/crypto.html#cryptoverifyalgorithm-data-key-signature-callback),
with no new dependency, filesystem, network or app integration. It accepts a UTF-8 envelope of at most 1 MiB with
exactly `key_id`, `payload` and `signature`; the latter two are canonical padded base64. The key id is a lowercase
ASCII identifier of at most 64 characters. Only a caller-supplied pinned public Ed25519 key can authenticate it.
The result is authenticated UTF-8 bytes, not parsed JSON or authorization. Schema, cache and freshness checks below
remain unimplemented. Production signing material is never needed for its ephemeral in-memory test fixtures.

- One schema version, monotonically increasing revision, issued/expiry times and an explicit minimum supported app
  version. Reject unsupported versions, duplicate identities, unknown fields, oversized documents and invalid values.
- Sign the exact UTF-8 payload bytes with Ed25519. The envelope carries base64 payload/signature and a key id;
  the app trusts only public keys shipped with it. A key supplied by the feed cannot establish trust.
- The payload carries catalog metadata and compatibility records separately. A compatibility record binds provider,
  exact CLI version, operating system, architecture, outcome and immutable evidence identity. A green record requires
  evidence for protocol startup, one recorded turn, cancellation and process cleanup. Test fixtures alone do not
  qualify a real binary. No entry may grant permissions or choose a model.
- Provider policy switches can suspend new managed launches or installations at a session boundary, with plain words
  and evidence; they never kill active work, delete history or modify the person's account. A remote flag cannot
  increase access or override a person's disabled item.
- Declarative driver descriptors may select only app-compiled protocol variants. They contain no executable code,
  shell commands, downloaded modules, arbitrary endpoint or new permission. An unknown variant needs an app update.
- Verify signature before parsing the payload; then validate schema, freshness, revision and local support. Only
  after every check passes may the last-good copy be atomically replaced. A stale fetch cannot overwrite a newer one.
  A cached copy is reverified on load. Persist the accepted revision separately so an older signed document cannot
  roll it back. Missing or damaged trust state after initialization blocks new managed installs until recovery;
  it must never reset the accepted revision to zero. No first-download trust and no private signing key in the
  app, repository or agent instructions.
- Network failure, bad signature, rollback or invalid metadata preserves the last good copy. An expired copy may
  describe existing installed tools but cannot authorize a new managed install; the bundled baseline authorizes
  only its own qualified exact versions before any live document has been accepted. Once a live document has been
  accepted, cache corruption or expiry blocks new managed installs until a valid, current document at or above
  the accepted revision is recovered. The baseline may describe existing tools but cannot clear a suspension or
  authorize installation in that recovery state. Show the recovery status without interrupting active work.
- Fetch only from one app-owned HTTPS origin with bounded response, redirect policy, timeout and cancellation.
  Check on explicit Catalog refresh and existing maintenance opportunities, coalescing concurrent requests.
  No extra daemon, idle polling, CLI process or work in a hidden Catalog view.

## Qualification and publication

Nightly qualification runs exact official binaries in disposable environments on supported operating systems and
architectures. It uses synthetic projects and isolated sign-in-free protocol recordings where possible; an actual
authenticated smoke test needs an explicitly maintained external test account and cannot be faked by a recording.
A row lacking required evidence stays unqualified, with its reason. A failing night retains the previously qualified
version; it never promotes `latest` merely because the vendor released it.

Publication uses the existing release provenance workflow for the document and evidence. Production signing needs
the owner's selected signing/publication setup; agents must not inspect or create private signing material. Until
that setup and real qualification evidence exist, the offline contract must remain disconnected from installation.
Key rotation ships the next public key in an app update before the publisher uses it. Revocation and emergency
recovery must be exercised before enabling the first feed.

## Acceptance evidence

- Offline fixtures: valid signature, changed bytes, wrong key, malformed base64, excessive input, unknown schema,
  duplicates, missing evidence, invalid dates, expired authorization and unsupported protocol variant.
- Cache fixtures: corrupt/truncated file, failed replacement, concurrent refresh, monotonic revision, replay after
  restart, missing cache and offline fallback, including a suspension followed by corrupt cache and damaged trust
  state; neither may authorize a baseline install. No fixture touches a real account, network or personal installation.
- Installation proof: latest unqualified release loses to a qualified exact version; a policy suspension refuses new
  launches without interrupting an active turn; existing digest/source checks remain mandatory.
- Catalog proof: existing detail consent, disabled state, session selection, Undo, keyboard focus, narrow window,
  longest language and reduced motion still work. Each shipped tool gets a real platform smoke test and rollback.
- Two independent security reviews for signed metadata/executable installation; a fresh coherence review for the
  Catalog change. Local proofs and the queued tree's required checks pass before any shipped-status claim.
- Measure request bounds, idle CPU, resident memory and process cleanup. The document adds no runtime cost;
  implementation measurements are still outstanding. No recommendation or quality gain is claimed without evidence.
