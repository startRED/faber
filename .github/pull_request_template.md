## What changed and why

<!-- One or two sentences. Title the pull request `type(scope): summary` (feat, fix, perf, docs, test, ci, refactor,
     chore, style, build, revert), as `.github/workflows/pr-title.yml` checks, and target `phase-1-base`. -->

## Related issue

<!-- `Closes #<number>`, or delete this section. -->

## Checks run

- [ ] `npm run lint`, `npm run typecheck`, `npm run docs`, `npm run build`
- [ ] `cargo fmt --all --check`, `npm run test:scripts`, `node scripts/cargo.mjs t --claims`
- [ ] The heavy suites and Clippy run in CI; a maintainer queues and lands an approved pull request

## For user-facing or state and process changes

<!-- Optional for documentation and small fixes. -->

- **Coherence.** A change a person sees: `Extends: <entry of docs/product-map.md>` or `New: <closest existing thing and
  why it does not fit>`. Anything else: "Not a change a person sees."
- **Memory and processor cost.** Needed when the change touches processes, long-lived state or how much the screen
  keeps drawn. Measured, or why there is none.
- [ ] `docs/product-map.md` and the `docs/design/` pages updated when they needed it
