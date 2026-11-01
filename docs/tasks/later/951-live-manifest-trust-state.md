# Keep the last good live manifest and the accepted revision, still unconnected to installs

Area: none
Files: engine/src/manifest.rs, engine/src/lib.rs, engine/src/store.rs, engine/tests/manifest.rs, docs/superpowers/specs/2026-10-24-cli-tools-live-manifest.md
Depends: live-manifest-schema
Size: about 400 changed lines

Why: slice 1-2 of `docs/superpowers/specs/2026-10-24-cli-tools-live-manifest.md`, "Signed document boundary". The app
must trust only public keys shipped with it, verify the signature before parsing, validate, and only then atomically
replace the last good copy; a stale fetch cannot overwrite a newer one; a cached copy is reverified on load; the accepted
revision is persisted separately so an older signed document cannot roll it back; missing or damaged trust state after
initialization blocks new managed installs and never resets the revision to zero. Nothing of this exists in the engine:
the only code is the Node script `scripts/live-manifest.mjs`, and `engine/src/install.rs` knows no manifest.

Build:

- A new engine module with: the Ed25519 verification of the envelope in Rust (check the `ring` crate first: the
  updater already compiles it, `engine/Cargo.toml` uses it for TLS; add no new dependency if it can do this), the
  payload validation from the schema task re-implemented against the same fixtures, the last-good copy, and the
  accepted-revision record, both under the data folder, written atomically (temp file, flush, rename) with a failed
  replacement keeping the old copy.
- States as a pure function with unit tests: no document yet (bundled baseline authorizes only its own qualified
  exact versions), accepted and current, expired (may describe installed tools, cannot authorize a new install),
  corrupt cache or damaged trust state after acceptance (blocks new managed installs until a valid current document at
  or above the accepted revision is recovered; the baseline cannot clear a suspension).
- Ship no public key and no fetch: a test key only. Key material and the fetch origin are the owner's setup.

Leave out: any call from `install.rs` or the screen, the HTTPS fetch, the compatibility gate, command-line catalog
items, and the qualification workflow. Those wait for the owner's production signing and publication setup and for real
qualification evidence (a decision card, not a task).

Done: tests in `engine/tests/manifest.rs` that fail today for the cache fixtures of the spec (corrupt or truncated file,
failed replacement, replay after restart, missing cache, suspension followed by corrupt cache and damaged trust state
never authorizing a baseline install). The commit body states idle cost (none: nothing runs). Extends: the Catalog and
CLI installation entries of the product map; add a row for the data files. Two independent security reviews, one by the
other vendor (R146).
