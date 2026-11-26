# The AI operates Faber through its own command table

Area: Shell commands
Files: engine-drivers/src/helper.rs, engine/src/engine.rs, engine/src/engine/faber_tool.rs, host/src/table.rs, host/src/lib.rs, host/src/reach.rs, engine/src/sharing-sources.txt, docs/product-map.md, docs/decisions.md, docs/design/app-control.md
Depends: none
Size: about 700 changed lines

Why: plan item F1, owner's answer 6 (25/11/2026). The AI operates Faber in everything except signing in, spending money,
and changing its own permissions, trust and hooks. The table already exists (`host/src/table.rs`, R459), and so do the
reach checks (`host/src/reach.rs`, `Host::dispatch_as`, R499). `cli/src/mcp.rs` already serves every command to an outside
agent under its channel identity. A conversation, though, has no tool for these commands. It simulates clicks in the hidden
app renderer (`browse` with `faber-live`), which is slow and costs tokens.

Build:

- Add one conversation tool, `faber`, with two actions:
  - `find`: takes words and returns the few matching commands, each with its short description and input schema;
  - `run`: takes a command name and its arguments.

  Offer it in Chat and Projects to every driver, the same way the other Faber tools are offered (`engine-drivers/src/helper.rs`,
  and `engine/src/engine.rs` near the tool flags). The tool list costs one definition, not one per command.
- `run` goes through `Host::dispatch_as` with a caller that names the conversation, so every command keeps its reach.
  `Person` stays refused. `Ask` records the request and answers `error.reach.ask`, as today.
- Signing in, money, permissions, trust and hooks are `Person` reach. List which commands that covers. Make `Person` any of
  them that are not yet, with one test per group.
- `find` never returns a command the caller could not run at all.

Leave out:
- Running `Ask` commands without a card: that is the next task.
- The app renderer: it stays for what the table does not cover and for checking the screen.

Done:
- Tests that:
  - `find` returns a command by its words;
  - `run` reaches a `Read` command;
  - a `Person` command (signing in, a permission or trust change) is refused through the tool;
  - an unknown command is refused.
- Unit tests never spawn processes.
- Record the owner's answer 6 as a decision row.
- Permissions: two reviews from different vendors (R146).
