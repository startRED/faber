# B5b: Layout and Running Column, Screen Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The screen gets the layout of the approved design: a 36 px titlebar, a sidebar with the two areas, the projects, the chats by day and the quota footer, a top bar with the crumb, the model chip and the run button, a reading column in the stage, and a running column that sits beside the stage from 1,100 px and over it below that. The new conversation greets the user, and the column opens by itself, once, for each question.

**Architecture:** The screen keeps the newest running list it has seen (`ui/running.ts`: a store that never goes back to an older revision, the blocks, the auto-open rule), and draws it in three places: the run button, the sidebar's project rows and `ui/Running.tsx`. `ui/App.tsx` owns one `place` signal (area, project, conversation) and two small state machines, the sidebar overlay and the column. The permission card moves to `ui/Ask.tsx` and is shared by the thread and the column. The width classes (`ui/layout.ts`) decide where the sidebar and the column sit, and CSS does the rest. The pure logic is in `ui/layout.ts`, `ui/sidebar.ts` and `ui/running.ts` so that the checks in `tests/ui/running.check.ts` run it with a fixed clock, and the screen scenarios in `testkit/src/screen/shell.rs` drive the real window.

**Tech Stack:** SolidJS + TypeScript (Vite), plain CSS, the Rust test runner `cargo t` (suites `ui` and `screen`, Windows only for `screen`).

**Spec:** `docs/superpowers/specs/2026-10-11-faber-redesign-design.md`: B5 ("Shell layout and running list", the screen half, commit B5b), B0 (commit 7), A7, A13, and the Review log lines about layout, the running column, focus, the greeting, the empty states and the widths. Erick's decisions R52 to R54 are in `E:\Documentos\ProjetosIA\faber-notas\decisoes.md`. The approved visual design is `E:\Documentos\ProjetosIA\faber-notas\design-final\` (`index.html` and `notes.md`); the CSS in this plan was taken from it. This is commit 7 of the order in B0. It uses B5a (the engine's running list, the `Running` event, `answer` with a turn id) and B4 (`ui/Mark.tsx`: `Lockup` and `Peek`).

## Global Constraints

- Code, comments, docs and commit messages are in English. Original code only; nothing from TL-ADE, TL-Control or TL-Orchestrator.
- Never read, copy, store or forward CLI credentials or session tokens. This plan touches none.
- Child processes never go through a shell. This plan starts none.
- Every text on screen lives in `ui/locales/`, written by hand in each of the five languages, never machine-translated; English (`en.ts`) is the source. A new language needs a native speaker. The texts in Task 3 are the ones that were reviewed with the code. The engine returns keys (`error.*`, `reason.*`); the screen shows `fromEngine(key)`.
- Do not use vendor names in product, feature or package names. "Claude" and "ChatGPT" appear on screen only as plain text for the CLIs.
- Memory: Faber with no CLI running stays under 500 MB when complete, and the end-to-end scenarios fail above 250 MB today. This plan adds no process and no long-lived task. The column's 15 s clock exists only while the column is open; the minute clock of the sidebar is the only timer that always runs.
- Telemetry stays off by default; this plan adds none.
- Spec values, verbatim: titlebar 36 px, sidebar 248 px, topbar 52 px, reading column at most 660 px, running column 352 px; widths: 1,100 px and wider (column beside the stage, stage at least 500 px), 900 to 1,099 px (column over the stage, sidebar beside it), 720 to 899 px (sidebar behind a button, both are overlays); the column opens by itself 1.5 s after the last keystroke, once per `conversationId:turnId:requestId`, including questions present at launch; Esc closes the column only while focus is inside it; text size 18 must not push the page past the window width.
- Motion at most 400 ms; the looks that turn motion down keep working (the new animations are in the `reduced`/`none` rules).
- Accessibility: choice buttons use `aria-pressed`; the run button has `aria-expanded` and `aria-controls`; every overlay says whether it is modal.
- rustfmt: `max_width = 140`, `use_small_heuristics = "Max"`. The Rust in this plan is already formatted; `cargo fmt --all --check` must stay clean.
- One commit, made after every proof command exits 0 (Task 8). The proof commands: `npm run lint`, `npm run typecheck`, `npm run build`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p faber --features test-hooks -- -D warnings`, `cargo t --all`.
- Rust environment on the maintainer's PC, before any `cargo` (and before `cargo t`):

```bash
export RUSTUP_HOME='E:\Apps\rust\rustup' CARGO_HOME='E:\Apps\rust\cargo' PATH="/e/Apps/rust/cargo/bin:$PATH"
```

- The scenarios of the `screen` suite need Windows and WebView2. Do not kill any process by hand; the runner ends the apps it starts.

## Review Focus

The inputs the spec implies and the checks do not spell out, most likely first. Each has a test named in the task that owns the code.

1. **A question arrives while the user is typing.** Expected: the column does not jump open under the cursor; it opens 1.5 s after the last key, once, and focus stays where it was. Pinned by: Task 2, `auto-open: waits until the user has stopped typing for 1.5 s`; Task 7, `the running column opens by itself once for each question, and closing it leaves the button amber` (the user has just typed the request) and `over the stage the column opens above the composer without taking focus, and the user's own opening is modal`.
2. **The snapshot and the `Running` events arrive in either order, or the first list is revision 0.** Expected: an old list never replaces a newer one, and the first list from the engine is taken. A question already waiting when the window loads shows the amber button at once and opens the column once. Pinned by: Task 2, `running: a list never goes back to an older one, in whatever order they arrive` and `running: the first list from the engine is taken, even revision 0`; Task 7, `a question that waits when the screen loads shows the amber button at once and opens the column once`.
3. **The window is resized while the column or the sidebar is open.** Expected: each follows the new width, nothing sticks out, and the controls work again once the width allows (the top bar is out of reach only while a modal overlay is open). Pinned by: Task 7, `the layout follows the width, from 720 to 1440, and the running column fits each one` (its last part narrows 1440 to 1000 with the column open and widens 720 to 1000 with the sidebar open).
4. **Long unbroken text: a path in a question, a very long title.** Expected: it wraps or is cut with an ellipsis inside its own box, and never pushes the stage or the column wider. Pinned by: Task 7, `the running column opens by itself once for each question, and closing it leaves the button amber` (the card and the tool line of the fixture, which carry a long path, stay inside the stage) and `a larger text size still fits every width`.
5. **A quota reading that is old, or has never come.** Expected: the footer shows "—", never an old percentage, and after the page reloads it shows the engine's reading again. Pinned by: Task 2, `quota: the tightest window of the vendor counts, and an old reading counts for nothing`; Task 7, `the quota of each vendor shows in the footer, and comes back after the screen reloads`.

## Where this plan meets B4 and B5a

B4 (commit `c8e63e7`, "Curioso icon set, wordmark and titlebar lockup") is already committed. B5a (the engine half, commit 6) is committed before this plan starts; this plan reads only what B5a gives the screen (`AppState.running`, `AppState.quotas`, the `running` event, `answer(id, turnId, requestId, allow, always)`) and edits around it. These are the files that both B4 and this plan changed, and how this plan keeps B4's work:

| File | What B4 put there | What this plan does with it |
| :--- | :--- | :--- |
| `ui/App.tsx` | `<Lockup height={20} />` in the titlebar and `<Peek />` in the empty states | Rewritten as a whole (Task 6). The new file keeps the lockup in the titlebar with the window controls, and shows `Peek` in the empty states and the no-AI state |
| `ui/styles.css` | The `.peek` rules, with `margin-left: -2px` on `.peek svg` | The patch in Task 6 keeps both lines of `.peek` as they are |
| `ui/locales/*.ts` | `brand.name` | Task 3 adds keys next to it and removes none of B4's |
| `testkit/src/screen/app.rs` | The `shows_the_brand` scenario (title bar lockup in ink, Curioso only in the project empty states) | Task 7 moves its two selectors (`.nav .item` to `.prow .item`) and keeps every check |
| `ui/Mark.tsx` | `Mark`, `Lockup`, `Peek` | Not edited. `Sidebar.tsx`, `Running.tsx` and `App.tsx` import `Peek` and `Lockup` from it |

B5a touched `ui/App.tsx` (one `answer` call), `ui/Thread.tsx` and `ui/api.ts`. Task 4 and Task 6 replace those lines as part of their edits (the patches are made against the tree with B5a in it); nothing else of B5a is touched.

If a patch in this plan does not apply, stop and report: the file changed under the plan. Do not edit the patch by hand.

## Line endings

`ui/Thread.tsx` is stored with CRLF in the index; every other file in this plan is LF. The patches are written with LF. Task 1 converts `ui/Thread.tsx` to LF, and Task 8 converts it back, so the commit shows only the real changes. Apply every patch with `git -c core.autocrlf=false apply --whitespace=nowarn <patch>` so that Git does not rewrite the line endings. Do not run a formatter over `ui/` in between.

## Deviations from spec

Each one is on purpose. Each is small, and none changes a rule of the spec.

