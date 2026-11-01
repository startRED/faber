# Settings in plain words and by keyboard

Area: Settings screen, Composer, Permissions
Files: ui/Settings.tsx, ui/Accounts.tsx, ui/Composer.tsx, ui/controls.tsx, ui/Appearance.tsx, ui/ProjectSettings.tsx, ui/styles.css, ui/locales/, tests/ui/settings.check.ts, tests/ui/composer.check.ts, tests/ui/money-words.check.ts, testkit/src/screen/settings.rs, docs/design/settings.md, docs/design/composer.md, docs/product-map.md
Depends: none
Size: two pull requests: first plain words and access choices, then keyboard traversal

Why: Settings uses unexplained hooks, CLI, cache, permission cards, API value, quota windows and symbols, and inconsistent names for the AI and app. The access menu hides what each choice permits. Full keyboard traversal has not been proven.

Build: Search all five languages and rewrite existing Settings text by hand in short plain sentences, normally under 25 words, explaining necessary technical terms next to them. Use the product map's consistent words; do not revive audit terms no longer present. The owner reviews the pt-BR wording before landing. Show each existing access description while choosing in both composer and Settings, extending the existing choice control; preserve choices and composer position. Ask the owner separately whether Plan belongs there, since that changes a decision. On the hidden screen test open Settings by shortcut, traverse all sections and controls in reading order by keyboard, operate each with appropriate keys, show a visible nonzero focus ring, and return focus after Esc. Also check focus order and hidden controls in the fast UI test; fix only implicated components using existing styles.

Leave out: new settings, unrelated conversation wording and first-run or work-screen keyboard coverage; do not weaken security text.

Done: Checks fail today on the old pt-BR and English examples and on access choices missing descriptions; verify current choice, Esc and returned focus. Keyboard checks cover 720 px and longest language; if no defect exists, remove focus or a ring once to prove the guard fails, then restore it. Walk access in light and dark, reduced motion and longest language; compare visual critique before and after. Update map and affected design pages, note skipped absent audit terms, add what's-new in every language when behavior or words change, and obtain fresh and coherence reviews with taste cards checked. Report runtime and screen-test cost.
