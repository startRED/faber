# Make the Full access warning short and plain without losing a warning

Area: Permissions
Files: ui/locales/en.ts, ui/locales/pt-BR.ts, ui/locales/es.ts, ui/locales/fr.ts, ui/locales/de.ts, tests/ui/settings.check.ts
Depends: none
Size: about 150 changed lines

Why: `docs/research/2026-10-29-faber-diagnosis/README.md`, part 4: a 90-word security text in Settings is unreadable.
It is `set.permissionsDontAskBody` (`ui/locales/pt-BR.ts` line 1299; en line 1298) and its sibling
`set.permissionsAskBody`, plus the `.more` texts of the modes (`set.permissions.acceptEdits.more`, `.plan.more`,
`.auto.more`, lines 1311 to 1313). The pt-BR text of `set.permissionsDontAskBody` is one block: Chat and Projects run
commands and change files without asking, with internet access, in any folder the person chooses except system and
sign-in folders; the person's own connected tools and hooks stay on; a page or file the AI reads can make it change
files or use the network without asking; Codex can read the sign-in and key folders (`.claude`, `.codex`, `.ssh`) directly
and Claude Code can through commands, and the block on those folders in commands is best effort, so use Ask for what is
not trusted; Stop asks the tools to stop and ends the programs they started.

What to build: split it into short plain sentences (under 25 words each) in a short list: what Full access does, what
can go wrong (a page the AI reads can steer it; sign-in and key folders can be read), what to do (use Ask for what is not
trusted), and what Stop does. Every warning stays; none is softened, none is dropped. Write English (`en.ts`, the source)
and each language by hand. Do not change what the setting does.

Leave out: any change to the permission modes themselves (`docs/product-map.md` "Permissions"; Plan being listed as an
access level is the question of task `access-choices-say-what-they-allow`), the other Settings words (task `plain-words-in-settings`).

Done: a check in `tests/ui/settings.check.ts` fails today because the Full access text is one 90-word block; it passes
with each point of the list above (the five warnings and what Stop does) found as its own sentence, in English and pt-BR, and none longer than 25 words. A
security text loses no warning, so this change needs TWO reviews by fresh sessions (R146: one of the other vendor, one
more), each given the old and new text side by side and asked to list any warning that was weakened. The owner reads
the pt-BR diff in a short card before landing. Extends: product-map "Permissions". What's-new entry in every language.
Walk it at 720 px and in the longest language. No memory or processor cost.