1. **More files than the spec lists.** The spec lists `Sidebar.tsx`, `Running.tsx`, `Ask.tsx` and `running.check.ts` as new. This plan also adds `ui/layout.ts`, `ui/sidebar.ts` and `ui/running.ts` (the pure logic, so that `tests/ui/running.check.ts` can run it without a window), `ui/Glyphs.tsx` (the icons and the vendor mark), `ui/Welcome.tsx` (s1) and `testkit/src/screen/shell.rs` (the new scenarios, in their own file, so that the scenarios of other pieces never meet this one). It also changes `ui/main.tsx` (the startup order), `ui/Composer.tsx` (a suggestion fills the composer), `ui/models.ts` (the label of Auto), `tests/ui/index.ts` and `testkit/src/screen/mod.rs` (the registrations).
2. **The docked column's box is 344 px, in a 352 px track.** The stage keeps its 8 px frame on the right (`--stage-inset`), so the column is 352 px with that frame. At 1,100 px the stage is 500 px, as the spec requires. A column of 352 px plus the frame would leave 492 px.
3. **"AI: choose for me" is a new key.** The spec says the composer keeps "AI: choose for me". The old text was "Auto". The key `model.autoChoose` carries the new text and `ui/models.ts` uses it for the first option; `model.auto` stays for the answer's header.
4. **The vendor lines of the footer open Settings.** The spec says "Settings > Usage limits"; no such page exists yet, so they open Settings.
5. **The progress bars and the "stopped" tails of the mock are left out.** The spec says the progress view is Phase 3.
6. **The run button is two parts, not one string.** "1 running" and "1 needs you" are two segments of one button with the same words; the second is amber and has the speech bubble.
7. **"A message sent goes to that conversation" is checked by its cause, not by sending.** The fake CLI replays one recording and cannot take a second turn after `tools.jsonl`. `openPlace` is checked in `running.check.ts`, and the scenario checks that after a click on a row of a project, the crumb, the area, the marked row in the sidebar and the composer's placeholder all point at the project.
8. **The four suggestions are general.** The mock's suggestions are about a bakery; the spec asks for four hand-written suggestions per language, and these are not tied to any trade.
9. **The scenario for text size 18 sets the size through `settings.json`**, the same file the slider writes, rather than moving the slider.
10. **The check that effects do not lie over the greeting (B3's `fits_every_look`) is now mask-aware.** The loom fades out from its corner, so words lie over it only when they come within half of its visible reach. The check also measures `.hello` and `.lead` (the greeting of this piece) instead of `.empty h1` and `.empty p`.
11. **`docs/design.md` is not changed.** Its Layout section already describes this screen, word for word.

## File structure

| File | Responsibility after this commit |
| :--- | :--- |
| `ui/layout.ts` | The width classes (`wide` from 1,100 px, `mid` from 900 px, `narrow` below), and a signal that follows the window |
| `ui/sidebar.ts` | The chats grouped by day, the quota readings and the tightest window, the tail of a project row |
| `ui/running.ts` | The running store (newest revision wins), the blocks, the auto-open rule, the place a row opens, what an answer's error means, the elapsed time |
| `ui/Glyphs.tsx` | `Icon` (the drawings of the mock) and `VendorMark` (the shape of each vendor) |
| `ui/Ask.tsx` | The permission card, moved out of `Thread.tsx`, and `Asks`, the first card plus the "more waiting" note; shared by the thread and the column |
| `ui/Running.tsx` | The running column |
| `ui/Sidebar.tsx` | The sidebar |
| `ui/Welcome.tsx` | The greeting, the four suggestions and "Pick up where you left off" |
| `ui/App.tsx` | The shell: titlebar, sidebar, topbar, stage, the two overlay state machines, the auto-open effect, the place, the keyboard |
| `ui/main.tsx` | The startup order: listener first, then `state()` |
| `ui/Thread.tsx`, `ui/Composer.tsx`, `ui/models.ts`, `ui/api.ts` | Small changes: use `Asks`, expose `fill`, the label of Auto, the new types |
| `ui/styles.css` | The shell, the sidebar, the stage, the welcome grid, the run button, the running column and the width rules |
| `ui/locales/*.ts` | The new texts, in five languages |
| `tests/ui/running.check.ts` | The checks of the pure logic |
| `testkit/src/screen/shell.rs` | The scenarios: layout, large text, auto-open, overlay, project row, greeting, no-AI, quota |
| `testkit/src/screen/app.rs` | Two moved selectors, the new greeting measures, the mask-aware effect check |

How this plan was checked: every file and patch below was applied, in the order of the tasks, to a copy of the tree at commit `c8e63e7` with B5a applied, and the result is byte for byte the tree that passed all seven proof commands (`cargo t --all`: 301 passed, 0 failed). The text of each block is the text that was applied.

---


## How to read the code blocks

- A block under "Create" is a whole file: write it exactly as shown, with LF line endings, and a final newline.
- A block under "Patch" is a unified diff of one file, made against the tree this plan starts from. Save its text exactly (the leading space of the context lines, the final newline) to a file in a scratch folder outside the repo, and from the repo root run:

```bash
git -c core.autocrlf=false apply --whitespace=nowarn <path-to-the-patch-file>
```

  `git apply` checks the whole patch before it changes anything. If it refuses, stop and report; do not edit the patch.
- `ui/App.tsx` is replaced as a whole.
- The tests come first where they can: the checks of the pure logic (Task 2) fail before the code exists. The UI cannot be tested without the code that draws it, so the scenarios of Task 7 are the failing tests of Tasks 4 to 6, and Task 6 shows the old scenarios failing on the selectors this commit moves.

---

### Task 1: Check the base and prepare the line endings  (model: `haiku`)

**Files:**
- Modify: `ui/Thread.tsx` (line endings only)

**Interfaces:**
- Consumes: nothing.
- Produces: a tree where `ui/Thread.tsx` has LF, so the patches of Task 4 apply.

- [ ] **Step 1: Check where the tree stands**

Run, from the repo root:

```bash
git log --oneline -3
git status --short
grep -n "export function Peek" ui/Mark.tsx
grep -n "turnId" ui/api.ts
grep -n "turn_id" src-tauri/src/main.rs
grep -n "pub struct RunningItem" engine/src/model.rs
```

Expected: the log shows B4 (`feat(identity): Curioso icon set, wordmark and titlebar lockup`) at or below HEAD; `ui/Mark.tsx` exports `Peek`; `ui/api.ts` has `answer: (id: string, turnId: string, ...`; `src-tauri/src/main.rs` has `turn_id`; `engine/src/model.rs` has `RunningItem`. These last three are B5a. If any is missing, stop and report: this plan needs B4 and B5a.

B5a is commit 6 and must be committed before this plan starts: `git status --short` must list no changed file under `ui/`, `engine/`, `src-tauri/`, `testkit/` or `tests/`. (Untracked `.ignore`, `.impeccable/` and plan files under `docs/` are fine.) If B5a is still uncommitted, stop and report. The status must not list `ui/Sidebar.tsx`, `ui/Running.tsx`, `ui/Ask.tsx`, `ui/layout.ts`, `ui/running.ts`, `ui/sidebar.ts`, `ui/Glyphs.tsx`, `ui/Welcome.tsx` or `testkit/src/screen/shell.rs`. If one of them exists, stop and report.

- [ ] **Step 2: Convert `ui/Thread.tsx` to LF**

```bash
python3 - <<'EOF'
p = 'ui/Thread.tsx'
data = open(p, 'rb').read().replace(b'\r\n', b'\n')
open(p, 'wb').write(data)
EOF
```

- [ ] **Step 3: Check it**

```bash
python3 -c "d=open('ui/Thread.tsx','rb').read(); print(d.count(b'\r'), 'CR left')"
npm run typecheck
```

Expected: `0 CR left`, and the type check exits 0 (the tree is still B4 plus B5a).

Do not commit. The commit is in Task 8.

---

### Task 2: The pure logic, with its checks  (model: `sonnet`)

**Files:**
- Create: `tests/ui/running.check.ts`, `ui/layout.ts`, `ui/sidebar.ts`, `ui/running.ts`
- Modify: `tests/ui/index.ts` (one import), `ui/api.ts` (the types)

**Interfaces:**
- Consumes: `AppState.running` and `AppState.quotas` and the `running` event, as B5a sends them.
- Produces, for Tasks 4 to 6:
  - `ui/api.ts`: `RunningItem`, `RunningList`, `QuotaReading`; `AppState` gains `running: RunningList` and `quotas: QuotaReading[]`; `AppEvent` gains `({ type: 'running' } & RunningList)`.
  - `ui/layout.ts`: `type Width = 'wide' | 'mid' | 'narrow'`, `widthClass(px: number): Width`, `createWidth(): () => Width`.
  - `ui/sidebar.ts`: `groupChats(chats, now)`, `type Quotas`, `seedQuotas(readings)`, `withQuota(held, reading)`, `tightest(quotas, vendor, now)`, `tailOf(items, projectId)`.
  - `ui/running.ts`: `newest`, `createRunningStore()` and the singleton `store` (`list()`, `offer(list)`), `blocksOf`, `askKey`, `waitingKeys`, `QUIET_MS`, `createAutoOpen(quiet?)` (`typed(now)`, `check(keys, now)`), `type Place`, `openPlace`, `placeOfRunning`, `answerOutcome`, `elapsed`.

- [ ] **Step 1: Write the checks**

Create `tests/ui/running.check.ts`:

```ts
import type { QuotaReading, RunningItem } from '../../ui/api.ts';
import { widthClass } from '../../ui/layout.ts';
import { answerOutcome, askKey, blocksOf, createAutoOpen, createRunningStore, elapsed, newest, openPlace, placeOfRunning, QUIET_MS, waitingKeys } from '../../ui/running.ts';
import { groupChats, seedQuotas, tailOf, tightest, withQuota } from '../../ui/sidebar.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const item = (over: Partial<RunningItem> = {}): RunningItem => ({
  conversationId: 'c1',
  turnId: 'm1',
  title: 'A',
  area: 'chat',
  state: 'working',
  since: 0,
  permissions: [],
  ...over,
});
const ask = (requestId: string) => ({ requestId, tool: 'Bash', input: {} });

check('layout: the width decides where the column and the sidebar sit', () => {
  equal(widthClass(1440), 'wide');
  equal(widthClass(1100), 'wide');
  equal(widthClass(1099), 'mid');
  equal(widthClass(900), 'mid');
  equal(widthClass(899), 'narrow');
  equal(widthClass(720), 'narrow');
});

check('running: a list never goes back to an older one, in whatever order they arrive', () => {
  const store = createRunningStore();
  store.offer({ revision: 5, items: [item()] });
  store.offer({ revision: 4, items: [] });
  equal(store.list().revision, 5);
  equal(store.list().items.length, 1);
  store.offer({ revision: 5, items: [] });
  equal(store.list().items.length, 1, 'the same revision keeps the list already held');
  store.offer({ revision: 6, items: [] });
  equal(store.list().items.length, 0);
  equal(newest({ revision: 1, items: [] }, { revision: 2, items: [item()] }).revision, 2);
});

check('running: the first list from the engine is taken, even revision 0', () => {
  const store = createRunningStore();
  store.offer({ revision: 0, items: [item()] });
  equal(store.list().items.length, 1);
});

check('running: one block per project and one for Chat, in the engine order', () => {
  const blocks = blocksOf([
    item({ conversationId: 'a', area: 'project', projectId: 'p2', state: 'needsYou' }),
    item({ conversationId: 'b', area: 'project', projectId: 'p1' }),
    item({ conversationId: 'c', area: 'project', projectId: 'p2' }),
    item({ conversationId: 'd' }),
  ]);
  deepEqual(blocks.map((b) => [b.key, b.items.length, b.needs]), [['p2', 2, 1], ['p1', 1, 0], ['chat', 1, 0]]);
});

check('running: a question is named by conversation, turn and request together', () => {
  const a = item({ conversationId: 'c1', turnId: 'm1', permissions: [ask('0')] });
  const b = item({ conversationId: 'c2', turnId: 'm1', permissions: [ask('0')] });
  deepEqual(waitingKeys([a, b]), ['c1:m1:0', 'c2:m1:0']);
  equal(askKey(a, '7'), 'c1:m1:7');
});

check('auto-open: opens once for each question, even if the user closes the column', () => {
  const open = createAutoOpen();
  deepEqual(open.check([], 0), { open: false });
  deepEqual(open.check(['k1'], 0), { open: true });
  deepEqual(open.check(['k1'], 10), { open: false }, 'the same question does not open it again');
  deepEqual(open.check(['k1', 'k2'], 20), { open: true }, 'a new question does');
});

check('auto-open: waits until the user has stopped typing for 1.5 s', () => {
  const open = createAutoOpen();
  open.typed(1000);
  deepEqual(open.check(['k1'], 1200), { open: false, retryIn: QUIET_MS - 200 });
  open.typed(2000);
  deepEqual(open.check(['k1'], 2500), { open: false, retryIn: QUIET_MS - 500 });
  deepEqual(open.check(['k1'], 2000 + QUIET_MS), { open: true });
});

check('auto-open: a question that waits when Faber starts opens the column too', () => {
  const open = createAutoOpen();
  deepEqual(open.check(waitingKeys([item({ permissions: [ask('3')] })]), Date.now()), { open: true });
});

check('places: opening a conversation sets area, project and id together', () => {
  const from = { area: 'project' as const, projectId: 'p1' };
  deepEqual(openPlace(from, { id: 'x', area: 'chat' }), { area: 'chat', projectId: 'p1', id: 'x' });
  deepEqual(openPlace({ area: 'chat' }, { id: 'y', area: 'project', projectId: 'p9' }), { area: 'project', projectId: 'p9', id: 'y' });
  deepEqual(placeOfRunning({ area: 'chat' }, item({ conversationId: 'z', area: 'project', projectId: 'p2' })), { area: 'project', projectId: 'p2', id: 'z' });
});

check('answers: a stale question is dropped quietly; any other error is shown', () => {
  equal(answerOutcome('error.permission.stale'), 'stale');
  deepEqual(answerOutcome('error.notDelivered'), { problem: 'error.notDelivered' });
});

check('elapsed: seconds, minutes, then hours', () => {
  deepEqual(elapsed(0, 5_000), { key: 'time.seconds', n: 5 });
  deepEqual(elapsed(0, 59_999), { key: 'time.seconds', n: 59 });
  deepEqual(elapsed(0, 60_000), { key: 'time.minutes', n: 1 });
  deepEqual(elapsed(0, 3_600_000), { key: 'time.hours', n: 1 });
  deepEqual(elapsed(10_000, 0), { key: 'time.seconds', n: 0 }, 'a clock that runs behind never shows a negative time');
});

check('sidebar: chats are grouped by the user\'s own days', () => {
  const now = new Date(2026, 9, 11, 15, 0).getTime();
  const at = (days: number, hour = 12) => new Date(2026, 9, 11 - days, hour).getTime();
  const groups = groupChats(
    [
      { id: 'a', updatedAt: at(0, 0) },
      { id: 'b', updatedAt: at(1, 23) },
      { id: 'c', updatedAt: at(6, 0) },
      { id: 'd', updatedAt: at(7, 23) },
    ],
    now,
  );
  deepEqual(groups.map((g) => [g.group, g.chats.map((c) => c.id)]), [['today', ['a']], ['week', ['b', 'c']], ['earlier', ['d']]]);
  deepEqual(groupChats([], now), []);
});

check('quota: the tightest window of the vendor counts, and an old reading counts for nothing', () => {
  const now = 1_000_000;
  const reading = (window: string, used: number, resetsAt: number, vendor: QuotaReading['vendor'] = 'claude'): QuotaReading => ({ vendor, window, used, resetsAt });
  let quotas = seedQuotas([reading('five_hour', 0.2, 2000), reading('seven_day', 0.6, 5000), reading('five_hour', 0.9, 2000, 'codex')]);
  equal(tightest(quotas, 'claude', now)?.window, 'seven_day');
  equal(tightest(quotas, 'codex', now)?.used, 0.9);
  quotas = withQuota(quotas, reading('seven_day', 0.7, 900));
  equal(tightest(quotas, 'claude', now)?.window, 'five_hour', 'the 7-day reading has passed its reset time');
  quotas = withQuota(quotas, reading('five_hour', 0.1, 0));
  equal(tightest(quotas, 'claude', now)?.window, 'five_hour', 'a reading with no reset time stands');
  equal(tightest({}, 'claude', now), undefined);
});

check('sidebar: a project row says how many questions wait and whether anything runs', () => {
  const items = [item({ area: 'project', projectId: 'p1', state: 'needsYou' }), item({ conversationId: 'c2', area: 'project', projectId: 'p1' }), item({ conversationId: 'c3' })];
  deepEqual(tailOf(items, 'p1'), { needs: 1, running: true });
  deepEqual(tailOf(items, 'p2'), { needs: 0, running: false });
  ok(!tailOf([], 'p1').running);
});
```

Register it. Patch `tests/ui/index.ts`:

```diff
diff --git a/tests/ui/index.ts b/tests/ui/index.ts
index 6467b3b..122d3bc 100644
--- a/tests/ui/index.ts
+++ b/tests/ui/index.ts
@@ -16,6 +16,7 @@ import './check.check.ts';
 import './i18n.check.ts';
 import './models.check.ts';
 import './opening.check.ts';
+import './running.check.ts';
 import './setup.check.ts';
 
 type Vars = Record<string, string | number>;
```

- [ ] **Step 2: Run the type check and see it fail**

```bash
npm run typecheck
```

Expected: FAIL, with `Cannot find module '../../ui/layout.ts'` (and the same for `sidebar.ts` and `running.ts`).

- [ ] **Step 3: Add the types to `ui/api.ts`**

Patch `ui/api.ts`:

```diff
diff --git a/ui/api.ts b/ui/api.ts
index 4ca8320..a1acda8 100644
--- a/ui/api.ts
+++ b/ui/api.ts
@@ -125,6 +125,35 @@ export interface ModelInfo {
   description?: string;
   isDefault: boolean;
 }
+/** One conversation that is answering now or waits for the user: a row of the running list (engine/src/model.rs). */
+export interface RunningItem {
+  conversationId: string;
+  /** The id of the turn's reply message. With the request id it names a question: request ids alone repeat. */
+  turnId: string;
+  /** Empty while the conversation has none. */
+  title: string;
+  area: Area;
+  projectId?: string;
+  /** None while Auto is still choosing the model. */
+  vendor?: Vendor;
+  model?: string;
+  state: 'working' | 'needsYou';
+  /** The `at` of the turn's reply message, in milliseconds. */
+  since: number;
+  permissions: Permission[];
+}
+/** The running list in the engine's order, with a revision that grows by one with every list the engine sends. */
+export interface RunningList {
+  revision: number;
+  items: RunningItem[];
+}
+/** The latest reading of one quota window of one vendor. `resetsAt` is in seconds, and 0 when the CLI did not say. */
+export interface QuotaReading {
+  vendor: Vendor;
+  window: string;
+  used: number;
+  resetsAt: number;
+}
 export interface AppState {
   conversations: Summary[];
   projects: Project[];
@@ -132,6 +161,8 @@ export interface AppState {
   settings: Settings;
   look: Look;
   models: ModelInfo[];
+  running: RunningList;
+  quotas: QuotaReading[];
   version: string;
 }
 export interface SavedSettings {
@@ -162,7 +193,8 @@ export interface SetupProgressEvent {
 }
 export type AppEvent =
   | { type: 'message'; conversationId: string; message: Message }
-  | { type: 'quota'; vendor: Vendor; window: string; used: number; resetsAt: number }
+  | ({ type: 'quota' } & QuotaReading)
+  | ({ type: 'running' } & RunningList)
   | { type: 'list' }
   | SetupProgressEvent;
 
```

- [ ] **Step 4: Write the three modules**

Create `ui/layout.ts`:

```ts
import { type Accessor, createSignal, onCleanup } from 'solid-js';

// The window widths where the shell changes (docs/design.md, "Layout"). ui/styles.css repeats the same two numbers in its
// media queries, so the first paint is right before any script runs; a screen test checks both sides of each edge.
//   wide:   1,100 px and more. The sidebar and an open running column sit beside the stage.
//   mid:    900 to 1,099 px. The sidebar sits beside the stage; an open running column overlays it.
//   narrow: 720 to 899 px. The sidebar is behind a button; an open running column overlays the stage.
export type Width = 'wide' | 'mid' | 'narrow';
export const WIDE_FROM = 1100;
export const MID_FROM = 900;

export function widthClass(px: number): Width {
  return px >= WIDE_FROM ? 'wide' : px >= MID_FROM ? 'mid' : 'narrow';
}

/** The width class of the window, kept up to date while the calling component lives. */
export function createWidth(): Accessor<Width> {
  const [width, setWidth] = createSignal(widthClass(window.innerWidth));
  const onResize = () => setWidth(widthClass(window.innerWidth));
  window.addEventListener('resize', onResize);
  onCleanup(() => window.removeEventListener('resize', onResize));
  return width;
}
```

Create `ui/sidebar.ts`:

```ts
import type { QuotaReading, RunningItem, Vendor } from './api.ts';

// The data behind the sidebar: how the chats are grouped, which quota reading each vendor shows, and what the tail of a
// project says. Pure functions, so the screen's checks run them with a fixed clock.

export type ChatGroup = 'today' | 'week' | 'earlier';

const startOfDay = (ms: number, back = 0) => {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() - back).getTime();
};

/** Chats, newest first as given, in three groups: today, the six days before today ("this week"), and earlier. Empty
 * groups are left out. Local midnight draws the lines, so the groups follow the user's own days. */
export function groupChats<T extends { updatedAt: number }>(chats: T[], now: number): { group: ChatGroup; chats: T[] }[] {
  const today = startOfDay(now);
  const week = startOfDay(now, 6);
  const groups: Record<ChatGroup, T[]> = { today: [], week: [], earlier: [] };
  for (const chat of chats) groups[chat.updatedAt >= today ? 'today' : chat.updatedAt >= week ? 'week' : 'earlier'].push(chat);
  return (['today', 'week', 'earlier'] as const).filter((g) => groups[g].length > 0).map((g) => ({ group: g, chats: groups[g] }));
}

/** The latest reading of each window, by vendor and window name. */
export type Quotas = Record<string, QuotaReading>;

export const seedQuotas = (readings: QuotaReading[]): Quotas => Object.fromEntries(readings.map((q) => [`${q.vendor}:${q.window}`, q]));

export const withQuota = (held: Quotas, q: QuotaReading): Quotas => ({ ...held, [`${q.vendor}:${q.window}`]: q });

/** A reading whose reset time has passed says nothing about now. A reading with no reset time (0) cannot be told to be
 * old, so it stands until a newer one replaces it. */
const expired = (q: QuotaReading, now: number) => q.resetsAt > 0 && q.resetsAt * 1000 <= now;

/** The vendor's tightest window: the reading with the highest used share among those that still count, or undefined when
 * there is none (the vendor has not reported yet, or every reading is old). */
export function tightest(quotas: Quotas, vendor: Vendor, now: number): QuotaReading | undefined {
  const live = Object.values(quotas).filter((q) => q.vendor === vendor && !expired(q, now));
  return live.reduce<QuotaReading | undefined>((best, q) => (best && best.used >= q.used ? best : q), undefined);
}

/** What a project's row says about its running conversations: how many wait for the user, and whether any runs. */
export function tailOf(items: RunningItem[], projectId: string): { needs: number; running: boolean } {
  const mine = items.filter((i) => i.area === 'project' && i.projectId === projectId);
  return { needs: mine.filter((i) => i.state === 'needsYou').length, running: mine.length > 0 };
}
```

Create `ui/running.ts`:

```ts
import { createSignal } from 'solid-js';
import { type Area, errorText, type RunningItem, type RunningList } from './api.ts';

// The running list on the screen (docs/design.md, "Running list"). The engine builds and orders the list; the screen
// keeps the newest one it has seen, groups it into blocks and decides when the column opens by itself.

/** The newer of two lists. The same revision keeps the one already held. */
export function newest(held: RunningList, offered: RunningList): RunningList {
  return offered.revision > held.revision ? offered : held;
}

/** The list the screen shows. The snapshot from `state()` and every `Running` event are offered to it, in whatever order
 * they arrive; an old list never replaces a newer one. */
export function createRunningStore() {
  const [list, setList] = createSignal<RunningList>({ revision: -1, items: [] });
  return { list, offer: (next: RunningList) => setList((held) => newest(held, next)) };
}

export const store = createRunningStore();

export interface Block {
  /** The project id, or `chat` for the conversations of the Chat area. */
  key: string;
  items: RunningItem[];
  /** How many of its items wait for the user. */
  needs: number;
}

/** The engine's order, cut into one block per project plus one for Chat. A block keeps the place of its first item. */
export function blocksOf(items: RunningItem[]): Block[] {
  const blocks = new Map<string, Block>();
  for (const item of items) {
    const key = item.area === 'project' && item.projectId ? item.projectId : 'chat';
    const block = blocks.get(key) ?? { key, items: [], needs: 0 };
    block.items.push(item);
    if (item.state === 'needsYou') block.needs++;
    blocks.set(key, block);
  }
  return [...blocks.values()];
}

/** A question is named by its conversation, its turn and its request together: a request id alone repeats (Codex counts
 * from 0 in every server). */
export const askKey = (item: Pick<RunningItem, 'conversationId' | 'turnId'>, requestId: string) => `${item.conversationId}:${item.turnId}:${requestId}`;

export const waitingKeys = (items: RunningItem[]) => items.flatMap((item) => item.permissions.map((p) => askKey(item, p.requestId)));

export const QUIET_MS = 1500;

/** When the column opens by itself: once for each question, and not while the user is typing. */
export function createAutoOpen(quiet = QUIET_MS) {
  const shown = new Set<string>();
  let typed = Number.NEGATIVE_INFINITY;
  return {
    /** The user typed in a text field at `now` (milliseconds). */
    typed(now: number) {
      typed = now;
    },
    /** `open`: show the column now; every question waiting then counts as shown, so none of them opens it again, even if
     * the user closes the column. Otherwise `retryIn` is how long until it may open (it is waiting for the typing to
     * stop), or undefined when no new question waits. */
    check(keys: string[], now: number): { open: boolean; retryIn?: number } {
      const fresh = keys.filter((key) => !shown.has(key));
      if (fresh.length === 0) return { open: false };
      const wait = typed + quiet - now;
      if (wait > 0) return { open: false, retryIn: wait };
      for (const key of fresh) shown.add(key);
      return { open: true };
    },
  };
}

/** Where the user is: the area, the project of a project conversation, and the conversation. */
export type Place = { area: Area; projectId?: string; id?: string };

/** The place of a conversation the user opens, from the place they are in. A project conversation sets the project; a chat
 * leaves the last project as it was, so the Projects area comes back to it. Area, project and id go in together. */
export function openPlace(from: Place, c: { id: string; area: Area; projectId?: string }): Place {
  return { area: c.area, projectId: c.area === 'project' ? c.projectId : from.projectId, id: c.id };
}

export const placeOfRunning = (from: Place, item: RunningItem): Place => openPlace(from, { id: item.conversationId, area: item.area, projectId: item.projectId });

/** What an answer that was refused means for its card: a stale question is dropped quietly; anything else is shown by its
 * error key (an undelivered answer keeps the card so the user can try again). */
export function answerOutcome(err: unknown): 'stale' | { problem: string } {
  const key = errorText(err);
  return key === 'error.permission.stale' ? 'stale' : { problem: key };
}

/** How long a turn has been running, as a translation key and a count. */
export function elapsed(since: number, now: number): { key: 'time.seconds' | 'time.minutes' | 'time.hours'; n: number } {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  if (seconds < 60) return { key: 'time.seconds', n: seconds };
  return seconds < 3600 ? { key: 'time.minutes', n: Math.floor(seconds / 60) } : { key: 'time.hours', n: Math.floor(seconds / 3600) };
}
```

- [ ] **Step 5: Run the checks and see them pass**

```bash
npm run typecheck
npm run lint
cargo t --suite ui
```

Expected: the type check and the lint exit 0; the suite ends with `39 passed, 0 failed` (the 14 checks of `running.check.ts` are the new ones: `layout:`, `running:`, `auto-open:`, `places:`, `answers:`, `elapsed:`, `sidebar:` and `quota:`).

Do not commit.

---

### Task 3: The texts, in five languages  (model: `haiku`)

**Files:**
- Modify: `ui/locales/en.ts`, `ui/locales/pt-BR.ts`, `ui/locales/es.ts`, `ui/locales/fr.ts`, `ui/locales/de.ts`, `ui/models.ts`

**Interfaces:**
- Consumes: nothing.
- Produces: the keys `side.*` (new, label, hide, show, projects, newIn, noProjects, running, today, week, earlier, usage), `quota.unknown`, `crumb.new`, `crumb.project`, `topbar.model`, `empty.noAiButton`, `s1.try`, `s1.pickUp`, `suggest.1` to `suggest.4`, `run.*` (title, toggle, close, none, running, needs, needsYou, working, routing, untitled, empty, foot), `time.seconds`, `time.minutes`, `time.hours`, `model.autoChoose`; new texts for `empty.chatTitle` and `side.addProject`; the old `side.newChat`, `side.newConversation` and `side.chats` are removed. `run.running` and `run.needs` are plurals (`{ one, other }`, picked by `n`). `ui/models.ts` uses `model.autoChoose` for the label of Auto.

The texts are written by hand in each language. Do not translate them with a tool and do not change a word: they are the texts that were reviewed.

- [ ] **Step 1: Apply the five locale patches and the one for `ui/models.ts`**

Patch `ui/locales/en.ts`:

```diff
diff --git a/ui/locales/en.ts b/ui/locales/en.ts
index c864d88..fad7def 100644
--- a/ui/locales/en.ts
+++ b/ui/locales/en.ts
@@ -22,19 +22,54 @@ export const en = {
   'tray.open': 'Open Faber',
   'tray.quit': 'Quit Faber',
   'toast.dismiss': 'Dismiss',
-  'side.newChat': '+ New chat',
-  'side.addProject': '+ Add project folder',
-  'side.newConversation': '+ New conversation',
-  'side.chats': 'Chats',
+  'side.addProject': 'Add project folder',
   'side.noAi': 'No AI connected',
   'side.delete': 'Delete {title}',
   'side.confirmDelete': 'Delete?',
+  'side.new': 'New conversation',
+  'side.label': 'Sidebar',
+  'side.hide': 'Close the sidebar',
+  'side.show': 'Open the sidebar',
+  'side.projects': 'Projects',
+  'side.newIn': 'New conversation in {name}',
+  'side.noProjects': 'No projects yet',
+  'side.running': 'Running',
+  'side.today': 'Today',
+  'side.week': 'This week',
+  'side.earlier': 'Earlier',
+  'side.usage': 'Usage shows once the subscription reports it',
+  'quota.unknown': '—',
+  'crumb.new': 'New conversation',
+  'crumb.project': 'Projects',
+  'topbar.model': 'Model for the next message',
+  'empty.noAiButton': 'Connect a subscription',
+  's1.try': 'To start',
+  's1.pickUp': 'Pick up where you left off',
+  'suggest.1': 'Explain a topic I am stuck on in plain words',
+  'suggest.2': 'Help me write a message that is hard to start',
+  'suggest.3': 'Compare two options and tell me which fits me',
+  'suggest.4': 'Turn my rough notes into a clear plan',
+  'run.title': 'Running now',
+  'run.toggle': 'Show or hide what is running',
+  'run.close': 'Close the running list',
+  'run.none': 'Nothing running',
+  'run.running': { one: '{n} running', other: '{n} running' },
+  'run.needs': { one: '{n} needs you', other: '{n} need you' },
+  'run.needsYou': 'Needs you',
+  'run.working': 'working',
+  'run.routing': 'choosing a model',
+  'run.untitled': 'Untitled',
+  'run.empty': 'Nothing is running. Start something and it shows up here.',
+  'run.foot': 'You can keep talking while the work goes on.',
+  'time.seconds': '{n} s',
+  'time.minutes': '{n} min',
+  'time.hours': '{n} h',
   'empty.noAiTitle': 'No AI found',
   'empty.noAiBody': 'Connect your Claude or ChatGPT subscription in Settings → Subscriptions to start.',
   'empty.openProjectTitle': 'Open a project',
   'empty.openProjectBody': 'Add a folder and Faber works inside it, like a coding agent.',
   'empty.projectBody': 'Faber works inside {path}',
-  'empty.chatTitle': 'What can I help with?',
+  'empty.chatTitle': 'Where do we start?',
   'empty.chatBody': 'Chat never changes files on your computer.',
   'composer.chatPlaceholder': 'Ask anything',
   'composer.projectPlaceholder': 'Ask Faber to do something in this folder',
@@ -46,6 +81,7 @@ export const en = {
   'composer.send': 'Send',
   'composer.stop': 'Stop',
   'model.auto': 'Auto',
+  'model.autoChoose': 'AI: choose for me',
   'model.default': 'Default model',
   'reason.chosen': 'chosen by you',
   'reason.autoDefault': 'Auto: everyday default',
```

Patch `ui/locales/pt-BR.ts`:

```diff
diff --git a/ui/locales/pt-BR.ts b/ui/locales/pt-BR.ts
index 0c76267..7214b7f 100644
--- a/ui/locales/pt-BR.ts
+++ b/ui/locales/pt-BR.ts
@@ -21,19 +21,54 @@ export const ptBR: Messages = {
   'tray.open': 'Abrir o Faber',
   'tray.quit': 'Sair do Faber',
   'toast.dismiss': 'Dispensar',
-  'side.newChat': '+ Novo chat',
-  'side.addProject': '+ Adicionar pasta de projeto',
-  'side.newConversation': '+ Nova conversa',
-  'side.chats': 'Chats',
+  'side.addProject': 'Adicionar pasta de projeto',
   'side.noAi': 'Nenhuma IA conectada',
   'side.delete': 'Apagar {title}',
   'side.confirmDelete': 'Apagar?',
+  'side.new': 'Nova conversa',
+  'side.label': 'Barra lateral',
+  'side.hide': 'Fechar a barra lateral',
+  'side.show': 'Abrir a barra lateral',
+  'side.projects': 'Projetos',
+  'side.newIn': 'Nova conversa em {name}',
+  'side.noProjects': 'Nenhum projeto ainda',
+  'side.running': 'Rodando',
+  'side.today': 'Hoje',
+  'side.week': 'Esta semana',
+  'side.earlier': 'Antes',
+  'side.usage': 'O uso aparece quando a assinatura informar',
+  'quota.unknown': '—',
+  'crumb.new': 'Conversa nova',
+  'crumb.project': 'Projetos',
+  'topbar.model': 'Modelo da próxima mensagem',
+  'empty.noAiButton': 'Conectar uma assinatura',
+  's1.try': 'Para começar',
+  's1.pickUp': 'Continuar de onde parou',
+  'suggest.1': 'Explique, em palavras simples, um assunto em que estou travado',
+  'suggest.2': 'Me ajude a escrever uma mensagem difícil de começar',
+  'suggest.3': 'Compare duas opções e diga qual combina comigo',
+  'suggest.4': 'Transforme minhas anotações soltas em um plano claro',
+  'run.title': 'Rodando agora',
+  'run.toggle': 'Mostrar ou ocultar o que está rodando',
+  'run.close': 'Fechar a lista do que está rodando',
+  'run.none': 'Nada rodando',
+  'run.running': { one: '{n} rodando', other: '{n} rodando' },
+  'run.needs': { one: '{n} precisa de você', other: '{n} precisam de você' },
+  'run.needsYou': 'Precisa de você',
+  'run.working': 'trabalhando',
+  'run.routing': 'escolhendo um modelo',
+  'run.untitled': 'Sem título',
+  'run.empty': 'Nada rodando. Comece algo e aparece aqui.',
+  'run.foot': 'Você pode continuar a conversa enquanto o trabalho avança.',
+  'time.seconds': '{n} s',
+  'time.minutes': '{n} min',
+  'time.hours': '{n} h',
   'empty.noAiTitle': 'Nenhuma IA encontrada',
   'empty.noAiBody': 'Conecte a sua assinatura nas Configurações → Assinaturas para começar.',
   'empty.openProjectTitle': 'Abra um projeto',
   'empty.openProjectBody': 'Adicione uma pasta e o Faber trabalha dentro dela, como um agente de programação.',
   'empty.projectBody': 'O Faber trabalha dentro de {path}',
-  'empty.chatTitle': 'Em que posso ajudar?',
+  'empty.chatTitle': 'Por onde começamos?',
   'empty.chatBody': 'O chat nunca altera arquivos do seu computador.',
   'composer.chatPlaceholder': 'Pergunte qualquer coisa',
   'composer.projectPlaceholder': 'Peça ao Faber para fazer algo nesta pasta',
@@ -45,6 +80,7 @@ export const ptBR: Messages = {
   'composer.send': 'Enviar',
   'composer.stop': 'Parar',
   'model.auto': 'Automático',
+  'model.autoChoose': 'IA: escolher por mim',
   'model.default': 'Modelo padrão',
   'reason.chosen': 'escolhido por você',
   'reason.autoDefault': 'Automático: padrão do dia a dia',
```

Patch `ui/locales/es.ts`:

```diff
diff --git a/ui/locales/es.ts b/ui/locales/es.ts
index 321ccbf..f8e95bf 100644
--- a/ui/locales/es.ts
+++ b/ui/locales/es.ts
@@ -21,19 +21,54 @@ export const es: Messages = {
   'tray.open': 'Abrir Faber',
   'tray.quit': 'Salir de Faber',
   'toast.dismiss': 'Descartar',
-  'side.newChat': '+ Nuevo chat',
-  'side.addProject': '+ Añadir carpeta de proyecto',
-  'side.newConversation': '+ Nueva conversación',
-  'side.chats': 'Chats',
+  'side.addProject': 'Añadir carpeta de proyecto',
   'side.noAi': 'Ninguna IA conectada',
   'side.delete': 'Eliminar {title}',
   'side.confirmDelete': '¿Eliminar?',
+  'side.new': 'Conversación nueva',
+  'side.label': 'Barra lateral',
+  'side.hide': 'Cerrar la barra lateral',
+  'side.show': 'Abrir la barra lateral',
+  'side.projects': 'Proyectos',
+  'side.newIn': 'Conversación nueva en {name}',
+  'side.noProjects': 'Aún no hay proyectos',
+  'side.running': 'En marcha',
+  'side.today': 'Hoy',
+  'side.week': 'Esta semana',
+  'side.earlier': 'Antes',
+  'side.usage': 'El uso aparece cuando la suscripción lo informe',
+  'quota.unknown': '—',
+  'crumb.new': 'Conversación nueva',
+  'crumb.project': 'Proyectos',
+  'topbar.model': 'Modelo del próximo mensaje',
+  'empty.noAiButton': 'Conectar una suscripción',
+  's1.try': 'Para empezar',
+  's1.pickUp': 'Seguir donde lo dejaste',
+  'suggest.1': 'Explícame con palabras sencillas un tema en el que estoy atascado',
+  'suggest.2': 'Ayúdame a escribir un mensaje que me cuesta empezar',
+  'suggest.3': 'Compara dos opciones y dime cuál me conviene',
+  'suggest.4': 'Convierte mis apuntes sueltos en un plan claro',
+  'run.title': 'En marcha ahora',
+  'run.toggle': 'Mostrar u ocultar lo que está en marcha',
+  'run.close': 'Cerrar la lista de lo que está en marcha',
+  'run.none': 'Nada en marcha',
+  'run.running': { one: '{n} en marcha', other: '{n} en marcha' },
+  'run.needs': { one: '{n} te necesita', other: '{n} te necesitan' },
+  'run.needsYou': 'Te necesita',
+  'run.working': 'trabajando',
+  'run.routing': 'eligiendo un modelo',
+  'run.untitled': 'Sin título',
+  'run.empty': 'No hay nada en marcha. Empieza algo y aparecerá aquí.',
+  'run.foot': 'Puedes seguir conversando mientras avanza el trabajo.',
+  'time.seconds': '{n} s',
+  'time.minutes': '{n} min',
+  'time.hours': '{n} h',
   'empty.noAiTitle': 'No se encontró ninguna IA',
   'empty.noAiBody': 'Conecta tu suscripción en Ajustes → Suscripciones para comenzar.',
   'empty.openProjectTitle': 'Abre un proyecto',
   'empty.openProjectBody': 'Añade una carpeta y Faber trabaja dentro de ella, como un agente de programación.',
   'empty.projectBody': 'Faber trabaja dentro de {path}',
-  'empty.chatTitle': '¿En qué te puedo ayudar?',
+  'empty.chatTitle': '¿Por dónde empezamos?',
   'empty.chatBody': 'El chat nunca modifica archivos de tu ordenador.',
   'composer.chatPlaceholder': 'Pregunta lo que quieras',
   'composer.projectPlaceholder': 'Pídele a Faber que haga algo en esta carpeta',
@@ -45,6 +80,7 @@ export const es: Messages = {
   'composer.send': 'Enviar',
   'composer.stop': 'Detener',
   'model.auto': 'Automático',
+  'model.autoChoose': 'IA: elegir por mí',
   'model.default': 'Modelo predeterminado',
   'reason.chosen': 'elegido por ti',
   'reason.autoDefault': 'Automático: el habitual',
```

Patch `ui/locales/fr.ts`:

```diff
diff --git a/ui/locales/fr.ts b/ui/locales/fr.ts
index 87318cf..80e9cd3 100644
--- a/ui/locales/fr.ts
+++ b/ui/locales/fr.ts
@@ -21,19 +21,54 @@ export const fr: Messages = {
   'tray.open': 'Ouvrir Faber',
   'tray.quit': 'Quitter Faber',
   'toast.dismiss': 'Ignorer',
-  'side.newChat': '+ Nouveau chat',
-  'side.addProject': '+ Ajouter un dossier de projet',
-  'side.newConversation': '+ Nouvelle conversation',
-  'side.chats': 'Chats',
+  'side.addProject': 'Ajouter un dossier de projet',
   'side.noAi': 'Aucune IA connectée',
   'side.delete': 'Supprimer {title}',
   'side.confirmDelete': 'Supprimer ?',
+  'side.new': 'Nouvelle conversation',
+  'side.label': 'Barre latérale',
+  'side.hide': 'Fermer la barre latérale',
+  'side.show': 'Ouvrir la barre latérale',
+  'side.projects': 'Projets',
+  'side.newIn': 'Nouvelle conversation dans {name}',
+  'side.noProjects': "Aucun projet pour l'instant",
+  'side.running': 'En cours',
+  'side.today': "Aujourd'hui",
+  'side.week': 'Cette semaine',
+  'side.earlier': 'Plus tôt',
+  'side.usage': "L'usage s'affiche quand l'abonnement le communique",
+  'quota.unknown': '—',
+  'crumb.new': 'Nouvelle conversation',
+  'crumb.project': 'Projets',
+  'topbar.model': 'Modèle du prochain message',
+  'empty.noAiButton': 'Connecter un abonnement',
+  's1.try': 'Pour commencer',
+  's1.pickUp': 'Reprendre où vous en étiez',
+  'suggest.1': 'Expliquez-moi avec des mots simples un sujet où je bloque',
+  'suggest.2': 'Aidez-moi à écrire un message difficile à commencer',
+  'suggest.3': 'Comparez deux options et dites-moi laquelle me convient',
+  'suggest.4': 'Transformez mes notes en vrac en un plan clair',
+  'run.title': 'En cours',
+  'run.toggle': 'Afficher ou masquer ce qui tourne',
+  'run.close': 'Fermer la liste de ce qui tourne',
+  'run.none': 'Rien en cours',
+  'run.running': { one: '{n} en cours', other: '{n} en cours' },
+  'run.needs': { one: '{n} a besoin de vous', other: '{n} ont besoin de vous' },
+  'run.needsYou': 'A besoin de vous',
+  'run.working': 'travaille',
+  'run.routing': 'choisit un modèle',
+  'run.untitled': 'Sans titre',
+  'run.empty': 'Rien ne tourne. Lancez quelque chose et cela apparaîtra ici.',
+  'run.foot': 'Vous pouvez continuer à discuter pendant que le travail avance.',
+  'time.seconds': '{n} s',
+  'time.minutes': '{n} min',
+  'time.hours': '{n} h',
   'empty.noAiTitle': 'Aucune IA trouvée',
   'empty.noAiBody': 'Connectez votre abonnement dans Réglages → Abonnements pour commencer.',
   'empty.openProjectTitle': 'Ouvrez un projet',
   'empty.openProjectBody': 'Ajoutez un dossier et Faber travaille dedans, comme un agent de programmation.',
   'empty.projectBody': 'Faber travaille dans {path}',
-  'empty.chatTitle': 'Comment puis-je vous aider ?',
+  'empty.chatTitle': 'Par où commençons-nous ?',
   'empty.chatBody': 'Le chat ne modifie jamais les fichiers de votre ordinateur.',
   'composer.chatPlaceholder': 'Posez votre question',
   'composer.projectPlaceholder': 'Demandez à Faber de faire quelque chose dans ce dossier',
@@ -45,6 +80,7 @@ export const fr: Messages = {
   'composer.send': 'Envoyer',
   'composer.stop': 'Arrêter',
   'model.auto': 'Auto',
+  'model.autoChoose': 'IA : choisir pour moi',
   'model.default': 'Modèle par défaut',
   'reason.chosen': 'choisi par vous',
   'reason.autoDefault': 'Auto : le choix du quotidien',
```

Patch `ui/locales/de.ts`:

```diff
diff --git a/ui/locales/de.ts b/ui/locales/de.ts
index 4e406dc..a34ea8f 100644
--- a/ui/locales/de.ts
+++ b/ui/locales/de.ts
@@ -21,19 +21,54 @@ export const de: Messages = {
   'tray.open': 'Faber öffnen',
   'tray.quit': 'Faber beenden',
   'toast.dismiss': 'Ausblenden',
-  'side.newChat': '+ Neuer Chat',
-  'side.addProject': '+ Projektordner hinzufügen',
-  'side.newConversation': '+ Neue Unterhaltung',
-  'side.chats': 'Chats',
+  'side.addProject': 'Projektordner hinzufügen',
   'side.noAi': 'Keine KI verbunden',
   'side.delete': '{title} löschen',
   'side.confirmDelete': 'Löschen?',
+  'side.new': 'Neue Unterhaltung',
+  'side.label': 'Seitenleiste',
+  'side.hide': 'Seitenleiste schließen',
+  'side.show': 'Seitenleiste öffnen',
+  'side.projects': 'Projekte',
+  'side.newIn': 'Neue Unterhaltung in {name}',
+  'side.noProjects': 'Noch keine Projekte',
+  'side.running': 'Läuft',
+  'side.today': 'Heute',
+  'side.week': 'Diese Woche',
+  'side.earlier': 'Früher',
+  'side.usage': 'Die Nutzung erscheint, sobald das Abo sie meldet',
+  'quota.unknown': '—',
+  'crumb.new': 'Neue Unterhaltung',
+  'crumb.project': 'Projekte',
+  'topbar.model': 'Modell für die nächste Nachricht',
+  'empty.noAiButton': 'Abo verbinden',
+  's1.try': 'Zum Einstieg',
+  's1.pickUp': 'Dort weitermachen, wo du aufgehört hast',
+  'suggest.1': 'Erkläre mir in einfachen Worten ein Thema, bei dem ich nicht weiterkomme',
+  'suggest.2': 'Hilf mir, eine Nachricht zu schreiben, bei der mir der Anfang schwerfällt',
+  'suggest.3': 'Vergleiche zwei Möglichkeiten und sag mir, welche zu mir passt',
+  'suggest.4': 'Mach aus meinen losen Notizen einen klaren Plan',
+  'run.title': 'Läuft gerade',
+  'run.toggle': 'Anzeigen oder ausblenden, was läuft',
+  'run.close': 'Liste der laufenden Arbeit schließen',
+  'run.none': 'Nichts läuft',
+  'run.running': { one: '{n} läuft', other: '{n} laufen' },
+  'run.needs': { one: '{n} braucht dich', other: '{n} brauchen dich' },
+  'run.needsYou': 'Braucht dich',
+  'run.working': 'arbeitet',
+  'run.routing': 'wählt ein Modell',
+  'run.untitled': 'Ohne Titel',
+  'run.empty': 'Nichts läuft. Starte etwas, dann erscheint es hier.',
+  'run.foot': 'Du kannst weiterschreiben, während die Arbeit läuft.',
+  'time.seconds': '{n} s',
+  'time.minutes': '{n} min',
+  'time.hours': '{n} h',
   'empty.noAiTitle': 'Keine KI gefunden',
   'empty.noAiBody': 'Verbinde dein Abo unter Einstellungen → Abonnements, um zu beginnen.',
   'empty.openProjectTitle': 'Öffne ein Projekt',
   'empty.openProjectBody': 'Füge einen Ordner hinzu, und Faber arbeitet darin wie ein Coding-Agent.',
   'empty.projectBody': 'Faber arbeitet in {path}',
-  'empty.chatTitle': 'Wobei kann ich helfen?',
+  'empty.chatTitle': 'Wo fangen wir an?',
   'empty.chatBody': 'Der Chat ändert nie Dateien auf deinem Computer.',
   'composer.chatPlaceholder': 'Frag einfach',
   'composer.projectPlaceholder': 'Bitte Faber, in diesem Ordner etwas zu erledigen',
@@ -45,6 +80,7 @@ export const de: Messages = {
   'composer.send': 'Senden',
   'composer.stop': 'Stoppen',
   'model.auto': 'Automatisch',
+  'model.autoChoose': 'KI: für mich wählen',
   'model.default': 'Standardmodell',
   'reason.chosen': 'von dir gewählt',
   'reason.autoDefault': 'Automatisch: die Wahl für den Alltag',
```

Patch `ui/models.ts`:

```diff
diff --git a/ui/models.ts b/ui/models.ts
index b8f493d..5d86c98 100644
--- a/ui/models.ts
+++ b/ui/models.ts
@@ -10,7 +10,7 @@ const VENDOR_NAME: Record<Vendor, string> = { claude: 'Claude', codex: 'ChatGPT'
 
 // Claude Code has no model list, so its aliases are fixed; ChatGPT's list comes from the account.
 export function modelOptions(vendors: Vendor[], codexModels: ModelInfo[] = []): Option[] {
-  const options: Option[] = [{ label: t('model.auto'), choice: { vendor: 'auto' } }];
+  const options: Option[] = [{ label: t('model.autoChoose'), choice: { vendor: 'auto' } }];
   if (vendors.includes('claude')) {
     for (const [label, model] of [['Opus', 'opus'], ['Sonnet', 'sonnet'], ['Haiku', 'haiku']] as const) {
       options.push({ label: `Claude ${label}`, choice: { vendor: 'claude', model } });
```

- [ ] **Step 2: Check them**

```bash
npm run typecheck
cargo t --suite ui
```

Expected: the type check exits 0 and the suite passes with `i18n: every language fills every text and keeps the same placeholders as English` among the results. If a placeholder differs between languages, that check names the key.

Do not commit.

---

### Task 4: The icons, the shared permission card, the thread and the composer  (model: `sonnet`)

**Files:**
- Create: `ui/Glyphs.tsx`, `ui/Ask.tsx`
- Modify: `ui/Thread.tsx`, `ui/Composer.tsx`

**Interfaces:**
- Consumes: `answerOutcome` from `ui/running.ts` (Task 2); `api.answer(id, turnId, requestId, allow, always)` from B5a; the `ask.*` keys that already exist.
- Produces:
  - `ui/Glyphs.tsx`: `Icon(props: { name: IconName; small?: boolean })` with `IconName = 'chat' | 'folder' | 'plus' | 'ask' | 'right' | 'down' | 'close' | 'menu' | 'sliders'`, and `VendorMark(props: { vendor?: Vendor })` (a square for Claude, a diamond for the other, a plain dot while Auto is choosing).
  - `ui/Ask.tsx`: `Asks(props: { conversationId: string; turnId: string; permissions: Permission[] })`. It shows the first waiting question as a card and a note for the others. A stale answer drops the card quietly; any other refusal keeps it and shows the text of the error key.
  - `ui/Composer.tsx`: an optional prop `onReady?: (fill: (text: string) => void) => void`. `fill` puts the text in the composer without sending it, moves focus there and puts the cursor at the end.
  - `ui/Thread.tsx`: no longer takes `onAnswer`.

- [ ] **Step 1: Write the two new files**

Create `ui/Glyphs.tsx`:

```tsx
import { For } from 'solid-js';
import type { Vendor } from './api.ts';

// The small drawings of the shell, from the approved mock: thin lines in the colour of the text around them. They are
// decoration; the words next to them say what they mean.
const PATHS = {
  chat: ['M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z'],
  folder: ['M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'],
  plus: ['M5 12h14', 'M12 5v14'],
  // The speech bubble with a question mark: the one shape that means "needs you".
  ask: ['M7.9 20A9 9 0 1 0 4 16.1L2 22Z', 'M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3', 'M12 17h.01'],
  right: ['m9 18 6-6-6-6'],
  down: ['m6 9 6 6 6-6'],
  close: ['M18 6 6 18', 'm6 6 12 12'],
  menu: ['M4 6h16', 'M4 12h16', 'M4 18h16'],
  sliders: ['M20 7h-9', 'M14 17H5', 'M17 14a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z', 'M7 4a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z'],
} as const;

export function Icon(props: { name: keyof typeof PATHS; small?: boolean }) {
  return (
    <svg class="i" classList={{ sm: props.small }} viewBox="0 0 24 24" aria-hidden="true">
      <For each={PATHS[props.name]}>{(d) => <path d={d} />}</For>
    </svg>
  );
}

/** A vendor's shape and colour: a square for Claude, a diamond for Codex, a neutral dot while Auto is still choosing. The
 * vendor's name always comes with it. */
export function VendorMark(props: { vendor?: Vendor }) {
  return <span class="mk" classList={{ claude: props.vendor === 'claude', codex: props.vendor === 'codex' }} aria-hidden="true" />;
}
```

Create `ui/Ask.tsx`:

```tsx
import { createMemo, createSignal, Show } from 'solid-js';
import { api, type Permission } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import { answerOutcome } from './running.ts';

// What a permission request means, in plain words.
function describe(tool: string, input: unknown): { title: string; note?: string; body: string } {
  const i = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
  const raw = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
  if (tool === 'Bash' || tool === 'command') {
    return { title: t('ask.command'), note: typeof i.description === 'string' ? i.description : undefined, body: typeof input === 'string' ? input : String(i.command ?? raw) };
  }
  if (['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool)) return { title: t('ask.file'), body: String(i.file_path ?? i.path ?? raw) };
  if (tool === 'WebFetch') return { title: t('ask.page'), body: String(i.url ?? raw) };
  return { title: t('ask.other', { tool }), body: raw };
}

/** One question the CLI waits on. The conversation, the turn and the request name it, so an answer can only reach the
 * question it was given for. The same question can be on screen twice (in the thread and in the running column). */
function Card(props: { conversationId: string; turnId: string; permission: Permission }) {
  const [dropped, setDropped] = createSignal(false);
  const [sending, setSending] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  const d = () => describe(props.permission.tool, props.permission.input);
  const answer = async (allow: boolean, always = false) => {
    setProblem(undefined);
    setSending(true);
    try {
      await api.answer(props.conversationId, props.turnId, props.permission.requestId, allow, always);
    } catch (err) {
      // A question already answered (here, in the other copy, or elsewhere) is dropped without an alarm; an answer the
      // CLI did not get keeps the card, with the reason, so the user can try again.
      const outcome = answerOutcome(err);
      if (outcome === 'stale') setDropped(true);
      else setProblem(fromEngine(outcome.problem));
    } finally {
      setSending(false);
    }
  };
  return (
    <Show when={!dropped()}>
      <div class="ask" role="alertdialog" aria-label={t('ask.label')}>
        <b>{d().title}</b>
        <Show when={d().note}>
          <span class="note">{d().note}</span>
        </Show>
        <pre>{d().body}</pre>
        <div>
          <button class="btn primary" disabled={sending()} onClick={() => void answer(true)}>
            {t('ask.allow')}
          </button>
          <button class="btn" disabled={sending()} onClick={() => void answer(true, true)}>
            {t('ask.always')}
          </button>
          <button class="btn" disabled={sending()} onClick={() => void answer(false)}>
            {t('ask.deny')}
          </button>
        </div>
        <Show when={problem()}>
          <div class="note err" role="alert">
            {problem()}
          </div>
        </Show>
      </div>
    </Show>
  );
}

/** The first waiting question as a card, and a note for the others. `turnId` is the id of the message that holds the
 * question (in the thread) or of the running item's turn (in the column): the same id. */
export function Asks(props: { conversationId: string; turnId: string; permissions: Permission[] }) {
  // The card is kept while the same question stays first, however often the list is rebuilt around it.
  const first = createMemo(() => props.permissions[0], undefined, { equals: (a, b) => a?.requestId === b?.requestId });
  return (
    <>
      <Show when={first()} keyed>
        {(p) => <Card conversationId={props.conversationId} turnId={props.turnId} permission={p} />}
      </Show>
      <Show when={props.permissions.length > 1}>
        <div class="note">{t('ask.moreWaiting', { n: props.permissions.length - 1 })}</div>
      </Show>
    </>
  );
}
```

- [ ] **Step 2: Move the card out of the thread, and let the composer be filled**

Patch `ui/Thread.tsx` (it is LF since Task 1):

```diff
diff --git a/ui/Thread.tsx b/ui/Thread.tsx
index 5d13831..dd2175a 100644
--- a/ui/Thread.tsx
+++ b/ui/Thread.tsx
@@ -1,7 +1,8 @@
 import DOMPurify from 'dompurify';
 import { marked } from 'marked';
 import { createResource, createSignal, For, Index, Show, type Accessor } from 'solid-js';
-import { api, type Choice, type Message, type Permission, type ToolUse } from './api.ts';
+import { api, type Choice, type Message, type ToolUse } from './api.ts';
+import { Asks } from './Ask.tsx';
 import { fromEngine, t } from './i18n.ts';
 import { prettyModel, type Option } from './models.ts';
 
@@ -52,18 +53,6 @@ function toolLabel(tool: ToolUse): string {
   return detail ? `${tool.name}: ${String(detail)}` : tool.name;
 }
 
-// What a permission request means, in plain words.
-function describe(tool: string, input: unknown): { title: string; note?: string; body: string } {
-  const i = (input && typeof input === 'object' ? input : {}) as Record<string, unknown>;
-  const raw = typeof input === 'string' ? input : JSON.stringify(input, null, 2);
-  if (tool === 'Bash' || tool === 'command') {
-    return { title: t('ask.command'), note: typeof i.description === 'string' ? i.description : undefined, body: typeof input === 'string' ? input : String(i.command ?? raw) };
-  }
-  if (['Write', 'Edit', 'MultiEdit', 'NotebookEdit', 'fileChange'].includes(tool)) return { title: t('ask.file'), body: String(i.file_path ?? i.path ?? raw) };
-  if (tool === 'WebFetch') return { title: t('ask.page'), body: String(i.url ?? raw) };
-  return { title: t('ask.other', { tool }), body: raw };
-}
-
 function usage(m: Message): string {
   const parts: string[] = [];
   if (m.costUsd) parts.push(t('answer.cost', { cost: m.costUsd.toFixed(4) }));
@@ -71,41 +60,14 @@ function usage(m: Message): string {
   return parts.join(' · ');
 }
 
-/** `turnId` is the id of the message that holds the card. */
-type OnAnswer = (turnId: string, requestId: string, allow: boolean, always?: boolean) => void;
 type OnBranch = (messageId: string, text: string, choice?: Choice) => void;
 
 interface Shared {
   conversationId: string;
   options: Option[];
-  onAnswer: OnAnswer;
   onBranch: OnBranch;
 }
 
-function Ask(props: { permission: Permission; turnId: string; onAnswer: OnAnswer }) {
-  const d = () => describe(props.permission.tool, props.permission.input);
-  return (
-    <div class="ask" role="alertdialog" aria-label={t('ask.label')}>
-      <b>{d().title}</b>
-      <Show when={d().note}>
-        <span class="note">{d().note}</span>
-      </Show>
-      <pre>{d().body}</pre>
-      <div>
-        <button class="btn primary" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, true)}>
-          {t('ask.allow')}
-        </button>
-        <button class="btn" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, true, true)}>
-          {t('ask.always')}
-        </button>
-        <button class="btn" onClick={() => props.onAnswer(props.turnId, props.permission.requestId, false)}>
-          {t('ask.deny')}
-        </button>
-      </div>
-    </div>
-  );
-}
-
 function Answer(props: Shared & { m: Accessor<Message>; prompt?: Message }) {
   const m = () => props.m();
   const streaming = () => m().status === 'streaming';
@@ -135,12 +97,7 @@ function Answer(props: Shared & { m: Accessor<Message>; prompt?: Message }) {
           </For>
         </div>
       </Show>
-      <Show when={waiting()[0]} keyed>
-        {(p) => <Ask permission={p} turnId={m().id} onAnswer={props.onAnswer} />}
-      </Show>
-      <Show when={waiting().length > 1}>
-        <div class="note">{t('ask.moreWaiting', { n: waiting().length - 1 })}</div>
-      </Show>
+      <Asks conversationId={props.conversationId} turnId={m().id} permissions={waiting()} />
       <div class="md" onClick={openLinks}>
         <div innerHTML={html(m().text)} />
         <Show when={streaming() && !waiting().length}>
```

Patch `ui/Composer.tsx`:

```diff
diff --git a/ui/Composer.tsx b/ui/Composer.tsx
index 83e729f..26466c0 100644
--- a/ui/Composer.tsx
+++ b/ui/Composer.tsx
@@ -23,6 +23,9 @@ interface Props {
   onWarm: () => void;
   /** The user stopped typing for a moment: Auto routes the draft before it is sent. */
   onPause: (text: string) => void;
+  /** Hands the screen a function that puts text in the box and focuses it, without sending (the suggestions of the new
+   * conversation use it). */
+  onReady?: (fill: (text: string) => void) => void;
 }
 
 // Small text files go inside the message (no tool call needed); anything else is attached as a file.
@@ -65,6 +68,7 @@ export function Composer(props: Props) {
   let pause: ReturnType<typeof setTimeout> | undefined;
   onCleanup(() => clearTimeout(pause));
   let fileInput: HTMLInputElement | undefined;
+  let field: HTMLTextAreaElement | undefined;
   const selected = () => Math.max(0, props.options.findIndex((o) => o.choice.vendor === props.choice.vendor && o.choice.model === props.choice.model));
   const hasContent = () => draft().text.trim() !== '' || draft().images.length > 0 || draft().files.length > 0;
   const canSend = () => !props.busy && hasContent() && props.vendors.length > 0;
@@ -85,6 +89,12 @@ export function Composer(props: Props) {
     setNames([]);
     warmed = false;
   };
+  props.onReady?.((text) => {
+    setDraft((d) => ({ ...d, text }));
+    warm();
+    field?.focus();
+    field?.setSelectionRange(text.length, text.length);
+  });
   const add = async (list: Iterable<File>) => {
     const result = await attach(list, draft());
     setDraft(result.draft);
@@ -140,7 +150,10 @@ export function Composer(props: Props) {
           }
         }}
         autofocus
-        ref={(el) => queueMicrotask(() => el.focus())}
+        ref={(el) => {
+          field = el;
+          queueMicrotask(() => el.focus());
+        }}
       />
       <div class="cbar">
         <button type="button" class="pill" onClick={() => fileInput?.click()}>
```

- [ ] **Step 3: Check that the only errors left are in `ui/App.tsx`**

`ui/App.tsx` still passes `onAnswer` to the thread; Task 6 rewrites it.

```bash
npx tsc --noEmit 2>&1 | grep -E "^(ui|tests)/" | grep -v "^ui/App.tsx"
npm run lint
```

Expected: the `grep` prints nothing, and the lint exits 0.

Do not commit.

---

### Task 5: The running column, the sidebar and the greeting  (model: `sonnet`)

**Files:**
- Create: `ui/Running.tsx`, `ui/Sidebar.tsx`, `ui/Welcome.tsx`

**Interfaces:**
- Consumes: Tasks 2, 3 and 4, and `Peek` from `ui/Mark.tsx` (B4).
- Produces:
  - `Running(props: { items: RunningItem[]; projects: Project[]; onOpen: (item: RunningItem) => void; onClose: () => void; modal: boolean; ref: (el: HTMLElement) => void })`: an `<aside id="running" class="runcol">`. Its close button is the first button (focus lands there when the user opens it). `modal` sets `role="dialog"` and `aria-modal`.
  - `Sidebar(props: { conversations; projects; vendors; version; place: Place; running: RunningItem[]; quotas: Quotas; now: number; onNew; onArea; onProject; onNewIn; onAddProject; onOpen; onRemove; onSettings })`.
  - `Welcome(props: { recent: Summary[]; running: RunningItem[]; projects: Project[]; onSuggest: (text: string) => void; onOpen: (c: Summary) => void; children: JSX.Element })`: the greeting, the composer it was given as `children`, the four suggestions and "Pick up where you left off".

- [ ] **Step 1: Write the three components**

Create `ui/Running.tsx`:

```tsx
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { Project, RunningItem } from './api.ts';
import { Asks } from './Ask.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { vendorName } from './models.ts';
import { blocksOf, elapsed } from './running.ts';

/** The running column: what runs now, one block per project and one for Chat, in the order the engine sent. A question
 * that waits is answered here, in place; a click on a row opens its conversation. */
export function Running(props: {
  items: RunningItem[];
  projects: Project[];
  onOpen: (item: RunningItem) => void;
  onClose: () => void;
  /** Opened by the user over the stage: it is a modal dialog. Docked, or opened by itself, it is not. */
  modal: boolean;
  ref: (el: HTMLElement) => void;
}) {
  // The times move while the column is open and stop with it.
  const [now, setNow] = createSignal(Date.now());
  const tick = setInterval(() => setNow(Date.now()), 15_000);
  onCleanup(() => clearInterval(tick));
  const blocks = () => blocksOf(props.items);
  const nameOf = (key: string) => (key === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === key)?.name ?? t('area.projects')));
  const words = (item: RunningItem) => (item.state === 'needsYou' ? t('run.needsYou') : item.vendor ? t('run.working') : t('run.routing'));
  const since = (item: RunningItem) => {
    const e = elapsed(item.since, now());
    return t(e.key, { n: e.n });
  };
  return (
    <aside id="running" class="runcol" aria-labelledby="running-title" role={props.modal ? 'dialog' : undefined} aria-modal={props.modal ? 'true' : undefined} ref={props.ref}>
      <div class="rc-head">
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={() => props.onClose()}>
          <Icon name="close" />
        </button>
        <h2 id="running-title" class="display">
          {t('run.title')}
        </h2>
        <span class="n num">{props.items.length}</span>
      </div>
      <div class="rc-body">
        <Show
          when={props.items.length > 0}
          fallback={
            <div class="rc-empty">
              <Peek />
              <p>{t('run.empty')}</p>
            </div>
          }
        >
          <For each={blocks()}>
            {(block) => (
              <section class="proj-g">
                <h3>
                  <span class="t">{nameOf(block.key)}</span>
                  <Show when={block.needs > 0}>
                    <span class="amber-tag">
                      <Icon name="ask" small />
                      {t('run.needs', { n: block.needs })}
                    </span>
                  </Show>
                </h3>
                <ul class="ritems">
                  <For each={block.items}>
                    {(item) => (
                      <li class="ri" classList={{ need: item.state === 'needsYou' }}>
                        <div class="rhead">
                          <Show when={item.state === 'needsYou'} fallback={<span class="vm" classList={{ claude: item.vendor === 'claude', codex: item.vendor === 'codex' }}>
                            <VendorMark vendor={item.vendor} />
                            <Show when={item.vendor}>{(v) => <b>{vendorName(v())}</b>}</Show>
                            <span class="verb">{words(item)}</span>
                          </span>}>
                            <span class="ask-label">
                              <Icon name="ask" small />
                              {words(item)}
                            </span>
                          </Show>
                          <span class="time num">{since(item)}</span>
                        </div>
                        <button class="name" onClick={() => props.onOpen(item)}>
                          {item.title || t('run.untitled')}
                        </button>
                        <Show when={item.permissions.length > 0}>
                          <Asks conversationId={item.conversationId} turnId={item.turnId} permissions={item.permissions} />
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>
              </section>
            )}
          </For>
        </Show>
      </div>
      <div class="rc-foot">{t('run.foot')}</div>
    </aside>
  );
}
```

Create `ui/Sidebar.tsx`:

```tsx
import { createSignal, For, Show } from 'solid-js';
import type { Area, Project, RunningItem, Summary, Vendor } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { Peek } from './Mark.tsx';
import { vendorName } from './models.ts';
import type { Place } from './running.ts';
import { groupChats, type Quotas, tailOf, tightest } from './sidebar.ts';

function untilReset(resetsAt: number): string {
  const hours = (resetsAt * 1000 - Date.now()) / 3_600_000;
  if (hours <= 0) return t('quota.now');
  return hours < 24 ? t('quota.hours', { n: Math.ceil(hours) }) : t('quota.days', { n: Math.ceil(hours / 24) });
}

function windowName(window: string): string {
  const key = `quota.${window}`;
  return key in en ? t(key as Key) : window.replace('_', ' ');
}

interface Props {
  conversations: Summary[];
  projects: Project[];
  vendors: Vendor[];
  version: string;
  place: Place;
  running: RunningItem[];
  quotas: Quotas;
  /** The time now in milliseconds, which the screen moves on every minute: the groups of chats and the quota readings age. */
  now: number;
  /** A new conversation in the area and project the user is in. */
  onNew: () => void;
  onArea: (area: Area) => void;
  /** The user picked a project: its row is selected and no conversation is open. */
  onProject: (id: string) => void;
  /** A new conversation in this project. */
  onNewIn: (id: string) => void;
  onAddProject: () => void;
  onOpen: (conversation: Summary) => void;
  onRemove: (id: string) => void;
  onSettings: () => void;
}

/** The sidebar: new conversation, the two areas, the projects with their conversations, the chats by day, and at the
 * bottom Settings and the quota of each connected vendor. */
export function Sidebar(props: Props) {
  const [confirming, setConfirming] = createSignal<string>();
  // The projects whose conversations show. The project the user is in starts open.
  const [open, setOpen] = createSignal<ReadonlySet<string>>(new Set(props.place.projectId ? [props.place.projectId] : []));
  const toggle = (id: string, on: boolean) =>
    setOpen((held) => {
      const next = new Set(held);
      if (on) next.add(id);
      else next.delete(id);
      return next;
    });
  const inProject = (p: Project) => props.conversations.filter((c) => c.area === 'project' && c.projectId === p.id);
  const chats = () => groupChats(props.conversations.filter((c) => c.area === 'chat'), props.now);
  // The first click asks, the second deletes.
  const remove = (id: string) => {
    if (confirming() !== id) return setConfirming(id);
    setConfirming(undefined);
    props.onRemove(id);
  };

  const Row = (row: { c: Summary }) => (
    <div class="crow">
      <button class="item" aria-current={row.c.id === props.place.id} onClick={() => props.onOpen(row.c)}>
        <span>{row.c.title}</span>
      </button>
      <button class="x" classList={{ confirm: confirming() === row.c.id }} aria-label={t('side.delete', { title: row.c.title })} onClick={() => remove(row.c.id)}>
        {confirming() === row.c.id ? t('side.confirmDelete') : '×'}
      </button>
    </div>
  );

  return (
    <>
      <button class="newchat" onClick={() => props.onNew()}>
        <Icon name="plus" />
        <span>{t('side.new')}</span>
        <kbd>Ctrl N</kbd>
      </button>
      <div class="areas" role="group" aria-label={t('area.label')}>
        <button class="area" aria-pressed={props.place.area === 'chat'} onClick={() => props.onArea('chat')}>
          <Icon name="chat" />
          <span>{t('area.chat')}</span>
        </button>
        <button class="area" aria-pressed={props.place.area === 'project'} onClick={() => props.onArea('project')}>
          <Icon name="folder" />
          <span>{t('area.projects')}</span>
          <span class="count num">{props.projects.length}</span>
        </button>
      </div>
      <div class="side-scroll" onMouseLeave={() => setConfirming(undefined)}>
        <section class="projects" aria-labelledby="side-projects">
          <div class="group-h">
            <span id="side-projects">{t('side.projects')}</span>
            <button class="plus" aria-label={t('side.addProject')} title={t('side.addProject')} onClick={() => props.onAddProject()}>
              <Icon name="plus" small />
            </button>
          </div>
          <Show
            when={props.projects.length > 0}
            fallback={
              <div class="side-empty">
                <Peek />
                <span>{t('side.noProjects')}</span>
              </div>
            }
          >
            <For each={props.projects}>
              {(p) => {
                const tail = () => tailOf(props.running, p.id);
                const here = () => props.place.area === 'project' && props.place.projectId === p.id;
                return (
                  <div class="proj">
                    <div class="prow">
                      <button
                        class="item"
                        aria-current={here() && !props.place.id}
                        aria-expanded={open().has(p.id)}
                        title={p.path}
                        onClick={() => {
                          // A second click on the project the user is in folds its conversations.
                          toggle(p.id, !(here() && open().has(p.id)));
                          props.onProject(p.id);
                        }}
                      >
                        <span class="t">{p.name}</span>
                        <Show when={tail().running && tail().needs === 0}>
                          <span class="run-dot" role="img" aria-label={t('side.running')} />
                        </Show>
                        <Show when={tail().needs > 0}>
                          <span class="amber-tag">
                            <Icon name="ask" small />
                            {t('run.needs', { n: tail().needs })}
                          </span>
                        </Show>
                      </button>
                      <button class="plus" aria-label={t('side.newIn', { name: p.name })} title={t('side.newIn', { name: p.name })} onClick={() => (toggle(p.id, true), props.onNewIn(p.id))}>
                        <Icon name="plus" small />
                      </button>
                    </div>
                    <Show when={open().has(p.id)}>
                      <div class="proj-chats">
                        <For each={inProject(p)}>{(c) => <Row c={c} />}</For>
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </Show>
        </section>
        <div class="list">
          <For each={chats()}>
            {(g) => (
              <section>
                <div class="group-h">{t(`side.${g.group}` as Key)}</div>
                <For each={g.chats}>{(c) => <Row c={c} />}</For>
              </section>
            )}
          </For>
        </div>
      </div>
      <div class="side-foot">
        <button class="area" onClick={() => props.onSettings()} title={t('settings.shortcut')}>
          <Icon name="sliders" />
          <span>{t('settings.open')}</span>
        </button>
        <div class="usage" role="group" aria-label={t('quota.label')}>
          <For each={props.vendors}>
            {(v) => {
              const q = () => tightest(props.quotas, v, props.now);
              const share = () => (q() ? `${Math.round(q()!.used * 100)}%` : t('quota.unknown'));
              return (
                <button
                  class="vendor"
                  onClick={() => props.onSettings()}
                  title={q() ? `${t('quota.window', { window: windowName(q()!.window) })} · ${untilReset(q()!.resetsAt)}` : t('side.usage')}
                >
                  <VendorMark vendor={v} />
                  <span>{vendorName(v)}</span>
                  <span class="share num">{share()}</span>
                </button>
              );
            }}
          </For>
          <Show when={props.vendors.length === 0}>
            <span class="note">{t('side.noAi')}</span>
          </Show>
        </div>
        <div class="version num" lang={lang()}>
          v{props.version}
        </div>
      </div>
    </>
  );
}
```

Create `ui/Welcome.tsx`:

```tsx
import { For, type JSX, Show } from 'solid-js';
import type { Project, RunningItem, Summary } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { vendorName } from './models.ts';

const SUGGESTIONS = ['suggest.1', 'suggest.2', 'suggest.3', 'suggest.4'] as const;

/** The greeting with its last word set apart, so a look with an italic emphasis (Workshop) can draw it. */
function splitLast(text: string): [string, string] {
  const at = text.lastIndexOf(' ');
  return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + 1)];
}

function when(ms: number): string {
  const day = new Date(ms).toDateString() === new Date().toDateString();
  return new Intl.DateTimeFormat(lang(), day ? { hour: '2-digit', minute: '2-digit' } : { day: 'numeric', month: 'short' }).format(ms);
}

/** The new conversation (s1): the greeting, the composer it was given, four suggestions that fill the composer without
 * sending anything, and the three conversations touched last. The effects of the look sit behind it (ui/Ambient.tsx). */
export function Welcome(props: {
  recent: Summary[];
  running: RunningItem[];
  projects: Project[];
  onSuggest: (text: string) => void;
  onOpen: (conversation: Summary) => void;
  children: JSX.Element;
}) {
  const title = () => splitLast(t('empty.chatTitle'));
  const live = (id: string) => props.running.find((i) => i.conversationId === id);
  const where = (c: Summary) => (c.area === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects')));
  return (
    <div class="welcome-grid">
      <div class="w-main">
        <h1 class="display hello">
          {title()[0]}
          <Show when={title()[1]}>
            {' '}
            <em>{title()[1]}</em>
          </Show>
        </h1>
        <p class="lead">{t('empty.chatBody')}</p>
        {props.children}
        <section class="try" aria-labelledby="try-title">
          <h2 id="try-title">{t('s1.try')}</h2>
          <For each={SUGGESTIONS}>
            {(key) => (
              <button type="button" onClick={() => props.onSuggest(t(key))}>
                <span>{t(key)}</span>
                <Icon name="right" small />
              </button>
            )}
          </For>
        </section>
      </div>
      <Show when={props.recent.length > 0}>
        <section class="resume" aria-labelledby="resume-title">
          <h2 id="resume-title">{t('s1.pickUp')}</h2>
          <For each={props.recent}>
            {(c) => {
              const item = () => live(c.id);
              return (
                <button type="button" class="rrow" classList={{ ask: item()?.state === 'needsYou' }} onClick={() => props.onOpen(c)}>
                  <span class="t">{c.title}</span>
                  <span class="s">
                    <Show
                      when={item()}
                      fallback={
                        <>
                          {where(c)} · {when(c.updatedAt)}
                        </>
                      }
                    >
                      {(i) => (
                        <Show
                          when={i().state === 'needsYou'}
                          fallback={
                            <span class="vm" classList={{ claude: i().vendor === 'claude', codex: i().vendor === 'codex' }}>
                              <VendorMark vendor={i().vendor} />
                              <Show when={i().vendor}>{(v) => <b>{vendorName(v())}</b>}</Show>
                              <span class="verb">{i().vendor ? t('run.working') : t('run.routing')}</span>
                            </span>
                          }
                        >
                          <span class="amber-tag">
                            <Icon name="ask" small />
                            {t('run.needsYou')}
                          </span>
                        </Show>
                      )}
                    </Show>
                  </span>
                </button>
              );
            }}
          </For>
        </section>
      </Show>
    </div>
  );
}
```

- [ ] **Step 2: Check**

```bash
npx tsc --noEmit 2>&1 | grep -E "^(ui|tests)/" | grep -v "^ui/App.tsx"
npm run lint
```

Expected: the `grep` prints nothing, and the lint exits 0.

Do not commit.

---

### Task 6: The shell, the startup order and the styles  (model: `sonnet`)

**Files:**
- Modify: `ui/App.tsx` (replaced as a whole), `ui/main.tsx`, `ui/styles.css`

**Interfaces:**
- Consumes: everything above.
- Produces: the screen. In `ui/App.tsx`: one `place` signal (`Place`) and the helpers `go`, `fresh`, `openSummary`, `openRunning`; the sidebar overlay and the column state machines; the auto-open effect (it asks `createAutoOpen` on every new list, and retries when the user was typing); the keyboard (Ctrl N, Ctrl comma, Ctrl period for the column, Esc). In `ui/main.tsx`: the `Running` listener is registered, and awaited, before `state()` is asked; the snapshot's list is offered to the store with the same rule as an event.

- [ ] **Step 1: Replace `ui/App.tsx`**

Replace the whole file with:

```tsx
import { createEffect, createMemo, createSignal, Match, on, onCleanup, onMount, Show, Switch } from 'solid-js';
import { Ambient } from './Ambient.tsx';
import { api, type Appearance, type AppState, type Choice, type Conversation, type RunningItem, type Settings as Saved, type Summary } from './api.ts';
import { inOrder, saveAndApply, watchSystem } from './appearance.ts';
import { Composer, type Draft } from './Composer.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { failureText, lang, t, useLanguage } from './i18n.ts';
import { createWidth } from './layout.ts';
import { Lockup, Peek } from './Mark.tsx';
import { modelOptions, prettyModel } from './models.ts';
import { createOpening, upsert } from './opening.ts';
import { Running } from './Running.tsx';
import { createAutoOpen, openPlace, type Place, placeOfRunning, store, waitingKeys } from './running.ts';
import { Settings } from './Settings.tsx';
import { Setup } from './Setup.tsx';
import { shouldOpenSetup } from './setup.ts';
import { Sidebar } from './Sidebar.tsx';
import { seedQuotas, withQuota } from './sidebar.ts';
import { Thread } from './Thread.tsx';
import { Welcome } from './Welcome.tsx';
import { mac, WindowControls } from './WindowControls.tsx';

// Small per-window conveniences; storage can be missing, and the app works without it.
function recall<T>(key: string, fallback: T): T {
  try {
    return (JSON.parse(localStorage.getItem(key) ?? 'null') as T | null) ?? fallback;
  } catch {
    return fallback;
  }
}
function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // per-viewer convenience only
  }
}

// A long conversation draws only its latest messages, and more as the user scrolls up, so memory stays flat.
const PAGE = 40;

const TYPING = 'textarea, input:not([type=checkbox], [type=radio], [type=range], [type=color], [type=file]), [contenteditable]';

/** `initial` is the state the page started from; `main.tsx` has already applied its look and handed the running list to
 * the store. */
export function App(props: { initial: AppState }) {
  const saved = recall<Place | undefined>('faber.place', undefined);
  useLanguage(props.initial.settings.language);
  const [state, setState] = createSignal<AppState>(props.initial);
  // The area, the project and the conversation the user is in, changed together so no screen ever shows a conversation
  // with another conversation's project.
  const [place, setPlace] = createSignal<Place>({ area: saved?.area ?? 'chat', projectId: saved?.projectId });
  const [current, setCurrent] = createSignal<Conversation>();
  const [choice, setChoice] = createSignal<Choice>(recall<Choice>('faber.choice', { vendor: 'auto' }));
  const [quotas, setQuotas] = createSignal(seedQuotas(props.initial.quotas));
  const [clock, setClock] = createSignal(Date.now());
  const [error, setError] = createSignal<string>();
  // The text of the last refused or failed save of settings; the page stays as it was.
  const [saveError, setSaveError] = createSignal<string>();
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [setupOpen, setSetupOpen] = createSignal(shouldOpenSetup(props.initial.settings, props.initial.conversations));
  const [from, setFrom] = createSignal(0);
  const opening = createOpening();
  const width = createWidth();
  let thread: HTMLDivElement | undefined;
  let work: HTMLElement | undefined;
  let fill: ((text: string) => void) | undefined;
  let stick = true;

  const area = () => place().area;
  const projectId = () => place().projectId;
  const refresh = () =>
    api.state().then((s) => {
      store.offer(s.running);
      setState(s);
    });
  const fail = (err: unknown) => setError(failureText(err));
  const items = () => store.list().items;
  const needing = () => items().filter((i) => i.state === 'needsYou').length;

  // ---- The sidebar: beside the stage from 900 px, behind a button below that. Opened, it is modal.
  const [sideOpen, setSideOpen] = createSignal(false);
  const narrow = () => width() === 'narrow';
  const sideOverlay = () => narrow() && sideOpen();
  let sideEl: HTMLElement | undefined;
  let menuButton: HTMLButtonElement | undefined;
  const openSide = () => {
    setSideOpen(true);
    queueMicrotask(() => sideEl?.querySelector<HTMLElement>('button')?.focus());
  };
  const closeSide = (focusBack = true) => {
    if (!sideOpen()) return;
    setSideOpen(false);
    if (focusBack) menuButton?.focus();
  };
  createEffect(() => {
    if (!narrow()) setSideOpen(false);
  });

  // ---- The running column: closed when the window opens. Beside the stage from 1,100 px, over it below that. Opened by
  // the user over the stage it is modal; opened by itself it is never modal and never moves focus.
  const [columnOpen, setColumnOpen] = createSignal(false);
  const [byUser, setByUser] = createSignal(false);
  const docked = () => width() === 'wide';
  const trapped = () => columnOpen() && byUser() && !docked();
  let column: HTMLElement | undefined;
  let runButton: HTMLButtonElement | undefined;
  const openColumn = (user: boolean) => {
    if (columnOpen()) return;
    setByUser(user);
    setColumnOpen(true);
    if (user && !docked()) queueMicrotask(() => column?.querySelector<HTMLElement>('button')?.focus());
  };
  const closeColumn = () => {
    if (!columnOpen()) return;
    setColumnOpen(false);
    setByUser(false);
    runButton?.focus();
  };
  const toggleColumn = () => (columnOpen() ? closeColumn() : openColumn(true));

  const autoOpen = createAutoOpen();
  let retry: ReturnType<typeof setTimeout> | undefined;
  const consider = () => {
    clearTimeout(retry);
    const verdict = autoOpen.check(waitingKeys(items()), Date.now());
    if (verdict.open) openColumn(false);
    else if (verdict.retryIn !== undefined) retry = setTimeout(consider, verdict.retryIn + 20);
  };
  // Every list that arrives, and the window's first one, may hold a question the column has not shown yet.
  createEffect(() => {
    items();
    consider();
  });

  // The conversation the user is in: opening it asks the engine for a snapshot and the answer takes a while.
  const go = async (next: Place) => {
    setPlace(next);
    remember('faber.place', next);
    const token = opening.begin(next.id);
    setError(undefined);
    stick = true;
    const snapshot = (next.id ? await api.open(next.id) : undefined) ?? undefined;
    // Messages that came while the snapshot was on its way are already in it, merged by `opening`.
    const done = opening.finish(token, snapshot);
    if (done.stale) return;
    setFrom(Math.max(0, (done.conversation?.messages.length ?? 0) - PAGE));
    setCurrent(done.conversation);
  };
  const fresh = (next: Pick<Place, 'area' | 'projectId'>) => go({ ...next, id: undefined });
  const openSummary = (c: Summary) => {
    setSettingsOpen(false);
    closeSide(false);
    void go(openPlace(place(), c));
  };
  // A row of the running column: the area, the project and the conversation change together, so the crumb, the sidebar,
  // the composer and the next send all point at the same place. A conversation that is gone does nothing.
  const openRunning = (item: RunningItem) => {
    if (!state().conversations.some((c) => c.id === item.conversationId)) return;
    setSettingsOpen(false);
    if (!docked()) closeColumn();
    void go(placeOfRunning(place(), item));
  };

  onMount(() => {
    if (saved?.id && props.initial.conversations.some((c) => c.id === saved.id)) void go({ ...saved, area: saved.area ?? 'chat' });
    const stopFollowing = watchSystem();
    const unlisten = api.on((e) => {
      if (e.type === 'list') void refresh();
      if (e.type === 'quota') setQuotas((q) => withQuota(q, e));
      if (e.type === 'message' && opening.hear(e.conversationId, e.message) === 'apply') {
        setCurrent((c) => (c?.id === e.conversationId ? upsert(c, e.message) : c));
      }
    });
    const minute = setInterval(() => setClock(Date.now()), 60_000);
    // Typing in any text field holds the running column back for a moment.
    const typing = (e: Event) => {
      if (e.target instanceof Element && e.target.matches(TYPING)) autoOpen.typed(Date.now());
    };
    const onKey = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        setSettingsOpen(false);
        setSetupOpen(false);
        closeSide(false);
        void fresh(place());
      }
      if (ctrl && e.key === ',') setSettingsOpen(true);
      if (ctrl && e.key === '.') {
        e.preventDefault();
        toggleColumn();
      }
      if (e.key === 'Escape') {
        if (sideOverlay()) return closeSide();
        // A column the user opened over the stage is modal; one that opened itself answers Esc only while focus is in it.
        if (columnOpen() && (trapped() || column?.contains(document.activeElement))) return closeColumn();
        setSettingsOpen(false);
        setSetupOpen(false);
        setSaveError(undefined);
      }
    };
    window.addEventListener('input', typing, true);
    window.addEventListener('keydown', onKey);
    onCleanup(() => {
      stopFollowing();
      void unlisten.then((stop) => stop());
      clearInterval(minute);
      clearTimeout(retry);
      window.removeEventListener('input', typing, true);
      window.removeEventListener('keydown', onKey);
    });
  });

  // The tray menu lives outside the page, so it gets its words from here.
  createEffect(on(lang, () => void api.setTray(t('tray.open'), t('tray.quit')).catch(() => undefined)));

  // Follow the answer while it streams, unless the user scrolled up to read.
  createEffect(
    on(current, () => {
      if (thread && stick) thread.scrollTop = thread.scrollHeight;
    }),
  );

  const onScroll = (el: HTMLDivElement) => {
    stick = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    if (el.scrollTop < 400 && from() > 0) {
      // The earlier messages appear above; keep what the user is reading in place.
      const before = el.scrollHeight;
      setFrom((f) => Math.max(0, f - PAGE));
      el.scrollTop += el.scrollHeight - before;
    }
  };

  // A saved choice that is no longer in the list (a model the account dropped, or the old generic ChatGPT entry)
  // falls back to Auto without forgetting it, so it comes back if the model does.
  const picked = (): Choice => {
    const c = choice();
    return options().some((o) => o.choice.vendor === c.vendor && o.choice.model === c.model) ? c : { vendor: 'auto' };
  };

  const pickChoice = (c: Choice) => {
    setChoice(c);
    remember('faber.choice', c);
  };

  const target = () => ({ area: area(), projectId: area() === 'project' ? projectId() : undefined });

  const send = async (draft: Draft) => {
    setError(undefined);
    stick = true;
    try {
      const id = await api.send({ conversationId: current()?.id, ...target(), text: draft.text, images: draft.images, files: draft.files, choice: picked() });
      if (id !== opening.id) await go({ ...place(), id });
    } catch (err) {
      fail(err);
    }
  };

  const branch = async (messageId: string, text: string, picked: Choice) => {
    const c = current();
    if (!c) return;
    setError(undefined);
    try {
      await go({ ...place(), id: await api.branch({ conversationId: c.id, messageId, text, choice: picked }) });
    } catch (err) {
      fail(err);
    }
  };

  // Saves run one at a time, so each is built from what the one before stored and answers in order.
  const inTurn = inOrder();
  // The page changes only after the save succeeded. A refusal or a failed write keeps it as it was and says why.
  const persist = async (patch: Saved): Promise<boolean> => {
    setSaveError(undefined);
    try {
      const saved = await saveAndApply(patch, api.saveSettings);
      setState((s) => ({ ...s, settings: saved.settings, look: saved.look }));
      if (patch.language !== undefined) useLanguage(saved.settings.language);
      return true;
    } catch (err) {
      setSaveError(failureText(err));
      return false;
    }
  };
  const saveSettings = (patch: Saved) => inTurn(() => persist(patch));
  const changeAppearance = (next: (current: Appearance | undefined) => Appearance) =>
    inTurn(() => persist({ appearance: next(state().settings.appearance) }));

  const switchArea = (next: Place['area']) => {
    setSettingsOpen(false);
    closeSide(false);
    void fresh({ area: next, projectId: projectId() });
  };
  const addProject = async () => {
    const project = await api.addProject();
    if (project) {
      closeSide(false);
      void fresh({ area: 'project', projectId: project.id });
    }
  };
  const remove = async (id: string) => {
    await api.remove(id);
    if (opening.id === id) void fresh(place());
  };

  const options = createMemo(() => (lang(), modelOptions(state().vendors, state().models)));
  const project = () => state().projects.find((p) => p.id === projectId());
  const busy = () => current()?.messages.at(-1)?.status === 'streaming';
  const needsProject = () => area() === 'project' && !project();
  // The new conversation of a chat with a connected AI is the one place the effects of a look appear.
  const newChat = () => area() === 'chat' && state().vendors.length > 0 && !current()?.messages.length;
  const composerKey = () => current()?.id ?? `new-${area()}-${projectId() ?? ''}`;
  const title = () => state().conversations.find((c) => c.id === place().id)?.title ?? current()?.title;
  // The model the open conversation is answering with now, else the one the next message will go to.
  const answering = () => current()?.messages.findLast((m) => m.role === 'assistant' && m.vendor);

  const composer = () => (
    <Show when={composerKey()} keyed>
      <Composer
        vendors={state().vendors}
        options={options()}
        choice={picked()}
        onChoice={pickChoice}
        busy={busy()}
        placeholder={area() === 'chat' ? t('composer.chatPlaceholder') : t('composer.projectPlaceholder')}
        onSend={(d) => void send(d)}
        onStop={() => current() && void api.interrupt(current()!.id)}
        onWarm={() => void api.warm({ conversationId: current()?.id, ...target(), choice: picked() })}
        onPause={(text) => {
          if (picked().vendor === 'auto') void api.routeDraft({ conversationId: current()?.id, ...target(), choice: picked() }, text);
        }}
        onReady={(f) => (fill = f)}
      />
    </Show>
  );

  const runningColumn = () => (
    <Running
      items={items()}
      projects={state().projects}
      modal={trapped()}
      onOpen={openRunning}
      onClose={closeColumn}
      ref={(el) => (column = el)}
    />
  );

  return (
    <div class="app">
      {/* The title bar is the window's title bar: empty space drags the window, double click maximizes. */}
      <header class="top" classList={{ mac }} data-tauri-drag-region>
        <span class="brand" data-tauri-drag-region>
          <Lockup height={20} />
        </span>
        <WindowControls />
      </header>
      <Show when={saveError()}>
        <div class="toast" role="alert">
          <span>{saveError()}</span>
          <button type="button" aria-label={t('toast.dismiss')} onClick={() => setSaveError(undefined)}>
            ×
          </button>
        </div>
      </Show>
      <Show
        when={!setupOpen()}
        fallback={
          <Setup
            onClose={() => setSetupOpen(false)}
            onFinish={(result) =>
              void saveSettings({ setup: result }).then((saved) => {
                if (!saved) return;
                setSetupOpen(false);
                void refresh();
              })
            }
          />
        }
      >
        <Show
          when={!settingsOpen()}
          fallback={
            <Settings
              look={state().look}
              saved={state().settings}
              onAppearance={changeAppearance}
              onSave={saveSettings}
              onClose={() => setSettingsOpen(false)}
              onOpenSetup={() => {
                setSettingsOpen(false);
                setSetupOpen(true);
              }}
            />
          }
        >
          <div class="shell" classList={{ docked: columnOpen() && docked() }}>
            <aside
              class="side"
              classList={{ shown: sideOverlay() }}
              ref={(el) => (sideEl = el)}
              aria-label={t('side.label')}
              role={sideOverlay() ? 'dialog' : undefined}
              aria-modal={sideOverlay() ? 'true' : undefined}
              inert={trapped()}
            >
              <Sidebar
                conversations={state().conversations}
                projects={state().projects}
                vendors={state().vendors}
                version={state().version}
                place={place()}
                running={items()}
                quotas={quotas()}
                now={clock()}
                onNew={() => (closeSide(false), void fresh(place()))}
                onArea={switchArea}
                onProject={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onNewIn={(id) => (closeSide(false), void fresh({ area: 'project', projectId: id }))}
                onAddProject={() => void addProject()}
                onOpen={openSummary}
                onRemove={(id) => void remove(id)}
                onSettings={() => (closeSide(false), setSettingsOpen(true))}
              />
            </aside>
            <Show when={sideOverlay()}>
              <button class="scrim" aria-label={t('side.hide')} onClick={() => closeSide()} />
            </Show>
            <main class="main" inert={sideOverlay()}>
              <div class="topbar" classList={{ bare: newChat() }} inert={trapped()}>
                <Show when={narrow()}>
                  <button class="iconbtn menu" ref={(el) => (menuButton = el)} aria-label={t('side.show')} aria-expanded={sideOverlay()} onClick={openSide}>
                    <Icon name="menu" />
                  </button>
                </Show>
                <div class="crumb">
                  <Switch>
                    <Match when={place().id}>
                      {area() === 'chat' ? t('area.chat') : t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        ›
                      </span>
                      <b>{title() ?? ''}</b>
                    </Match>
                    <Match when={area() === 'project' && project()}>
                      {t('crumb.project')}
                      <span class="sep" aria-hidden="true">
                        ›
                      </span>
                      <b>{project()?.name}</b>
                    </Match>
                    <Match when={area() === 'project'}>
                      <b>{t('crumb.project')}</b>
                    </Match>
                    <Match when={true}>
                      <b>{t('crumb.new')}</b>
                    </Match>
                  </Switch>
                </div>
                <div class="right">
                  <Show when={state().vendors.length > 0}>
                    <span class="chip" title={t('topbar.model')}>
                      <Show when={answering()} fallback={<>{options().find((o) => o.choice.vendor === picked().vendor && o.choice.model === picked().model)?.label}</>}>
                        {(m) => (
                          <>
                            <VendorMark vendor={m().vendor} />
                            {prettyModel(m().model)}
                          </>
                        )}
                      </Show>
                    </span>
                  </Show>
                  <button
                    class="run-btn"
                    classList={{ need: needing() > 0 }}
                    ref={(el) => (runButton = el)}
                    aria-expanded={columnOpen()}
                    aria-controls="running"
                    title={t('run.toggle')}
                    onClick={toggleColumn}
                  >
                    <Show when={items().length > 0} fallback={<span class="seg">{t('run.none')}</span>}>
                      <span class="seg">
                        <span class="live" aria-hidden="true" />
                        {t('run.running', { n: items().length })}
                      </span>
                      <Show when={needing() > 0}>
                        <span class="seg ask">
                          <Icon name="ask" small />
                          {t('run.needs', { n: needing() })}
                        </span>
                      </Show>
                    </Show>
                  </button>
                </div>
              </div>
              <div class="work" ref={(el) => (work = el)}>
                <Show when={newChat()}>
                  {/* The effects of the look sit behind the greeting; dust keeps clear of the text and the composer. */}
                  <Ambient vendors={state().vendors} avoid={() => [...work!.querySelectorAll('.hello, .lead, .composer, .try, .resume')]} />
                </Show>
                <div class="thread" classList={{ welcome: newChat() }} ref={(el) => (thread = el)} onScroll={(e) => onScroll(e.currentTarget)} inert={trapped()}>
                  <Show
                    when={newChat()}
                    fallback={
                      <div class="col">
                        <Show
                          when={current()?.messages.length ? current() : undefined}
                          fallback={<Empty vendors={state().vendors.length} area={area()} project={project()} onConnect={() => setSetupOpen(true)} />}
                        >
                          {(c) => (
                            <Thread
                              messages={c().messages}
                              from={from()}
                              conversationId={c().id}
                              options={options()}
                              onBranch={(messageId, text, chosen) => void branch(messageId, text, chosen ?? picked())}
                            />
                          )}
                        </Show>
                        <Show when={error()}>
                          <div class="note err" role="alert">
                            {error()}
                          </div>
                        </Show>
                      </div>
                    }
                  >
                    <Welcome
                      recent={state().conversations.slice(0, 3)}
                      running={items()}
                      projects={state().projects}
                      onSuggest={(text) => fill?.(text)}
                      onOpen={openSummary}
                    >
                      {composer()}
                    </Welcome>
                    <Show when={error()}>
                      <div class="note err" role="alert">
                        {error()}
                      </div>
                    </Show>
                  </Show>
                </div>
                <Show when={!newChat() && !needsProject()}>
                  <div class="dock" inert={trapped()}>
                    {composer()}
                  </div>
                </Show>
                <Show when={columnOpen() && !docked()}>{runningColumn()}</Show>
              </div>
            </main>
            <Show when={columnOpen() && docked()}>{runningColumn()}</Show>
          </div>
        </Show>
      </Show>
    </div>
  );
}

function Empty(props: { vendors: number; area: Place['area']; project?: { name: string; path: string }; onConnect: () => void }) {
  return (
    <div class="empty">
      <Switch>
        <Match when={props.vendors === 0}>
          <Peek />
          <h1 class="display">{t('empty.noAiTitle')}</h1>
          <p>{t('empty.noAiBody')}</p>
          <button class="btn primary" onClick={() => props.onConnect()}>
            {t('empty.noAiButton')}
          </button>
        </Match>
        <Match when={props.area === 'project' && !props.project}>
          <Peek />
          <h1 class="display">{t('empty.openProjectTitle')}</h1>
          <p>{t('empty.openProjectBody')}</p>
        </Match>
        <Match when={props.area === 'project'}>
          <Peek />
          <h1 class="display">{props.project?.name}</h1>
          <p>{t('empty.projectBody', { path: props.project?.path ?? '' })}</p>
        </Match>
      </Switch>
    </div>
  );
}
```

- [ ] **Step 2: Startup order and the styles**

Patch `ui/main.tsx`:

```diff
diff --git a/ui/main.tsx b/ui/main.tsx
index 9441c1c..3f5ba6a 100644
--- a/ui/main.tsx
+++ b/ui/main.tsx
@@ -4,12 +4,21 @@ import './styles.css';
 import { api } from './api.ts';
 import { App } from './App.tsx';
 import { applyLook } from './appearance.ts';
+import { store } from './running.ts';
 
 // Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
 if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');
 
-// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
+// The running list is listened for first, and only then is the snapshot asked for. Both carry a revision and the store
+// keeps the highest one, so a change that lands between the two is never lost and an old snapshot never overwrites a
+// newer event.
+await api.on((e) => {
+  if (e.type === 'running') store.offer({ revision: e.revision, items: e.items });
+});
 const state = await api.state();
+store.offer(state.running);
+
+// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
 applyLook(state.look);
 
 render(() => <App initial={state} />, document.getElementById('root')!);
```

Patch `ui/styles.css`:

```diff
diff --git a/ui/styles.css b/ui/styles.css
index 1caf70e..9c19702 100644
--- a/ui/styles.css
+++ b/ui/styles.css
@@ -16,62 +16,100 @@ input, textarea { caret-color: var(--accent); }
 .display em { font-style: var(--em-style); color: var(--em-color); font-weight: var(--em-weight); }
 
 /* The window: a title bar over the frame, the sidebar on the frame, and the stage where the work happens. */
-.app { display: grid; grid-template-rows: 40px 1fr; height: 100%; background: var(--frame); }
+.app { display: grid; grid-template-rows: 36px minmax(0, 1fr); height: 100%; background: var(--frame); }
 .top { display: flex; align-items: center; gap: 14px; padding: 0 0 0 20px; user-select: none; }
 .top.mac { padding: 0 14px 0 78px; } /* room for the traffic lights */
 /* The lockup is a drawing in the ink colour, in every theme. The drag region is the span: Tauri reads the attribute only on the element clicked. */
 .brand { display: flex; color: var(--ink); }
 .brand svg { display: block; pointer-events: none; }
-.caption { display: flex; align-self: stretch; margin-left: -4px; }
+.caption { display: flex; align-self: stretch; margin-left: auto; }
 .caption button { width: 46px; display: grid; place-items: center; color: var(--ink-3); border-radius: 0; }
 .caption button:hover { background: var(--s3); color: var(--ink); }
 .caption .close:hover { background: var(--danger); color: var(--bg); }
-.gear { color: var(--ink-3); font-size: calc(var(--fs-ui) - 1px); padding: 4px 6px; }
-.gear:hover, .gear[aria-pressed='true'] { color: var(--ink); }
-.quota { margin-left: auto; display: flex; gap: 14px; font-size: calc(var(--fs-ui) - 2px); color: var(--ink-3); align-items: center; }
-.q { display: inline-flex; align-items: center; gap: 7px; }
-.q i { display: block; width: 56px; height: var(--bar-h); border-radius: var(--bar-r); background: var(--bar-track); overflow: hidden; position: relative; }
-.q i::after { content: ''; position: absolute; inset: 0 auto 0 0; width: var(--v); background: var(--bar-fill); }
 
-/* Choices that exclude each other: the area toggle and the segmented controls of Settings. */
-.toggle, .seg-ctl { display: flex; padding: 3px; gap: 2px; border-radius: var(--r-sm); background: var(--s1); box-shadow: inset 0 0 0 1px var(--line-2); }
-.toggle button, .seg-ctl button { padding: 0 11px; height: 26px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; border-radius: calc(var(--r-sm) - 2px); font-size: calc(var(--fs-ui) - 1px); color: var(--ink-2); white-space: nowrap; transition: background var(--dur-fast), color var(--dur-fast); }
-.toggle button:hover, .seg-ctl button:hover { color: var(--ink); }
-.toggle button[aria-pressed='true'], .seg-ctl button[aria-pressed='true'] { background: var(--s3); color: var(--ink); box-shadow: 0 0 0 1px var(--line-2); }
+/* Choices that exclude each other: the segmented controls of Settings. */
+.seg-ctl { display: flex; padding: 3px; gap: 2px; border-radius: var(--r-sm); background: var(--s1); box-shadow: inset 0 0 0 1px var(--line-2); }
+.seg-ctl button { padding: 0 11px; height: 26px; display: inline-flex; align-items: center; justify-content: center; gap: 6px; border-radius: calc(var(--r-sm) - 2px); font-size: calc(var(--fs-ui) - 1px); color: var(--ink-2); white-space: nowrap; transition: background var(--dur-fast), color var(--dur-fast); }
+.seg-ctl button:hover { color: var(--ink); }
+.seg-ctl button[aria-pressed='true'] { background: var(--s3); color: var(--ink); box-shadow: 0 0 0 1px var(--line-2); }
 .seg-ctl button { flex: 1; height: 30px; }
 
-.toast { position: fixed; top: 48px; right: 16px; z-index: 20; display: flex; align-items: center; gap: 10px; max-width: min(480px, calc(100vw - 32px)); padding: 10px 8px 10px 16px; border-radius: var(--r-md); background: var(--s3); color: var(--ink); box-shadow: inset 3px 0 0 var(--danger), var(--lift-shadow); animation: arrive var(--dur-arrive) var(--ease) both; }
+.toast { position: fixed; top: 44px; right: 16px; z-index: 20; display: flex; align-items: center; gap: 10px; max-width: min(480px, calc(100vw - 32px)); padding: 10px 8px 10px 16px; border-radius: var(--r-md); background: var(--s3); color: var(--ink); box-shadow: inset 3px 0 0 var(--danger), var(--lift-shadow); animation: arrive var(--dur-arrive) var(--ease) both; }
 .toast button { flex: none; width: 24px; height: 24px; border-radius: var(--r-xs); color: var(--ink-2); }
 .toast button:hover { background: var(--s2); color: var(--ink); }
 
-/* Sidebar */
-.body, .settings { display: grid; grid-template-columns: 248px 1fr; min-height: 0; }
-.side { display: grid; grid-template-rows: auto 1fr auto; gap: 14px; padding: 8px 12px 12px; min-height: 0; min-width: 0; }
+/* Sidebar and the shell around the stage. From 1,100 px an open running column sits beside the stage (.docked); from 900 px
+   the sidebar does; below that the sidebar is behind a button. ui/layout.ts holds the same two numbers. */
+.shell { position: relative; display: grid; grid-template-columns: 248px minmax(0, 1fr); min-height: 0; }
+.shell.docked { grid-template-columns: 248px minmax(500px, 1fr) 352px; }
+.settings { display: grid; grid-template-columns: 248px 1fr; min-height: 0; }
+.side { display: flex; flex-direction: column; gap: 14px; padding: 8px 12px 12px; min-height: 0; min-width: 0; }
 .s-nav { padding: 8px 12px; align-content: start; }
-.nav, .list { display: grid; grid-template-columns: minmax(0, 1fr); gap: 2px; align-content: start; min-height: 0; overflow-y: auto; overflow-x: hidden; }
+.nav { display: grid; grid-template-columns: minmax(0, 1fr); gap: 2px; align-content: start; min-height: 0; overflow-y: auto; overflow-x: hidden; }
 .item { position: relative; display: flex; align-items: center; gap: 9px; width: 100%; height: calc(28px + 2px * var(--dens)); padding: 0 10px; border-radius: var(--r-xs); color: var(--ink-2); font-size: calc(var(--fs-ui) - 0.5px); text-align: left; white-space: nowrap; transition: background var(--dur-fast), color var(--dur-fast); }
 .nav .item { height: calc(34px + 2px * var(--dens)); border-radius: var(--r-sm); }
 .s-nav .item { font-family: var(--nav-family); font-size: var(--nav-size); font-weight: var(--nav-weight); }
-/* The ring is a shadow and .nav clips what lies outside it, so the card keeps one pixel of room around it. */
-.nav .item.new { width: calc(100% - 2px); height: 38px; margin: 1px; padding: 0 12px; background: var(--card-bg); box-shadow: var(--card-shadow); color: var(--ink); font-weight: 500; }
 .item span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
 .item:hover { background: var(--s2); color: var(--ink); }
 .item[aria-current='true'] { background: var(--s3); color: var(--ink); }
 .nav .item[aria-current='true']::before { content: ''; position: absolute; left: -12px; top: 9px; bottom: 9px; width: 2px; background: var(--accent); }
-.item .x { flex: none; visibility: hidden; color: var(--ink-3); padding: 0 4px; }
-.item .x.confirm { color: var(--ink); font-weight: 600; }
-.item:hover .x, .item:focus-within .x { visibility: visible; }
-.item kbd { margin-left: auto; font: 500 calc(var(--fs-ui) - 3px) var(--font-data); color: var(--ink-3); border: 1px solid var(--line-2); border-radius: 5px; padding: 1px 5px; }
-.list small { padding: 10px 10px 3px; font-size: calc(var(--fs-ui) - 2px); color: var(--ink-3); }
-.foot { font-size: calc(var(--fs-ui) - 2px); color: var(--ink-3); padding: 8px 10px 0; border-top: 1px solid var(--line); }
+.newchat { flex: none; display: flex; align-items: center; gap: 8px; height: 38px; margin: 1px; padding: 0 10px; font-size: calc(var(--fs-ui) - 0.5px); border-radius: var(--r-sm); background: var(--card-bg); box-shadow: var(--card-shadow); color: var(--ink); font-weight: 500; text-align: left; }
+.newchat span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
+.newchat kbd, .item kbd { margin-left: auto; font: 500 calc(var(--fs-ui) - 3px) var(--font-data); color: var(--ink-3); border: 1px solid var(--line-2); border-radius: 5px; padding: 1px 4px; white-space: nowrap; }
+.areas { flex: none; display: grid; gap: 2px; }
+.area { position: relative; display: flex; align-items: center; gap: 10px; width: 100%; height: calc(34px + 2px * var(--dens)); padding: 0 10px; border-radius: var(--r-sm); color: var(--ink-2); font-family: var(--nav-family); font-size: var(--nav-size); font-weight: var(--nav-weight); text-align: left; transition: background var(--dur-fast), color var(--dur-fast); }
+.area:hover { background: var(--s2); color: var(--ink); }
+.area[aria-pressed='true'] { background: var(--s3); color: var(--ink); }
+.area[aria-pressed='true']::before { content: ''; position: absolute; left: -12px; top: 9px; bottom: 9px; width: 2px; background: var(--accent); }
+.area .count { margin-left: auto; font: 400 calc(var(--fs-ui) - 2px) var(--font-data); color: var(--ink-3); }
+.num { font-family: var(--font-data); font-variant-numeric: tabular-nums; }
+.side-scroll { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; gap: 16px; scrollbar-width: thin; scrollbar-color: var(--line-3) transparent; }
+.group-h { display: flex; align-items: center; min-height: 24px; padding: 0 2px 4px 10px; font-size: calc(var(--fs-ui) - 2px); color: var(--ink-3); }
+.plus { flex: none; width: 24px; height: 24px; display: grid; place-items: center; margin-left: auto; border-radius: var(--r-xs); color: var(--ink-3); }
+.plus:hover { background: var(--s3); color: var(--ink); }
+.list { display: grid; gap: 16px; align-content: start; }
+.prow { position: relative; display: flex; align-items: flex-start; }
+.prow .item { height: auto; min-height: calc(28px + 2px * var(--dens)); padding-block: 5px; padding-right: 30px; flex-wrap: wrap; row-gap: 1px; white-space: normal; }
+.prow .item .t { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
+.prow .plus { position: absolute; right: 3px; top: 3px; visibility: hidden; }
+.prow:hover .plus, .prow:focus-within .plus { visibility: visible; }
+.item .amber-tag { flex: 1 0 100%; overflow: visible; white-space: normal; }
+.item .run-dot { flex: none; }
+.run-dot { width: 7px; height: 7px; border-radius: 1px; background: var(--accent); animation: var(--live-anim); }
+.proj-chats { display: grid; gap: 1px; padding-left: 10px; }
+.crow { position: relative; }
+.crow .item { padding-right: 30px; }
+.crow .x { position: absolute; right: 2px; top: 0; bottom: 0; display: grid; place-items: center; padding: 0 6px; color: var(--ink-3); border-radius: var(--r-xs); visibility: hidden; }
+.crow .x.confirm { color: var(--ink); font-weight: 600; }
+.crow:hover .x, .crow:focus-within .x { visibility: visible; }
+.side-empty { display: grid; gap: 6px; justify-items: start; padding: 4px 10px 0; color: var(--ink-3); font-size: calc(var(--fs-ui) - 1px); }
+.side-foot { flex: none; border-top: 1px solid var(--line); padding-top: 8px; display: grid; gap: 4px; }
+.usage { display: grid; gap: 1px; }
+.vendor { display: flex; align-items: center; gap: 8px; width: 100%; padding: 4px 10px; border-radius: var(--r-xs); color: var(--ink-2); font-size: calc(var(--fs-ui) - 1px); text-align: left; }
+.vendor:hover { background: var(--s2); color: var(--ink); }
+.vendor span:first-of-type + span { flex: 1; }
+.vendor .share { margin-left: auto; color: var(--ink-3); font-size: calc(var(--fs-ui) - 2px); }
+.usage .note { padding: 4px 10px; }
+.version { padding: 0 10px; font-size: calc(var(--fs-ui) - 3px); color: var(--ink-3); }
+.scrim { display: none; }
 
-/* The stage */
+/* The stage: a top bar over the work area. The work area holds the thread, the composer and, when it overlays the
+   stage, the running column. */
 .main, .s-main, .setup-container { margin: 0 var(--stage-inset) var(--stage-inset) 0; border-radius: var(--r-stage); background: var(--bg); box-shadow: var(--stage-shadow); min-height: 0; min-width: 0; }
-.main { position: relative; overflow: hidden; display: grid; grid-template-rows: 1fr auto; }
-.thread { position: relative; z-index: 1; overflow-y: auto; overflow-x: hidden; padding: calc(22px * var(--dens)) 24px 8px; }
-.col { width: min(100%, 660px); margin: 0 auto; display: grid; gap: calc(18px * var(--dens)); }
-.thread.welcome { padding-left: clamp(24px, 6vw, 76px); }
-.welcome .col { margin: 0; width: min(100%, 640px); }
+.main { position: relative; overflow: hidden; display: grid; grid-template-rows: 52px minmax(0, 1fr); }
+.shell.docked > .main { margin-right: 0; border-radius: var(--r-stage) 0 0 var(--r-stage); }
+.topbar { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 0 16px 0 24px; border-bottom: 1px solid var(--line); position: relative; z-index: 3; }
+.topbar.bare { border-bottom-color: transparent; }
+.topbar .menu { flex: none; margin-left: -8px; }
+.crumb { display: flex; align-items: center; gap: 7px; min-width: 0; color: var(--ink-3); font-size: calc(var(--fs-ui) - 0.5px); white-space: nowrap; overflow: hidden; }
+.crumb b { min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--ink); font-weight: 500; }
+.crumb .sep { flex: none; }
+.topbar .right { margin-left: auto; display: flex; align-items: center; gap: 8px; flex: none; }
+.topbar .chip { height: 30px; flex: none; }
+.work { position: relative; display: grid; grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(0, 1fr) auto; min-height: 0; min-width: 0; }
+.thread { position: relative; z-index: 1; grid-column: 1; grid-row: 1; overflow-y: auto; overflow-x: hidden; padding: calc(22px * var(--dens)) 24px 8px; }
+.dock { position: relative; z-index: 1; grid-column: 1; grid-row: 2; }
+.col { width: min(100%, 660px); margin: 0 auto; display: grid; grid-template-columns: minmax(0, 1fr); gap: calc(18px * var(--dens)); }
 .empty { display: grid; justify-items: start; gap: 16px; padding-top: 14vh; }
 .empty h1 { font-size: min(var(--fs-display), 9vw); max-width: 8.4em; overflow-wrap: anywhere; }
 .empty p { margin: 0; color: var(--ink-2); font-size: var(--fs-lead); max-width: 44ch; line-height: 1.5; }
@@ -79,6 +117,94 @@ input, textarea { caret-color: var(--accent); }
 .peek { width: min(100%, 44ch); height: 45px; overflow: hidden; color: var(--ink-3); border-bottom: 1px solid var(--line-2); }
 .peek svg { display: block; margin-left: -2px; } /* the drawing has 2 px of room on its left; his edge lines up with the heading */
 
+/* The new conversation: the greeting and the composer on the left; the conversations to pick up on the right when the
+   stage is wide enough, below the suggestions when it is not. */
+.thread.welcome { display: grid; align-content: safe center; padding: 0 clamp(24px, 4vw, 60px) 40px clamp(24px, 6vw, 76px); container-type: inline-size; }
+.welcome-grid { display: grid; grid-template-columns: minmax(0, 640px); gap: 40px 64px; }
+.w-main, .resume { position: relative; z-index: 2; min-width: 0; }
+.hello { font-size: min(var(--fs-display), 9vw); max-width: 8.4em; overflow-wrap: anywhere; padding-bottom: 4px; }
+.lead { margin: 16px 0 0; color: var(--ink-2); font-size: var(--fs-lead); max-width: 44ch; line-height: 1.5; }
+.welcome-grid .composer { width: 100%; margin: 32px 0 0; }
+.try { margin-top: 26px; }
+.try h2, .resume h2 { margin: 0; font: 600 calc(var(--fs-ui) - 1px) var(--font-ui); color: var(--ink-2); }
+.try h2 { margin-bottom: 4px; }
+.try button { display: flex; align-items: center; gap: 12px; width: 100%; padding: calc(10px * var(--dens)) 0; border-top: 1px solid var(--line); font-size: calc(var(--fs-ui) + 0.5px); color: var(--ink); text-align: left; transition: padding var(--dur-ui) var(--ease), color var(--dur-fast); }
+.try button:last-child { border-bottom: 1px solid var(--line); }
+.try button:hover { padding-left: 6px; color: var(--accent-text); }
+.try button .i { margin-left: auto; color: var(--ink-3); }
+.resume h2 { padding-bottom: 10px; border-bottom: 1px solid var(--line); }
+.rrow { display: grid; gap: 4px; width: 100%; padding: calc(12px * var(--dens)) 0; border-bottom: 1px solid var(--line); text-align: left; transition: padding var(--dur-ui) var(--ease); }
+.rrow:hover { padding-left: 6px; }
+.rrow .t { font-weight: 500; font-size: calc(var(--fs-ui) + 0.5px); overflow-wrap: anywhere; }
+.rrow .s { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; font-size: calc(var(--fs-ui) - 1px); color: var(--ink-2); }
+.rrow.ask .t { color: var(--amber-ink); }
+@container (min-width: 1000px) {
+  .welcome-grid { grid-template-columns: 640px minmax(0, 1fr); }
+  .resume { align-self: end; padding-bottom: 40px; }
+}
+
+/* The meaning marks. Colour, shape and word go together: a square for Claude, a diamond for Codex, a plain dot while Auto
+   is still choosing. Amber is always a speech bubble with the words "needs you". */
+.mk { display: inline-block; flex: none; width: 8px; height: 8px; border-radius: 50%; background: var(--ink-3); }
+.mk.claude { background: var(--claude); border-radius: 1px; }
+.mk.codex { background: var(--codex); border-radius: 1px; transform: rotate(45deg) scale(0.86); }
+.vm { display: inline-flex; align-items: center; gap: 7px; font-weight: 500; white-space: nowrap; }
+.vm.claude { color: var(--claude-ink); }
+.vm.codex { color: var(--codex-ink); }
+.vm .verb { color: var(--ink-2); font-weight: 400; }
+.amber-tag { display: inline-flex; align-items: center; gap: 5px; color: var(--amber-ink); font-size: calc(var(--fs-ui) - 1.5px); font-weight: 500; white-space: nowrap; }
+svg.i { width: 16px; height: 16px; flex: none; stroke: currentColor; fill: none; stroke-width: 1.75; stroke-linecap: round; stroke-linejoin: round; }
+svg.i.sm { width: 14px; height: 14px; }
+.iconbtn { width: 32px; height: 32px; display: grid; place-items: center; border-radius: var(--r-sm); color: var(--ink-3); }
+.iconbtn:hover { background: var(--s3); color: var(--ink); }
+
+/* The run button: how much runs, and in amber with the words when something needs the user. */
+.run-btn { display: inline-flex; align-items: center; height: 32px; border-radius: var(--r-pill); background: var(--s1); box-shadow: inset 0 0 0 1px var(--line-2); font-size: calc(var(--fs-ui) - 1px); font-weight: 500; color: var(--ink); white-space: nowrap; }
+.run-btn .seg { display: inline-flex; align-items: center; gap: 8px; padding: 0 12px; height: 100%; }
+.run-btn .seg + .seg { border-left: 1px solid var(--line-2); }
+.live { position: relative; width: 7px; height: 7px; border-radius: 1px; background: var(--accent); }
+.live::after { content: ''; position: absolute; inset: -4px; border: 1.5px solid var(--accent); border-radius: 2px; opacity: 0; animation: var(--live-anim); }
+@keyframes ping { 0% { transform: scale(0.4); opacity: 0.8; } 100% { transform: scale(1.5); opacity: 0; } }
+@keyframes pulse { 50% { opacity: 0.35; } }
+[data-base='oficina'] .live, [data-base='bancada'] .live { animation: var(--live-anim); }
+[data-base='oficina'] .live::after, [data-base='bancada'] .live::after { display: none; }
+.run-btn.need { box-shadow: inset 0 0 0 1px var(--amber-line), var(--amber-light); }
+.run-btn.need .ask { color: var(--amber-ink); background: var(--amber-soft); border-radius: 0 var(--r-pill) var(--r-pill) 0; }
+.run-btn.need .seg + .seg { border-left-color: var(--amber-line); }
+
+/* The running column: beside the stage from 1,100 px, over it (above the composer) below that. */
+.runcol { display: flex; flex-direction: column; min-width: 0; min-height: 0; background: var(--s1); }
+.shell > .runcol { margin: 0 var(--stage-inset) var(--stage-inset) 0; border-radius: 0 var(--r-stage) var(--r-stage) 0; box-shadow: var(--stage-shadow); border-left: 1px solid var(--line); overflow: hidden; }
+.work > .runcol { grid-column: 1; grid-row: 1; justify-self: end; z-index: 6; width: min(352px, 100%); border-left: 1px solid var(--line-2); box-shadow: -18px 0 40px -22px rgba(0, 0, 0, 0.7); }
+.rc-head { display: flex; align-items: center; gap: 8px; padding: 14px 12px 10px 18px; }
+.rc-head h2 { font-size: var(--fs-lead); }
+.rc-head .n { color: var(--ink-3); font-size: calc(var(--fs-ui) - 1px); }
+.rc-head .iconbtn { order: 3; margin-left: auto; }
+.rc-body { flex: 1; min-height: 0; overflow-y: auto; overflow-x: hidden; padding: 0 12px 12px; display: grid; align-content: start; gap: 16px; scrollbar-width: thin; scrollbar-color: var(--line-3) transparent; }
+.proj-g h3 { display: flex; align-items: center; gap: 8px; margin: 0; padding: 0 6px 8px; font: 600 calc(var(--fs-ui) - 1px) var(--font-ui); color: var(--ink-2); }
+.proj-g h3 .t { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
+.proj-g h3 .amber-tag { margin-left: auto; }
+.ritems { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }
+.ri { display: grid; gap: 6px; padding: calc(10px * var(--dens)) 12px; border-radius: var(--r-md); background: var(--card-bg); box-shadow: var(--card-shadow); }
+.ri .rhead { display: flex; align-items: center; gap: 8px; font-size: calc(var(--fs-ui) - 1px); }
+.ri .time { margin-left: auto; color: var(--ink-3); font-size: calc(var(--fs-ui) - 2px); }
+.ri .name { justify-self: start; max-width: 100%; text-align: left; font-weight: 500; overflow-wrap: anywhere; }
+.ri .name:hover { text-decoration: underline; text-decoration-color: var(--line-3); }
+.ri.need { background: linear-gradient(180deg, var(--amber-soft), transparent 120%), var(--card-bg); box-shadow: inset 0 0 0 1px var(--amber-line), var(--amber-light); animation: arrive var(--dur-arrive) var(--ease) both; }
+.ri .ask { padding: 0; background: none; box-shadow: none; animation: none; }
+.ask-label { display: inline-flex; align-items: center; gap: 6px; color: var(--amber-ink); font-weight: 600; font-size: calc(var(--fs-ui) - 1.5px); }
+.rc-empty { display: grid; justify-items: start; gap: 12px; padding: 12px 6px; color: var(--ink-2); }
+.rc-empty p { margin: 0; }
+.rc-foot { padding: 11px 18px 14px; border-top: 1px solid var(--line); font-size: calc(var(--fs-ui) - 1.5px); color: var(--ink-3); }
+
+@media (max-width: 899px) {
+  .shell { grid-template-columns: minmax(0, 1fr); }
+  .main { margin-left: var(--stage-inset); }
+  .side { display: none; }
+  .side.shown { display: flex; position: absolute; left: 0; top: 0; bottom: 0; z-index: 30; width: 248px; background: var(--frame); box-shadow: 18px 0 40px -20px rgba(0, 0, 0, 0.7); }
+  .scrim { display: block; position: absolute; inset: 0; z-index: 25; background: rgba(0, 0, 0, 0.5); cursor: default; }
+}
+
 /* Messages */
 .you { position: relative; justify-self: end; max-width: min(80%, 500px); padding: 11px 15px; background: var(--s2); border-radius: var(--r-lg) var(--r-lg) var(--r-xs) var(--r-lg); box-shadow: var(--card-shadow); font-size: var(--fs-read); white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.5; }
 .you img { display: block; max-width: 220px; max-height: 160px; border-radius: var(--r-sm); margin-bottom: 8px; }
@@ -86,7 +212,7 @@ input, textarea { caret-color: var(--accent); }
 .you:hover .edit, .you .edit:focus-visible { visibility: visible; }
 .you.editing { width: 80%; display: grid; gap: 8px; }
 .you.editing textarea { resize: vertical; border: 0; outline: none; background: none; line-height: 1.5; }
-.ai { display: grid; gap: 10px; font-size: var(--fs-read); line-height: 1.6; min-width: 0; }
+.ai { display: grid; grid-template-columns: minmax(0, 1fr); gap: 10px; font-size: var(--fs-read); line-height: 1.6; min-width: 0; }
 .model { display: flex; gap: 8px; align-items: center; font-size: calc(var(--fs-ui) - 1px); color: var(--ink-3); }
 .model b { color: var(--ink-2); font-weight: 600; }
 .model em { font-style: normal; }
@@ -252,13 +378,13 @@ html[data-motion='full'] .amb-glow .beam { animation: glow 7s var(--ease-io) inf
 @container (max-width: 1140px) { .amb-flow { display: none; } }
 
 /* Motion levels. Reduced keeps state changes and drops loops and travel; none drops everything. */
-:is([data-motion='reduced'], [data-motion='none']) :is(.cursor, .loom-row, .loom-shuttle, .flow-trace, .amb-glow .beam) { animation: none !important; }
+:is([data-motion='reduced'], [data-motion='none']) :is(.cursor, .live, .live::after, .run-dot, .loom-row, .loom-shuttle, .flow-trace, .amb-glow .beam) { animation: none !important; }
 :is([data-motion='reduced'], [data-motion='none']) :is(.loom-shuttle, .flow-trace) { opacity: 0; }
 [data-motion='reduced'] { --arrive-y: 0px; }
 /* Reduced keeps only short fades: every duration is capped at 120 ms and the switch thumb stops travelling. */
 [data-motion='reduced'] * { --dur-fast: 120ms; --dur-ui: 120ms; --dur-move: 120ms; --dur-arrive: 120ms; }
 :is([data-motion='reduced'], [data-motion='none']) .sw2::after { transition: background var(--dur-fast); }
-[data-motion='reduced'] :is(.ask, .toast) { animation: fade 120ms ease-out both; }
+[data-motion='reduced'] :is(.ask, .toast, .ri.need) { animation: fade 120ms ease-out both; }
 @keyframes fade { from { opacity: 0; } }
 :is([data-motion='reduced'], [data-motion='none']) .btn:active { transform: none; }
 [data-motion='none'], [data-motion='none'] * { --dur-fast: 0ms; --dur-ui: 0ms; --dur-move: 0ms; --dur-arrive: 0ms; }
@@ -269,8 +395,8 @@ html.hidden-win .amb * { animation-play-state: paused !important; }
   :root { --arrive-y: 0px; }
   * { --dur-fast: 120ms; --dur-ui: 120ms; --dur-move: 120ms; --dur-arrive: 120ms; }
   .sw2::after { transition: background var(--dur-fast); }
-  .cursor, .loom-row, .loom-shuttle, .flow-trace, .amb-glow .beam { animation: none !important; }
+  .cursor, .live, .live::after, .run-dot, .loom-row, .loom-shuttle, .flow-trace, .amb-glow .beam { animation: none !important; }
   .loom-shuttle, .flow-trace { opacity: 0; }
-  .ask, .toast { animation: fade 120ms ease-out both; }
+  .ask, .toast, .ri.need { animation: fade 120ms ease-out both; }
   .btn:active { transform: none; }
 }
```

- [ ] **Step 3: Check**

```bash
npm run typecheck
npm run lint
npm run build
```

Expected: all three exit 0.

- [ ] **Step 4: See the old scenarios fail where this commit moves things**

```bash
cargo t --suite screen --filter "opens on an empty"
```

Expected: FAIL (the old scenario looks for `.foot`, which is now `.version`). Task 7 fixes it, with the others.

Do not commit.

---

### Task 7: The scenarios  (model: `sonnet`)

**Files:**
- Create: `testkit/src/screen/shell.rs`
- Modify: `testkit/src/screen/app.rs`, `testkit/src/screen/mod.rs`

**Interfaces:**
- Consumes: the screen of Task 6, and the fixtures `claude/chat.jsonl` and `claude/tools.jsonl`.
- Produces: ten scenarios in `shell.rs`, and the old ones moved to the new selectors. Each scenario gets its own sandbox.

What changes in `app.rs`: `opens_empty` looks for `.version` (the footer's `v0.0.2`) instead of `.foot`; `deletes_after_asking` uses `.list .crow .item` and `.list .crow .x`; `shows_the_brand` uses `.prow .item` to click the project; the check of `S1` measures the greeting (`.hello`, `.lead`), checks that the heading is at most 9vw wide in its font size, clicks a suggestion at 1,100 px and checks that it fills the composer and sends nothing, and decides that the loom lies over words only within half of its visible reach (it fades out from its corner).

- [ ] **Step 1: Apply the patches and write the new file**

Patch `testkit/src/screen/app.rs`:

```diff
diff --git a/testkit/src/screen/app.rs b/testkit/src/screen/app.rs
index d118008..bee41ff 100644
--- a/testkit/src/screen/app.rs
+++ b/testkit/src/screen/app.rs
@@ -32,12 +32,22 @@ const S1: &str = r#"(() => {
   const hit = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
   // The extent of the words themselves, not of the block that holds them.
   const ink = (e) => { const range = document.createRange(); range.selectNodeContents(e); return range.getBoundingClientRect(); };
-  const text = [...document.querySelectorAll('.empty h1, .empty p')].map(ink);
-  const collide = ['.amb-loom', '.amb-flow'].filter((s) => drawn(s) && text.some((r) => hit(rect(document.querySelector(s)), r)));
-  const past = [...document.querySelectorAll('.top, .top *, .side, .side *, .empty, .empty *, .composer, .composer *')]
+  const text = [...document.querySelectorAll('.hello, .lead, .empty h1, .empty p')].map(ink);
+  // The loom fades out from its bottom right corner (its mask is clear beyond about two thirds of the way to the far corner),
+  // so words lie over it only when they come within half of that way; the flow line is drawn whole.
+  const near = (r, x, y) => Math.hypot(Math.max(r.left - x, 0, x - r.right), Math.max(r.top - y, 0, y - r.bottom));
+  const over = (s) => {
+    const box = rect(document.querySelector(s));
+    if (s === '.amb-flow') return text.some((r) => hit(box, r));
+    return text.some((r) => near(r, box.right, box.bottom) < 0.5 * Math.hypot(box.width, box.height));
+  };
+  const collide = ['.amb-loom', '.amb-flow'].filter((s) => drawn(s) && over(s));
+  const past = [...document.querySelectorAll('.top, .top *, .topbar, .topbar *, .side, .side *, .welcome-grid, .welcome-grid *, .empty, .empty *, .composer, .composer *')]
     .filter((e) => e.getBoundingClientRect().right > innerWidth + 0.5)
     .map((e) => String(e.className || e.tagName));
-  return { page: document.documentElement.scrollWidth <= innerWidth, past, collide, beam: drawn('.amb-beam'), canvas: !!document.querySelector('.amb-beam canvas'), loom: drawn('.amb-loom'), flow: drawn('.amb-flow') };
+  const hello = document.querySelector('.hello');
+  const heading = !hello || parseFloat(getComputedStyle(hello).fontSize) <= innerWidth * 0.09 + 0.5;
+  return { page: document.documentElement.scrollWidth <= innerWidth, past, collide, heading, beam: drawn('.amb-beam'), canvas: !!document.querySelector('.amb-beam canvas'), loom: drawn('.amb-loom'), flow: drawn('.amb-flow') };
 })()"#;
 
 /// The families the six looks draw with between them.
@@ -78,8 +88,8 @@ fn opens_empty(ctx: &mut Ctx) -> Result {
     let page = &mut app.page;
     let heading = page.named("heading", "empty.chatTitle")?;
     page.wait_visible(&heading)?;
-    let foot = page.inner_text(&css(".foot"))?;
-    ensure(super::has_version_after(&foot, "Claude · v"), || format!("the footer should name the CLI version, it says {foot:?}"))?;
+    let foot = page.inner_text(&css(".version"))?;
+    ensure(super::has_version_after(&foot, "v"), || format!("the footer should name the version, it says {foot:?}"))?;
     let scrolls = page.eval("document.documentElement.scrollHeight > innerHeight")?;
     same(scrolls.as_bool(), Some(false), "the window should not scroll as a whole")?;
     // No system title bar: the top bar carries the window buttons and drags the window.
@@ -139,7 +149,7 @@ fn shows_the_brand(ctx: &mut Ctx) -> Result {
     looks(page, "no project open")?;
     ctx.shot(&mut app, "peek-no-project")?;
     let page = &mut app.page;
-    let demo = css(".nav .item").has_text("Demo");
+    let demo = css(".prow .item").has_text("Demo");
     page.click(&demo)?;
     let name = role("heading", "Demo");
     page.wait_visible(&name)?;
@@ -191,8 +201,8 @@ fn deletes_after_asking(ctx: &mut Ctx) -> Result {
     ask(page, QUESTION)?;
     let actions = page.named("article", "answer.label")?.css(".actions");
     page.wait_visible(&actions)?;
-    page.hover(&css(".list .item"))?;
-    let x = css(".list .item .x");
+    page.hover(&css(".list .crow .item"))?;
+    let x = css(".list .crow .x");
     page.click(&x)?;
     let confirm = page.text_of("side.confirmDelete")?;
     same(page.inner_text(&x)?, confirm, "the first click asks")?;
@@ -343,6 +353,17 @@ fn fits_every_look(ctx: &mut Ctx) -> Result {
             ensure(seen["collide"].as_array().is_some_and(|over| over.is_empty()), || {
                 format!("{at}: these lie over the greeting: {}", seen["collide"])
             })?;
+            ensure(seen["heading"] == true, || format!("{at}: the heading is wider than 9vw in its font size"))?;
+            if width == 1100 {
+                // A suggestion fills the composer and sends nothing.
+                app.page.click(&css(".try button:nth-of-type(1)"))?;
+                same(
+                    app.page.eval("document.querySelector('.composer textarea').value.length > 0")?.as_bool(),
+                    Some(true),
+                    &format!("{at}: the suggestion in the composer"),
+                )?;
+                same(app.page.count(&css("article"))?, 0, &format!("{at}: nothing was sent"))?;
+            }
             if width >= 1280 {
                 let name = if width == 1440 { format!("s1-{id}") } else { format!("s1-{id}-{width}") };
                 ctx.shot(&mut app, &name)?;
```

Patch `testkit/src/screen/mod.rs`:

```diff
diff --git a/testkit/src/screen/mod.rs b/testkit/src/screen/mod.rs
index bd01ca7..9c7450d 100644
--- a/testkit/src/screen/mod.rs
+++ b/testkit/src/screen/mod.rs
@@ -6,6 +6,7 @@ pub mod app;
 pub mod containment;
 pub mod permissions;
 pub mod setup;
+pub mod shell;
 
 use crate::app::{Ctx, Paths};
 use crate::page::Page;
@@ -24,7 +25,7 @@ pub struct Scenario {
 }
 
 pub fn all() -> Vec<Scenario> {
-    let all = app::scenarios().into_iter().chain(permissions::scenarios()).chain(setup::scenarios());
+    let all = app::scenarios().into_iter().chain(permissions::scenarios()).chain(shell::scenarios()).chain(setup::scenarios());
     #[cfg(windows)]
     let all = all.chain(containment::scenarios());
     all.collect()
```

Create `testkit/src/screen/shell.rs`:

```rust
//! The shell of the screen: the layout at each width, the running column and when it opens, the new conversation with
//! its greeting, and the quota in the sidebar's footer.

use super::{Result, Scenario, ask, claude_chat, ensure, eventually, no_cli, pause, same, start_settings};
use crate::app::Ctx;
use crate::page::{Page, css};
use crate::sandbox::{CliSlot, Launch};
use serde_json::{Value, json};
use std::time::Duration;

const QUESTION: &str = "In one sentence: why is the sky blue?";

/// The request of the recording `claude/tools.jsonl`, which allows the first Write and denies the second.
const TWO_WRITES: &str = "Create a.txt containing 'one'. Then create b.txt containing 'two'. Use the Write tool for each, one at a time.";

pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario { name: "the layout follows the width, from 720 to 1440, and the running column fits each one", run: follows_the_width },
        Scenario { name: "a larger text size still fits every width", run: fits_large_text },
        Scenario {
            name: "the running column opens by itself once for each question, and closing it leaves the button amber",
            run: opens_by_itself,
        },
        Scenario {
            name: "a question that waits when the screen loads shows the amber button at once and opens the column once",
            run: opens_at_load,
        },
        Scenario {
            name: "over the stage the column opens above the composer without taking focus, and the user's own opening is modal",
            run: overlays_the_stage,
        },
        Scenario { name: "a click on a running conversation opens it", run: opens_a_running_conversation },
        Scenario { name: "a click on a running row of a project opens the project's conversation", run: opens_a_project_conversation },
        Scenario {
            name: "the new conversation greets, suggests without sending, and offers the last conversations",
            run: greets_and_suggests,
        },
        Scenario { name: "with no AI connected the new conversation gives way to the empty state with a button", run: gives_way_to_no_ai },
        Scenario { name: "the quota of each vendor shows in the footer, and comes back after the screen reloads", run: shows_the_quota },
    ]
}

fn set_size(page: &mut Page, width: u32, height: u32) -> Result {
    let size = json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": false });
    page.cdp.call("Emulation.setDeviceMetricsOverride", size)?;
    // The screen follows `resize`, which comes a moment after the override.
    let narrow = width < 900;
    page.wait_for("the screen to follow the new width", &format!("(!!document.querySelector('.topbar .menu')) === {narrow}"))
}

/// Where the parts of the shell are: each one's box and whether it is drawn, and what sticks out past the right edge.
const SHELL: &str = r#"(() => {
  const box = (selector) => {
    const e = document.querySelector(selector);
    if (!e) return null;
    const b = e.getBoundingClientRect();
    const s = getComputedStyle(e);
    return { x: b.x, y: b.y, w: b.width, h: b.height, r: b.right, b: b.bottom, shown: s.display !== 'none' && s.visibility !== 'hidden' };
  };
  const past = [...document.querySelectorAll('header.top *, .shell *')]
    .filter((e) => { const s = getComputedStyle(e); return !e.closest('.ambient, .scrim') && s.display !== 'none' && s.position !== 'fixed' && e.getBoundingClientRect().width > 0 && e.getBoundingClientRect().right > innerWidth + 0.5; })
    .map((e) => String(e.className && e.className.baseVal !== undefined ? e.className.baseVal : e.className || e.tagName));
  return {
    page: document.documentElement.scrollWidth <= innerWidth,
    past,
    title: box('header.top'),
    side: box('.side'),
    topbar: box('.topbar'),
    run: box('.run-btn'),
    view: { w: innerWidth, h: innerHeight },
    main: box('.main'),
    col: box('.thread .col'),
    docked: box('.shell > .runcol'),
    over: box('.work > .runcol'),
    composer: box('.dock .composer') || box('.composer'),
    modal: (document.querySelector('.runcol') || {}).getAttribute ? document.querySelector('.runcol').getAttribute('aria-modal') : null,
    focusIn: !!document.activeElement && !!document.activeElement.closest('.runcol'),
  };
})()"#;

fn num(v: &Value, path: &[&str]) -> f64 {
    path.iter().fold(v, |v, key| &v[*key]).as_f64().unwrap_or(f64::NAN)
}

fn shown(v: &Value, part: &str) -> bool {
    v[part]["shown"].as_bool().unwrap_or(false)
}

/// Closes the column the way the width allows: the button while the column sits beside the stage, its own close button
/// while it is a modal dialog over the stage (the top bar is out of reach then).
fn close_column(page: &mut Page, width: u32) -> Result {
    if width >= 1100 {
        run_button(page)?;
    } else {
        let close = page.named("button", "run.close")?;
        page.click(&close)?;
    }
    page.wait_gone(&css(".runcol"))
}

fn run_button(page: &mut Page) -> Result {
    let button = css(".run-btn");
    page.click(&button)
}

/// The same checks at every width: nothing sticks out, the bars have their heights, the sidebar sits where the width says.
fn check_shell(at: &str, seen: &Value, width: u32) -> Result {
    ensure(seen["page"] == true, || format!("{at}: the page scrolls sideways"))?;
    ensure(seen["past"].as_array().is_some_and(Vec::is_empty), || format!("{at}: these stick out past the right edge: {} ", seen["past"]))?;
    same(num(seen, &["title", "h"]), 36.0, &format!("{at}: the title bar"))?;
    same(num(seen, &["topbar", "h"]), 52.0, &format!("{at}: the top bar"))?;
    if width >= 900 {
        same(num(seen, &["side", "w"]), 248.0, &format!("{at}: the sidebar"))?;
    } else {
        ensure(!shown(seen, "side"), || format!("{at}: the sidebar should be behind its button"))?;
    }
    // The composer and the run button are reachable: whole, inside the window.
    for part in ["composer", "run"] {
        let whole = seen[part].is_null()
            || (num(seen, &[part, "x"]) >= -0.5
                && num(seen, &[part, "r"]) <= num(seen, &["view", "w"]) + 0.5
                && num(seen, &[part, "b"]) <= num(seen, &["view", "h"]) + 0.5);
        ensure(whole, || format!("{at}: the {part} is cut off: {}", seen[part]))?;
    }
    ensure(num(seen, &["col", "w"]) <= 660.5 || seen["col"].is_null(), || format!("{at}: the reading column is {}", seen["col"]["w"]))
}

fn follows_the_width(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    for (width, height) in [(1440, 900), (1100, 700), (1099, 700), (900, 700), (899, 700), (720, 480)] {
        set_size(page, width, height)?;
        let at = format!("{width}x{height}");
        let closed = page.eval(SHELL)?;
        check_shell(&at, &closed, width)?;
        ensure(closed["docked"].is_null() && closed["over"].is_null(), || format!("{at}: the column starts closed"))?;
        // Opened by the user.
        run_button(page)?;
        page.wait_visible(&css(".runcol"))?;
        pause(0.45);
        let open = page.eval(SHELL)?;
        check_shell(&format!("{at}, column open"), &open, width)?;
        if width == 1440 {
            // Nothing runs: the column shows Curioso, 64 px, and his drawing does not move.
            let peek = page.eval(
                "(() => { const s = document.querySelector('.rc-empty svg'); const r = s.getBoundingClientRect(); return { w: r.width, h: r.height, still: getComputedStyle(s).animationName === 'none' && document.getAnimations().every((a) => !s.contains(a.effect.target)) }; })()",
            )?;
            same((peek["w"].as_f64(), peek["h"].as_f64()), (Some(64.0), Some(64.0)), "Curioso in the empty column")?;
            same(peek["still"].as_bool(), Some(true), "Curioso is still")?;
        }
        if width >= 1100 {
            // 352 px with the 8 px of frame that runs round the stage.
            same(num(&open, &["docked", "w"]) + 8.0, 352.0, &format!("{at}: the docked column"))?;
            ensure(num(&open, &["main", "w"]) >= 499.5, || format!("{at}: the stage is {} px", open["main"]["w"]))?;
            ensure(open["modal"].is_null(), || format!("{at}: a docked column is not a dialog"))?;
        } else {
            ensure(!open["over"].is_null(), || format!("{at}: the column should lie over the stage"))?;
            ensure(num(&open, &["over", "b"]) <= num(&open, &["composer", "y"]) + 0.5, || {
                format!("{at}: the column reaches the composer: {}", open["over"])
            })?;
            same(open["modal"].as_str(), Some("true"), &format!("{at}: opened by the user, the column is modal"))?;
            ensure(open["focusIn"] == true, || format!("{at}: focus should move into the column"))?;
        }
        page.screenshot(&ctx.paths.shots.join(format!("shell-{width}-column.png")))?;
        close_column(page, width)?;
        if width < 900 {
            // The sidebar is a modal dialog behind its button; Esc closes it and focus goes back to the button.
            let menu = page.named("button", "side.show")?;
            page.click(&menu)?;
            page.wait_visible(&css(".side.shown"))?;
            pause(0.3);
            page.screenshot(&ctx.paths.shots.join(format!("shell-{width}-sidebar.png")))?;
            same(page.attr(&css(".side.shown"), "aria-modal")?, Some("true".to_string()), &format!("{at}: the sidebar overlay is modal"))?;
            page.press("Escape")?;
            page.wait_gone(&css(".side.shown"))?;
            same(
                page.eval("document.activeElement === document.querySelector('.topbar .menu')")?.as_bool(),
                Some(true),
                "focus goes back to the menu button",
            )?;
        }
    }
    // The window narrows and widens while the column and the sidebar are open: each follows the width, nothing sticks out,
    // and the controls work again once the width allows.
    set_size(page, 1440, 900)?;
    run_button(page)?;
    page.wait_visible(&css(".shell > .runcol"))?;
    set_size(page, 1000, 700)?;
    page.wait_visible(&css(".work > .runcol"))?;
    check_shell("1440 narrowed to 1000, column open", &page.eval(SHELL)?, 1000)?;
    set_size(page, 1440, 900)?;
    page.wait_visible(&css(".shell > .runcol"))?;
    close_column(page, 1440)?;
    set_size(page, 720, 480)?;
    let menu = page.named("button", "side.show")?;
    page.click(&menu)?;
    page.wait_visible(&css(".side.shown"))?;
    set_size(page, 1000, 700)?;
    page.wait_gone(&css(".side.shown"))?;
    check_shell("720 widened to 1000, sidebar was open", &page.eval(SHELL)?, 1000)?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn fits_large_text(ctx: &mut Ctx) -> Result {
    ctx.write_settings(&json!({ "language": "en", "setup": "done", "appearance": { "base": "sala", "textSize": 18 } }))?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let heading = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&heading)?;
    same(page.eval("parseFloat(getComputedStyle(document.body).fontSize)")?.as_f64(), Some(18.0), "the text size")?;
    for (width, height) in [(1440, 900), (1100, 700), (1099, 700), (900, 700), (899, 700), (720, 480)] {
        set_size(page, width, height)?;
        let at = format!("{width}x{height} at 18 px");
        check_shell(&at, &page.eval(SHELL)?, width)?;
        run_button(page)?;
        page.wait_visible(&css(".runcol"))?;
        pause(0.45);
        let open = page.eval(SHELL)?;
        check_shell(&format!("{at}, column open"), &open, width)?;
        // The run button and the model chip stay whole in the top bar, and the column's own text fits its width.
        let clipped = page.eval(
            "[...document.querySelectorAll('.topbar .right *, .runcol *')].filter((e) => e.scrollWidth > e.clientWidth + 1 && getComputedStyle(e).overflowX === 'visible' && e.children.length === 0).map((e) => e.className || e.tagName)",
        )?;
        ensure(clipped.as_array().is_some_and(Vec::is_empty), || format!("{at}: text spills out of its box: {clipped}"))?;
        close_column(page, width)?;
    }
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

/// A launch that stops at a question: the recording asks to write two files and waits for the answers.
fn two_writes(ctx: &Ctx) -> Launch {
    Launch {
        claude: CliSlot::Fake,
        codex: CliSlot::Off,
        recording: Some(ctx.paths.fixtures.join("claude/tools.jsonl")),
        ..Launch::default()
    }
}

/// Sends the request and returns once the first question has been asked.
fn ask_two_writes(page: &mut Page) -> Result {
    // Auto would send the request to a router process first, which would replay this recording instead of the chat.
    let model = page.labelled("composer.model")?;
    page.select_option(&model, "Claude Haiku")?;
    ask(page, TWO_WRITES)?;
    page.wait_visible(&css("article .ask").has_text("a.txt"))
}

fn opens_by_itself(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    // Not at once: the user has just typed. It comes 1.5 s after the last key.
    let column = css(".runcol");
    page.wait_visible(&column)?;
    let seen = page.eval(SHELL)?;
    ensure(seen["modal"].is_null(), || "a column that opens by itself is not a dialog".to_string())?;
    ensure(seen["focusIn"] == false, || "a column that opens by itself does not take focus".to_string())?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    page.wait_visible(&css(".run-btn.need"))?;
    let words = page.text_with("run.needs", &json!({ "n": 1 }), None)?;
    let button = page.inner_text(&css(".run-btn"))?;
    ensure(button.contains(&words), || format!("the amber button should say {words:?}: {button:?}"))?;
    // A question with a long path stays inside the reading column and the stage, beside the docked column.
    let inside = page.eval("(() => { const stage = document.querySelector('.main').getBoundingClientRect().right; return [...document.querySelectorAll('article .ask, article .tools span')].every((e) => e.getBoundingClientRect().right <= stage + 0.5); })()")?;
    same(inside.as_bool(), Some(true), "the card and the tool line stay inside the stage")?;
    ctx.shot(&mut app, "running-column-asks")?;
    let page = &mut app.page;
    // Closing it leaves only the button amber, and the same question does not open it again.
    let close = page.named("button", "run.close")?;
    page.click(&close)?;
    page.wait_gone(&column)?;
    pause(2.0);
    ensure(!page.is_visible(&column)?, || "the column opened again for the same question".to_string())?;
    ensure(page.is_visible(&css(".run-btn.need"))?, || "the button should stay amber while a question waits".to_string())?;
    // The button brings it back, and the card there answers the question; the card in the thread goes with it.
    run_button(page)?;
    page.wait_visible(&column)?;
    let allow = page.text_of("ask.allow")?;
    let in_column = css(".runcol .ask").has_text("a.txt");
    page.click(&in_column.clone().role("button", &allow).exact())?;
    page.wait_gone(&css(".ask").has_text("a.txt"))?;
    // The next question is a new one: the column comes by itself again (it was open already, so it stays).
    page.wait_visible(&css(".runcol .ask").has_text("b.txt"))?;
    let deny = page.text_of("ask.deny")?;
    page.click(&css(".runcol .ask").has_text("b.txt").role("button", &deny).exact())?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    ensure(page.count(&css(".ask"))? == 0, || "no card should be left".to_string())?;
    // Answered, nothing waits any more: the button is no longer amber.
    page.wait_gone(&css(".run-btn.need"))
}

/// A question that waits when the screen loads (a window rebuilt after closing to the tray, seen from the engine): the
/// button is amber on first paint and the column opens once.
fn opens_at_load(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    page.eval("location.reload()")?;
    page.wait_visible(&css(".run-btn.need"))?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    // Closed by the user, it stays closed for the same question.
    let close = page.named("button", "run.close")?;
    page.click(&close)?;
    pause(2.0);
    ensure(!page.is_visible(&css(".runcol"))?, || "the column opened again for the same question".to_string())
}

fn overlays_the_stage(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    set_size(page, 1000, 700)?;
    ask_two_writes(page)?;
    let column = css(".work > .runcol");
    page.wait_visible(&column)?;
    pause(0.45);
    let seen = page.eval(SHELL)?;
    ensure(seen["modal"].is_null(), || "a column that opens by itself is not a dialog at any width".to_string())?;
    ensure(seen["focusIn"] == false, || "it does not take focus".to_string())?;
    ensure(num(&seen, &["over", "b"]) <= num(&seen, &["composer", "y"]) + 0.5, || format!("it lies over the composer: {}", seen["over"]))?;
    // Esc closes it only while focus is inside it: the composer has focus, so the column stays.
    let composer = page.labelled("composer.message")?;
    page.click(&composer)?;
    page.press("Escape")?;
    pause(0.3);
    ensure(page.is_visible(&column)?, || "Esc closed a column that did not have focus".to_string())?;
    ctx.shot(&mut app, "running-column-over")?;
    let page = &mut app.page;
    // Opened by the user it is a modal dialog: focus moves in, Esc closes it, and focus goes back to the button.
    run_button(page)?;
    page.wait_gone(&column)?;
    run_button(page)?;
    page.wait_visible(&column)?;
    let seen = page.eval(SHELL)?;
    same(seen["modal"].as_str(), Some("true"), "opened by the user")?;
    ensure(seen["focusIn"] == true, || "focus should be in the column".to_string())?;
    page.press("Escape")?;
    page.wait_gone(&column)?;
    same(
        page.eval("document.activeElement === document.querySelector('.run-btn')")?.as_bool(),
        Some(true),
        "focus goes back to the button",
    )?;
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    Ok(())
}

fn opens_a_running_conversation(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = two_writes(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    ask_two_writes(page)?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    // Leave the conversation for a new one; it keeps running, and the column still lists it.
    let new = page.named("button", "side.new")?;
    page.click(&new)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    page.wait_visible(&css(".runcol .ri .name"))?;
    page.click(&css(".runcol .ri .name"))?;
    // The conversation is back, with its question, and the crumb names it.
    page.wait_visible(&css("article .ask").has_text("a.txt"))?;
    page.wait_for(
        "the crumb to name the conversation",
        "document.querySelector('.crumb b')?.textContent.length > 0 && !document.querySelector('.crumb b').textContent.includes('New')",
    )?;
    let allow = page.text_of("ask.allow")?;
    page.click(&css("article .ask").has_text("a.txt").role("button", &allow).exact())?;
    page.wait_visible(&css("article .ask").has_text("b.txt"))
}

fn opens_a_project_conversation(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    // One project, so the screen can send into it.
    let folder = ctx.sb.data.join("demo");
    std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let projects = json!([{ "id": "demo", "name": "Demo", "path": folder.to_string_lossy() }]);
    std::fs::write(ctx.sb.data.join("projects.json"), projects.to_string()).map_err(|e| e.to_string())?;
    let launch = two_writes(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let demo = css(".prow .item").has_text("Demo");
    page.wait_visible(&demo)?;
    page.click(&demo)?;
    page.wait_visible(&css(".empty h1").has_text("Demo"))?;
    ask_two_writes(page)?;
    page.wait_visible(&css(".runcol .ask").has_text("a.txt"))?;
    // From Chat, a click on the row brings the project, the area and the conversation together.
    let chat = page.named("button", "area.chat")?.exact();
    page.click(&chat)?;
    let greeting = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&greeting)?;
    page.click(&css(".runcol .ri .name"))?;
    page.wait_visible(&css("article .ask").has_text("a.txt"))?;
    let projects = page.text_of("crumb.project")?;
    let crumb = page.inner_text(&css(".crumb"))?;
    ensure(crumb.starts_with(&projects), || format!("the crumb should start with {projects:?}: {crumb:?}"))?;
    let pressed = "document.querySelector('.areas .area[aria-pressed=\"true\"]')?.textContent.includes('Projects')";
    same(page.eval(pressed)?.as_bool(), Some(true), "the area")?;
    same(page.count(&css(".proj-chats .item[aria-current=true]"))?, 1, "the conversation marked in its project")?;
    let composer = page.labelled("composer.message")?;
    let placeholder = page.attr(&composer, "placeholder")?;
    same(placeholder, Some(page.text_of("composer.projectPlaceholder")?), "the composer points at the project")
}

fn greets_and_suggests(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = claude_chat(ctx);
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    same(page.inner_text(&title)?, page.text_of("empty.chatTitle")?, "the greeting says no name")?;
    // The heading is at most 9vw in its font size, at the widths the spec names.
    for (width, height) in [(1100, 700), (1440, 900), (720, 480)] {
        set_size(page, width, height)?;
        let found = page.eval("(() => { const h = document.querySelector('.hello'); return { size: parseFloat(getComputedStyle(h).fontSize), wide: innerWidth, past: h.getBoundingClientRect().right > innerWidth + 0.5, page: document.documentElement.scrollWidth <= innerWidth }; })()")?;
        ensure(found["size"].as_f64().unwrap_or(f64::MAX) <= width as f64 * 0.09 + 0.5, || {
            format!("{width}: the heading is {} px", found["size"])
        })?;
        ensure(found["past"] == false && found["page"] == true, || format!("{width}: the greeting sticks out: {found}"))?;
    }
    page.cdp.call("Emulation.clearDeviceMetricsOverride", json!({}))?;
    // Four suggestions; one fills the composer and sends nothing.
    same(page.count(&css(".try button"))?, 4, "suggestions")?;
    ensure(page.count(&css(".resume"))? == 0, || "there is nothing to pick up yet".to_string())?;
    let wanted = page.text_of("suggest.2")?;
    page.click(&css(".try button").has_text(&wanted))?;
    let composer = page.labelled("composer.message")?;
    let value = page.eval_on(&composer, "return { value: el.value, focused: document.activeElement === el };")?;
    same(value["value"].as_str(), Some(wanted.as_str()), "the composer holds the suggestion")?;
    same(value["focused"].as_bool(), Some(true), "the composer has focus")?;
    pause(0.4);
    let articles = page.named("article", "answer.label")?;
    same(page.count(&articles)?, 0, "the suggestion was sent")?;
    ctx.shot(&mut app, "greeting")?;
    // After a conversation exists, "pick up" lists it.
    let page = &mut app.page;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    let new = page.named("button", "side.new")?;
    page.click(&new)?;
    let title = page.named("heading", "empty.chatTitle")?;
    page.wait_visible(&title)?;
    let row = css(".resume .rrow").has_text(QUESTION);
    page.wait_visible(&row)?;
    ctx.shot(&mut app, "greeting-pick-up")?;
    let page = &mut app.page;
    page.click(&row)?;
    page.wait_visible(&actions)
}

fn gives_way_to_no_ai(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let mut app = ctx.launch(no_cli())?;
    let page = &mut app.page;
    let title = page.named("heading", "empty.noAiTitle")?;
    page.wait_visible(&title)?;
    same(page.count(&css(".hello"))?, 0, "a greeting with no AI")?;
    same(page.count(&css(".try"))?, 0, "suggestions with no AI")?;
    same(page.count(&css(".empty svg"))?, 1, "Curioso")?;
    let size = page.eval(
        "(() => { const r = document.querySelector('.empty svg').getBoundingClientRect(); return { w: r.width, h: r.height }; })()",
    )?;
    same((size["w"].as_f64(), size["h"].as_f64()), (Some(64.0), Some(64.0)), "Curioso's size")?;
    let connect = page.named("button", "empty.noAiButton")?;
    page.click(&connect)?;
    let setup = page.named("heading", "setup.title")?;
    page.wait_visible(&setup)
}

/// The recording `claude/chat.jsonl` with its reset times moved to the future: a reading past its reset says nothing.
fn chat_with_future_resets(ctx: &Ctx) -> std::result::Result<Launch, String> {
    let source = ctx.paths.fixtures.join("claude/chat.jsonl");
    let text = std::fs::read_to_string(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs();
    let text = text.replace("1791627000", &(now + 3 * 3600).to_string()).replace("1791878400", &(now + 3 * 86400).to_string());
    let copy = ctx.sb.data.join("chat-future.jsonl");
    std::fs::write(&copy, text).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok(Launch { claude: CliSlot::Fake, codex: CliSlot::Off, recording: Some(copy), ..Launch::default() })
}

fn shows_the_quota(ctx: &mut Ctx) -> Result {
    start_settings(ctx, "en")?;
    let launch = chat_with_future_resets(ctx)?;
    let mut app = ctx.launch(launch)?;
    let page = &mut app.page;
    let share = css(".usage .vendor").has_text("Claude").css(".share");
    let unknown = page.text_of("quota.unknown")?;
    page.wait_visible(&share)?;
    same(page.inner_text(&share)?, unknown, "before the CLI has reported")?;
    ask(page, QUESTION)?;
    let actions = page.named("article", "answer.label")?.css(".actions");
    page.wait_visible(&actions)?;
    // The tightest window of the vendor: 47 % of the 7-day one, not 7 % of the 5-hour one.
    page.wait_for("the quota to show", "document.querySelector('.usage .share')?.textContent === '47%'")?;
    ctx.shot(&mut app, "quota")?;
    let page = &mut app.page;
    // A screen that loads again asks the engine and shows the same reading.
    page.eval("location.reload()")?;
    eventually("the quota after a reload", Duration::from_secs(20), || {
        page.eval("document.querySelector('.usage .share')?.textContent === '47%'").is_ok_and(|v| v == true)
    })
}
```

- [ ] **Step 2: Lint and format**

```bash
cargo fmt --all --check
cargo clippy -p faber-testkit --all-targets -- -D warnings
```

Expected: both exit 0.

- [ ] **Step 3: Run each new scenario, then the old ones**

```bash
cargo t --suite screen --filter "layout follows"
cargo t --suite screen --filter "larger text"
cargo t --suite screen --filter "opens by itself"
cargo t --suite screen --filter "when the screen loads"
cargo t --suite screen --filter "over the stage"
cargo t --suite screen --filter "running conversation"
cargo t --suite screen --filter "running row of a project"
cargo t --suite screen --filter "greets"
cargo t --suite screen --filter "gives way"
cargo t --suite screen --filter "quota of each"
cargo t --suite screen
```

Expected: every line says `pass`, and the last run ends with `0 failed`. A scenario that fails leaves a screenshot in `target/faber-test/shots`; read it before changing anything. The pictures `running-column-asks.png`, `running-column-over.png`, `greeting.png`, `greeting-pick-up.png`, `quota.png`, `shell-720-sidebar.png`, `shell-720-column.png` and `shell-1100-column.png` are kept in that folder for the review in Task 8.

Do not commit.

---

### Task 8: Proof, review and the commit  (model: `haiku`)

**Files:**
- Modify: `ui/Thread.tsx` (line endings back to CRLF)

- [ ] **Step 1: Put the line endings of `ui/Thread.tsx` back**

```bash
python3 - <<'EOF'
p = 'ui/Thread.tsx'
data = open(p, 'rb').read().replace(b'\r\n', b'\n').replace(b'\n', b'\r\n')
open(p, 'wb').write(data)
EOF
git diff --stat -- ui/Thread.tsx
```

Expected: the stat shows a small change (about 3 insertions and 46 deletions), not the whole file. If it shows the whole file, the conversion went wrong: stop and report.

- [ ] **Step 2: Every proof command exits 0**

```bash
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p faber --features test-hooks -- -D warnings
cargo t --all
```

Expected: all seven exit 0; `cargo t --all` ends with `301 passed, 0 failed`.

- [ ] **Step 3: Look at the pictures**

Open the screenshots in `target/faber-test/shots` named in Task 7 and the `s1-*.png` ones, and check, in each of the six looks that are drawn there: the greeting, the composer and the four suggestions line up on the left; "Pick up where you left off" sits under them (or at the right from a 1,000 px wide stage); the run button reads "Nothing running", or "1 running" with an amber "1 needs you"; the amber card in the column carries the words "Needs you"; nothing is cut at the right edge; the sidebar footer shows the vendor's shape, name and share (or "—"). If something is wrong, do not patch around it: report it with the picture.

- [ ] **Step 4: Commit**

Only when every command above exited 0:

```bash
git add ui tests/ui testkit/src/screen
git status --short
```

Expected: the staged files are exactly the ones in the file structure table above (`ui/Ask.tsx`, `ui/Glyphs.tsx`, `ui/Running.tsx`, `ui/Sidebar.tsx`, `ui/Welcome.tsx`, `ui/layout.ts`, `ui/running.ts`, `ui/sidebar.ts`, `tests/ui/running.check.ts`, `testkit/src/screen/shell.rs`, and the modified `ui/App.tsx`, `ui/Composer.tsx`, `ui/Thread.tsx`, `ui/api.ts`, `ui/main.tsx`, `ui/models.ts`, `ui/styles.css`, `ui/locales/*.ts`, `tests/ui/index.ts`, `testkit/src/screen/app.rs`, `testkit/src/screen/mod.rs`). Nothing from B5a should appear here: it was committed before this plan started.

```bash
git commit -m "feat(shell): layout, running column and greeting on the screen

The window gets the approved layout: a 36 px titlebar, a sidebar with the
areas, the projects, the chats by day and the quota footer, a top bar with
the crumb, the model chip and the run button, and a running column that sits
beside the stage from 1,100 px and over it below that. The column opens by
itself once for each question, 1.5 s after the last key, and never takes
focus. The new conversation greets the user and offers four suggestions and
the last conversations. The permission card moves to Ask.tsx and is shared
by the thread and the column."
```

Add the attribution lines your instructions give for commits.

---

## After the cross-vendor review

The commit also carries the fixes of the review that followed this plan. Where the code above and the tree differ, the tree is the newer one.

- `ui/App.tsx`: a conversation that is still opening takes no message and no warm-up (`loading`, `Composer.locked`), the composer is keyed by the place, and a send is bound to the place it was written in. Dismissing or opening the column marks the questions as seen and cancels the pending check (`createAutoOpen().seen`). Keydown and composition count as typing, not only `input`. A resize that makes the column modal moves focus in, and one that makes it plain puts focus on the run button. Tab stays inside an open modal dialog. Ctrl+N closes a modal column and focuses the new composer.
- `ui/styles.css`: the sidebar lists are `minmax(0, 1fr)` grids, so a long title ends in an ellipsis before the delete button. The column over the stage leaves room for the greeting's composer (`.work.under`). The hover nudge of `.try button` and `.rrow` is a `transform` on their content, not a padding change.
- `ui/Sidebar.tsx`: opening a conversation opens its project, and the project row says `aria-current="location"` while one of its conversations is open. `ui/Running.tsx`: a waiting row names its vendor.
- `testkit/src/screen/shell.rs`: `check_shell` requires the composer and the run button, and checks inert state, the element on top and clipping. Six new scenarios cover the loading gate, the greeting's composer under the column, Tab and resize focus, every key (the arrows) holding the column back with the caret kept, a dismissal during the wait, and long titles.
