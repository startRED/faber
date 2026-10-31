# A conversation file that cannot be read shows up as damaged, with its copy kept, instead of vanishing

Area: Conversations
Files: engine/src/store.rs, engine/src/model.rs, ui/Recents.tsx, ui/locales/, tests/ui/recents.check.ts, engine/tests/store_summaries.rs, docs/product-map.md
Depends: none
Size: about 250 changed lines

Why: `Store::list` (engine/src/store.rs, around line 295) drops any file it cannot parse and only says "tried again at the next list". A file cut by a power loss or a full disk stays unreadable forever, and its conversation is simply missing, with no word and no copy. `budget.json` already sets a damaged file aside as `.quarantine`; conversations do not.

Build: after two failed reads in a row (the first may be a scanner lock), copy the file once to `<id>.json.damaged`. The list returns a row marked damaged (key `reason.conversation_damaged`). Recents shows it quietly: "This conversation could not be read." with "Show the file" and "Remove". Remove deletes only the `.damaged` copy after a confirm; nothing is ever repaired or rewritten.

Leave out: repairing partial JSON.

Done: an engine test that fails today: a truncated `conversations/x.json` yields a damaged row and a `.damaged` copy, and a later good rewrite clears the mark. A screen check shows the row and both buttons. Extends "Conversations" / Recents. What's-new entry in every language. Cost: one extra copy of a bad file, once.
