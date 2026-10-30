# Give each conversation a short title in the person's language

Area: Sidebar
Files: engine/src/engine.rs, engine/src/engine/title.rs, engine/src/budget.rs, engine/tests/title.rs, engine/tests/all.rs, ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts
Depends: none
Size: about 350 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 3 ("Header and sidebar"): the title is the first prompt cut
to 60 characters, often a bare URL, so the sidebar and the header read "https://github.com/pingdotgg/t3code/..." and
the owner's conversation titles are raw links. In the code: `c.title = req.text.split_whitespace()...chars().take(60)`
in `Core::send` (`engine/src/engine.rs` line 4438); a project goal does the same (`engine/src/engine/goals.rs` line 511)
and a picked-up session too (`pick_up.rs` line 36). `rename` (line 4793, `Engine::rename` line 1082) already lets the
person change it, and stays.

What to build: after a conversation's first answer ends, the engine asks the cheapest model of a connected AI, once,
for a title of at most 6 words in the language of the person's first message (or `Settings.language` when the message is
only a link or code), using the same door every side question uses (`Core::allows` in `engine/src/engine/auto.rs` with a
new `Purpose::Title` in `engine/src/budget.rs` line 343, subscription billing only, never when the quota gate refuses,
never for a conversation the person renamed). The reply is trimmed, stripped of quotes and control characters (the same
filter as `rename`) and replaces the cut prompt only when the title is still the cut prompt. When no AI answers in 30 s
the title stays as it is. A first message that is a bare link gets the fallback "Conversa sobre github.com" (a key with
the host) at once, before any model is asked, so the sidebar is never a raw link.

Leave out: the sidebar and header layout (tasks `clear-topbar`), retitling when the topic changes, renaming Codex
threads (a follow-up of task `name-codex-threads`), titles for project goals and picked-up sessions.

Done: an engine test in `engine/tests/title.rs` fails today (the title is the first 60 characters) and passes with the
fake CLI answering a title once, never twice, never over a rename, never when the quota gate says no, and a link
message getting the host fallback; every language has the fallback key by hand. Extends: product-map "Sidebar" (how a
title is made). What's-new entry in every language. The commit states the cost: one small model call per conversation,
none in Auto's quota-tight windows. Review by the other vendor (R146).
