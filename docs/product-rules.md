# Product rules

Faber has to feel like one product, whoever builds the next part. This page says where a new thing goes and which lessons
it must not break. The details live in [design.md](design.md) and [roadmap.md](roadmap.md); R-numbers are the decisions
in [decisions.md](decisions.md) and C-numbers are the taste cards below. When a change would break one of them, ask the
owner first; never ship it and explain later. A new decision takes the next R-number in decisions.md, in the same commit
as the change it governs.

## Where things go

| What it is | Where it lives | Never |
| :--- | :--- | :--- | :--- |
| The work (a conversation, a project goal) | The area's screen: hero, composer, a short "continue" list, one quiet status line (C011) | Switches, option rows, command lists or long explanations on that screen |
| A setting | Settings, in its sections (General, Appearance, Connected accounts, Usage limits, Privacy): one short line per option, details behind "Learn more" (C011) | A work screen; a new section when an existing one fits |
| A choice everyone must make before Faber is usable | A first-run step (today: accounts, appearance) | A new step for anything a good default covers |
| A one-time consent or confirmation | Just in time, when the action first needs it, then never again (C011) | The first run; a banner that stays |
| Teaching | A tutorial card, for beginners only (R18). After that people ask Faber about Faber; there is no help page | Tips that keep coming back |
| Something that needs the person | The running column and the amber run button, always with words; the tray only when an agent needs the person (R74) | A notification for everything that finishes |
| Status (model, effort, quota, running work) | The topbar, the sidebar footer, one line under an answer; shown as soon as it is known (C003) | A dialog |
| A notice that is not an error | One calm line, details behind a link (C001) | Error styling or a block of technical text |
| Quota and spend | Plain, calm words; alarm only near a real limit (C002); never behind reality (C010) | "Running out" at 30% |
| Actions on a row or on text | A short Faber right-click menu (R123, C009) | The browser's menu |
| What changed in a version | The what's-new screen, one entry per change a person notices, in every language (R142, not built yet) | Release notes only on GitHub |
| An empty or idle screen | The same quality as the Chat home, with quiet motion that stops when hidden or reduced (C005, C008) | A static grey card |

Chat and Projects behave alike: a thing that exists in both areas looks and works the same in both, and the two homes
share classes so a change to one reaches the other.

## Coherence review

A review of a change a person sees answers these with evidence (a map entry, a `file:line`), not with an opinion. A "no"
blocks; anything else is optional, and the review names about three findings at most:

1. Did it already exist? Name the [product-map.md](product-map.md) entry it extends, or the closest one and why it does
   not fit.
2. Is it where the table above puts it?
3. Are the map, the five locales and design.md updated in the same change?
4. Which cards did it touch, and does each still hold?
5. Does it work the same in Chat and Projects where both have it?

A reviewer who did not write the change does this from the diff and the map, without the writer's reasoning. Past
failures it must catch: "Redo with…" under every answer is a second model picker beside the composer's; connecting an
account has three buttons with three names ("Connect a subscription", "Manage accounts" twice).

Every review, of coherence or of code, works the same way, because each round samples again and feels pressed to find
something (R147):

- "Nothing to report" is a correct, complete answer, and the request says so up front.
- Blocking means it breaks for a person, loses data, weakens security, or breaks a written rule, decision or card.
  Everything else is optional and never blocks.
- A finding is fixed only with a failing test or a concrete repro (`file:line` and steps); without one it is a note.
- Each round gets the findings already decided (fixed, or dismissed with the reason) and does not raise them again.
  Where two reviews are required (security), a finding both raise on their own is confirmed; one raised by a single
  review needs the test or repro above.

## Taste cards

A card is written when the owner disliked something or a bug passed green tests. Before a change, read the cards whose
"Touches" match it; a review answers "cards checked: C00x ok" or names the `file:line` that breaks one. A new card takes
the next number, the files it touches, the rule in one sentence and its check. "Check" is the test that fails when the
card breaks, or "advisory" when only a review catches it; a card broken twice gets a check, proposed to the owner as one
yes/no question. The owner's own words and screenshots stay in the private notes.

| Card | Touches | Rule | Check |
| :--- | :--- | :--- | :--- |
| C001 | Thread hints | A notice that is not an error never looks like one: one short calm line, details behind a link. A permission mode is not a notice at all: it lives in Settings. | advisory |
| C002 | `reason.pace.*` keys, Auto pacing | Never alarm about quota unless it is really near the limit; say what happened in plain words ("Codex is spending fast this week; Auto used Claude to save it"). | `tests/ui/quota.check.ts` |
| C003 | `ui/Thread.tsx` answer header, Auto | Show what is known as soon as it is known: Auto's chosen model appears when the answer starts streaming, not when it ends. | advisory |
| C004 | Updater, `tauri.conf.json` | Updates install by themselves into the app's own folder (never the data folder), never pop a window or steal focus, and never interrupt running work. | tests in `src-tauri/src/update.rs` |
| C005 | Area homes, `ui/Welcome.tsx`, `ui/Ambient.tsx` | Every area's home has the quality of the Chat home: same structure, light, composer position and "pick up where you left off". | `testkit/src/screen/homes.rs` |
| C006 | Anything that starts a process or a window | Never open a visible window or console and never steal focus: people play full-screen games while Faber works. | advisory |
| C007 | `ui/locales/*` | No broken characters: a "?" in place of an accented letter is a bug, and the locale checks fail on one. | `tests/ui/mojibake.check.ts` |
| C008 | Empty and waiting states, `ui/Composer.tsx`, `ui/Ambient.tsx` | The screen never looks frozen while the person is idle: quiet motion, off under reduced or no motion, never on text being read, and zero CPU or GPU while the window is hidden. | `tests/ui/idle.check.ts` |
| C009 | Right-click, list rows, `ui/main.tsx` | Right-click never shows the browser's menu. Faber's menu is short, holds only actions people use, and puts the destructive one last with a confirm. | `tests/ui/contextmenu.check.ts`, `testkit/src/screen/menus.rs` |
| C010 | Quota readings, Auto pacing, mission admission | Spend never trails reality: vendor counters lag, so Faber adds its own uncounted use or marks the number "still counting". | `tests/ui/quota.check.ts`, `engine/tests/quota_live.rs` |
| C011 | Every screen, `ui/Settings.tsx` | Screens stay clean and premium: a work screen shows only the work, settings live in Settings, a one-time consent shows just in time, and decorative art under text is only a faint veil that leaves it easy to read (the owner, 2026-10-16: blend the black hole, no hard empty band). | advisory; `engine/tests/product_map.rs` keeps the map complete |
