# UX sentinel: design

Date: 2026-10-12. Status: proposed, for the maintainer's OK. Replaces the 77-line draft of the same date. Serves R98,
R99, R101, R96 and R103 (self-development MVP), R88 and R90 (daily line, own cost), R93, R94 and R95. Inputs:
[design.md](../../design.md), [roadmap.md](../../roadmap.md), `faber-notas/pesquisa/ux-referencias.md`, `studio-design.md`.

## 1. Goal

The owner should never have to report a teleporting panel, a composer that does not grow, garbled text or a screen that
only looks good when still. Faber finds these itself, turns each into a task with proof, and fixes it through
self-development. Later the same system checks the person's own apps.

### Principles

1. **Detectors first, AI second.** Code measures what it can. Vision models look for what code cannot name.
2. **Evidence or nothing.** A finding without a metric, frames or two agreeing judges is not a task.
3. **Invisible.** Everything runs in the hidden test build on the testkit's separate desktop, as design.md "Screen tests" says.
4. **Test build only.** Probes exist only with `test-hooks` and `--mode test`; the one exception is the dogfood logger.
5. **Nothing leaves the PC.** Telemetry stays off. AI review sees only pictures of Faber with fixture data, via the user's CLIs.
6. **Own rubrics.** Faber writes every rubric. No third-party skill pack is bundled or copied.
7. **Bounded cost.** AI calls draw from the R90 own-cost ledger under the R88 daily line. Detectors cost nothing.
8. **Ratchet.** A change may not add a finding. Old findings do not block unrelated work; they are tasks.

**Non-goals.** No replacing native speakers (de, es and fr copy findings become "needs native review"). No judging taste
alone. No pixel-equality snapshots, which fonts and GPUs make noisy; pictures feed review only.

## 2. Where it runs

| Part | Lives in | Job |
| :--- | :--- | :--- |
| Walkthrough runner | `testkit/src/sentinel/` | Visits every screen in a matrix of configs, drives interactions over CDP, gathers results |
| In-page probe | `tests/ux/probe.ts` | Samples geometry each frame; hears animations, layout shifts and long frames; scans text |
| Screen map | `tests/ux/screens.json` | Every screen: how to reach it, states, moving surfaces, interactions, keyboard paths |
| Reference data | `tests/ux/reference/*.json` | Bands from the best apps and design systems (section 7) |
| Review rubric | `tests/ux/rubric/visual-review.md` | Fixed questions for vision review (section 8) |
| Panel rubrics | `docs/panel/<role>.md` | One rubric per expert role (section 11) |
| Findings | `target/faber-test/ux/` (per run), `.faber/ux/` (deduplicated) | Raw results; the list across runs |
| Dogfood logger | `ui/` (production) | Local signals in the stable copy (section 9) |

| Trigger | Scope | Time budget | AI review |
| :--- | :--- | :--- | :--- |
| Every change (`cargo t`, merge queue) | Screens that depend on the changed files, smoke matrix | +90 s at most | Touched screens with a new picture, under the per-change cap |
| Nightly | Every screen, pairwise matrix, every detector | 60 min at most | Every new picture, under the nightly cap |
| Dogfood replay | The screen and interaction a signal points to | 5 min per lead | No |
| On demand ("check the UX") | What the lead asks for | Shown before start | Yes, within the daily share |

Selection reuses the testkit's dependency graph. A change to `ui/styles.css`, `ui/locales/` or a screen file pulls in the
sentinel scenarios of the screens that use it. A change to the probe or the screen map pulls in all of them.

## 3. Prerequisite: a real frame clock on the hidden desktop

Motion checks need frames. A window on a desktop no monitor shows may count as occluded, and Chromium then slows or stops
`requestAnimationFrame` and compositing. So the sentinel proves the clock before trusting motion results.

- Test launches add `--disable-renderer-backgrounding`, `--disable-backgrounding-occluded-windows` and
  `--disable-background-timer-throttling`. The clock check proves whether WebView2 honours them there; nothing is assumed.
