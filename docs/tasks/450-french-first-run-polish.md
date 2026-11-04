# French first-run polish: the selected cue, the Projects folder line and a truncated label

Area: First run
Files: ui/locales/fr.ts, ui/ (first-run appearance step, sidebar new-conversation button), tests/ui/
Depends: none
Size: about 60 changed lines

Why: the 2026-11-04 first-user pass for the 1.0 gate (docs/dev/release-1.0.md, reports in docs/research/first-user-2026-11-04-*.md) found three low problems, all in French:
1. The experience choice in the first-run appearance step shows no selected cue (`primeira-abertura-3.png`), although Continue works and General later shows the value selected.
2. The disconnected Projects message says the AI works in "vos projets" without tying a project to a local folder, unlike the English and pt-BR lines (`projetos-2.png`).
3. At 720x480 the new-conversation button shows "Nouvelle conver..." (`primeira-abertura-8.png`), though its accessible name is complete.

Build: first check what happens now in each language. Show the selected state of the experience choice on first run. Write the French Projects line by hand to match the folder idea of the other languages. Make the new-conversation label fit at 720x480 in French, by a shorter hand-written French label or a layout that fits, and check the longest language. A check per fix fails without it. Add a what's-new entry in every language as AGENTS.md says.

Leave out: other languages and any new setting or step.

Done: the three screenshots no longer show the problem at 720x480 and 1280x800 in French; the checks fail without the change.
