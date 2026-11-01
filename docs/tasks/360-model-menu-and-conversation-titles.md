# Model menu and conversation titles

Area: Composer, Sidebar
Files: ui/Composer.tsx, ui/ModelMenu.tsx, ui/models.ts, ui/styles.css, ui/locales/, engine/src/engine/title.rs, engine/src/engine.rs, engine/src/budget.rs, engine/tests/title.rs, tests/ui/models.check.ts, tests/ui/composer.check.ts, docs/design/composer.md, docs/product-map.md
Depends: none
Size: two pull requests: first the menu and model/effort chip, then titles

Why: Titles are truncated first messages (walkthrough shot `reports/reforma/w10/shots/e-tools-long/e-long-details.png`). The model menu is a flat native list without grouping or effort (shot `reports/reforma/w10/shots/e-tools-sonnet/e-son-model-menu.png`).

Build: Use existing popover and keyboard patterns for a styled menu opening down when room permits: Auto first, connected AI groups with marks, current models first and older ones folded. Define current from Auto profiles and the account default; check the rule against the owner's list. Show search only above eight models; put effort inside the menu and display model and effort in one chip, preserving default/reset and selection behavior, keyboard, screen reader and Esc. After the first answer, ask the cheapest connected AI once for at most six words in the first message's language, or Settings language for link-only or code-only messages. Use the side-question quota gate and subscription billing, never overwrite a manual rename or a changed title, and time out at 30 seconds. Filter quotes and control characters as rename does. Give bare links an immediate localized host fallback.

Leave out: new models, Auto routing, permission choices, retitling, project goals, picked-up sessions and native CLI thread names.

Done: Screen checks fail today and prove groups, folded older models, search by partial name, downward opening at 720 px, one chip and unchanged choice handling. Title tests fail today and prove one call, six-word limit, no overwrite after rename, no call when quota denies, timeout fallback and localized link host. Update map and design, add what's-new in every language, walk 720 px, longest language, reduced motion and both themes, and compare visual critique. Get fresh and coherence reviews. Report menu cost and one small model call per eligible conversation.