- **Clock check.** Each run starts with a 1 s CSS transition on a probe element. The probe must see at least 45 frames, a
  median interval under 20 ms and a `transitionend`.
- **Slow motion.** CDP `Animation.setPlaybackRate(0.25)` stretches CSS animations and transitions 4 times. Motion detectors
  run this way and divide times by 4, so even a slow clock sees intermediate frames. Script-driven motion ignores the rate;
  the CSS lint (M5) flags it for review.
- **Fail loudly.** If the clock check fails, motion results are "not measured" and the run fails as an infrastructure
  error. It never passes silently. Frame strips for review still come from screenshots in slow motion.
- **Focus.** A hidden window never has focus, so the runner turns on `Emulation.setFocusEmulationEnabled`.

## 4. Walkthroughs

### 4.1 The screen map

`tests/ux/screens.json` lists, per screen: `id` (`chat.thread`, `settings.appearance`, `first.look`, `running.overlay`...),
`reach` (settings to seed, fixture to replay, clicks or keys), `states` (`empty`, `loading`, `error`, `long`, `many`,
`normal`, each with a fixture), `surfaces` (moving parts with a `data-surface` name), `interactions` (open, close, switch,
send, resize, theme change, each naming the surfaces it moves) and `keys` (the keyboard path for every task).

**Coverage guard.** In the test build every screen root carries `data-screen` and every moving part `data-surface`. A check
fails when the page shows one the map does not list. A new screen cannot ship unseen.

### 4.2 Matrix

The full product is 7 widths × 6 looks × 5 languages × 3 text sizes × 2 densities × 3 motion levels: too many to run whole.
The sentinel uses an all-pairs set (every pair of values appears at least once) plus full sweeps where a check is cheap.

| Axis | Smoke (every change) | Nightly |
| :--- | :--- | :--- |
| Width (px) | 720, 1100, 1440 | 720, 899, 900, 1099, 1100, 1440, 1920 |
| Look | Dark Room, Morning, High contrast | All 6 |
| Language | en, de (longest), pt-BR | All 5 |
| Text size | 15, 18 | 13, 15, 18 |
| Density | comfortable | both |
| Motion | full, reduced | full, reduced, none |

- Text checks are cheap DOM reads, so each night they sweep every language × every width at size 18.
- Look, language and size change at run time through `save_settings`, with no relaunch. Width changes the real window, as
  `follows_the_width` does.
- Fixtures add hostile content: titles with no spaces, 2,000-character prompts, right-to-left and emoji text, `›`, `«»`,
  `ß`, `ç`, `ã`, long project names, 200-message threads, empty lists.
- **States.** Every screen is visited in each state it lists. New fake-CLI recordings add a slow first token (3 s), an error
  mid-turn, a failed conversation write and a vendor with no quota reading.

## 5. Detectors

Each detector reports `{id, screen, surface, element, config, metric, threshold, evidence}`. Severities are in section 10.

### 5.1 Motion

| ID | Catches | Method | Rule (full motion) |
| :--- | :--- | :--- | :--- |
| M1 Teleport | A surface that jumps between states | Probe samples box, opacity and transform of each watched surface every frame of an interaction | A visible change (move or resize ≥ 8 px, opacity change ≥ 0.5) with 0 intermediate frames |
| M2 Abrupt | Motion too short to read | Same samples | 1 or 2 intermediate frames; under 100 ms for a panel, under 50 ms for a control |
| M3 Slow | Motion that makes people wait | Same samples, `getAnimations()` | Over 400 ms (design.md), or over the reference band |
| M4 No feedback | Hover or press with no response | Computed style before and after hover and press on each control | No change, or a change not transitioned over 50 to 240 ms |
| M5 CSS lint | Rules that break the motion language | Static read of `ui/styles.css` | Duration not from a `--dur-*` token or over 400 ms; `transition: all`; animated `width`, `height`, `top`, `left` without an approved reason; script-driven animation |
| M6 Reduced motion | Movement when asked for less | Emulated `prefers-reduced-motion`, plus `data-motion` reduced and none | Reduced: only opacity, 120 ms or less. None: no running animation |
| M7 Exit and origin | Panels leaving the wrong way | Direction of the box path | Exit not faster than enter, or a panel not moving from its trigger's side |
| M8 Hidden effects | Effects running while hidden | Minimize through the shell's test hook, then `getAnimations()` | Any ambient effect still running |

