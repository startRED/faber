# Exploratory frontend evaluation

Status: completed local comparison; no causal or statistical superiority established.

This is one exploratory pair (N=1), made on October 22, 2026 in Fortaleza
(October 23 UTC). Both authors inherited the same session configuration and brief.
The session identified Codex based on GPT-6, but the exact runtime model variant
was neither exposed nor pinned. There were no separately specified model calls.

## Exact shared brief

> Faça um site de uma pequena padaria chamada Casa do Trigo, em Fortaleza. Quero ver os pães do dia, horário e endereço de exemplo claramente identificados, e reservar até 12 unidades pra retirada. Precisa funcionar bem no celular e no computador. A reserva aqui é demonstração local: inclua um modo de testar espera, erro e sucesso sem enviar nada a um servidor; preserve os dados quando falhar. Faça um visual próprio pra padaria e deixe tudo fácil de usar com teclado.

## Conditions and evidence

Both samples are standalone native HTML/CSS/JavaScript pages with original inline
SVG, system fonts, fictional bakery details and local-only simulation. Neither
used external assets, installed dependencies, a backend or publication.

The conditions were not a clean single-variable intervention:

- A consulted the full main taste instructions (first 420 lines), part of its
  critique reference, ponytail full and the first 170 lines of TDD. It ran the
  taste inspector and source checker. It did not consult craft/proof.
- B consulted complete frontend-craft and frontend-proof, only the first 120 and
  last 180 lines of taste, ponytail full and the first 160 lines of TDD. It also
  consulted debugging instructions, visited a bakery reference and used a design
  draw. It did not run the taste inspector.

Different consultation, references, iteration and verification prevent attributing
any difference specifically to the new instructions. Both authors used the same
existing headless browser helper. Their receipts disclose their actual choices.

Local evidence is under `target/frontend-evaluation/`: each sample's receipt and
assertion output, the frozen `blind-review.md`, and `matched-captures.json`.
These are disposable local artifacts, not durable checked-in attachments.

The root session recaptured both pages in Headless Chrome 156.0.8078.47 at matched
widths 320, 390, 768 and 1440. Document width matched viewport width in all eight
measurements; each page had one primary heading. Twelve captures were inspected:
four full-page widths and first folds at 390/1440 for each sample. The independent
reviewer saw only blinded labels, captures and source until scores were frozen.

The capture receipt maps the frozen HTML bytes as follows (SHA-256):

| Sample | Blind label | Bytes | SHA-256 |
| --- | --- | --- | --- |
| A | field | 19,464 | `8bb49d667db9a22f71885d666b37a9f13880b19475ad4dc2399a3b8b4e5e0909` |
| B | river | 17,138 | `1ea36f2dca87a15020edbf8e20c34bd16619c45c55754139722861b659e9532f` |

These hashes identify HTML, not individual PNG files. Generation and scoring were
not rerun after unblinding; the original review and samples were not altered.
A separate runtime follow-up is recorded below.

## Frozen independent assessment

Scale: 0 absent/broken, 1 major weaknesses, 2 mixed, 3 good with limitations,
4 particularly strong for this brief. Scores are judgments, not effect sizes.

| Axis | A / field | B / river |
| --- | --- | --- |
| Task clarity | 4 | 3 |
| Hierarchy and legibility | 3 | 4 |
| Composition and identity | 4 | 3 |
| Responsive layout | 3 | 4 |
| States and semantics | 3 | 3 |
| Total | 17/20 | 17/20 |

A made the fictional demonstration explicit earlier, provided direct reservation
navigation and had a distinctive navy split-stage composition with differentiated
bread drawings. Its desktop brand panel restricted the working area; its long
simulation option was visibly clipped in closed mobile selects. Product text
wrapped heavily at 320, and tablet product details and quantities were far apart.

B placed the desktop menu and reservation beside each other, emphasized prices
and input borders and grouped the form clearly. Alternating product rows made
selection easier to scan. Its tall mobile hero delayed menu discovery, and there
was no direct reservation navigation link. Its composition was coherent but more
conventional than A's; that preference alone is not usability evidence.

The reviewer narrowly preferred B for completing a desktop reservation because
the menu and form share a working band. There was no clear mobile winner: A
reached functional context sooner, while B grouped the working form more clearly.
The overall scores tie. This does not demonstrate that either instruction set
improves output quality in general.

## Runtime evidence, distinct from visual judgments

A recorded 24 passing assertions at `2026-10-23T01:28:54.790Z`; B recorded 16.
These are different suites with unequal coverage, not comparable pass rates or
evidence that A is 50% better. Both exercised quantity caps, empty selection,
failure preservation, waiting feedback, duplicate prevention, cancellation,
success, Tab reachability, four widths, runtime exceptions and network observation.

A additionally recorded negative/fractional quantities, whitespace names,
HTML-looking names treated as text, mixed price sums, manual wait completion and
focus return, ArrowUp quantity changes, reduced motion and a no-script fallback.
B recorded doubled-root-text overflow at 320 and reported reduced-motion and
forced-colors emulation. Its actual eight-second wait completion was not exercised;
waiting entry/cancellation and the standard success timer were exercised.

Both initial pre-implementation failures were missing-file assertions. This is
weak RED evidence: it shows absence of the page, not failure of requested behavior
in an existing implementation. B later found and corrected actual doubled-text
overflow. A's taste inspector still exited 1: its reported remaining failures
were eyebrow, few-pictures and still-page; a functional pass is not a style pass.

## Supplemental completed-wait proof

After scores were frozen, the root session independently exercised B's actual
eight-second wait to completion, without editing either HTML or rescoring. A fresh
hidden browser entered a synthetic name and two units, submitted in wait mode,
observed busy/disabled state and checked again after 8,259 ms. It reached the
explicit local-only success message, unlocked the fields and kept both inputs.
No external requests or script exceptions were observed. The receipt is
`target/frontend-evaluation/wait-completion-receipt.json`; this closes that runtime
gap in the author's original report, not the comparison's causal limitations.

## Limits

The blind reviewer inspected all twelve real captures and both HTML sources, but
did not execute interactions. Runtime evidence above comes from author receipts,
not an independent rerun. Static first states cannot prove dynamic announcements,
focus handling, data preservation or completion of every simulation path.

No screen-reader study, full keyboard-only task, other browser engine, physical
phone, real browser zoom or accessibility AA conformance was established. Doubled
root text is not equivalent to browser zoom. Visual taste and heuristic scans are
not accessibility certification. Memory, CPU and frame timing were not measured.
No Faber application proof or build was run for this isolated comparison.
