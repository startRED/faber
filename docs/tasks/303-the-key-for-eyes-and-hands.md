# The key: one setting that lets the AI see other windows, off by default

Area: Settings screen
Files: engine-core/src/model.rs, engine-store/src/settings.rs, host/src/reach.rs, ui/Settings.tsx, ui/FirstRun.tsx, ui/locales/, tests/ui/settings.check.ts, tests/ui/first-run.check.ts, docs/design/settings.md, docs/design/first-run.md, docs/product-map.md, docs/decisions.md
Depends: move-existing-data
Size: about 400 changed lines

Why: plan item E4, owner's answers 1 to 4 (25/11/2026):
- eyes come in this update, hands only after the signed installer;
- both follow the conversation's permission mode, with brakes in code;
- the key appears on the install screen and starts off.

The plan says it plainly: with the key on, what is on screen goes to the AI of the person's subscription, and a malicious
text on screen can try to steer that AI.

Build:

- One stored setting, off by default, with a switch in Settings and a step on the first-run screen. Both say in plain words
  what turning it on sends, and the risk above.
- No second permission system. Once the key is on, a conversation in an ask mode still asks before each new window. Full
  Access does not ask.
- The setting is `Person` reach: the AI can never turn it on (`host/src/reach.rs`, `PERSON_SETTINGS`).
- A list of programs kept out of reach (password managers, banks, security tools), which the person edits. It is stored
  with the setting.

Leave out: the eyes themselves (next task) and hands (after the signed installer).

Done:
- A test that the AI cannot change the key or the list.
- Screen checks of the switch, the first-run step and the list.
- The first-run rule exception recorded as a decision row, citing answer 4.
- Permissions: two reviews from different vendors (R146).