Watched surfaces are the `data-surface` elements, plus any element whose box moved 8 px or more between the start and the
end of an interaction. The second rule catches surfaces the map forgot.

### 5.2 Layout and stability

| ID | Catches | Method | Rule |
| :--- | :--- | :--- | :--- |
| L1 Layout shift | Content that jumps by itself | `PerformanceObserver` for `layout-shift` without recent input | Over 0.05 per screen visit (web.dev calls 0.1 good; a desktop app is held tighter) |
| L2 Anchors | Composer, scroll position or rows moving while streaming | Boxes of named anchors while the fake CLI streams | An anchor moving 1 px or more without input; running rows changing order |
| L3 Overflow | A page wider than the window | `scrollWidth` of the document and each scroll area | Wider than its box by more than 1 px |
| L4 Thread end gap | The last message sitting on the composer | Thread scrolled to its end: bottom of the last message's last line box against the composer's top | Under 24 px, or covered by the composer's fade or shadow |
| L5 Scroll lie | Looks scrollable but is not, or the reverse | Wheel at the end of a scroll area, compare position | An area at its end whose last item is cut by its edge |
| L6 Overlap | Text on top of text | `Range.getClientRects()` of visible text nodes | Boxes of different elements intersect by more than 2 px |
| L7 Alignment | Edges that almost line up | Left edges of siblings in a column | 1 to 3 px apart (equal is fine; 4 px and up counts as intended) |

### 5.3 Text

| ID | Catches | Method | Rule |
| :--- | :--- | :--- | :--- |
| T1 Mojibake | `â€º` for `›`, `Ã§` for `ç`, `�` | Rendered text nodes, `aria-label`, `title`, `placeholder`, window title, tray and notification texts; static scan of `ui/locales/`, `ui/**` and Rust strings sent to the screen | U+FFFD anywhere, or a string that becomes fewer, valid characters when its Windows-1252 bytes are decoded as UTF-8 |
| T2 Raw keys | `composer.send` on screen | Visible text equal to a locale key, or starting `error.` or `reason.` | Any match |
| T3 Leaked English | English in another language | The test build's `t()` records which key made each string | Text equal to the English value while the language is not English and its own value differs |
| T4 Placeholders | `{n}`, `undefined`, `NaN`, `null`, `[object Object]` | Rendered text scan | Any match |
| T5 Clipped | Text cut with no way to read it | `scrollWidth` or `scrollHeight` over client size with `overflow` hidden | Cut without an ellipsis, or with one but no `title` or tooltip holding the full text |
| T6 Truncation policy | Ellipsis where wrapping was meant | `data-truncate` marks places allowed to cut | An ellipsis on an element without it |
| T7 System words | Raw errors | Rendered text scan | `os error`, `ENOENT`, stack frames, sandbox paths, bare HTTP codes |

T1 also runs as a unit check in the `ui` suite, so a garbled locale or source string fails in seconds. A fixture
round-trips non-ASCII titles through the engine's JSON files back to the screen, so garbling on the way shows too.

### 5.4 Inputs

| ID | Catches | Rule |
| :--- | :--- | :--- |
| I1 Grows | A composer stuck at one line | Typing 1, 2, 5, 10, 40 lines raises the height about one line height per line until the cap |
| I2 Caps and scrolls | Growing forever, or hiding text | Cap at 30% to 40% of the stage height (reference band); past it, inner scroll and the caret stays visible |
| I3 Shrinks | Staying tall | After send or clear, back to one line within one transition |
| I4 Controls anchored | Send or model picker moving | Controls stay on the composer's bottom edge while it grows |
| I5 Thread kept | Composer covering the last message | Growing at the thread's end keeps L4 true |
| I6 Long tokens | One long word breaking out | 300 characters with no spaces cause no overflow (L3) |
| I7 Draft kept | Typed text lost | Switch conversation and back, or open and close Settings: the draft is still there |

