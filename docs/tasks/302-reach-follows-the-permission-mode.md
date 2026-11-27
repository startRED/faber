# In Full Access, the AI runs Faber's "ask first" commands without a card

Area: Shell commands
Files: host/src/reach.rs, host/src/lib.rs, host/src/ask.rs, engine/src/engine/faber_tool.rs, ui/locales/, docs/product-map.md, docs/decisions.md, docs/design/app-control.md
Depends: faber-operates-itself
Size: about 400 changed lines

Why: plan item F2, owner's answer 6 (25/11/2026). Today, when the AI calls an `Ask` command, Faber records a request
(`host/src/ask.rs`, `error.reach.ask`) and waits for the person. It does so even in Full Access, where the conversation
already runs every other tool without asking. Following the conversation's mode keeps a single permission system.

Build:

- When the calling conversation is in Full Access (`engine-core/src/model.rs`, `Permissions::FullAccess`), an `Ask` command
  runs at once and leaves a line in the conversation saying what it did. Other modes keep the card.
- `Person` commands never follow the mode. Signing in, money, and the AI's own permissions, trust and hooks stay with the
  person in every mode, so a malicious text cannot make the AI raise its own power.
- In Full Access, the AI may open a panel to show the person something, such as a Settings section or a Catalog item.

State (2026-11-26): built on `reach-follows-mode` (R587) and fixed after two reviews (R146), not landed. Deleting for
good, spending quota, the installer, `open_link`, `stop_aside`, the tool commands and another family's conversation keep
their card in Full access, and opening one Catalog item is left out (only the Catalog opens); each is the owner's to widen.

Leave out: any change to which commands are `Person` (previous task).

Done:
- Tests that:
  - an `Ask` command runs without a request in Full Access;
  - the same command records a request in Ask mode;
  - a `Person` command is refused in Full Access.
- A screen check of the line the conversation shows.
- Permissions: two reviews from different vendors (R146).
