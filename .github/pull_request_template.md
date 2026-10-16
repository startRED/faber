## What changed and why

<!-- One or two sentences. The commit type in the title (feat, fix, perf, docs, ...) is a release decision, see AGENTS.md. -->

## For you

<!-- Paste the English `notes.<id>` entry this pull request adds to every `ui/locales/*.ts` file (R142); the release
     notes and the what's-new screen are built from those entries, not from this text.
     Write "Nothing a person sees." when that is true. -->

## Coherence

<!-- A change a person sees: `Extends: <product-map entry>` or `New: <closest existing thing and why it does not fit>`.
     Anything else: "Not a change a person sees." -->

## Cost

<!-- Memory and processor cost measured, or why there is none (docs, tests only). -->

## Proofs run

- [ ] `npm run lint`, `npm run typecheck`, `npm run build`
- [ ] `cargo fmt --all --check`, both `cargo clippy` commands
- [ ] `cargo t` (`cargo t --all` when the screen or the shell changed)
- [ ] `docs/product-map.md` and `docs/design.md` updated when they needed it