Every `textarea`, text `input` and `contenteditable` in the walkthrough gets I1, I2, I6 and I7, not only the composer.

### 5.5 Accessibility

| ID | Catches | Rule |
| :--- | :--- | :--- |
| A1 Rendered contrast | Ink on a surface the token check never planned | 4.5:1 against the composited background (screenshot pixels under images and gradients); 3:1 for 24 px or 18.66 px bold. Disabled and `aria-hidden` text exempt |
| A2 Names | Controls with no accessible name | Every interactive node in the CDP accessibility tree has a name |
| A3 Focus order | Tab jumping around | Tab order follows reading order by region; no inversion over one row; no visible control unreached |
| A4 Focus visible | Focus you cannot see | Crops before and after keyboard focus show a ring of 3:1 against its neighbours |
| A5 Traps and returns | Modals that leak or lock | design.md overlay rules: focus stays in, Esc closes where it should, focus returns to the trigger |
| A6 Hit targets | Controls too small | 24 × 24 px, or 24 px of free space around (WCAG 2.2, 2.5.8) |
| A7 Keyboard paths | Tasks that need a mouse | Each `keys` path of the screen map finishes with no pointer |
| A8 Not by colour | Amber, coral or violet alone | Each meaning colour comes with its shape and words |

These are Faber's own checks. No third-party audit library is added in v1; a later decision may add one, test-only.

### 5.6 States and budgets

| ID | Catches | Rule |
| :--- | :--- | :--- |
| S1 Blank state | A screen with nothing to read | The main region shows text in every state; an error state shows a next action |
| S2 Loading | A spinner forever, or a jump when content lands | A loading sign within 300 ms of a slow request; L1 holds when content arrives |
| B1 Memory | Faber too big | Idle, window open, no CLI: 250 MB (the end-to-end limit today, later 500 MB) |
| B2 Leak | Memory kept after closing screens | After the full walkthrough and a return to the new conversation: within 30 MB of the start |
| B3 Startup | Slow to usable | Process start to an enabled composer. Proposed: fail over 3 s; warn 25% over the 7-night median |
| B4 Long frames | Jank | No `long-animation-frame` over 200 ms during an interaction; warn when the 95th percentile grows 25% |
| B5 Input to paint | Slow response | Event timing from input to next paint over 100 ms |
| B6 DOM size | A big DOM kept alive | Node count per screen each night; a budget is set after the first week |

Timing on the hidden desktop differs from a real screen. Absolute limits stay loose; trends against the same machine do
the fine work, and dogfood (section 9) gives real-screen numbers.

## 6. The four owner findings

The first version catches all four. Each detector is proven red on today's build before any fix, and a planted defect on a
sandbox copy (never the repository) keeps proving it.

| Owner report | Detector | Fails today because | Planted defect that must fail |
| :--- | :--- | :--- | :--- |
| A panel teleports | M1 on every panel surface: running column docked and overlay, sidebar overlay under 900 px, dialogs, tutorial card | 0 intermediate frames between closed and open | `transition: none` on one panel |
| The composer does not grow | I1 to I5 | Height flat after 2 lines | A fixed composer height |
| Mojibake in the breadcrumb | T1 on the rendered crumb in all 5 languages, the static scan, the round-trip fixture | `â€º` in the crumb text | A locale string with `â€º` |
| Last message on the composer, so people try to scroll | L4, L5, and dogfood D4 (wheel at the end) | Gap under 24 px at the thread's end | Thread bottom padding set to 0 |

## 7. Reference baselines

**Rules.** References are numbers, never code, images or other assets. Faber never attaches DevTools to, unpacks or
instruments another company's app; it observes pixels only. Raw recordings stay on the PC outside the repository; only
numbers enter `tests/ux/reference/`.

