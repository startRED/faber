# Calm screens: one place per thing, one line per idea

Status: in progress (2026-10-16). The owner asked for the whole screen to lose redundancy and wordy text and to feel
premium, coherent and fast, then delegated the rest ("continue until done") while away. Every choice below is therefore
"AI (owner delegated)" and goes in `docs/decisions.md` as such. The before/after page shown to the owner:
https://claude.ai/artifact/V3kvimSJdeB7ZRd54b87jp. The two audits this spec rests on are appendices:
[text](2026-10-16-calm-screens-audit-text.md) and [code](2026-10-16-calm-screens-audit-code.md).

## Why

The base look is good. What breaks the premium feel is excess: the same choice in several places (the model in four),
sentences that explain what nobody asked (a permission sentence on top of every conversation, a paragraph per setting),
status shown when nothing happens ("Nothing running", "Effort applies to the next message"), and raw technical numbers
under every answer ("3.1k in · 73 out · cache warm until 12:50"). Settings mixes two row grammars, seven "pick one"
widgets and seven text-field styles; Usage limits stacks five notes per account. The screen re-parses a streaming answer
30 times a second and writes browser storage on every key.

## Rules for every change here

1. One thing, one place. A choice lives where product-rules.md puts it and nowhere else.
2. Status only when it changes. An idle indicator says nothing; it appears when something runs or waits.
3. One line at most. A detail goes behind hover (a `title`) or a "?" that unfolds the longer text in place.
4. One grammar. The same row, card, control height and text sizes on every screen, from the tokens.
5. Light. Nothing animates while hidden; no work per frame or per key that a person does not see; measured before and after.

Not in scope: new features, new settings, the black hole and the Projects home (the `project-home-calm` lane owns
`ui/Welcome.tsx` and the scene), the mission engine.

## Deliveries

Each is one squashed pull request into `phase-1-base`, with its own `.faber-lane`, all five locales written by hand,
`notes.next.better`, product-map and design.md updated, `cargo t --all` green and a fresh-session review (Codex when
available).

### 1. Work screens (conversation, composer, sidebar, topbar, running column)

- Answer header: the vendor mark and the model only. Effort and Auto's reason move to the header's `title`.
  Auto's pace reason (C002) stays as its one calm line when it applies.
- Answer actions: icon buttons with a tooltip and an accessible name (copy, retry, open in terminal), then the cost in the
  faint data style; tokens and "cache warm" go into the cost's `title`. The per-answer "Redo with…" `<select>` goes:
  the model is chosen in the composer only (known incoherence 8, decided here). The right-click "Redo" stays and says
  which model it uses. "Try again one step up" stays (it is a suggestion after a failed check, not a picker), shortened.
- The permission sentence at the top of every conversation ("Faber works without asking for approval. Details in
  Settings") goes; the composer's mode chip already shows the mode.
- The topbar's read-only model chip goes (the composer and the answer header show it).
- Composer: "Effort applies to the next message." goes. Placeholders shorten (text audit).
- Sidebar: in Chat the list shows chats only; in Projects it shows projects and their conversations only (no empty
  "Projects · No projects yet" block inside Chat). The footer is one row with the name and Settings, then one thin bar per
  connected AI with its share; the version moves to Settings.
- Running column: the permanent footer sentence goes; "Nothing running" says it once.
- Work log, agent and helper steps: one disclosure style (`details.activity` and `details.agent` share one rule); raw
  agent kinds (`general-purpose`) show as words; counts pluralise ("0 steps", not "0 step").
- Performance: the answer's HTML is memoised per text and only the open tail block re-renders while streaming (C1); the
  draft is still written on every change (a debounce lost text on a crash, found in review); the composer re-measures at
  most once per frame (C2); sidebar rows keep their nodes when the list refreshes (C4).

### 2. Settings

- One row grammar everywhere: a card per group, each option one row with its name and a one-line hint at the left and
  the control at the right. The long text of an option unfolds under it from a "?" next to its name (replaces every
  separate "Learn more" link; one key `set.learnMore`).
- General groups: You (language, name), How the AI works (permissions as a select whose menu shows each mode's one
  line, web search, CLI hooks, a folded "Codex" group with reasoning summary and answer length), Notifications and
  system, Updates. The Chat working folder row shows only where its control exists (incoherence 17).
- "Connected accounts" and "Usage limits" become one section, "Accounts and limits": one card per AI with its state,
  plan, a one-word status, a bar per window with the pace mark and the reset time, Faber's spend today in the person's
  number format, and one "Details" fold for the notes. "Manage accounts" appears once. Five sections remain.
- Switching sections is instant: no exit copies, and the new section opens at its top (D1, D2 of the code audit).
- Appearance: looks and accent first; contrast readings and the three effect switches fold under "Advanced".
- `color-scheme` follows the look; `.btn.secondary` is defined or removed; money and percentages use `Intl`.

### 3. Homes and empty states (after `project-home-calm` lands)

- The Chat home follows the same one-column order as the project home: greeting, composer, three short suggestion
  chips (no "To start" heading), "Recent" under them in the same column.
- One empty state (the peeking mark, a title, one line, one button) for no AI, no project and an empty list.

3a (done, R170): the one empty state (`ui/Empty.tsx`), the tutorial's third card pinned under its target from 720 px, and
the two near-duplicate what's-new entries. 3b, the Chat home in one column, waits for PR #56 (`project-home-calm`).

### 4. One grammar and lighter code

- Control heights from three tokens (28/32/40); text sizes from `--fs-xs`, `--fs-sm`, `--fs-ui` only; one timing family.
- One `Segmented` (the first-run copy goes), one text field class, one look card for first run and Settings.
- The unreachable mission screens leave the bundle (`ui/Projects.tsx` start card and board, `ui/LocalBuild.tsx` on
  screen) together with their 30 s poll; "Open a project" stays.
- Timers stop while the window is hidden; animated lists do not nest observers; the blur behind near-opaque panels goes.

## Found while capturing every screen (79 captures, 1440 and 720 px)

Folded into the deliveries above; each is fixed by the delivery that owns its screen.

- Delivery 1: the answer actions and the cost wrap onto two lines in the 660 px answer column; the sidebar footer wraps
  "1 min ago" and "still counting" onto four lines; the quota answer header says "Waiting for quota until 2 h" twice, and
  the card, the red note and the header repeat it; token counts mix formats ("80.0 mil", "2100"); a background helper's
  mark looks like a stop button; in Projects the sidebar also lists every Chat conversation; with no AI the composer and
  the permission sentence stay on screen.
- Delivery 2: the 7-day reset time is cut to "renov…" while the 5-hour one shows; amounts show four decimals; the plan
  reads lowercase ("max"); Settings content widths differ per section (880 against 720 px); the account cards show the
  picked AI only as a thicker outline.
- Delivery 3: on the project home the "Pick up" list sits over the scene and lists Chat conversations; the tutorial's
  third card floats without its pointer at 720 px; what's new shows two near-duplicate entries.

Baseline (hidden test build, Dark Room, other suites running on the PC): Chat home idle 113-119 MB and 3.6-5.7 % of a
core (the ambient scene); after one answer 147 MB; after visiting every screen about 205 MB, flat; a thread idle 0.2-1 %.

## Done means

The checks of each feature (product map) pass and fail without the change; memory and processor are measured with the
hidden test build before and after (`testkit/examples/ux_audit.rs` reports both) and stated in the commit body.
