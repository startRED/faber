# Stop rewriting the whole conversation on every helper step

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 1. `Core::keep_child_event` (`engine/src/engine.rs`)
calls `save` after every kept helper event, and `Store::save_checked` (`engine/src/store.rs`) serializes the whole
conversation and replaces its file, with the engine lock held. The owner's biggest conversation is 102 MB (35,902 tool
records, 92.6 MB of tool input and output); one rewrite costs about 0.8 s on his PC, so a busy helper keeps every
conversation waiting. The installed app used about 1.3 GB after 55 minutes.

What to build, measured before and after on a synthetic conversation of the same shape (never the owner's file):

- Coalesce saves per conversation: a helper event marks the conversation dirty; one save at most every 2 s, and always
  at turn end, on a permission or question, before quit (the existing quit flush) and when the conversation leaves
  memory. Save failures keep being surfaced as today.
- Keep bulky tool input and output out of the conversation file: the child-transcript pages already hold helper
  events; give the main agent's tool output the same page store and keep only the short preview in the conversation.
  Old files keep loading (read the old field, write the new shape).
- Page what the screen receives: opening sends the newest messages (the screen already shows 40) and older pages on
  scroll, instead of the whole conversation.

Done: a check that fails today shows one save for a burst of 50 helper events; the 100 MB synthetic case opens and
saves under a stated time; the commit states the memory and processor cost (R125). Extends: product-map entries for
conversations and the work log. Review by a fresh session of the other vendor (R146).