| Tier | Source | Example |
| :--- | :--- | :--- |
| A | Published design-system guidance, with a link | Atlassian motion: interactions 50 to 150 ms, transitions 150 to 400 ms |
| B | Frame analysis at 60 fps of a recording of the real, current app | Panel open time in a desktop chat app |
| C | Faber's own approved values | design.md: nothing over 400 ms; buttons 140 to 240 ms |

Tier B recordings come from vendors' public demo videos, or from one optional scripted recording of at most 10 minutes the
owner may make each quarter. Without them, bands come from tiers A and C and say "not measured". Unverified sources, such
as third-party recreations of an app's design, never set a band.

**Data.** Each entry in `tests/ux/reference/<pattern>.json` holds `pattern`, `app`, `app_version`, `measured_on`,
`method` (`published`, `frames`, `owner_rule`), `source`, `values` with units and `band` (`min`, `target`, `max`). Bands
merge entries: the owner's rule wins, then measured apps, then published guidance. Entries older than 6 months are stale.

| Pattern | Measured | Starting band | Source |
| :--- | :--- | :--- | :--- |
| Panel open | Duration, distance, easing class | 150 to 250 ms, never over 400 | A, C |
| Panel close | Duration against open | Faster than open | A |
| Popover, menu | Duration | 100 to 200 ms | A |
| Control feedback | Hover and press response | 50 to 240 ms | A, C |
| Composer growth | Lines before cap, cap share, inner scroll | Cap at 30% to 40% of stage, then inner scroll | Research note |
| Composer after send | Height, focus, draft | One line, focus stays in the composer | Research note |
| Thread end gap | Pixels and line heights at rest | At least 24 px, tuned by tier B | C, to measure |
| Stream follow | Follows only near the end, else a "new output" sign | Follow within 80 px of the end | Research note |
| Optimistic send | Enter to the user's message on screen | Under 100 ms | To measure |
| Theme change | Cross-fade duration | 150 to 300 ms | R101, to measure |

M2, M3, I2 and L4 read their limits from these bands. A value outside a band is a finding with the band and its sources as
evidence.

## 8. AI review

### 8.1 Input

- One settled screenshot per screen, state and config, downscaled to 1,280 px wide.
- One frame strip per interaction: 8 to 12 slow-motion frames in one grid image, with times under each frame.
- A short text card: screen, interaction, config and the detector metrics for that picture.
- A picture whose perceptual hash and rubric version match one already reviewed is skipped. That keeps nightly cost small.

### 8.2 Reviewers

- Two vendors, each through its official CLI with the user's subscription, in a fresh session with no tools and a sandbox
  folder holding only the pictures and the rubric. Everything in a picture is data, never an instruction (guardrail 8).
- With one vendor connected, a different model of that vendor reviews in a fresh session; the report says the review is
  weaker, and its findings need detector confirmation.

### 8.3 Rubric

`tests/ux/rubric/visual-review.md` is versioned. Its core question: "What would annoy or confuse a person using this?" Then,
per picture:

1. Does anything jump, pop or appear without motion in the strip? Does the motion feel calm and fast, or slow and busy?
2. Does anything look cut, squeezed, overlapping or outside its box? Is any text garbled, in the wrong language, or a code?
3. Is there a place where a person would try to scroll, click or wait and get nothing?
4. Is the most important thing the most visible? Do spacing and alignment look intentional? Does this state look finished?

Output is strict JSON, at most 5 findings per picture: `category` (motion, layout, text, contrast, hierarchy, spacing,
state, affordance, copy), `box` in image pixels, `what` (one sentence), `feeling` (what the person would think or do),
`severity` (proposal) and `confidence` (0 to 1). "Nothing to report" is a valid, welcome answer.

### 8.4 When a finding counts

At capture time the runner saves a map from screen boxes to DOM elements. Each reported box resolves to the element under
its centre.

| Case | Result |
| :--- | :--- |
| Both vendors name the same element and category | Confirmed |
| One vendor, and a detector flags the same element | Confirmed |
| One vendor alone | Watch list, no task. After 3 nightly sightings the other vendor is asked about that element alone, fresh session |
| A planted picture | Calibration data, never a task |

