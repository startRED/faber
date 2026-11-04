# Sandbox Claude commands in guarded modes to protect Faber's data folder

Area: Permissions
Files: engine/src/claude.rs, engine/src/permissions.rs, engine/tests/project_security.rs, docs/product-map.md, docs/design/permissions.md
Depends: none
Size: about 400 changed lines

Why: `engine/src/claude.rs:411` adds Edit and Write denies for Faber's data folder outside Full access, but the command
sandbox is enabled only in Isolated mode (`engine/src/claude.rs:1052`, using `permissions::checked_input`). In Ask,
Accept edits and Auto, a remembered command grant can skip Faber's
request-text check and change both `decisions-kept.json` and `faber-trust/learning-kept.json`. An agent could then give
its own decisions, preferences or lessons the person's approval.

Build:

- Extend the existing command sandbox to Ask, Accept edits and Auto, keeping Faber's data folder and its aliases out
  of reach, including through an already granted Bash or PowerShell command. Cover both approval records, not just
  names mentioned in the command text.
- Preserve each mode's existing approval choices and permitted project commands. Keep Isolated's restrictions and the
  person's explicit Full access choice. Fail closed if the required sandbox cannot start; never open a console window.
- Add focused driver and security checks for both records and each guarded mode, including remembered grants, computed
  paths, aliases and a failed sandbox start. Use fake processes for unit tests; qualify the real sandbox separately.

Leave out: changing Codex, new permission modes, new settings, approval-record formats, and protecting Full access
against the access the person explicitly granted.

Done: checks that fail today prove guarded Claude commands cannot read or write Faber's data folder or forge either
record through a remembered grant, while allowed project commands still work. Extends: Permissions and the two approval
record rows in the product map; update the permission design page and what's-new entry in every language. Record memory
and processor cost and obtain two independent security reviews, one by the other vendor (R146).
