# Say clearly which AI ran out of quota and which took over

Area: Quota window, pace, plan
Files: engine/src/model.rs, engine/src/engine/recovery.rs, ui/api.ts, ui/Thread.tsx, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/quota.check.ts, engine/tests/route.rs
Depends: none
Size: about 200 changed lines

Why: the live look of 2026-10-29. A reply header read "Sonnet is out of quota, the conversation went on in Claude": the
sentence names vendors only, beside the model's name. In the code, `reason.moved.full` and `reason.moved.summary`
("{from} has no quota. Continued on {to} ...", `ui/locales/en.ts` lines 1573 and 1574) are filled in `Answer`
(`ui/Thread.tsx` line 436) with `vendorName(m().movedFrom)` and `vendorName(m().vendor)`, and the engine stores only a
`Vendor` for the one that ran out (`Message.moved_from`, `engine/src/model.rs` line 389, set by
`m.moved_from = m.vendor` in `resume_move`, `engine/src/engine/recovery.rs` near line 375). The header next to it shows
the new model (`prettyModel`), so a move between two models of one vendor, or a vendor name next to a model name, reads
wrongly. Reproduce it with a screen check first; this brief does not know which move produced the photo.

What to build: keep the model that ran out too (`Message.moved_from_model: Option<String>`, set beside `moved_from`,
`serde(default)`), and say one sentence that names the one that ran out and the one that took over in the same words the
header uses: vendor names when the vendors differ ("Claude está sem cota; a conversa continuou no Codex"), and nothing
about quota when the vendor is the same (a move inside one vendor is a model choice, not a vendor's quota; the header's
model name already says what answers). Both `reason.moved.*` keep their "with the previous messages" or "with a summary"
part. An old message without the model keeps today's words.

Leave out: the wait-for-quota card (`QuotaCard`), the pace sentences (`reason.pace.*`), what Auto picks.

Done: a screen check in `tests/ui/quota.check.ts` fails today by rendering "Claude has no quota. Continued on Claude" for
a same-vendor move; it passes with the new words for both cases, and an engine test checks `moved_from_model`. Extends:
product-map "Quota window, pace, plan". What's-new entry in every language. Walk it at 720 px and in the longest
language. No memory or processor cost beyond one string per message. Review by the other vendor (R146).