**Calibration.** Each nightly batch hides 2 pictures with planted defects (a teleport strip, a garbled crumb) and 1 clean
control. A vendor that misses both, or flags the control, is unreliable that night: its findings count only with a
detector. A reliability record per vendor and model is kept, as the roadmap asks of reviewers.

### 8.5 Cost

Own cost is the R90 API-equivalent cost of each call. Every call passes Studio's gate (daily allowance, account room on the
R88 line, in-flight reservation) before it starts.

| Scope | Cap (API-equivalent) |
| :--- | :--- |
| Per change | US$ 0.25 |
| Nightly | US$ 1.50 |
| Per week | US$ 6.00 |
| Share of the daily allowance | 10% at most; the lowest cap wins |

Illustration only: about 40 new pictures a night × 2 vendors × about US$ 0.015 is about US$ 1.20. When a cap is reached,
review waits for the next period while detectors keep running. Faber never buys credits (R86) and never borrows from
tomorrow.

## 9. Dogfooding signals

The owner uses the stable copy in `E:\Apps\Faber`. It can keep a local log of rough spots.

- **Off by default for everyone.** The switch is in Settings > Privacy, and turning on self-development asks once. This is
  not telemetry: the logger has no network code, and a check fails if its module imports any network API.
- **No content.** Entries hold the screen id, a `data-surface` or stable element name, the kind, times and numbers. Never
  prompts, titles, file paths or answers.
- **Storage.** `ux-log/YYYY-MM-DD.jsonl` in Faber's data folder, 5 MB at most, kept 30 days. Privacy offers "See what was
  logged" and "Delete". A ring buffer with batched writes; under 1 MB of memory, measured by B1.

| Signal | Rule |
| :--- | :--- |
| D1 Repeated clicks | 3 clicks within 1 s and 30 px on the same element |
| D2 Dead click | No DOM change, focus change or navigation within 1 s of a click |
| D3 Back loops | Open then close the same surface within 2 s, twice in a row; A to B to A within 3 s |
| D4 Scroll at the end | 3 wheel events down within 2 s on an area already at its end |
| D5 Errors | Uncaught errors, rejected promises, an `error.*` key shown, a failed write |
| D6 Slow response | Input to paint over 200 ms; a long frame over 200 ms; an engine command over 1 s |
| D7 Retyping | The composer cleared and refilled within 10 s |

**From signal to finding.** Each night the self-development side groups signals by screen, element and kind. A group with
3 hits in 7 days, or any D5, becomes a lead. The lead replays that screen and interaction in the hidden build with the
matching detectors. Reproduced means confirmed; not reproduced stays a lead in the weekly summary. An error whose stack is
in Faber's own code is confirmed on its own.

## 10. Findings, tasks and gates

### 10.1 The record

| Field | Content |
| :--- | :--- |
| `fingerprint` | Hash of kind, screen, surface and stable element name. Config is left out: one bug in 30 configs is one finding |
| `sources` | Detector ids, AI verdicts of both vendors with rubric version, dogfood leads |
| `configs` | Every width, look, language, size and motion level where it showed |
| `metric` | Value, threshold, reference band and its sources |
| `evidence` | Frame strip, screenshot crop with the element boxed, probe trace JSON, DOM snippet |
| `seen` | First and last seen, count, the commit where it first appeared |
| `status` | open, in task, fixed, regressed, waived |

### 10.2 Severity

| Level | Meaning | Default kinds |
| :--- | :--- | :--- |
| S1 Blocker | Content unreadable, unreachable or harmful | T1, T2, T7, A1 on body text, A5, A7, M6, S1, B1, D5 errors in Faber's code |
| S2 Major | Clearly broken feel or layout | M1, I1 to I5, L1, L3, L4, T3 to T5, A2 to A4, A6, B2, B3 hard limit |
| S3 Minor | Noticeable, not broken | M2 to M4, M7, M8, L2, L5, L6, T6, B4, B5, band misses |
| S4 Polish | Fine detail | L7; AI-only spacing and hierarchy notes |

