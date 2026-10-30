# Show the start card exactly as it was sent

Area: Technical details
Files: engine/src/engine.rs, engine/src/events.rs, engine/src/model.rs, ui/Details.tsx, ui/api.ts, ui/locales/, docs/design/
Depends: start-card-in-sessions
Size: about 250 changed lines

Why: the spec's "The start card" (`docs/superpowers/specs/2026-10-22-own-work-method.md`) says Technical details shows
the card as sent, so the person can see what the agent was told about the project. Once the card reaches the CLIs, the
exact text must be kept with the session and shown, never rebuilt later from files that may have changed.

Build:

- Keep the text of the card with the session that received it (in memory, with the conversation's running record; it is
  at most 6 KB, so no new file) and send it to the screen on request only, not with every event.
- Add one quiet row to Technical details of a project conversation, "What the agent was told about this project", that
  opens the card in a read-only block with its byte size and the counts of what did not fit. Reuse the view's existing
  row and fold components in `ui/Details.tsx`; no new surface.
- Every text in `ui/locales/`, by hand, in every language.

Leave out: editing the card, a setting for it, and showing it in Chat.

Done: a screen check (`tests/ui/*.check.ts`) that fails today shows the row, the text equal to what the fake CLI
received, and nothing for a conversation that got no card; walk it at 720 px and in the longest language. The commit
body states memory and processor cost. Extends: the product-map row "Technical details" and the design page of Technical
details (`docs/design/`). What's-new entry in every language. One review by the other vendor (R146), plus the coherence
review of a change a person sees.
