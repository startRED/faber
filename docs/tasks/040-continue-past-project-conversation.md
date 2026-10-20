# A past project conversation can be continued

Parity row T2 ([parity.md](../product/parity.md#talking-and-steering)). Today a past project conversation opens read
only, with no composer (product map, "Past project conversation"), while Chat and every other app let you go on. Find
why it was made read only (search docs/decisions.md and the git log of `ui/Thread.tsx`); if no decision stands in the
way, give it the project composer and resume its CLI session as Chat does. If a decision does, take the question to
the owner in a short card and move on.

Done:
- a check in tests/ui/ opens a past project conversation, sends a message and sees the reply in the same conversation;
- the product map row says it can be continued, and the row T2 leaves parity.md;
- a what's-new line in every language.