Adjustments, once each: a **regression** (passed before, fails now) is at least S2. A finding on the **default path** (Dark
Room, size 15, 1440 px, pt-BR) moves up one level, to S2 at most. Within a level, more screens and configs come first.

### 10.3 Gates

| Gate | Blocks on |
| :--- | :--- |
| Merge queue, each change | A new S1 or S2 detector finding against the base commit. AI-only findings never block a merge; they make tasks |
| Self-update, next to stable (R93) | Any open S1; any S2 new since the current stable build |
| GitHub release (R94) | Any open S1 or S2. The owner may waive an S2 with a written reason in the decision list |

**Ratchet.** Findings open at the base commit are known and do not block unrelated changes. A change that touches the
element of a known finding must not make it worse.

### 10.4 From finding to task

- Each confirmed finding becomes one self-development task, or joins the open task with its fingerprint. A fixed finding
  that returns reopens its task as "regressed".
- The brief: what a person would notice (one sentence), where, the evidence and the acceptance.
- Acceptance: the detector passes on every listed config, and a regression check exists that fails without the fix
  (roadmap rule). An AI-confirmed finding with no detector first gets a detector when feasible; otherwise both vendors must
  pass the same picture.
- Copy findings in de, es or fr are parked as "needs native review". Faber does not rewrite those languages.
- Fixes go through the R103 mission runner and the merge queue like any change, in Seguro git mode.

## 11. Expert panel (R99)

### 11.1 Rubrics

- Faber writes each rubric in its own words, from the owner's decisions, design.md and public facts (WCAG, platform
  guidelines) cited by link. No third-party skill or rubric pack is bundled or copied.
- For Faber: `docs/panel/<role>.md`. For a person's project: `.faber/panel/<role>.md`, plain files in project memory.
- Each check has an id, one question, a pass rule, the evidence it needs, a default severity, its backing detectors and
  what is out of scope. Rubrics are versioned. A rubric change is reviewed by the other vendor and listed as "not reviewed"
  in the decision list until the owner sees it.

### 11.2 Roles

| Role | Checks (short) | Backed by |
| :--- | :--- | :--- |
| UX | One clear main action; every state finished; no dead ends; back and undo paths; feedback within 100 ms; drafts kept; no duplicate of an existing screen or control | S1, S2, I7, A7, B5, coverage guard, D1 to D4 |
| Visual design | Tokens only; spacing rhythm; alignment; one clear hierarchy; same look across screens; all 6 looks hold; thread end gap | M5 token part, L4, L6, L7, A1, AI review |
| Motion | Every surface change animates; durations in band; exit faster than enter; motion from the trigger; no layout-property animation; calm streaming; reduced and none respected | M1 to M8, L2, reference bands, frame strips |
| Accessibility | Contrast; names and roles; focus order and visibility; modals; hit targets; keyboard paths; size 18 fits; meaning not by colour | A1 to A8, L3 at size 18, M6 |
| Performance | Idle memory; leaks; startup; long frames; input to paint; DOM size; nothing kept alive without a measured reason | B1 to B6, D6 |
| Security | No shell for child processes; no credential paths; no secrets; theme files validated; markdown sanitized; no network in the logger; Tauri command list changed only with a reason | Secret scan, spawn and path rules, command list diff, logger import check |
| Architecture | Engine knows nothing about windows; only `look.rs` computes colours; text only in locales; small focused files; no new runtime dependency without a reason | Import boundary checks, dependency diff, locale scan |
| Testing | A failing-before test for every fix; no real CLI, git or network in unit tests; no blind sleeps; new screens in the map; stable on rerun | Testkit audit, planted defects, coverage guard, rerun diff |
| Copy | Plain short sentences; one word per idea; amber always with words; no vendor names in product names; fits at 720 px in de; placeholders kept | T2 to T5, L3, i18n checks, glossary check |

### 11.3 How two vendors judge

- Each role in scope gets two judges, one per vendor, in fresh sessions, with the same rubric, blind to each other and to
  who wrote the change. The writer never judges its own work (guardrail 5).
