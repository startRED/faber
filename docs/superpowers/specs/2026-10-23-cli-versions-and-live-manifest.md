# Recorded version checks and the live manifest

Status: In progress. Slice 1 passes four focused checks, independent source review and all seven local proofs. It is not yet qualified remotely. Later slices remain proposals.

## Boundaries

The existing catalog delivers checked skills, plugins and tool servers from pinned original sources.
Command-line tools such as ripgrep, gh or ffmpeg would be a distinct delivery kind, not skills disguised as installers.
They need source and version pins, whole-artifact digests, platform support, license review, safe installation,
removal and launch validation before admission. Slice 1 adds no tool download or catalog card.

The engine already parses recorded sessions. Slice 1 attributes that evidence to the exact version recorded in
those fixtures. It never claims that a newer installed binary works because an older recording passed.
It changes no shipped app code, account, installed version, launch policy or user setting.

## Slices

1. **Offline recorded parser evidence.** Use the existing production parsers and chat, tools and resume fixtures.
   Require attributable versions and the expected event shapes. Emit a result per provider, version and fixture.
   Reject malformed input, missing versions, mismatched versions and missing event shapes. Codex startup replies
   and approval requests are session-driver responsibilities; this notification-parser slice does not cover them.
   The optional scheduled workflow runs only these offline checks. A manual run opts in; scheduled runs require
   the repository variable to be enabled. No unit test spawns a process, calls a real CLI or uses the network.
2. **Newest-binary compatibility harness.** In an isolated integration environment, obtain exact upstream versions,
   verify their identity and exercise their real session drivers with controlled inputs and protocol recordings.
   Recorded output alone cannot prove the behavior of a newly downloaded executable. Separate recorded parser
   results from actual binary, driver, startup, tool, approval and resume results. Failed or untested cases must
   never authorize an update. No personal credentials or provider tokens enter reports or recordings.
3. **Signed static manifest.** Publish a bounded, versioned compatibility matrix, provider policy switches and
   declarative driver descriptors. Bind every result to exact binary identity, platform, tested capability and
   proof provenance. Descriptors are validated data, never downloadable executable code. Define expiry, rollback,
   signature verification, incompatible schemas and policy precedence before implementing a reader.
4. **Nonblocking reader and delivery.** Fetch without blocking launch, validate before replacement and retain the
   last good signed copy when offline or invalid. Keep telemetry off. Only qualified versions become eligible;
   a failed refresh cannot silently relax local policy. No backend service is needed for the static files.
5. **Command-line catalog delivery.** Specify and qualify one useful tool using the existing catalog's lock,
   integrity, recovery and inventory boundaries before expanding the tool list.

## Owner decides

These are proposals, not approvals; previously unanswered owner questions remain unanswered.

- Public hosting and publication: recommend static files in the existing project infrastructure to avoid a server;
  publishing a manifest or creating a public repository requires the owner's authority.
- Signing identity and key custody: recommend a dedicated signing identity with a documented recovery procedure;
  no key is created, read or stored by this slice.
- Default CLI updates: recommend retaining the installed version until the exact platform/version passes the
  complete integration matrix; changing which version people receive requires the owner's choice.
- Provider policy switches: recommend allowing remote data to restrict compatibility, never grant permissions;
  policy precedence and delivery remain pending the existing owner decision.
- Paid infrastructure or provider use: recommend offline evidence first and bounded integration runs later;
  any new cost or account-dependent test needs its own authorized budget and execution boundary.

## Acceptance and cost

Slice 1 needs focused executable checks, rejection evidence, an independent review and the normal local proofs.
Its workflow must remain opt-in and must not publish results as a production compatibility verdict.
Passing recorded fixtures is not the nightly newest-binary test described in the product roadmap.
Task 998 stays open. The product's live manifest, CLI-tool delivery and nightly binary qualification remain unbuilt.

The change adds only test and workflow work: no app idle memory, processor use or persistent background process.
Test runtime and compilation costs are recorded with the actual proof, not estimated as measured results.