- **Selective scope.** As in `cargo t`, changed files pick the roles: styles and screens pull UX, visual, motion,
  accessibility and copy; locales pull copy; engine code pulls architecture, performance, security and testing.
- **Cheap first.** A small change outside the screen gets one judge, from the vendor that did not write it. The second
  judge is called only when the first raises a blocking item.
- **Blocking needs proof.** A blocking item stands when both judges agree, or one judge plus a detector, test or
  reproduction shows it. A lone judge may turn its claim into a check; if the check fails on the change, the item stands.
- **Disagreement.** A lone non-blocking item becomes an S3 or S4 note in the task list, never a gate.
- **Calibration.** Each role's judges meet one planted defect per week on a copy. A judge that misses it loses blocking
  weight for that role until it passes again. Judge calls pass the cost gate of 8.5.

| Where | Use |
| :--- | :--- |
| Merge queue | Studio's "Independent review" and "Final acceptance" steps call the roles in scope |
| UX sentinel | The UX, visual, motion, accessibility and copy rubrics drive the AI review of section 8 |
| The person's own apps (roadmap Phases 3 and 4) | The same roles and detectors, rubrics in `.faber/panel/`, the screen map built from the project's product map, the probe injected over CDP into a hidden browser and never shipped; the project's design system replaces Faber's own taste |
| Faber itself | The rubrics in `docs/panel/`, on every self-development change |

## 12. Rollout

R96 targets self-development around 2026-10-20 to 22, and R103 cut the MVP to about 8 tasks. The sentinel's first slice is
one task beside them, small enough to land first, so Faber's first fixes to itself are its own findings.

| Slice | Contents | Exit |
| :--- | :--- | :--- |
| S0, before 2026-10-20 | Clock check and slow motion; probe with M1 and M6; I1 to I5; T1 rendered, static and round-trip; L4, L5; `data-screen` and `data-surface` on today's screens; smoke matrix for those screens; findings file with fingerprints; merge-queue gate for new S1 and S2; planted-defect twins; the sentinel section in design.md "Screen tests" | The four owner findings fail on today's build, every planted defect fails, and the run adds 90 s at most |
| S1, built by Faber, late October | Every other detector of section 5; full screen map and coverage guard; nightly pairwise matrix; state fixtures; budgets in the report; tasks in the mission runner | Every mapped screen passes or has an open task; nightly ends within 60 min |
| S2, early November | Dogfood logger in the stable copy (shipped with a v0.1.x release, R94 and R95); lead replay; reference files with tier A and C bands, tier B when a recording exists | Signals logged with no content; one lead reproduced end to end |
| S3, once Studio proves bounded spending (studio-design.md, section 6) | AI review with rubric, calibration and caps; panel rubrics v1 for all nine roles; panel in the merge queue | Planted defects caught by both vendors; caps never passed in two weeks of nightlies |
| S4, roadmap Phases 3 and 4 | The person's own apps | One real project checked end to end |

**Cost of the sentinel itself.** Production gains only the dogfood logger (under 1 MB, off by default) and the `data-screen`
and `data-surface` attributes. Pictures in `target/faber-test/ux/` are pruned after 14 days, except evidence of open findings.

## 13. Risks and open questions

| Risk | Answer |
| :--- | :--- |
| WebView2 throttles the hidden desktop | Clock check, slow motion, fail loudly (section 3). Never fall back to the person's desktop |
| Timing noise on CI | Loose absolute limits; trends on the same machine; slow motion for motion |
| AI false positives | Two vendors or a detector; calibration; watch list; at most 5 findings per picture |
| Too many tasks at once | Fingerprint dedupe; severity order; the ratchet keeps old findings from blocking work |

Open for the maintainer:
1. The 24 px thread end gap and the 30% to 40% composer cap are starting values. Confirm or change.
2. Should new S2 findings block self-update, or only releases?
3. May Faber offer the owner the optional quarterly 10-minute recording, or should tier B use public videos only?
