# Faber redesign: design specification

Date: 2026-10-11. Status: draft, revised after two rounds of cross-vendor review (see "Review log" at the end). It
replaces the product direction in `docs/roadmap.md` and the visual direction in `docs/design.md`. Phase 1 rewrites
both from this document (B1).

The document has two parts:

- **Part A** describes the whole product briefly: what Faber is, its principles, and the order in which it gets built.
- **Part B** specifies Phase 1 ("new base") in enough detail to write an implementation plan.

## How to read the citations

Every requirement names its source in brackets.

| Tag | Source |
| :--- | :--- |
| [R1]–[R76] | Redesign rows in `faber-notas/decisoes.md`. A later R overrides an earlier R, and any R overrides a D line. Each row names its author, Erick or the AI. |
| [D1]–[D184] | Older numbered lines in the same ledger. They hold only where no R row says otherwise. |
| [roadmap] | `docs/roadmap.md` as of commit 3189434 |
| [design.md] | `docs/design.md` |
| [estado] | `faber-notas/estado.md`, the record of what is built |
| [plano] | `faber-notas/plano-fundacao.md` |
| [notes] | `faber-notas/design-final/notes.md` |
| [tokens] | `faber-notas/design-final/tokens.json` |
| [mock] | `faber-notas/design-final/index.html`, the approved mock (screens s1–s6) |
| [symbol] | `faber-notas/design-final/simbolo-final/notes.md` and its SVG files |
| [catalog-v1] | `faber-notas/catalogo-v1.md` |
| [inspiração] | `faber-notas/design-inspiracao.md` |
| [AGENTS] | `AGENTS.md` |
| [modelos] | `faber-notas/modelos.md`, the model and price notes (golden rules of economy) |
| [research-cli] | `faber-notas/pesquisa/praticas-claude-codex.md`, the 2026-10-11 research on driving Claude Code and Codex |
| [research-tools] | `faber-notas/pesquisa/tl-ade-ferramentas.md`, the 2026-10-11 inventory of the tools TL-ADE used |
| [code] | The repository as it is today |

Where the sources leave a gap, this document makes the choice itself. It marks the choice "(AI proposal, AI-n)".
The section "AI choices made in this spec" lists every such choice. Contradictions that only Erick can settle are in
"Open questions for Erick".

**Who wins.** A row that Erick wrote outranks every AI proposal in this document and every R row the AI wrote [R1,
R72, R76]. If an AI proposal clashes with an Erick-authored row, the row wins and the clash goes to "Open questions for
Erick". While Erick is away, the AI approves this spec [R76]. R72 still keeps five kinds of question for him: product
direction, his taste in visible identity, money, license, and anything that contradicts one of his earlier decisions.
R76 says these stay noted for him to see in the morning.

**Decided by Erick.** Both questions are now decided: the three meaning colours stay locked [R77], and the license is
FSL-1.1-ALv2 [R79]. Phase 1 is built on the local branch `phase-1-base`, which is not merged and not pushed until Erick
approves (AI-24). The license change is its own small commit (B1).

The brief for this document asked for the catalog decisions "R94 to R100". No R row has those numbers; the R rows end
at R76. This document reads that request as the catalog lines D94–D100 in section G of the ledger.

---

# Part A: product overview

## A1. What Faber is and who it is for

Faber is a desktop app that runs the official Claude Code and Codex CLIs on the user's own subscriptions
[D3, D4, R4]. It aims to beat every harness and meta-harness Erick uses or knows, in orchestration, quality,
optimization and token economy [R2]. It does anything AI can do, at commercial quality, so the user never needs
another AI app [R3]. It has everything Claude Code and Codex have [R3].

It is for anyone. People who use AI every day should find it familiar, and someone who never has should learn it just
by using it [R6]. "Enterprise-grade" means professional quality and robustness, with maintenance and updates that
happen on their own. Team and admin features are out until Erick asks for them [R49].

## A2. Principles

- **Priorities.** Quality is roughly equal to economy, and both come before speed. Quality is slightly ahead. When
  speed and economy conflict, economy wins [R10].
- **Good at everything from day one.** The order of excellence decides which areas get specialized tools and
  benchmarks first [R11]. It never limits what Faber does [R12]. The floor is the unmodified CLI underneath, with
  nothing cut [R12].
- **Only Claude Code and Codex** [R4]. Faber works with Claude only, Codex only, or both [R9].
  - The subscription is the main path.
  - An API key is optional and goes only through the CLI's own login, so Faber never sees it [R28].
  - Sign-in always happens in the CLI's own flow [R9]. Faber never reads, copies or stores credentials [AGENTS].
- **Every decision has an author.** The ledger records each decision as Erick's or the AI's [R1]. From R72 on, the AI
  asks Erick only about the following; it decides everything else and records it as "IA (Erick delegou)" [R72]:
  - product direction;
  - his taste in visible identity;
  - money;
  - license;
  - anything that contradicts one of his earlier decisions.
  - Erick-authored decisions outrank AI proposals. While he is away, the AI approves this spec and leaves those
    questions open for him [R76]. Phase 1 builds any such question on an unmerged branch until Erick approves (AI-24).
- **Light.** Memory is a feature, with a hard budget (A12) [AGENTS, D14].
- **Nothing third-party ships with the app.** Telemetry is off by default [AGENTS, roadmap].
- **Faber builds itself.** Claude and Codex build Faber together, and whoever writes a part never reviews it [R26].
  Once the software area is ready, Erick builds Faber with Faber [R27].

## A3. The two areas: Chat and Projects

Faber has two areas, a normal Chat and Projects [R7].

**Chat** works like claude.ai and ChatGPT and must beat them [D18].

- It has many conversations [D19]. It never changes files on the PC [D20].
- Each conversation has a hidden workspace where the model runs code, draws charts and creates files [R19].
  - The user never sees a folder [R19].
  - Files show up in the conversation with "download" and "take to a project" [R19].
- The user picks a model or Auto [D25] and always sees which model answered and, in Auto, why [D26].

**Projects** works like Claude Code [D23].

- The user opens a folder, keeps many projects, and has many conversations per project [D23].
- This is where the lead agent works (A6).

## A4. Normal and autonomous modes

Projects has a normal mode and an autonomous mode [D63].

- **Normal.** Normal mode feels like Claude Code: the user talks only to Faber, and Claude and Codex work behind it
  [D65].
- **Autonomous.**
  - It can be switched on at any moment, even while the agent is working, and it takes effect at once [D64].
  - Question cards apply their default answer. Sending, publishing, paying and deleting are parked with a note, and a
    report comes at the end [D66, roadmap].
- **Both modes.** The agent calls the user only when it needs them and never stops just to wait [D67]. Everything
  persists across closing, crashes and reboots [D68].

## A5. Auto

Auto aims to pick a model that does the task well without waste, and quickly ("quality and economy, with speed")
[R20].

**How it works today.**

- A small model only classifies the kind of task and its difficulty. A table picks the model, tuned for quality first
  [R21].
  - This is built and measured: 81 of 88 picks were right [estado].
  - The router never sees model or vendor names [D47].
- A send waits at most 30 s for the pick [D54].
- A conversation that began easy moves up when the work does, and never moves down [D56, estado].

**Still to build.**

- The table learns from what the user approves or redoes [R21].
- The draft is routed at most twice per message while the user types; today it is routed at every pause [R22].

## A6. How Projects builds software

**The lead agent.**

- The lead agent picks the path for each request [R8]: do it now, plan it in conversation, or split it across several
  Claude and Codex workers [D58].
- Before big work it shows the plan, the time and the quota, and asks once [R8].
- There are no fixed pipelines per kind of request ("powers, not tracks") [D7, D59].

**One voice, a team behind it** [D62, roadmap].

- One lead session keeps the thread.
- Workers get short briefs and return short reports.
- One product map keeps the result coherent: no duplicate screens or buttons [D70, roadmap].

**Powers and toolbox.**

- The agent has 15 powers [R47, R48]:
  - the 12 of R47, which the rewritten roadmap keeps as its table (B1):
    1. ask only what changes the result;
    2. learn from what worked;
    3. cheap draft before expensive work;
    4. see it where it will live;
    5. another model checks;
    6. better than the original, or it does not stay;
    7. proof before work;
    8. facts with sources;
    9. redo only the part that changed;
    10. pick up where it stopped;
    11. drive the user's apps;
    12. remember taste;
  - "right-sized": each part goes to the cheapest model that does it well [R48];
  - "don't reinvent": reuse ready work with a good license [R48];
  - "safe to sell": check for leaked secrets, known vulnerabilities and commercial licenses before delivery [R48].
- The toolbox, an MCP server shared by every CLI, is how the powers work [R47, roadmap].

**Guardrails enforced by code, never by the model** [R8, roadmap]:

- another model reviews the work;
- "done" needs proof;
- there is a snapshot before any change, so it can be undone;
- sending, publishing, paying or deleting asks every time;
- whatever shows up on screen or in a file is data, not instructions.

**Reviewers.**

- With both vendors, the reviewer comes from the other vendor [R9].
- With one vendor, a different model of the same vendor reviews in a fresh session, and the screen says the review is
  weaker [R9].

**Big work always reaches a conclusion** [R36, R37].

- "Stop" pauses and can resume.
- Going 50% over the estimate only warns and switches to economy mode.
- A stuck attempt is dropped while what it produced stays.
- When a quota runs out, the work moves to the other subscription or waits for the renewal.

**Decision list.** Every project keeps a decision list with authors. The AI's decisions show as "not reviewed" [R16].

## A7. Following the work: the progress view and the running list

**Progress view** [R52, R40, R17]. Each part of the work is one row that moves left to right through four columns:
Waiting, Doing, Reviewing and Done.

- **Rows.** Each row shows the AI's name and a verb, such as "Claude drawing" [R52].
- **Top.** One plain sentence, the overall bar, the time left, the share of the plan's limit used, and who is working
  now [R52].
- **Bottom.** A feed of short sentences [R52].
- **Detail screen.** Clicking a part opens a detail screen [R52]. It shows:
  - what is happening now, and a preview that fills in;
  - the steps, who makes the part and who reviews it;
  - the time and the limit spent;
  - choices made alone, each with "Change";
  - review findings and the proof;
  - a message box to the part;
  - pause, redo and open files;
  - technical details behind a click.
- **Words.** Plain words that explain themselves [R52].
- **No agent map.** The map with the lead in the centre was dropped [R52].

**Running list** [R53, R54].

- **The column.** It sits on the right and is closed by default. A button at the top is always visible and reads, for
  example, "2 running" [R53].
- **When the user is needed.** The button turns amber [R53]. The column opens by itself once per question [R53].
  - It never opens while the user is typing, and it never moves the cursor [R53].
  - If the user closes it, only the button stays amber until they answer [R53].
- **Order.** Items are grouped by project, one block per project [R54].
  - Projects that have something needing the user come first [R54].
  - Inside a project, the amber item comes first [R54].

**Tray.** The tray icon shows a number. A notification appears only when an agent needs the user, never each time
something finishes [R74].

## A8. Visual answers

**What they are.** Answers can contain visual pieces, such as checklists, charts and forms. Faber calls them "Visual
answers" [R73, R39]. They work in both Chat and Projects [R39].

**How they start.**

- They start with about 10 pieces built into Faber [R73]: big numbers, table, chart, slider or −/+ tied to a
  calculation, checklist, tabs, steps or quiz, options form, notice and timeline [inspiração].
- Free-form mini apps come later, and only if they are missed [R73].

**Rules.**

- What the user checks or moves stays saved, even after the conversation closes [R73].
- Text is the default; a piece appears only when it saves the reader effort [inspiração].
- A form never sends on its own. It only fills the composer [inspiração].
- A broken piece falls back to plain text [inspiração].

**Comparisons.** Side-by-side comparison of options is part of visual answers: images, animations or live sites,
whatever fits [R42].

## A9. Learning and memory

**Learning.** Faber learns from use, in a way that stays visible and organized [R15].

- What belongs to a project stays in that project [R15].
- Global memory keeps only what holds 100% in every situation [R15].
- A project lesson becomes global in one of two ways [R51]:
  - A clear objective improvement becomes global by itself after it repeats in 3 or more projects. It can be deleted
    [R51].
  - A personal preference becomes global only with the user's approval [R51].
- When in doubt, Faber treats a lesson as a preference and asks [R51, the row's AI note].
- Everything sits in a visible list with one-click delete [R51, the row's AI note].

**Where things are stored.**

- Decisions, taste, plans and recipes live in plain files in the project's hidden `.faber/` folder, so other programs
  and AIs can read them [R35].
- The conversation history of both areas, Chat and Projects, stays private inside Faber [R35]. Only the named
  project-memory files above live in `.faber/`.

## A10. Updates

- **Faber itself.** Faber updates itself [R50].
  - Today it downloads a signed update in the background and installs it on quit, so it never interrupts work
    [estado, code].
- **The CLIs.** Claude Code and Codex move only to versions that GitHub tested overnight with recorded conversations
  [R34, R50].
  - If a version breaks, Faber stays on the previous one until a fix arrives as a published file, with no new Faber
    download [R34].
- **Catalog tools** never update on their own [R50].
  - Faber suggests an update at a good moment. If the user agrees, Faber runs the update and the check [R50].
  - A good moment is when nothing is running, or when the update fixes something the user is using; never in the
    middle of work [R50, the row's AI note].
  - Before updating, Faber shows what changed, checks safety and keeps the old version [R34].

## A11. The catalog

**What the catalog recommends.** Faber recommends only tools it has vetted [R33]. A vetted tool:

- works;
- is safe;
- has a license that allows the use;
- is maintained;
- improves the result.

**Tools that are not vetted.** The agent may find and install any other tool. That tool is marked "not vetted", and
the agent asks before installing it [R33].

**Rules from the older lines.**

- Nothing comes inside the app. The user downloads each tool from its original source [D94, AGENTS].
- The curated list stays updated over time [D95].
- Items are optional but recommended, and there is a button to install them all [D96].
- The user may add their own tools, skills, plugins and MCP servers freely [D97].
- Each item shows its pinned version, its checksum and its license, with a warning for non-commercial licenses [D100].

**The v1 draft** [catalog-v1].

- 1 Recommended item (OSV-Scanner), 123 Experimental items and 28 removed.
- Erick's preferences are applied: ponytail in, caveman out, Taste Skill in, HyperFrames in [R33].

**When it arrives.** The catalog screens and the catalog step in the first run come in Phase 3, with the lead agent's
toolbox (AI proposal, AI-1).

## A12. Memory budget

- **The budget.** Faber with no CLI running stays under 500 MB when complete, and the end-to-end test fails above
  250 MB today [AGENTS].
  - Anything that keeps a process or a large DOM alive needs a reason and a measurement [AGENTS].
- **Measured today** [estado]:
  - 89 MB idle;
  - 114 MB with a 1,000-message conversation, of which only the last 40 messages are drawn;
  - 4 MB when closed to the tray.
- **The CLIs** [estado]:
  - Claude Code starts on the first keystroke and closes after 2 idle minutes.
  - Codex runs one shared app-server per area.

## A13. Design system

There is one screen structure, and every theme is only a set of token overrides on a shared base [notes, tokens].

**Themes** [R58, R59, R60]. There are three:

| Theme | From | What it looks like |
| :--- | :--- | :--- |
| Sala escura (default) | D2 | Black and white with strong contrast and a projector light on the new conversation |
| Oficina | D4 | Warm graphite, book-serif headings, chalk-blue details and a slow loom effect |
| Bancada | D5 | Dark olive, narrow headings, mint details and a flow line from idea to proof |

Erick called the D2 style the best one to work in, and D4 the style he likes most [R57].

**Presets** [R59, R60]. There are three, on top of the themes:

- Manhã: the light look.
- Alto contraste: high contrast.
- Cobalto: navy and gold, inspired by the TL-ADE.

**Customization** [R58, notes].

- The user can change:
  - the accent, from five swatches or any colour;
  - the ground;
  - the text font and the heading font;
  - the text size, from 13 to 18 px;
  - the density and the corners;
  - the motion level: full, reduced or none;
  - three effect switches.
- A custom look saves to a small file that can be shared [R60].
- There is no light/dark switch that follows the system. The light look is the Manhã preset [R59] (AI proposal,
  AI-9).

**Meaning colours are locked** [notes, tokens]. These colours are fixed meanings, not decoration:

| Meaning | Colour | Hue | Shape |
| :--- | :--- | :--- | :--- |
| Needs you | Amber | 40 | Speech bubble, always shown with the words "needs you" |
| Claude | Coral | 14 | Square |
| Codex | Violet | 245 | Diamond |

- A theme may shift their tone for its ground, but never their hue family [tokens].
- An accent whose hue is less than 24° from any of the three is refused [R60]. Exactly 24° passes.
- An accent that lacks contrast is corrected automatically [R60].
- The lock is Erick's decision [R77]. It lives in three places only: the hue block of the validator (B3), the `locked`
  section of `themes.json` (B3) and the read-only row in Settings (B8).

## A14. Identity

**The symbol.** Faber's symbol is "Curioso" [R66, R67]. It is a lime creature with one lopsided, rounded shape, two
pill-shaped eyes and a notch on the right, so it looks as if it is peeking. It is meant to be recognized, not to tell
a story [R61].

**The app icon** is a single dark icon in every Windows mode [R70]:

- a lime #D6DC48 figure on a near-black #141A11 tile;
- no border [R71];
- corner radius about 22.6% [symbol].
- Hand-tuned 24 px and 16 px versions keep both eyes readable [symbol].
- On a dark taskbar the tile disappears and the lime figure carries the icon [R71, symbol].

**Inside the app.**

- Empty states and the tutorial use Curioso itself, for example peeking into an empty screen [R75].
- The interface font belongs to each theme [R75].
- The wordmark font is chosen with this spec [R75]. It is Rubik Bold, converted to outlines (AI proposal, AI-12).

## A15. License

- **The decision.** Faber is licensed under the Functional Source License 1.1, ALv2 Future License (SPDX
  `FSL-1.1-ALv2`) [R79]. It replaces the AGPL-3.0 choice of R25.
  - Faber is source-available, not open source. Anyone may use, change and share it for any purpose except a
    Competing Use, as defined in `LICENSE`.
  - Each version becomes Apache-2.0 two years after its release.
  - What users create with Faber belongs to them.
- **Changes.** Phase 1 changes `LICENSE`, `NOTICE`, the license fields and the documents in one small commit of its
  own (B1).

## A16. Distribution

**Installers.**

- Faber is downloaded directly from GitHub Releases, for now with the Windows warning [R43, D158].
- The Microsoft Store and a code-signing certificate come back to the table when everything is complete [R43].
- Windows comes first. macOS and Linux compatibility is kept from now on [R44].
- The formats are .exe (NSIS), .dmg, AppImage and .deb [D162, code].

**macOS updates.**

- macOS installs are not notarized [D160].
- R50 says Faber updates itself, so macOS uses the same self-update.
- If a hand test on a real Mac shows that an unsigned update cannot install, macOS falls back to D160's notification
  about a new version (AI proposal, AI-2).

## A17. Phases

The phase order Erick approved [R45]:

1. New base.
2. Full Chat.
3. Projects with the lead agent.
4. Excellent software, ending when Faber builds Faber.
5. First public version.
6. Image and design.
7. Phone and routines.
8. Video.
9. PC apps.
10. Mods and reverse engineering.

How the phases relate to the work:

- The order says what gets specialized tools and tests first. Faber does everything from the start [R45, R12].
- Before each version, the benchmark measures it, and the version ships only with Erick's OK [R23].
- Creative results are judged blind, as "A" and "B" [R24].
- Development uses a large team of Haiku agents [R46]. A19 (rule 9) sets their request size, and B0 names the tools
  the team uses.

**Placement where R45 is silent** (AI proposal, AI-1):

| Work | Phase |
| :--- | :--- |
| Benchmark | 2 |
| Visual answers | 2 |
| Asking Faber about Faber | 2 |
| Search | 2 |
| Moving a chat to a project | 2 |
| Closing the engine gaps of A19 rules 2, 3, 5 and 6 | 2 (AI-22) |
| Progress view | 3 |
| Decision list | 3 |
| Catalog | 3 |
| Learning | 3 |
| A19 rule 4 (hand-over) and rule 7 (running budget) | 3 (AI-22) |
| Nightly CLI tests and the live manifest | 4 |

Phase 1 publishes no public release, because R23 requires the benchmark first. Erick checks Phase 1 in the live app,
"Faber (ao vivo)" [D180], and through a local installer build (AI proposal, AI-3).

## A18. Non-goals

These are out of scope:

- Vendors other than Claude Code and Codex: no Gemini, Copilot, Cursor, OpenCode or ACP drivers [R4].
- A path with no subscription, such as local models or Faber holding an API key [R28].
- Team, admin and permission features [R49].
- Driving a chat website or copying its login session [D181]. Using ChatGPT's connectors [D182] was dropped by Erick.
- Shipping any third-party tool, skill, model or MCP server inside the app or the repo [AGENTS, D94].
- Telemetry that is on by default [AGENTS].
- A phone app that runs work. The phone is only a remote control [R14].
- A separate help page. People ask Faber about Faber [R18].

## A19. Rules for driving the CLIs

These are product rules. They come from the 2026-10-11 research [research-cli] and from the model-use rules in
[modelos]. The research marks its own inferences "(inf)"; the part of a rule that is inference is an AI proposal
(AI-22), and each rule is re-measured when a CLI version changes. Each rule ends with what Phase 1 does about it.

**Scope.** Every piece of code that starts a CLI or talks to one follows these rules, in every phase. That includes the
code Phase 1 writes: the sign-in step (B7), the fake CLI (B2) and the installer (B9). Phase 1 does not rebuild the
engine that already drives the CLIs, so today's engine has a short list of **known legacy exceptions**. The table
after rule 9 names each one and the phase that closes it. Until then, an exception is a known defect of the old code,
not a promise that the engine already obeys the rule, and no new code may add to it.

1. **Never use Codex app-server login.** The app-server documentation says its authentication "has never been
   permitted for commercial or hosted services". Faber never calls `account/login/*` and never stores `auth.json`.
   Sign-in happens only through `codex login` or the CLI's own flow.
   - Source: [research-cli A5, D; R9, R28, AGENTS].
   - Phase 1: already followed. B7 step 1 only starts the CLI's own sign-in [code]; Phase 1 adds no login call.
2. **Always pass the permission mode, the full model id and the effort when starting a CLI session.** Since Claude
   Code 2.1.285, `claude -p` can start in auto mode when no mode is set, so an omitted flag is a hidden choice.
   - Source: [research-cli A4, D].
   - Phase 1: not touched. A legacy exception (table below): Projects sessions pass no `--permission-mode`, and a
     session may start on the account's default model or effort [code]. Closed in Phase 2 (AI proposal, AI-22).
3. **Set the model and the effort before a conversation starts. Never change model, effort, tools or system prompt in
   the middle of a conversation without need.** The prompt cache is the biggest saving (2.7 to 5.3 times). What a
   change costs depends on what changes [Claude Code docs, prompt caching]:
   - A change of **model** always starts a new cache, because each model has its own.
   - A change of the **tools** or the **system prompt** invalidates everything after it.
   - A change of **effort** keeps the cache on Opus 5.5, Sonnet 5.5, Haiku 5.5 and Fable 5.1 when Claude Code signs in
     with an API key or a Claude subscription, which is how Faber uses it. It does not on other models, on Amazon
     Bedrock, on Google Cloud's Agent Platform, on a Claude apps gateway, when `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`
     is set, or under a HIPAA configuration. On the API, changing effort without breaking the cache needs a beta header
     [modelos]. For Codex the research gives no such exception.

   So Faber keeps R76's rule, never change effort without need, and does not count on the exception: it holds on
   today's setup, but a CLI update or another sign-in path can remove it. Changing facts travel as text inside the next
   message, never as a system-prompt edit.
   - Source: [modelos rules 4, 5; research-cli A1; R76; Claude Code docs, prompt caching: "Changing effort level"].
   - Phase 1: not touched. Auto's move up (A5) is the one existing case; Phase 2 checks how it hands over.
4. **Hand work to another model through a written summary or a diff, not the conversation.** Thinking blocks are tied
   to the model that wrote them. A side call with its own prompt would also bill the whole conversation uncached, so
   Faber uses the CLI's own compaction instead.
   - Source: [modelos rule 9; research-cli A1].
   - Phase 1: not touched. It applies to the lead agent's workers in Phase 3.
5. **Interrupt gracefully before killing, watch for silence, and kill the whole process tree on Windows.** A bare kill
   leaves the turn unfinished with no result. `claude -p` can hang for about 600 s on Windows. A Rust parent cannot
   send SIGINT on Windows, so the stop uses the CLI's control-protocol interrupt, waits for the result, then kills the
   tree.
   - Source: [research-cli A4 (Stopping, Windows hang), A5 (process build-up)].
   - Phase 1: only the test runner kills a whole tree (B2, using the rule from commit 93e9040). The engine's
     watchdog is a legacy exception, closed in Phase 2.
6. **Read the CLI's start-up capabilities, the result subtype and the terminal reason, and map them to `reason.*`
   keys.** Faber detects features from the `capabilities` list in `system/init`, never by comparing version strings,
   and never shows a raw reason string.
   - Source: [research-cli A4; AGENTS].
   - Phase 1: not touched. A legacy exception, closed in Phase 2. B5 adds only the `error.permission.*` and
     `error.settings.*` keys it needs.
7. **Keep Faber's own running budget, and apply R37 when it is passed.** The CLI's budget flag is a client-side
   estimate; it can overshoot and ignores totals restored by a resumed session. So Faber counts spend per turn itself.
   What it does with the count is Erick's R37, and nothing more:
   - Passing the estimate by 50 percent only warns and changes to economy mode: cheaper models for support work and
     extras put off. Faber never kills a process because of the budget.
   - "Stop" only pauses, and the work resumes from where it stopped.
   - When a quota runs out, Faber moves to the other subscription, or pauses and resumes by itself at renewal.
   - Source: [research-cli A4, D; R36, R37].
   - Phase 1: not touched. A legacy exception, closed in Phase 3 with the big-work rules (A6).
8. **No SDK prewarm.** A spare process waiting for a folder costs about 230 to 260 MB, which breaks the idle budget.
   - Source: [research-cli A4; AGENTS, A12].
   - Phase 1: followed. Phase 1 adds no spare process, and B10 measures the app with no CLI running.
9. **Keep each Haiku request under 100K tokens and each GPT-6 request under 272K.** Above those points the whole
   request is billed more (5 times for Haiku; 2 times on input and 1.5 times on output for GPT-6).
   - Source: [modelos rule 6; R46, R76].
   - Phase 1: applies to the build team now (B0).

**Known legacy exceptions in today's engine** [code]. Rules 1, 8 and 9 hold now.

| Rule | What the engine does today | Closed in |
| :--- | :--- | :--- |
| 2 | Chat sessions pass `--permission-mode`. Projects sessions pass none. A session may start on the account's default model or effort | Phase 2 |
| 3 | Auto's move up changes the model inside a conversation | Phase 2 (check the hand-over) |
| 4 | There is no lead agent yet, so there is no hand-over | Phase 3 |
| 5 | Interrupt sends the CLI's control-protocol interrupt, but nothing waits for the result, nothing watches for silence, and `proc::stop` kills the child alone, not its tree | Phase 2 |
| 6 | `terminal_reason` or `subtype` is passed on as a raw string, and `capabilities` from `system/init` is not read | Phase 2 |
| 7 | The engine keeps no running budget | Phase 3 |

---

# Part B: Phase 1, the new base

## B0. Scope and order

Phase 1 is the first step of R45 ("new base") [R45]. It contains the redesign pieces that every later phase builds
on. It adds no new AI capability. The base app that exists today keeps working throughout [estado]: the engine, the
Tauri shell, Chat, Projects, Auto, 5 locales and the updater.

| # | Piece | Why it belongs in Phase 1 |
| :--- | :--- | :--- |
| B1 | License and product documents | R25 is replaced by FSL-1.1-ALv2 [R79]. Agents build from these documents [AGENTS]. |
| B2 | Rust test runner | R29 replaces the Node suite "from now on". Every later piece is tested with it. |
| B3 | Design system and themes in the real UI | Every screen depends on it [R55–R60, notes]. |
| B4 | Identity: icon, mark and wordmark | [R66–R71, R75] |
| B5 | Shell layout and running list | [mock, R53, R54] |
| B6 | Tray number and notifications | [R74] |
| B7 | First run and tutorial | [R18, R59, mock s6] |
| B8 | Settings | [mock s5, D141] |
| B9 | CLI download written straight to disk | Today it holds about 200 MB in memory [estado, A12]. |

Build the pieces in this order, on the local branch `phase-1-base` (AI-24). Make one commit per coherent change, after
all proof commands pass [AGENTS]. A piece may take more than one commit. The contents and the order are an AI proposal
(AI-4).

**Commit order.** Each commit passes every proof command on its own. The last column says why that holds.

| # | Commit | Why it passes alone |
| :-- | :--- | :--- |
| 1 | B1: license | Metadata and one document line each; no code. Swapping the identifier, or reverting the license, is this one commit. |
| 2 | B1: documents | Documents only. |
| 3 | B2: test runner | Ports every current test one to one, the appearance scenario unchanged, then deletes the Node suite. The new proof commands replace the old ones in the same commit. |
| 4 | B3: looks | The saved appearance changes shape on both sides at once, so the shell, the screen's two callers (`App.tsx`, `Settings.tsx`) and the appearance scenario move together. Its checks use only what the screen shows today: the old empty state, not the greeting. |
| 5 | B4: identity | Static assets, `Mark.tsx` and the titlebar; no logic. |
| 6 | B5a: running list, engine and shell | Additive: new fields and a new event that the old screen ignores. `answer` gains a `turnId` and returns a result, so its one old caller (`Thread.tsx`, `App.tsx`, `api.ts`) is ported in the same commit. |
| 7 | B5b: layout and running column, screen | Uses B5a. It also renders the greeting, so the greeting's checks live here. |
| 8 | B6: tray and notifications | Uses B5a's `Running` event. |
| 9 | B7: first run and tutorial | Uses B3's `preview_look` and error-carrying `save_settings`, and B5b's run button. |
| 10 | B8: settings | Replaces B3's simple Appearance section; uses B7's account-management mode. |
| 11 | B9: CLI download to disk | Independent of the screen. |

**How Phase 1 is built: the build team's tools.**

- Claude and Codex build the pieces together, and whoever writes a part never reviews it [R26]. The large Haiku team
  keeps every request under 100K tokens [R46, R76; A19 rule 9].
- For each piece that changes a screen (B3, B4, B5b, B7, B8), the team also uses three UI quality tools. They are
  installed on the maintainer's PC [research-tools §4]:
  - `redesign-existing-projects` audits the current screen before the piece changes it. The audit is the starting list
    for the piece (AI proposal, AI-23).
  - `design-taste-frontend` (taste-skill) is the style guide the team reads while writing markup and styles [R56,
    R76]. Its README excludes dashboards, so only its brief-reading and anti-slop rules apply to Faber's tool-like
    screens [research-tools §4].
  - `impeccable` is the UI judge. After the piece is built, its critique and detection passes run on the changed
    screens, and every finding is fixed or written down [R56]. The team checks the installed engine version first,
    because TL-ADE records two different versions [research-tools, notes].
- These tools are used during development only. They never ship in the app or the repo, and the app never calls them
  [AGENTS]. Their output is advice. The proof commands, the screen tests and Erick's own look at the live app decide
  whether a piece is done.

**Rules for every piece.**

- **Text.** Every new text goes into all five `ui/locales/*.ts` files, written by hand [AGENTS].
  - English (`en.ts`) is the source.
  - The pt-BR wording comes from the mock wherever the mock has it [mock].
- **Errors.** The engine and the shell return keys (`error.*`, `reason.*`), never sentences [AGENTS].
- **Processes.** Child processes never go through a shell [AGENTS].
- **Accessibility** [notes]:
  - Choice buttons use `aria-pressed`, or native radio groups.
  - Switches are native checkboxes.
  - "Open file" is a real button.
  - Theme thumbnails are `aria-hidden`.
  - Each theme card is named by its name and its description.
  - Areas whose content can grow scroll; they never cut content off.
  - Every overlay (the first-run steps, the tutorial cards, the narrow-window sidebar, the overlay running column)
    says whether it is modal. A modal overlay moves focus into itself when it opens and gives focus back when it
    closes. Esc closes it, except where this document says otherwise (the first-run steps). The one non-modal case is
    the running column when it opens by itself: R53 says it does not move the cursor, so it moves no focus at any
    width (B5).
- **Motion.** No arrival or transition lasts longer than 400 ms [notes].

## B1. License and product documents

### What changes

This piece is two commits. The license goes first and alone [R79], so it can be reverted in one step.

**Commit 1, license** [R79].

- Replace `LICENSE` with the FSL-1.1-ALv2 text, keeping the copyright line, and add `NOTICE`.
- Set the license field to `FSL-1.1-ALv2` in:
  - `package.json` (`license`);
  - `package-lock.json` (the `license` of the root package entry, which `npm` keeps in step with `package.json`);
  - `Cargo.toml` (`[workspace.package] license`).
- In `README.md`, change the license line to `[FSL-1.1-ALv2](LICENSE)`.
- In `AGENTS.md`, change the license wording to FSL-1.1-ALv2 and source-available.
- The commit stays on `phase-1-base`, unmerged and unpushed until Erick approves (AI-24).

**Commit 2, documents.**

**`README.md`.**

- One-paragraph description from A1.
- The two areas.
- Only Claude Code and Codex.
- Build-from-source steps with today's proof commands. B2 replaces them.
- Remove the old any-plan and other-vendor claims [R4]. The license line is commit 1's.

**`docs/roadmap.md`.**

- Rewrite it from Part A of this document.
- Keep the table of the 12 powers (the roadmap's "Powers, not tracks" table) as it is, and add rows 13 to 15 from R48.
  Part A names the 12 but does not repeat the table, so this is where it must survive [R47, R48].
- Remove these sections, which R4 and R28 overrule:
  - "Works with the plans you have";
  - the Gemini, ACP and OpenCode drivers;
  - "Provider terms", for vendors other than the two;
  - the "no subscription" path.
- Replace "Faber's own test system" with B2.
- Replace the phase list F0–F12 with R45.

**`docs/design.md`.**

- Rewrite it from A13, A14 and B3–B5, as the direction the following pieces build.
- Remove the Mono, Studio and Cockpit theme and the Appearance table [D142, D139, D140].
- Remove the Playwright end-to-end section only when B2 replaces it.

Each later piece updates the documents for its own change, in the same commit:

- B2 updates the test lines in `AGENTS.md` and `README.md` and adds the "Screen tests" section to `docs/design.md`.
- B4 adds the "Brand" section to `docs/design.md`.

### Data flow and error handling

None. This piece changes documents and license metadata only.

### Tests

- **Checks.** Behaviour does not change. All current proof commands must still pass [AGENTS], after each of the two
  commits.
- **Commits.** Two: the license, then the documents. A reviewer can check that commit 1 touches no file outside the
  list below.

### Files touched

- **Commit 1, license:**
  - `LICENSE`;
  - `package.json`;
  - `package-lock.json`;
  - `Cargo.toml`;
  - `README.md` (the license line only);
  - `NOTICE` (new);
  - `AGENTS.md` (the license wording only).
- **Commit 2, documents:**
  - `README.md`;
  - `docs/roadmap.md`;
  - `docs/design.md`.

## B2. Faber's own test runner, in Rust

### Requirement

**The decision.** One Rust program replaces the whole current suite: no Node, Vitest or Playwright in tests [R29].
The reason is to avoid the CPU and RAM load the TL-ADE suite caused [R29].

**How the runner stays light** [R30]:

- it is one program;
- it compiles with half the cores;
- locally, it runs only the affected tests;
- the full set runs on GitHub;
- screen tests run inside a hidden Faber, opened only when needed.

**Other rules.**

- Unit tests never spawn processes, touch real git or call a real CLI [AGENTS, D167].
- Fake CLIs replay recorded conversations [D165].

Node stays a development tool for Vite, `tsc` and `oxlint`. It never runs tests and never ships [R31].

### Components

The test runner's design is an AI proposal (AI-5).

**A new workspace crate, `testkit/`** (package `faber-testkit`, `publish = false`). Add it to the workspace
`members`.

- **Binary `faber-test`: the runner.**
  - **Suites.** It knows three suites:

    | Suite | What it runs | Platforms |
    | :--- | :--- | :--- |
    | `rust` | `cargo test --workspace` as a child process, with `-j` set to half the logical cores (at least 1) and `--test-threads` set to the same number | Every OS |
    | `ui` | The TypeScript checks in `tests/ui/`, run inside a hidden test build of Faber that reports the results to the runner itself, with no DevTools port | Every OS (on Linux under `xvfb-run -a`) |
    | `screen` | The Rust scenarios in `testkit/src/screen/`, driving a hidden test build of Faber through the DevTools port | Windows only |

    On other systems, `screen` prints one line saying it needs WebView2's DevTools port and is skipped. This is the
    same platform limit as today's end-to-end test [code]. `ui` is not skipped anywhere. Today's Node checks
    (`models`, `i18n`, `setup`) run on all three systems in CI, and R44 asks for macOS and Linux compatibility from now
    on, so a check that ran everywhere must keep running everywhere [code, R44].
  - **Affected suites.** The runner maps the changed paths (staged, unstaged and untracked, from `git status
    --porcelain`) to suites by path prefix:

    | Changed path | Suites |
    | :--- | :--- |
    | `engine/` | `rust`, `screen` |
    | `src-tauri/`, `Cargo.toml`, `Cargo.lock`, `.cargo/` | `rust`, `ui`, `screen` |
    | `ui/`, `vite.config.ts`, `tsconfig.json`, `package.json`, `package-lock.json` | `ui`, `screen` |
    | `tests/ui/` | `ui` |
    | `tests/fixtures/` | `rust`, `screen` |
    | `testkit/` | all suites |
    | `docs/`, any `*.md` at any depth | none |
    | Anything else | all suites |

    With no changes, the runner says so and runs nothing. `--all` runs every suite. `--suite <name>` runs one.
    `--filter <text>` keeps only the tests whose names contain the text.
  - **Building the test app.** The `ui` and `screen` suites share one build per run:
    - The runner builds the UI with Vite in `--mode test` into `dist-test/`.
    - It compiles the app with the Tauri CLI: `build --debug --no-bundle --features test-hooks --config
      src-tauri/tauri.test.conf.json`, into `target/faber-test/`, with half the cores passed to `cargo`.
    - `src-tauri/tauri.test.conf.json` is a test-only override. It sets `build.frontendDist` to `../dist-test`, sets
      `build.beforeBuildCommand` and `build.beforeDevCommand` to `null`, and sets `bundle.active` to `false`. The
      committed `tauri.conf.json` has `beforeBuildCommand: "npm run build"` [code]. Tauri runs that hook through a shell
      and it writes `dist/`, whatever way the Tauri CLI itself was started. The override removes the hook, so the test
      build never goes through `npm`, a `.cmd` shim or a shell [AGENTS].
    - The runner starts `node` directly on the Vite and Tauri CLI entry scripts in `node_modules`.
    - The test bundle never reaches `dist/` or an installer. The runner records the modification time of `dist/`
      before the build and fails the run if it changed.
    - The Cargo feature `test-hooks` (off by default and never in a release build) adds one shell command,
      `report_checks`, and lets the window open the checks page when `FABER_CHECKS_OUT` is set (UI checks, below).
  - **Output.**
    - One line per test: pass or fail, with the time.
    - A summary at the end.
    - Exit code 1 on any failure.
    - Screenshots go to `target/faber-test/shots/<test>.png`: on each failure, and at the checkpoints a scenario
      names.
- **Binary `fake-cli`.**
  - It replaces `tests/support/fake-cli.ts`, `tests/support/replay.ts` and `tests/fixtures/setup/launcher.cs`.
  - It replays a recording from `tests/fixtures/`, chosen by `FABER_FAKE_RECORDING`, and keeps its state in
    `FABER_FAKE_STATE_DIR`, with the same behaviour as the Node fake [code].
  - Because it is a real `.exe`, the setup scenarios no longer compile a C# launcher.
- **Module `cdp`.** A small DevTools client.
  - It reads the target list over HTTP on `127.0.0.1:<FABER_CDP_PORT>`.
  - It talks over a WebSocket, using the `tungstenite` crate.
  - Operations:
    - evaluate a script;
    - wait for a condition, polling every 100 ms with a deadline (15 s by default) and failing with the condition's
      own message;
    - click;
    - type text;
    - press keys;
    - take a screenshot.
- **Module `app`.** Launches the test build in a sandbox. It has a part that runs on every OS and a part that runs on
  Windows only (listed at its end).
  - Every launch uses the full isolation contract of today's setup test [code: `tests/e2e/setup.e2e.ts`], not only
    the setup scenarios. Today's app scenarios inherit the real environment; this closes that gap. The engine looks for
    a CLI in `PATH`, then in `%USERPROFILE%\.local\bin` and `%APPDATA%\npm` [code: `engine/src/cli.rs`], so every one of
    those must point inside the sandbox.
  - It builds the child environment from the parent's:
    - **Removed:** every variable whose name starts (any case) with `FABER_`, `CLAUDE`, `CODEX`, `OPENAI`,
      `ANTHROPIC`, `GEMINI`, `GOOGLE_API`, `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` or `NO_PROXY`; and the Windows
      spelling `Path`.
    - **Pointed into a sandbox folder:** `HOME` and `USERPROFILE` (a sandbox profile), `APPDATA` and `LOCALAPPDATA`
      (inside that profile), `TEMP` and `TMP`, `CODEX_HOME` (a fake folder in the profile),
      `WEBVIEW2_USER_DATA_FOLDER`, and `PATH` (an empty sandbox folder, so no real CLI is on it). On macOS and Linux
      the same rule also covers `TMPDIR` and `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` and
      `XDG_STATE_HOME`, all inside the profile. `DISPLAY` is kept, so the hidden window has a screen.
    - **Set:** `FABER_HIDDEN=1`, `FABER_CDP_PORT`, `FABER_DATA_DIR`, `FABER_SETUP_SOURCE`, `FABER_FAKE_RECORDING` and
      `FABER_FAKE_STATE_DIR`; and a dead proxy (`HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` on 127.0.0.1:1, with
      `NO_PROXY` for loopback only).
  - `FABER_CLI_CLAUDE` and `FABER_CLI_CODEX` are always chosen by the scenario: a launch config for `fake-cli`, or the
    empty string, which turns discovery off. Only the install scenarios leave `FABER_CLI_CODEX` absent, because they
    check that a CLI installed into the data folder is found. There, discovery can reach only the sandbox. On macOS
    and Linux the engine also looks in `/opt/homebrew/bin` and `/usr/local/bin` [code: `Cli::find_in`], which no
    sandbox can redirect. The `ui` suite therefore always sets both variables to the empty string, and the install
    scenarios are `screen` scenarios, which run on Windows only.
  - **Audit.** After every scenario the runner checks, as today's test does [code: `audit()`, R5.5]:
    - no process born from the app, or from a recorded fake-CLI command, is still alive;
    - every recorded CLI command ran with its working directory and write targets inside the sandbox;
    - a canary folder outside the sandbox is unchanged, and no file appeared in its enclosing folders;
    - the real process environment and the real user `PATH` are unchanged;
    - every download used only the loopback source.
  - **Every OS: the process tree and the cleanup** (AI-25). The runner lists processes as rows of `(id, parent id,
    born)`, where `born` is a process's start time at the best precision the system gives.
    - **Windows** (where ids are reused fast, which is why commit 93e9040 exists): `born` is the creation time of
      `Win32_Process.CreationDate`, read through the `wmi` crate, which has microsecond precision. Today's test uses the
      same value [code: `tests/e2e/setup.e2e.ts`]. The `sysinfo` start time is not used there, because it is a whole
      number of seconds [sysinfo docs] and a reused id inside one second would pass for the old process.
    - **macOS and Linux:** `born` is the `sysinfo` start time, in whole seconds, and the row also keeps the process
      name. These systems hand out ids in sequence, so reusing one inside the same second needs the whole id space to
      wrap in that second. That is the ceiling of this check.
    - It walks the app's process tree with the rule from commit 93e9040: a child counts only if it was born at or after
      its parent.
    - **Cleanup.** The app that the runner started is killed through its own `std::process::Child` handle, which holds
      the process open so its id cannot be reused meanwhile. The rest of the tree is killed by id, but only after the
      runner reads the process's `born` again at that moment and finds it equal to the recorded one [estado]. A
      mismatch means the id was reused, and that process is left alone. The check and the kill are not one atomic step;
      the gap is a few milliseconds.
    - The audit's "no process is still alive" check uses the same identity.
    - The walk and the identity comparison are pure functions over rows, so they are tested without processes.
  - **Windows only: memory.** It reads the private working set of each process from
    `Win32_PerfRawData_PerfProc_Process`, through the `wmi` crate. These are the same numbers Task Manager and today's
    test use [code]. Only the memory scenarios of `screen` (B10) read them; the `ui` suite measures no memory. The
    same crate also supplies the Windows creation times for process identity (above). The `wmi` crate is declared under
    `[target.'cfg(windows)'.dependencies]` of `faber-testkit` and its code sits under `#[cfg(windows)]`, so the crate
    builds and the `ui` suite launches on macOS and Linux without it.
- **Module `server`.** A minimal loopback HTTP server on `std::net`. It serves the setup scenarios' release archive,
  as today's test does [code].
- **Alias `cargo t`.** `.cargo/config.toml` gets an alias, `t`, that runs `faber-test` from `faber-testkit`.

**UI checks: `tests/ui/`.**

- `check.ts` holds a tiny registry: `check(name, fn)` plus equality and truth assertions.
- `index.ts` imports every `*.check.ts` file and exposes `window.__faberChecks`, which runs them all and returns
  `{name, ok, message}` for each.
- **Reporting without a DevTools port.** When the shell was started with `FABER_CHECKS_OUT`, a `test-hooks` build opens
  the page with `?checks` in its address. In test mode, `index.ts` sees that, runs every check at load and sends the
  JSON result to `report_checks`. That command writes it to the file named by `FABER_CHECKS_OUT` and exits the app. The
  runner waits for the file (30 s deadline) and prints one line per check. This path uses no DevTools port, so it works
  on Windows, macOS and Linux.
- In test mode, `index.ts` also exposes `window.__faberText(key, vars)`, which returns the current translation. Screen
  scenarios find elements by role and translated name through it, never by text written into Rust.
- `ui/main.tsx` imports `../tests/ui/index.ts` only when `import.meta.env.MODE === 'test'`. Production builds drop it.
  A unit test checks this (Tests, below).

### Data flow

1. `cargo t` starts `faber-test`, which reads the changed paths and picks the suites.
2. The `rust` suite runs `cargo test` with half the cores.
3. If `ui` or `screen` is picked, the runner builds the test app once.
4. For the `ui` suite, it launches the app hidden in checks mode and reads the report file the app writes before it
   exits. For the `screen` suite, it launches the app hidden with the DevTools port and runs each scenario. Scenarios
   that need a fresh data folder or a different environment relaunch the app.
5. The runner audits each scenario, kills the process tree it launched, prints the summary and exits.

### Ported tests

Port the existing tests one to one before adding new ones, so nothing is lost [code]:

| Today | Becomes |
| :--- | :--- |
| `tests/i18n.test.ts` | `tests/ui/i18n.check.ts` |
| `tests/models.test.ts` (2 tests) | `tests/ui/models.check.ts` |
| `tests/setup.test.ts` (C4.1–C4.6) | `tests/ui/setup.check.ts` |
| `tests/e2e/app.e2e.ts` (9 tests, including the two memory tests) | `testkit/src/screen/app.rs` |
| `tests/e2e/setup.e2e.ts` (C5.1–C5.5, R5.5, and the "existing recordings still replay unchanged" test) | `testkit/src/screen/setup.rs`; the replay test becomes a `rust` suite test of `fake-cli` |

The "appearance changes apply at once" scenario is ported unchanged here, against today's appearance screen. B3
rewrites it in the same commit that replaces the appearance model, so B2 never asks for a screen that does not exist
yet.

After the port, delete the following:

- `tests/*.test.ts`;
- `tests/e2e/`;
- `tests/support/`;
- `tests/fixtures/setup/launcher.cs`;
- the `test` and `test:e2e` scripts and the `playwright-core` dependency in `package.json`.

Change `lint` to `oxlint ui tests/ui`.

### New proof commands

All must exit 0 before any commit [AGENTS]:

```bash
npm run lint
npm run typecheck
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo t
```

**CI.** In `.github/workflows/ci.yml`, replace `npm test` and `npm run test:e2e` with `cargo t --all`.

- All three OS run the `rust` and `ui` suites. On Linux the webview libraries step also installs `xvfb`, and the
  command runs under `xvfb-run -a`, so the hidden window has a display.
- Windows also runs `screen`, and uploads `target/faber-test/shots/`.
- **The three-system run is a gate for merging, not for finishing B2.** CI runs only for pushes to `main` and for pull
  requests [code: `.github/workflows/ci.yml`], and `phase-1-base` is not pushed until Erick approves (AI-24). So B2 is
  done on local Windows proof: every proof command passes on the maintainer's PC, including `cargo t --all` with the
  `rust`, `ui` and `screen` suites. The `ui` suite on macOS and Linux, and the `xvfb-run` step, stay unproven until
  the first push. After Erick approves that push, the branch becomes a pull request, and it is merged only when
  `cargo t --all` has passed on Windows, Linux and macOS. If a runner cannot host the hidden window, that is fixed
  before the merge and not skipped, because R44 asks for compatibility now.

### Error handling

- **Build failure.** A failed build stops the run with the build's own output and exit code 1. The suites that need
  the build are reported as "not run", never as passed.
- **Startup failure.** If the hidden app exits early, or its DevTools port does not answer within 30 s (`screen`), or
  its checks report does not appear within 30 s (`ui`), the run fails. The message carries the app's stderr.
- **Cleanup.** The runner always kills the process tree it launched, even after a failure or Ctrl+C. It removes its
  temporary folders afterwards.
- **Audit failure.** A scenario whose audit finds a leaked process, an outside write or a changed environment fails,
  even if its own assertions passed.

### Tests of the runner itself

These are `rust` suite tests inside `faber-testkit`:

- The path-to-suite map, using the table above.
- The half-cores calculation, including 1 core giving 1.
- The process-tree rule for reused ids, as a pure function over rows of `(id, parent id, born)`; the same test runs on
  every OS.
- Identity under id reuse: two rows with the same id and the same whole second but a `born` that differs by 5 ms (the
  Windows precision) are different processes, so the cleanup skips the second one. The test also checks that a row
  whose `born` equals the recorded one is the same process. For macOS and Linux rows, an equal id with a different
  second, or with a different name, is a different process.
- `fake-cli` replays every file in `tests/fixtures/` unchanged.
- The production `dist/` contains no `__faberChecks`. This test runs only when `dist/` exists.
- The sandbox environment builder, given a parent environment that holds `USERPROFILE`, `Path`, `CODEX_HOME`,
  `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` and `HTTPS_PROXY`: the child gets none of the real values, every profile
  variable points inside the sandbox, and `PATH` is the empty sandbox folder.
- The audit rejects a write outside the sandbox, as today's R5.5 test does [code]. It is portable, so it runs on every
  OS, and the `ui` suite's launch goes through it in CI on all three systems.
- `src-tauri/tauri.test.conf.json` sets `frontendDist` to `../dist-test`, has both hooks `null` and `bundle.active`
  `false`; and `test-hooks` is not in the `default` features of `src-tauri/Cargo.toml`.

### Files touched

- **New:**
  - `testkit/`;
  - `.cargo/config.toml`;
  - `src-tauri/tauri.test.conf.json`;
  - `tests/ui/` (`check.ts`, `index.ts`, `*.check.ts`).
- **Changed:**
  - `Cargo.toml` (workspace members);
  - `src-tauri/Cargo.toml` (the `test-hooks` feature);
  - `src-tauri/src/main.rs` (`report_checks` and the checks page, both only under `test-hooks`);
  - `package.json`;
  - `ui/main.tsx`;
  - `.github/workflows/ci.yml`;
  - `.gitignore` (adds `dist-test/`);
  - `AGENTS.md` (the `tests/` bullet, the proof-command list, and `npm run test:e2e` replaced by `cargo t`);
  - `README.md` (the proof commands);
  - `docs/design.md` (a "Screen tests" section replaces the Playwright one).
- **Deleted:** the files listed under "Ported tests".

## B3. Design system and themes in the real UI

### Requirement

- **Structure.** One structure for every screen. Themes and presets are token overrides on the Sala base
  [notes, tokens].
- **Themes and presets.** Three themes, Sala escura (the default), Oficina and Bancada, plus the presets Manhã, Alto
  contraste and Cobalto [R59, R60].
- **Validator and theme file.** Everything that sets a colour goes through one colour validator [notes]. The theme file
  stores choices, not CSS [notes].
- **Motion and effects.** Three motion levels and three effect switches [notes].

### Components

**`src-tauri/src/themes.json`: the single source of tokens** (AI proposal, AI-6).

- It is copied from `faber-notas/design-final/tokens.json` and keeps these sections:
  - `css` (the base, themes and presets maps);
  - `locked`;
  - `user-controls`;
  - `validator`;
  - `theme-file`;
  - the contrast table, regenerated for the twenty-six required pairs below (the tokens file lists a different set of
    nine, so its table is not copied as it is).
- Short hex colours in the tokens file, such as Alto's `#fff`, are written out as `#rrggbb` when copied.
- The Portuguese display names and descriptions move to the locales.
  - Theme and preset names are translated like every other text (AI proposal, AI-8). In English they are Dark Room,
    Workshop, Workbench, Morning, High contrast and Cobalt. pt-BR keeps R60's names.
  - The pt-BR descriptions are the ones in the tokens file [tokens].
- The keys of the theme file become English (AI-7).
- The `brand-*` tokens are deleted, because the titlebar shows the lockup instead (B4).
- The UI imports the `user-controls` section of the same file, through Vite's JSON import, for the lists of options:
  swatches, grounds, fonts, sizes. The UI never computes colours itself; only `look.rs` does.

**`src-tauri/src/look.rs`: the look engine in the shell** (AI-6).

- **Storage.** The engine keeps the saved appearance as an opaque map, as it does today [code]: "Appearance belongs
  to the screen, so the engine only keeps it." Every theme decision lives in the shell, so it is tested by
  `cargo test` with no window.
- **The appearance object.** The saved appearance and the theme file share one shape (AI-7):

  | Field | Allowed values |
  | :--- | :--- |
  | `base` | `sala`, `oficina`, `bancada`, `manha`, `alto`, `cobalto` |
  | `ground` | `sala`, `oficina`, `bancada`, `cobalto`, `manha`, for Black, Warm graphite, Olive, Navy and Light paper [tokens] |
  | `accent` | `#rrggbb` |
  | `textFont` | `archivo`, `plex`, `atkinson` |
  | `headingFont` | `narrow`, `book`, `poster`, `text` |
  | `textSize` | 13 to 18, in steps of 0.5 |
  | `density` | 1 or 0.8 |
  | `corners` | 0, 1 or 1.6 |
  | `motion` | `full`, `reduced`, `none` |
  | `effects` | `{ambient, texture, glow}`, each a boolean |
  | `colors` | Only `frame`, `bg`, `s1`, `s2`, `s3`, `ink`, `ink-2`, `ink-3` as `#rrggbb`, and `line`, `line-2`, `line-3` as `#rrggbb` or `rgba()` [tokens] |

  - `base` is required. Every other field is optional and falls back to the base's own value.
  - The theme file adds three fields: `format: "faber-theme"`, `version: 1`, and `name` (text of 1 to 80
    characters).
  - The file extension is `.faber-theme.json` (AI-7).
- **`look(appearance) -> Look`.** It computes the look in this order:
  1. Start from the base map. Apply the overrides of `base`. Apply its default motion; for example, Alto starts in
     reduced motion [tokens].
  2. If `ground` is set, take that ground's frame, surfaces, lines and inks, and its meaning colours [tokens].
  3. Apply `colors`. If they change `bg`:
     - The meaning colours come from Sala when white text reads better on the new `bg`, and from Manhã when black
       text reads better (AI-6).
     - Every text ink is pushed toward white or black in 5% steps until it passes [tokens]. It stops at pure white or
       pure black. A pair that still fails is then reported, and it blocks the look (see the required pairs).
  4. If `accent` is set, run it through the validator (below). A factory look has no `accent` of its own to check: it
     keeps the `accent`, `accent-ink` and `accent-text` its base supplies, for example Sala's `#0e0e10` on `#f3f0e8`
     (16.93:1), which the required pairs below still check (AI-6).
  5. Map the fonts:
     - `textFont` selects Archivo, IBM Plex Sans or Atkinson Hyperlegible [tokens].
     - `headingFont` takes the display tokens of the theme that owns that style [tokens]: `narrow` from Sala (Archivo
       at 80% width, weight 380), `book` from Oficina (Newsreader 300 with italic emphasis) and `poster` from
       Bancada (Barlow Condensed 500). `text` uses the text font.
  6. Set `textSize`, which drives `--fs-ui`. Every interface size is `--fs-ui` plus or minus a few pixels, and reading
     text is `--fs-ui` + 1.5 px [tokens].
  7. Set `density` (`--dens`), and multiply every radius token by `corners`.
  8. Return:
     - the CSS variables;
     - the motion level;
     - the effect switches;
     - the twenty-six contrast checks, each with its pair, ratio, minimum and pass or fail;
     - any notices.
- **The validator** [notes, tokens]. Swatches, the free colour picker and theme files all use one function:
  - **Hue block.** The hue is the HSL hue, in degrees around the circle. It refuses a colour whose hue is less than
    24° from amber (40), Claude coral (14) or Codex violet (245), measured the short way round, so 350° is 24° from
    14° through 0°. Exactly 24° passes. The bands of Claude (350° to 38°) and amber (16° to 64°) overlap and join, so
    the allowed hues are 64° to 221° and 269° to 350°, ends included, and everything else is refused. A hue 25° from
    Claude on the amber side (39°) is therefore refused, and one 25° from it on the other side (349°) passes. Error
    keys: `error.look.nearAmber`, `error.look.nearClaude`, `error.look.nearCodex`. When a hue is inside more than one
    band, the nearest of the three names the sentence (a tie goes to amber, then Claude, then Codex).
  - **Neutral exemption.** A colour whose three RGB channels are within 16 of each other has no hue and is exempt. So
    #f3f0e8 passes.
  - **Contrast fix.** Below 3:1 on `bg`, the accent is mixed toward white (dark ground) or black (light ground) in 5%
    steps until it passes. The text version (`accent-text`) needs 4.5:1. The look then carries the notice
    `look.accentAdjusted`.
  - **Re-check.** The fixed colour goes through the hue block again. A new collision is refused.
  - **Accent ink.** For an accent the user picks (a swatch, the picker or a theme file), `accent-ink` is pure black or
    pure white, whichever reads better. The factory looks keep their own inks (step 4 above).
- **The twenty-six required pairs** [tokens, extended: AI-6]. The tokens file lists nine pairs. But its CSS also puts
  text on the other surfaces. It puts `ink` on `s3` for the selected sidebar row and hovered buttons, and it puts
  `amber-ink` on `s3` for the amber tag inside a selected row [mock]. A theme file that set `s3` equal to `ink` would
  hide that text while all nine passed. The look checks every pair the screens use, so each ink is checked on every
  surface where it can appear:
  - `ink`, `ink-2` and `ink-3` on each of `bg`, `s1`, `s2` and `s3` (twelve pairs): 4.5;
  - `amber-ink`, `claude-ink` and `codex-ink` on each of `bg`, `s1`, `s2` and `s3` (twelve pairs): 4.5;
  - `accent` on `bg`: 3;
  - `accent-ink` on `accent`: 4.5.

  All six factory looks pass all twenty-six. The lowest ratio is `ink-3` on `s3`, at 4.59 in Sala, and the lowest
  meaning-ink ratio is `claude-ink` on `s3`, at 5.39 in Cobalto (checked on the token values when this spec was
  revised). The twenty-six checks also stop a counterexample: with all three plain inks white and `s3` set to
  `#666666`, the plain inks pass (5.74), but Sala's `amber-ink` on that surface is only 3.68, so the look is refused. Any failing pair blocks applying the look. The preview names the pair
  (`error.look.pairFailed`).
- **`parse_theme_file(bytes)`** [tokens, notes].
  - It rejects files over 20,000 bytes (`error.look.fileTooBig`).
  - It rejects a wrong `format` or `version`, any unknown key, any value of the wrong type or outside the allowed
    list, and any value containing `var(`. These give `error.look.fileInvalid`, with the field name as technical
    detail.
  - It drops `amber*`, `claude*` and `codex*` keys from `colors` without failing.
  - A refused accent refuses the file, with the validator's sentence.
- **`write_theme_file(appearance, name)`.** It produces the file, with keys in a stable order.
- **Legacy settings.** The stored appearance is parsed with the same rules, minus the header fields. If the stored
  value fails, for example an old Mono, Studio or Cockpit map [code], the shell uses the default look (Sala). It
  treats the appearance as absent, so the first run asks for it once (B7) (AI-6).

**Shell commands** in `src-tauri/src/main.rs`.

- **`state`.** `FullState` gains `look`, the computed look of the saved appearance.
- **`preview_look(appearance) -> Look`.** It computes the look without saving it.
- **`save_settings`.** When the patch has `appearance`, the shell validates it with `look` first. It rejects a
  failing appearance with its key and saves nothing. This is a trust boundary, because a theme file comes from other
  people.
  - **Its success result** (AI-25). Today it returns `Settings` only [code: `src-tauri/src/main.rs`, `ui/api.ts`], and no
    caller reads that value. It now returns `SavedSettings`, `{settings, look}`: the settings as stored, and the look
    computed from the stored appearance. The screen applies this `look`, so what it shows is exactly what was saved,
    after a patch with or without `appearance`. `state()` returns the same `look` type. On a refusal or a failed
    write it returns only an error key.
  - **Callers migrated together in this commit.** The Rust command's return type, the `SavedSettings` type and
    `api.saveSettings` in `ui/api.ts`, and the two `api.saveSettings` calls in `ui/App.tsx` (`changeAppearance`, and
    the generic `saveSettings(patch)` helper that Setup and Settings reach through `onSave`). The helper awaits the
    result, ignores its `look` when the patch has no `appearance`, and handles a rejection as described under Error
    handling.
  - **It reports a failed write.** Today the store discards write and rename errors and the engine returns the
    settings whatever happened [code: `engine/src/store.rs`, `Core::save_settings`]. So `Store::save_settings` returns
    an `io::Result` and removes its temporary file when the rename fails. `Core::save_settings` returns
    `Err("error.settings.saveFailed")` before it applies anything live (such as restarting idle CLIs for a changed
    hooks choice) and keeps the settings it had. The shell command passes the result through,
    and `api.saveSettings` rejects. Only settings are covered in Phase 1; failed writes of conversation and project
    files are listed in B11.
- **`save_theme(appearance, name)` and `open_theme()`.** They use the existing dialog plugin. Their file logic is
  `write_theme_file` and `parse_theme_file`.
- **First paint.** `create_window` sets the window background to the look's `frame` colour instead of the fixed
  #15171b. The first paint then matches the theme (AI-6).

**`ui/appearance.ts`, rewritten.**

- **The old module goes.** It drops the Mono, Studio and Cockpit model.
- **What it does now.** It applies a `Look`:
  - It writes the CSS variables on `:root`, or on a given element for previews.
  - It sets `data-motion`, the effect flags and `data-base` on `<html>`.
- **Effective motion.** It is the user's level, capped by the system: a system that asks for less motion turns
  `full` into `reduced`. A change in the system setting changes only the motion level [notes].
- **The CSS** also applies `prefers-reduced-motion` before any script runs [notes].

**Callers ported in this same commit.** The old `appearance.ts` goes, and two screens import it today
(`applyAppearance`, `followSystem`, `toAppearance` in `ui/App.tsx`; `ACCENTS`, `PRESETS`, `presetOf` in
`ui/Settings.tsx`) [code]. They cannot be left for B5 and B8, because the proof commands must pass after this commit.
So B3 moves both to the new model and nothing more; B5b and B8 change their layout later:

- **`ui/App.tsx`.**
  - Start-up replaces `toAppearance`, `applyAppearance` and `followSystem` with applying `state().look`. The system
    motion listener now lives in `appearance.ts`, and it changes only the motion level.
  - `changeAppearance` becomes an awaited `save_settings`. It applies the returned look only after the save succeeded.
  - The text brand in the titlebar is styled with the heading font, because the `brand-*` tokens are gone. B4 replaces
    it with the lockup.
  - The new-chat empty state (today's `empty.chatTitle` heading and body) gets the display-heading style and mounts
    `Ambient`. B3 adds no greeting, suggestions or "Pick up" list; B5b replaces that content (see B5b, s1).
- **`ui/Settings.tsx`.** The Appearance section is rebuilt on the new model with the controls it has today (accent
  swatches and a custom colour, ground, fonts, size, density, corners, motion) and six preset cards, whose options come
  from `user-controls` in `themes.json`. Each choice calls `save_settings` and applies at once, as today [code]. A
  refused or failing look shows its error and changes nothing. B8 replaces this with the draft, preview and "Use this
  look" flow.

**`ui/main.tsx`.**

- **Before rendering**, it awaits `state()` and applies `look`.
- **Fonts.** It imports the font faces from Fontsource, latin and latin-ext subsets only (AI-10):
  - Archivo, with its width axis;
  - IBM Plex Sans;
  - IBM Plex Mono;
  - Newsreader;
  - Barlow Condensed;
  - Atkinson Hyperlegible;
  - Bodoni Moda, for Cobalto;
  - Geist Mono, for keyboard shortcuts.
- **Cleanup.** Remove the Geist, JetBrains Mono and Mona Sans packages if nothing uses them.
- **Font loading.** The WebView downloads a font file only when a face is used.
- **CSP.** The CSP already allows `font-src 'self'` [code].

**`ui/styles.css`, rewritten from the mock's component CSS** [mock, notes].

- **Tokens.** Every colour, size, radius, duration and shadow comes from a token.
- **Contrast.** `--control-line` gives 3:1 for choice controls that are still open, and chart bars other than the
  accent have a 3:1 outline [notes].
- **Motion** [notes]:
  - New items rise 6 px over 400 ms.
  - Buttons respond in 140 to 240 ms.
  - The "running" dot pulses every 1.6 to 2 s.
  - Oficina only fades and never moves anything vertically.
  - `reduced` keeps only fades of up to 120 ms. `none` removes everything.
- **Display headings** use `min(var(--fs-display), 9vw)`, so an 86 px heading fits small windows (AI-11). In B3 the
  style is applied to the heading of today's new-chat empty state. B5b puts the greeting in that place.

**`ui/Ambient.tsx`: the effects of the new conversation (s1)** [notes].

- **Sala.**
  - The projector beam over the greeting is static.
  - The glow around the lens breathes every 7 s.
  - 72 dust grains are drawn on one canvas at 24 fps. They fade out within 28 px of any text block.
- **Oficina.** The loom sits at the bottom right, and its shuttle crosses every 9 s. Paper grain is at 4.5%.
- **Bancada.** The flow line at the top right reads "Your idea → Claude and Codex → Done, with proof", with a short
  glow every 9 s. It names only the vendors that are connected (AI-11).
- **Where and when effects run.**
  - Effects appear only on s1, never behind text the user reads.
  - Effect loops stop when the window is hidden.
  - Motion levels `reduced` and `none`, and the `ambient` switch, turn them off.
  - `texture` controls the grain and the dot grid. `glow` controls the focus light and the amber light, which exist
    only in Sala and Cobalto; the other themes use borders [notes].

### Data flow

1. **Start.** The shell reads the settings and computes `look`. The window opens with `frame` as its background.
   `state()` returns `look`. `main.tsx` applies it and renders.
2. **First run (s6).** Each choice calls `preview_look` and applies the result to `:root` at once [notes]. Continue
   saves (B7).
3. **Settings.**
   - **In this commit** each choice saves through `save_settings` and, once the save succeeded, applies at once
     (today's behaviour).
   - **From B8 on (s5)**, each change updates a draft and calls `preview_look`. Only the preview panel takes the
     result ("Not applied yet"). "Use this look" is enabled only when all twenty-six checks pass. It saves through
     `save_settings`, and only after the save succeeded does it apply the look to `:root`.
4. **Preview ordering.** Every `preview_look` request carries a number that grows with each request the screen sends.
   The screen applies a response only if its number is the latest one sent, and drops older answers. A quick run of
   changes (a theme, then a size) can never end on an older draft, however the answers arrive.
5. **Theme changes keep some choices.** Choosing a theme or preset sets `base` and keeps `textSize`, `density` and
   `motion` [notes, tokens]. It clears `accent`, `ground`, the fonts, `corners`, `effects` and `colors`, so the new
   theme shows as designed.

### Error handling

- **Refused colour.** The draft keeps the previous accent and shows the sentence for the meaning colour that is too
  close [notes].
- **Bad theme file.** Nothing changes, and the reason is shown with the field as detail.
- **Failing pair.** "Use this look" stays disabled, and the preview names the failing pair [notes].
- **Failed save.** If the settings file cannot be written, `save_settings` rejects. The draft stays, the page keeps
  the look it had, and the error key's text is shown. The same rule holds for every screen that saves settings: the
  first run (B7) does not advance and Settings does not apply the draft.
- **Bad saved appearance on disk.** The default look is used and the error is logged on stderr. The app never fails to
  start because of its look.

### Tests

**Rust, in `look.rs`:**

- Each of the six looks passes all twenty-six pairs. The contrast table in `themes.json` is recomputed from the `css`
  section with the WCAG formula, and every ratio it lists matches to within 0.01. A factory look keeps its own
  `accent-ink`: Sala's stays `#0e0e10` at 16.93:1.
- A look with `ink`, `ink-2` and `ink-3` white and `s3` set to `#666666` is refused, naming `amber-ink` on `s3` (3.68).
  The plain-ink pairs alone would all pass at 5.74.
- The hue block, tested as a pure function of the hue:
  - The boundaries pass: 64°, 221°, 269° and 350°. The values just inside the bands are refused: 63.9°, 221.1°,
    268.9° and 350.1°.
  - Wrap-around: 355°, 5° and 0° are refused; 349° passes.
  - The bands join: 39° is refused, although it is 25° from Claude, because it is 1° from amber.
  - A hue inside two bands names the nearest meaning colour.
- The hue block, tested on colours well inside or outside the bands (never on the exact edge, because 8-bit colours
  round the hue):
  - #f2b13c, #e0673a and #8f87ea are refused.
  - A green such as #5fb36a and a blue such as #4f9dde pass.
  - #f3f0e8 passes through the neutral exemption.
  - A dark olive brown (#5a4a2a) is refused [notes].
- The contrast fix reaches 3:1 for the accent and 4.5:1 for its text version. A fix that lands inside the hue block
  is refused.
- For an accent the user picks, `accent-ink` is black on #f3f0e8 and white on #28604f.
- The theme file:
  - 20,001 bytes are refused.
  - An unknown key is refused.
  - `var(--amber)` is refused.
  - `amber`, `claude` and `codex` entries in `colors` are dropped.
  - A wrong type is refused.
  - Writing a file and parsing it back gives the same appearance.
- A legacy Mono appearance falls back to the default look.
- Switching the theme keeps `textSize`, `density` and `motion`.
- A file whose `bg` is #6b6b6b fails a meaning-colour pair, and the result names it.
- A file whose `s3` equals `ink` fails `ink` on `s3`, and a file whose `s1` equals `ink-2` fails `ink-2` on `s1`. The
  result names the pair in both cases, and the look is blocked.
- A failed write keeps the stored settings. The test first saves valid settings that are not the defaults (a language,
  an appearance, `userHooks`). It then makes the temporary path `settings.json.tmp` a directory, so the write fails
  while `settings.json` stays valid. A `Core::save_settings` call that changes the language and `userHooks` then:
  - returns `error.settings.saveFailed`;
  - leaves the bytes of `settings.json` unchanged, and `state().settings` and `Store::settings()` still return the
    saved values (not the defaults, which an unreadable file would give);
  - applies nothing live: no idle project CLI is restarted for the changed hooks choice;
  - leaves no temporary file behind once the directory is removed, and a later save then succeeds.
- A failed rename, tested on the store's write function alone: with a non-empty directory as the destination, the
  rename fails, the function returns the error and the temporary file is removed. No live behaviour is asserted there,
  because an unreadable destination reads as the defaults.

**UI checks:**

- `appearance.ts` writes every variable of a look.
- The effective motion is capped by the system preference.
- The effect loop stops when it is told the window is hidden, and when motion is `reduced` or `none`.
- Preview ordering: two `preview_look` answers delivered in reverse order leave the screen on the later request.
- A rejected `save_settings` leaves `:root` and the draft as they were.

**Screen:**

- The "appearance applies at once and is kept after reopening" scenario, ported unchanged in B2, is rewritten in this
  commit for the new model: change the theme in Settings, restart the app, and check that `--bg` matches.
- s1 at 1100×700 and 1440×900, showing today's empty-state heading, has no horizontal overflow in all six looks, and
  the ambient canvas is present in Sala. The greeting does not exist yet; its checks are in B5b.
- Opening a theme file with a failing pair is covered by the Rust test. The native dialog is not driven in screen
  tests.

**Memory:** see B10.

### Files touched

- **New:**
  - `src-tauri/src/themes.json`;
  - `src-tauri/src/look.rs`;
  - `ui/Ambient.tsx`;
  - `tests/ui/appearance.check.ts`.
- **Rewritten:** `ui/appearance.ts` and `ui/styles.css`.
- **Changed:**
  - `src-tauri/src/main.rs` (`FullState`, `preview_look`, `save_theme`, `open_theme`, the `save_settings`
    validation and result, the window background);
  - `engine/src/store.rs` (`save_settings` returns an `io::Result`);
  - `engine/src/engine.rs` (`Core::save_settings` returns a result);
  - `engine/tests/engine.rs`;
  - `testkit/src/screen/app.rs` (the appearance scenario);
  - `ui/main.tsx`;
  - `ui/App.tsx` and `ui/Settings.tsx` (the callers of the old module);
  - `ui/api.ts` (the `Look`, `Appearance` and `SavedSettings` types, the new commands, and `saveSettings` returning
    `SavedSettings` or rejecting);
  - `ui/locales/*.ts`;
  - `package.json` (the font packages).

## B4. Identity: icon, mark and wordmark

### Requirement

- **App icon.** One dark icon everywhere: a lime #D6DC48 Curioso on a #141A11 tile, with no border [R70, R71].
- **Small sizes.** The 16 and 24 px sizes are the hand-tuned versions [symbol].
- **Inside the app.** Empty states and the tutorial use Curioso [R75]. B4 gives the empty states that exist today their
  Curioso and provides the component. The empty states and the tutorial that later pieces add take it from there (B5b,
  B7).
- **Wordmark.** Its font is chosen with this spec [R75].

### Components

**Brand sources** in `docs/brand/`, copied from `faber-notas/design-final/simbolo-final/`:

- `mark.svg`;
- `icon-dark.svg`;
- `px24-dark.svg`;
- `px16-dark.svg`;
- `lockup.svg`.

The lime-tile extras are not copied [R70].

**Wordmark** (AI-12).

- "Faber" is set in Rubik Bold (SIL Open Font License) and converted to outlines.
- It replaces the placeholder letters in `lockup.svg`.
- It keeps the proportions measured on the board [symbol]:
  - the symbol is 1.65 times the cap height;
  - the gap to the F is 24/130 of the cap height;
  - the x-height is 0.75 of the cap height.
- No font file ships. The outlines live in the SVG.

**App icons in `src-tauri/icons/`, regenerated** (AI-14).

- Run `tauri icon` on `docs/brand/icon-dark.svg` to produce the PNG, ICNS and ICO set.
- Rebuild `icon.ico` so that its 16 and 24 px frames are 1:1 renders of `px16-dark.svg` and `px24-dark.svg`. Commit
  them as `16x16.png` and `24x24.png`.
- Commit `tray-16.png`, `tray-24.png` and `tray-32.png`. The tray picks one by display scale (B6).
- Write the exact steps in a new "Brand" section of `docs/design.md`.
- Delete the old `icon.svg`.

**`ui/Mark.tsx`.**

- It holds two inline SVG components, the mark and the lockup, in `currentColor` with even-odd eye holes [symbol].
- Inside the app the mark always uses `ink` and is never lime (AI-13). Lime sits about 23° of hue from amber, so the
  colour validator would refuse it as an accent.

**Titlebar.** It shows the lockup, 20 px tall, in `ink`, in every theme (AI-13). The mock's text brand (kept by B3
in the heading font) is replaced, because R75 is newer than the mock.

**Empty states** (AI-13).

- Curioso peeks, static and at 64 px in `ink-3`, from the edge of each empty state.
  - **In B4**, on the empty states the screen has today: no project open (`empty.openProjectBody`) and a project with
    no conversations (`empty.projectBody`) [code: `ui/App.tsx`].
  - **Added by later pieces, with B4's component:** the empty running column and the no-AI state (B5b), "no projects
    yet" in the sidebar (B5b), and the tutorial (B7).
- s1 (the new conversation) has no Curioso: it keeps its heading and ambient effect from B3, and the greeting that B5b
  puts there.
- Curioso does not animate in Phase 1.

### Data flow and error handling

The icons and SVGs are static assets, with no runtime logic. If a tray PNG fails to decode, the tray falls back to the
window icon and logs on stderr (B6).

### Tests

- **Rust (`src-tauri`).** `icon.ico` contains frames at 16, 24, 32, 48, 64 and 256 px. Its 16 and 24 px frames are
  byte-for-byte equal to `16x16.png` and `24x24.png`.
- **Screen.** The titlebar contains the lockup, and its fill is the computed `--ink`. The two empty states of today
  show Curioso at 64 px, and s1 shows none.

### Files touched

- **New:** `docs/brand/*.svg`, `ui/Mark.tsx`, and the new PNGs in `src-tauri/icons/`.
- **Regenerated:** `src-tauri/icons/*`.
- **Changed:** `src-tauri/tauri.conf.json` (the icon list), `ui/App.tsx` (the titlebar) and `docs/design.md` (the
  "Brand" section).

## B5. Shell layout and running list

This piece is two commits. B5a holds the engine and shell work (the running list, the quota readings, and `answer`
with its turn id and its acknowledged result). It adds fields and an event that the old screen ignores. `answer` is
the one call that changes shape, so B5a also ports its single old caller: `Thread.tsx` passes the id of the message
that holds the card as `turnId`, `App.tsx` and `api.ts` pass it on, and a rejected answer (`stale`, `undelivered`) is
caught and ignored there, as a lost answer is today. With that, B5a passes alone. B5b holds the screen work and uses
B5a.

### Requirement

- **Layout.** The mock's layout [mock]:
  - a 36 px titlebar across the full width;
  - a 248 px sidebar;
  - the stage, with a 52 px topbar;
  - a reading column of at most 660 px, centred in the stage;
  - a 352 px running column on the right.
- **Widths.** The mock's sizes add up to 1,260 px before any gap, but the window can be as narrow as 720 px
  [code: `create_window`]. So 660 px is a maximum, and the layout changes with the window width (AI-11):

  | Window width | Sidebar | Running column | Reading column |
  | :--- | :--- | :--- | :--- |
  | 1,100 px and wider | 248 px, beside the stage | 352 px when open, beside the stage | Shrinks with the stage, 660 px at most. The stage is never narrower than 500 px |
  | 900 to 1,099 px | 248 px, beside the stage | When open, an overlay on the stage | Up to 660 px |
  | 720 to 899 px | Hidden. A button in the topbar opens it as an overlay | When open, an overlay on the stage | Up to 660 px |

  The sidebar overlay is modal: focus moves into it, Esc closes it, and focus returns to the button that opened it.
  The running column overlay is modal only when the user opens it (the run button or its keyboard shortcut), and then
  it behaves the same way. When it opens by itself, it is non-modal at every width [R53]: focus stays where it was,
  nothing behind it becomes inert, and it ends above the composer so the composer is never covered. Esc closes it only
  while focus is inside it, and Tab reaches it after the stage. Text size up to 18 px must not push the page past the
  window width at any of these widths: content wraps or scrolls inside its own area.
- **Running column behaviour.** As in R53 and R54 (A7).

### Components

**Titlebar.**

- The lockup, plus the existing `WindowControls`.
- Dragging and double-click maximize keep working [estado].
- macOS keeps its native buttons [estado].

**Sidebar** (`ui/Sidebar.tsx`, new; extracted from `App.tsx`).

- **Top.**
  - "New conversation", with Ctrl N [mock, code].
  - The two areas, Chat and Projects, the latter with a count [mock].
- **Projects group.**
  - Each project row expands to show its conversations, and has a "+" [plano].
  - When items in a project need the user, its tail shows the amber speech bubble and "{m} needs you". Amber always
    comes with these words, never with a number alone [notes, tokens]. When something only runs, the tail shows a
    pulsing dot (AI-16).
  - The mock's percentage and "stopped" tails come with the progress view in Phase 3.
- **Chats.**
  - Chats are grouped as Today, This week and Earlier (AI-16). The mock shows only the first two.
  - Delete with confirmation is kept [code].
- **Footer.**
  - Settings.
  - One line per connected vendor: its shape, its name and the used share of its tightest quota window. It opens
    Settings > Usage limits. A vendor with no reading yet, or only an expired one, shows "—" (see the quota readings
    under the engine below).
  - The footer shows no user name, because Faber does not know it (AI-15).
  - The quota bars leave the header [code], so the user still sees the quota at all times [D26, roadmap] (AI-16). The
    footer reads `AppState.quotas`, not a value held only by the page, so a window rebuilt after closing to the tray
    shows the same readings.
- **Not in Phase 1.** Search (Phase 2, AI-1).

**Topbar** (in `App.tsx`).

- **Crumb.** "Chat › title" or "Project › title".
- **Model chip.** It shows the open conversation's current model and is read-only. Each answer keeps its existing
  model and reason line [D26].
- **Run button** [R53, tokens]:
  - With no items it reads "Nothing running" (AI-17).
  - Otherwise it reads "{n} running".
  - When an item needs the user, it turns amber, shows the speech bubble and reads "{n} running · {m} needs you". The
    words are always visible [tokens].

**s1, the new conversation** [mock, inspiração]. B5b replaces today's empty state (and B3's styled heading) with this
screen.

- **Greeting.** "Where do we start?", with no name (AI-15).
- **Suggestions.** Four hand-written suggestions per language [inspiração] (AI-15). Clicking one fills the composer
  and moves focus there. Nothing is sent.
- **"Pick up where you left off".** The three most recently updated conversations [mock].
- **Composer.** It keeps its model picker, "AI: choose for me" [mock, code].
- **Effects.** The ambient effect, already mounted by B3.
- **With no AI connected.** The greeting, suggestions and "Pick up" give way to the no-AI empty state (Curioso, B4),
  with a button that opens account management (B7). The composer stays on screen but cannot send, as today
  [code: `Composer.tsx`]. The tutorial does not run in this state (B7).

**Running column** (`ui/Running.tsx`, new).

- **Layout.** One block per project, plus one "Chat" block, in the order the engine sends [R54].
- **Each row shows:**
  - the vendor's shape, or a neutral dot while Auto is still choosing the model;
  - the conversation title;
  - the state in words, "Working" or "Needs you", or "Choosing the model" while Auto routes (`running.routing`);
  - the time since the turn started.
- **Questions.** An item that needs the user shows the existing permission card, `Ask` from `ui/Thread.tsx`, moved to
  `ui/Ask.tsx` and shared by both. The user can answer in place with Allow, Always or Deny [code]. The same card can
  be open in the thread and in the column at once. Every answer names the conversation, the turn and the request
  (`turnId` is the item's `turnId`, or the id of the message that holds the card in the thread). What an answer
  returns:
  - Accepted: both copies disappear when the next `Running` and `Message` events arrive.
  - `error.permission.stale` (the question was already answered, from the other copy or elsewhere, or its turn is over,
    or the CLI no longer knows the request): the card is dropped without an alarm.
  - `error.permission.undelivered` (the answer could not be written to the CLI, or the CLI did not confirm in time):
    the card stays and shows the error key's text, so the user can try again or stop the turn.
- **Opening the conversation.** Clicking a row calls `openConversation(item)`. It sets the area, the project and the
  conversation id in one update, then remembers the place. The crumb, the sidebar, the composer and the next send then
  all point at the same project. Today's `open(id)` changes only the conversation [code: `ui/App.tsx`], so a click
  from Chat on a row of another project would send into the wrong context. The sidebar's own rows use the same
  function, with the area and project of their summary.
- **Empty column.** It shows the empty state with Curioso (B4).
- **Opening and closing** (AI-17):
  - The column is closed when the window opens. The run button toggles it.
  - It opens by itself once for each question it has not yet shown in this window. A question is identified by its
    conversation, its turn and its request id together (`conversationId:turnId:requestId`), because request ids alone
    repeat: Codex numbers its requests from 0 in each server [code: `engine/src/codex.rs`, `tests/fixtures`]. The
    rule includes questions already waiting when the window opens [R53].
  - It waits until 1.5 s after the last keystroke in any text field [R53].
  - It never moves focus [R53], at any width. At the overlay widths it opens non-modal (see the width table).
  - If the user closes it while questions are waiting, those questions never reopen it. Only the button stays amber
    [R53].
- **Narrow windows.** See the width table in the Requirement: below 1,100 px the column overlays the stage.
- **Startup order.** The page registers its `Running` listener first and waits for the registration, and only then asks
  for `state()`. Both the snapshot and the event carry a revision (below), and the page keeps the highest one it has
  seen. A change that lands between the two is therefore never lost, and an old snapshot never overwrites a newer
  event.

**Engine: the running list and the quota readings** (AI-17).

- **The `RunningItem` type**, in `engine/src/model.rs`:
  - `conversationId`;
  - `turnId`, the id of the turn's reply message;
  - `title` (empty while the conversation has none);
  - `area`;
  - `projectId` (optional);
  - `vendor` (optional: none while Auto is still choosing the model);
  - `model` (optional);
  - `state`, either `working` or `needsYou`;
  - `since`, the `at` of the turn's reply message;
  - `permissions`, the unanswered ones.
- **Which conversations are listed.** A conversation is listed while its latest reply is streaming, or it has
  unanswered permissions. `needsYou` means at least one permission is unanswered.
  - A reply is streaming from the moment the user sends. That covers the time Auto spends choosing a model, up to 30 s
    [D54], when the reply exists but no live session has a turn yet [code: `Core::send`, `after_route`]. Such an item
    has no vendor and no model. It gets them when the pick is made.
  - Stopping during that wait drops the item (`stop_waiting`). A route that fails or arrives late falls back to the
    everyday default, and the item gets that vendor. A start that fails ends the reply with an error and drops the
    item.
- **Order** [R54]:
  - Blocks that contain a `needsYou` item come first.
  - Inside each of the two groups, blocks follow the sidebar's project order, with the Chat block last.
  - Inside a block, `needsYou` items come first, oldest first, then `working` items, oldest first.
  - This order is stable, so rows do not jump.
- **New fields and event.**
  - `AppState` gains `running`, an object `{revision, items}`.
  - `AppEvent` gains `Running { revision, items }`.
  - The revision starts at 0 and grows by one with every list the engine emits.
- **Quota readings.** `AppState` also gains `quotas`: the latest reading for each vendor and window, as `{vendor,
  window, used, resetsAt}`. Today the engine keeps only the used share and the page keeps the rest, so the reset times
  vanish when the window is destroyed [code: `Core.used`, `ui/App.tsx`]. The engine now keeps the whole reading from
  each `Quota` event. A reading whose `resetsAt` is in the past counts as unknown. The screen shows "—" (key
  `quota.unknown`) instead of an old percentage, and so for a connected vendor that has no reading yet. Auto's own
  use of the used share is unchanged.
- **When the engine recomputes.** One function, `refresh_running`, runs wherever `Core` emits a `Message` or a `List`
  event, and after any change to a permission. That covers a send, the route result, a turn start, every status
  change, `finish`, `answer`, `interrupt` and `stop_waiting`, `rename`, `remove`, and a session that ends or crashes.
  Because `rename` is among them, a renamed running conversation shows its new title.
- **When it emits.** Only when the new list differs from the last one it emitted (title and permissions included).
  The cost is one pass over the live conversations.
- **What `answer` takes and returns** (AI-25). Today `answer` has no turn id: it looks up the conversation's current turn,
  removes the permission before anyone has received the answer, and ignores a stale target. Writes to the CLI are
  queued and a failed write is dropped [code: `Core::answer`, `proc::write_line`, the `Answer` arms in `claude.rs` and
  `codex.rs`]. So a delayed answer to request `0` could approve request `0` of the next turn, and a queue that
  accepts a command says nothing about whether the CLI received it. Phase 1 changes this:
  - **The turn id is part of the call.** `answer(conversationId, turnId, requestId, allow, always)`, in the engine,
    the shell command and `api.answer`. The engine checks that the conversation is live, that its current turn is
    `turnId` and that `requestId` is waiting in that turn. If not, it returns `error.permission.stale` and sends
    nothing.
  - **Delivery is acknowledged by the session task.** `SessionCommand::Answer` carries a one-shot acknowledgement.
    After the work, the task replies with a `Delivery`:
    - `Delivered`: the answer was written and flushed to the CLI's input (`proc::write_line` now returns an
      `io::Result`, and the other callers keep ignoring it, as a dead process shows up as the end of its output);
    - `Unknown`: the session holds no such pending request (Claude: no entry in `pending`; Codex: no entry in
      `approvals`). It writes nothing;
    - `WriteFailed`: the write or the flush returned an error.

    **The session keeps its own record until the write succeeds.** Today both handlers remove the request from their
    pending map before they write [code: `claude.rs` `pending.remove`, `codex.rs` `approvals.remove`]. After a failed
    write, a retry would then find nothing and report `Unknown`, and the engine would drop a card the user never got to
    answer. So the handlers read the entry without removing it (Claude: a copy of the stored input; Codex: the stored
    JSON-RPC id), write, and remove the entry only when the write and the flush both succeeded. On `WriteFailed` the
    entry stays, and the same request can be answered again.
  - **`Session::answer` returns the receiver of that acknowledgement.** A closed queue drops the sender, and the
    receiver then reports an error.
  - **`Engine::answer` is asynchronous, and `Core` stays synchronous.** `Engine` owns the mutex, and `Engine::with`
    lends `Core` through a locked guard [code: `engine/src/engine.rs`], so a `Core` method cannot give the lock back and
    keep its borrow. The orchestration therefore lives in `Engine::answer(...)`, an `async fn`, in three steps:
    1. `self.with(|c| c.prepare_answer(...))` validates the conversation, the turn and the request, sends the commands
       and returns the acknowledgement receivers, or the stale error. It changes no permission.
    2. With the lock released, `Engine::answer` waits for every receiver, at most 5 s in all.
    3. `self.with(|c| c.settle_answer(...))` takes the lock again, applies the results and refreshes the running list.

    `Core::prepare_answer` and `Core::settle_answer` are plain synchronous functions, and settling re-checks that the
    conversation, the turn and the permission are still there, because other calls ran between steps 1 and 3. Settling
    each request works like this:

    | Acknowledgement | Permission | Result |
    | :--- | :--- | :--- |
    | `Delivered` | Removed (for "Always", the tool joins the conversation's allow list) | `Ok` |
    | `Unknown` | Removed, because the CLI no longer waits for it | `error.permission.stale` |
    | `WriteFailed`, a closed queue, or no acknowledgement in 5 s | Stays on the list | `error.permission.undelivered` |

    "Always" also answers the other waiting requests for the same tool; each is settled by its own acknowledgement, and
    the call returns the result of the request the user clicked.
  - **Both copies of a card.** The lock is free while the engine waits, so two clicks can both pass the first check.
    The session task handles them in order: the first is `Delivered`, the second `Unknown`, which is `stale`. Exactly
    one line reaches the CLI. An acknowledgement that arrives after the 5 s is ignored.

  The shell's `answer` command takes `turn_id` and passes the result through.

### Data flow

1. A turn starts, or a permission arrives.
2. The engine recomputes the list and, if it changed, emits `Running` with the next revision.
3. The UI updates the column, the run button and the sidebar tails, keeping the highest revision it has seen.
4. The shell receives the same event for the tray (B6).
5. The user answers in the column. The UI calls `answer` with the conversation, turn and request ids. The engine
   validates them, hands the answer to the session and waits for its acknowledgement. Only a `Delivered` answer removes
   the permission, and then the list updates. A stale or undelivered answer comes back as an error key.

### Error handling

- **Fewer items.** If an item's conversation is gone, the UI drops the row. Clicking a vanished row does nothing.
- **Failed answer.** `error.permission.stale` drops the card quietly. `error.permission.undelivered` keeps the card and
  shows the error key's text. A write failure never removes a permission, so the user is never told that a request was
  answered when the CLI did not get the answer.
- **No quota reading.** Unknown or expired readings show "—", never a made-up number.

### Tests

**Rust (`engine/tests/engine.rs`, with the fake spawner [code]):**

- Starting a turn adds a `working` item.
- A `can_use_tool` permission from `claude/tools.jsonl` turns it into `needsYou`.
- `answer` returns it to `working`.
- `finish`, `interrupt` and `remove` drop it.
- Streaming chunks that do not change the list emit no `Running` event.
- Order with two projects and Chat: the project with a `needsYou` item comes first; inside it, the `needsYou` item
  comes first; the Chat block comes last within its group.
- Revisions: each emitted list is the previous revision plus one, and `state()` returns the latest revision.
- Auto routing, with a slow stand-in for the router [code: the `ask` option]:
  - A send in Auto adds a `working` item at once, with no vendor and no model.
  - When the pick is made, the same item has the vendor and the model.
  - `interrupt` during the wait drops it.
  - A route that times out falls back to the default, and the item keeps running with that vendor.
  - A start that fails drops it.
- `rename` of a running conversation emits `Running` with the new title.
- Request ids that repeat: a Claude conversation in Chat and a Codex conversation in a project both hold a
  permission with request id `0` (the Codex tools fixture starts at 0). Both are listed, each is answered on its own,
  and a second Codex server (after a restart) that sends `0` again in a new turn is a different question.
- `answer`:
  - A second answer to the same request returns `error.permission.stale`, and the CLI receives exactly one line.
  - The turn id is checked. Turn 1 holds a permission with request id `0` and ends. Turn 2 holds its own request `0`. An
    answer that carries turn 1's id returns `stale`, nothing is written, and turn 2's permission still waits. The
    answer with turn 2's id then succeeds.
  - A session that reports `Unknown` makes the call return `stale` and removes the permission.
  - An answer to a session whose queue is closed returns `error.permission.undelivered` and leaves the permission on
    the list.
  - No acknowledgement within 5 s (tokio's paused clock) returns `undelivered`; an acknowledgement that arrives
    afterwards changes nothing.
  - A stand-in session built with `Session::from_fn` acknowledges `WriteFailed`: the call returns `undelivered`, the
    permission stays listed with the same revision, and a second answer acknowledged `Delivered` removes it.
  - `Engine::answer` is awaited while another thread calls `state()` and `send`: neither blocks for the length of the
    wait, because the lock is free between steps 1 and 3. A conversation removed during the wait makes settling return
    `stale` without a panic.
- Session tasks (`engine/tests/claude_session.rs` and `codex_session.rs`), through the real session handlers: a fake
  process replays the permission from `tools.jsonl`, and its input is an `AsyncWrite` that fails its first write and
  then passes writes through (the open queue accepts the command, the write fails).
  - The first `answer` reports `WriteFailed`.
  - The same request is answered again and reports `Delivered`. The recorded `control_response` or `decision` line is
    written exactly once, so the retry found the session's pending entry.
  - A request id the session does not hold reports `Unknown` and writes nothing, and a third answer to the delivered
    request reports `Unknown`.
- Quota readings: a `Quota` event with a reset time is returned by `state()`; a fresh call to `state()` after the page
  was destroyed (simulated by calling it again) still returns it.

**UI checks:**

- Chats group into Today, This week and Earlier, with a fixed "now".
- The auto-open rule, keyed by `conversationId:turnId:requestId`:
  - It opens once per new key.
  - The same request id in two conversations opens twice; the same request id in a new turn opens again.
  - It does not open within 1.5 s of a keystroke.
  - It does not reopen for dismissed keys.
  - It opens for questions present at launch.
- Revisions: a `Running` event with revision 5 followed by a snapshot with revision 4 leaves the event's list; a
  snapshot with revision 6 followed by an event with revision 5 leaves the snapshot's list.
- `openConversation` sets the area, the project and the conversation id in one update (one render, never a state with
  the new id and the old project).
- A quota reading whose `resetsAt` has passed shows "—".

**Screen:**

- Using `claude/tools.jsonl`:
  - The button turns amber and shows the words.
  - The column opens once, after typing stops, and focus stays in the composer.
  - After the user closes it, the button stays amber.
  - Answering Allow in the column clears the amber.
  - Reloading the page while the question waits (a window rebuilt after closing to the tray, seen from the engine)
    shows the amber button on first paint, and the column opens once.
- From Chat, click a running row that belongs to a project: the crumb reads "Project › title", the sidebar marks that
  project, and a message sent goes to that conversation and not to a new one.
- Using `claude/chat.jsonl`, which carries a quota reading: after reloading the page, the footer still shows it.
- s1 with the greeting, at 1100×700 and 1440×900, in all six looks: no horizontal overflow, the heading is at most
  9vw wide in its font size, and clicking a suggestion fills the composer and sends nothing. With no AI connected, s1
  shows the no-AI empty state and no greeting.
- The empty running column and the no-AI state show Curioso (B4's component), static, at 64 px.
- Layout, with text size 18, at 720×480, 899, 900, 1,099 and 1,100 px wide: the page never scrolls sideways; the
  composer and the run button are reachable; at 1,100 px the column pushes the stage and the stage stays at least
  500 px wide; at 1,099 px the column overlays it; at 899 px and below the sidebar is behind its button, and opening
  it moves focus into it, Esc closes it and focus returns to the button.
- The column at the overlay widths (899 and 1,099 px): when it opens by itself, focus stays in the composer, typing
  continues in the same field and the composer is not covered. Opened with the run button, focus moves into it, Esc
  closes it and focus returns to the button.

### Files touched

Commit B5a, running list (engine and shell):

- **Changed:**
  - `engine/src/model.rs` (`RunningItem`);
  - `engine/src/engine.rs` (`refresh_running`, `AppState`, `AppEvent`, quota readings, and for `answer`:
    `Engine::answer` as an `async fn` with `Core::prepare_answer` and `Core::settle_answer`);
  - `engine/src/events.rs` (`Delivery`, the acknowledgement in `SessionCommand::Answer`, `Session::answer`), and
    `engine/src/claude.rs`, `engine/src/codex.rs` and `engine/src/proc.rs` (the `Answer` arms remove their pending entry
    only after a successful write and reply after it; `write_line` returns an `io::Result`);
  - `engine/tests/engine.rs`, `engine/tests/claude_session.rs`, `engine/tests/codex_session.rs` and
    `engine/tests/support/mod.rs` (a process whose input fails);
  - `src-tauri/src/main.rs` (the `answer` command takes `turn_id` and passes the result through);
  - `ui/api.ts`, `ui/App.tsx` and `ui/Thread.tsx` (only the `answer` call: the message id as `turnId`, and a caught
    rejection).

Commit B5b, layout and running column (screen):

- **New:** `ui/Sidebar.tsx`, `ui/Running.tsx`, `ui/Ask.tsx` and `tests/ui/running.check.ts`.
- **Changed:**
  - `ui/App.tsx` (layout, topbar, s1, `openConversation`, startup order);
  - `ui/Thread.tsx` (uses `Ask.tsx`);
  - `ui/api.ts` (`RunningItem`, `quotas` and the `Running` event);
  - `ui/styles.css` (the width table);
  - `ui/locales/*.ts`;
  - `testkit/src/screen/app.rs` (the new scenarios).

## B6. Tray number and notifications

### Requirement

The tray icon shows a number, and a notification appears only when an agent needs the user [R74].

### Components

**`src-tauri/src/tray.rs` (new).** `create_tray` moves here from `main.rs`.

**Badge.**

- The badge counts the items that need the user.
- It is drawn at runtime onto the tray PNG for the current display scale: an amber circle (#f2b13c) at the bottom
  right, with the digit 1–9, or "+" for 10 and more, in #1d1404 [tokens] (AI-18).
- The glyphs come from a small built-in pixel font.
- With no such items, the plain icon shows.

**Tooltip.**

- "Faber" when nothing runs.
- Otherwise "Faber: {n} running", plus ", {m} needs you" when something needs the user (AI-18).
- The words are always visible next to the count, as the locked amber rule requires [tokens].

**Notification.**

- The notification uses `tauri-plugin-notification`, called from Rust only, so the screen gets no new capability.
- It is sent once per question, identified as in B5 by `conversationId:turnId:requestId`, and only while the main
  window is unfocused or closed (AI-18). The request id alone is not enough, because two servers can both send id 0.
- Its text is "{title} needs you".

**Opening from the tray.**

- Clicking the tray icon shows the window, as today [code].
- The new window then opens the running column under B5's rules.
- Faber does not rely on clicking the notification, which behaves differently on each OS (AI-18).

**Texts.**

- `set_tray` becomes `set_shell_texts`, which the UI calls whenever the language changes [code]. It carries the menu
  items, the tooltip templates and the notification template.
- Count texts are sent as `{one, other}`. The shell picks `one` when the count is 1 and `other` otherwise. This matches
  `Intl.PluralRules` in all five languages for counts of 1 and more.
- The texts persist in the shell, so a closed window still has them.

### Data flow

1. The emit closure in `main.rs` forwards every `AppEvent` to the window, as today. It also passes `Running` events to
   `tray.rs`.
2. `tray.rs` keeps the highest revision it has seen and ignores older lists. It updates the badge and the tooltip, and
   decides whether to notify.

### Error handling

- **Notification failure.** If the notification plugin fails, or permission is denied on macOS, Faber logs on stderr
  and keeps the badge.
- **Badge failure.** If drawing the badge fails, the plain icon stays.
- **Hidden runs.** Hidden runs (`FABER_HIDDEN`) create no tray, as today [code].

### Tests

**Rust (`tray.rs`):**

- The badge pixels for 1, 9 and 12 ("+"), and none for 0.
- The tooltip text for each case.
- The notification decision:
  - It notifies once per question key.
  - Two questions with the same request id in different conversations notify twice.
  - It never notifies while the window is focused.
  - It notifies while the window is closed.
- A list with a lower revision than the last one seen changes nothing.
- The plural pick for 1 and 2.

**Hand checks.** The tray and notifications cannot be seen from screen tests, because hidden runs have no tray.
Check them by hand on an installed Windows build, and on macOS and Linux before Phase 5.

- Build the installer locally (`tauri build`, the NSIS target) and install it for the current user. A development run
  (`npm run dev`) is not a valid check: Windows shows notifications from a development build under the identity of the
  program that started it, not Faber's, and only an installed application has its own (Tauri's notification plugin
  documentation says so).
- Check on the installed build:
  - **Delivery.** A permission request, with the window unfocused and again with the window closed to the tray, raises
    a notification.
  - **Identity.** The notification is named "Faber", with the Faber icon, and not "PowerShell" or the name of another
    program.
  - **Focus.** No notification appears while the Faber window is focused.
  - **Closed window.** Clicking the tray icon shows the window, and the running column opens under B5's rules.
  - **Once per question.** Answering and then asking again notifies again; one question never notifies twice.

### Files touched

- **New:** `src-tauri/src/tray.rs`.
- **Changed:**
  - `src-tauri/src/main.rs` (the emit closure, the commands, the plugin registration);
  - `src-tauri/Cargo.toml` (adds `tauri-plugin-notification` and the `image-png` feature of `tauri`);
  - `ui/App.tsx`;
  - `ui/api.ts`;
  - `ui/locales/*.ts`.

## B7. First run and tutorial

### Requirement

- **First-run question.** The first run asks whether the person has never used AI or already uses it a lot [R18].
- **What each answer gets.** Experienced people skip the tutorial and see only what is new to them. Beginners get a
  short, simple tutorial [R18]. After that, Faber stops teaching [R18].
- **Choosing a theme.** The person picks the theme with all three side by side, together with the other initial
  settings [R59].
- **The screen.** It is s6 [mock, notes].

### Components

**Stepper.** Three steps: "Connected accounts › Appearance › First conversation" [mock].

**Step 1: Connected accounts.**

- This is today's `Setup.tsx`, restyled, with the same cards and logic [code]:
  - install;
  - sign in through the CLI's own flow;
  - check again;
  - "None".
- Skip and "None" still save `setup` (`skipped` or `done`) at once [code]. Then they move to step 2 (AI-19).
- **Two modes** (AI-19). `Setup` takes a `mode`, either `firstRun` or `manage`:
  - In `firstRun` mode it is step 1 of three and moves on to step 2.
  - In `manage` mode, opened from Settings > Connected accounts (B8) or from the no-AI state on s1, it is only the
    account cards with a Close button. It saves `setup` as today, never opens step 2 and never touches `appearance`,
    `experience` or `tutorialDone`. Reconnecting an account later therefore cannot restart the first run or reset the
    tutorial [code: today `Settings` reopens `Setup`, which would send an existing user back through Appearance].
  - Esc closes `manage` mode. In `firstRun` mode, Esc and Ctrl N do nothing (today Esc closes the guide without
    saving anything [code]); the step's own buttons are the only way on.

**Step 2: Appearance (s6)** [mock, notes].

- **Title.** "How do you want Faber to look?"
- **Theme cards.** The three themes side by side. Each card has:
  - a small live preview drawn with that theme's tokens (from `preview_look`);
  - the theme's name;
  - its one-line description.
- Sala comes selected, with a "default" tag. Clicking another card applies it to the whole screen at once.
- **Text size.** Three "Aa" buttons: small = 13, normal = the theme's own value (field cleared), large = 16 [tokens].
- **Density.** Comfortable or Compact [tokens].
- **Motion.** Full, Reduced or None. It starts from the system preference. The hint reads "If your system asks for
  less motion, this starts in Reduced" [mock]. The mock says "Windows"; the English text names the system, because
  Faber also runs on macOS and Linux [R44].
- **Experience.** "Do you already use AI?", with "Never" ("Faber shows the way the first few times") and "A lot" ("no
  explanations") [notes].
  - Nothing is preselected.
  - Continue works without an answer, and no answer counts as beginner (AI-19).
- **Persistence.** Size, density and motion take effect at once and survive theme changes [notes].
- **Footer.** "You can change everything later in Settings > Appearance.", with Back and Continue [notes].
- **No sidebar** [notes].

**Continuing from step 2.**

- Continue saves `appearance` (`base`, plus any `textSize`, `density` and `motion` the user chose), `experience` and
  `tutorialDone: false` in one `save_settings` call. It leaves `tutorialDone` out of the call when it is already
  `true`, so an upgraded install that finished the tutorial is not taught again.
- The screen waits for the save. Only when it succeeds does the step close. On a failed save the step stays open, and
  nothing advances (see Error handling).
- Esc does nothing here (see step 1). Keyboard focus moves to the step's title when it opens and, when it closes, to
  the composer on s1.

**Step 3: First conversation.** It is s1 itself, with the stepper gone and the tutorial starting [mock].

**When the first run opens.**

- Step 1 opens under today's rule: a first launch with no saved conversations [code, C4.1].
- Step 2 opens when the saved appearance has no `base`. That covers new users after step 1, and upgraded installs
  once (AI-19). If step 1 is not due, the first run starts at step 2, with step 1 shown as done.

**Engine settings.** `Settings` gains two fields:

- `experience` (`beginner` or `experienced`);
- `tutorialDone` (boolean).

`save_settings` must merge both explicitly, as it does every field today [code].

**Tutorial** (`ui/Tutorial.tsx`, new) (AI-20).

- **Format.** Coach cards anchored to the real UI. An anchor is the element with a matching `data-tour` attribute.
- **Each card shows:**
  - a small static Curioso [R75];
  - the step number;
  - one or two sentences;
  - Next and Skip.
- **Keyboard and focus.** A card is a modal dialog (`role="dialog"`, `aria-modal="true"`, named by its title and
  described by its sentences), and the rest of the app is `inert` while it is open.
  - Focus moves to Next when a card opens. Tab and Shift+Tab cycle between Next and Skip only.
  - Enter or Space presses the focused button. Esc skips the whole tutorial.
  - Ctrl N and Ctrl , do nothing while a card is open, and the global Esc handler leaves Esc to the card.
  - When the tutorial ends, focus returns to the element that had it before the tutorial began, or to the composer if
    that element is gone.
- **Missing anchors.** If a card's anchor is missing or hidden when the card is due (for example the Projects button,
  in a window narrower than 900 px where the sidebar is behind its button), the card is shown in the centre with no
  pointer, and its words do not say "here".
- **No AI connected.** The tutorial does not start while no AI is connected, because its first card teaches the
  composer, which cannot send. `tutorialDone` stays `false`. The tutorial starts the next time s1 is shown with at
  least one AI connected, for example after the user connects an account from the no-AI state.
- **Beginners: three steps** [inspiração]:
  1. "Chat": the composer. Ask anything; Auto picks the model.
  2. "Projects": the Projects area. Open a folder and Faber works in it.
  3. "Follow along": the run button. Everything running shows here, and amber means it needs you.
- **Experienced users: two cards, for what is new to them** [R18]:
  - Auto, at the composer's model picker;
  - the running list, at the run button.
- **When it ends.** Finishing or skipping saves `tutorialDone: true`. It never shows again. If that save fails, the
  tutorial still ends for this session and may show once more at the next launch.
- **No other teaching.** There is no other in-app teaching [R18].

**CLI install.** Step 1 uses the install as it is. B9 later changes how the download reaches the disk, with no change
to this step.

### Data flow

1. The page starts, reads `state()` and decides what is due (see "When the first run opens"). Nothing is due for a
   user who has finished the first run.
2. Step 1 shows the accounts. Signing in goes through the CLI's own flow, and an install goes through the engine.
   Continuing, skipping and "None" save `setup` through `save_settings` (B3), and wait for it.
3. Step 2 sends each choice to `preview_look` and applies the answer to `:root` (B3, with its request numbering).
   Continue saves `appearance`, `experience` and, only when it is not already `true`, `tutorialDone: false`.
4. Step 3 is s1. With at least one AI connected, the tutorial starts. With none, it waits until one is.
5. Finishing or skipping the tutorial saves `tutorialDone: true`. Reconnecting an account later opens `Setup` in
   `manage` mode, which touches none of these fields.

### Error handling

- **Failed save.** If saving the settings fails (`error.settings.saveFailed`, from B3), the step stays open and shows
  the error key's text. The first run does not advance, the tutorial is not started, and nothing is lost.
- **Failed install.** An install failure keeps today's behaviour: a translated error key, with the technical detail
  kept separate [code, C4.4].

### Tests

**UI checks.** The ported C4.1–C4.6, plus:

- Step 2 is due exactly when `appearance.base` is missing.
- The tutorial has 3 cards for a beginner, 2 for an experienced user, and 3 when experience is unset.
- "normal" text size clears `textSize`.
- `manage` mode never produces a `save_settings` call with `appearance`, `experience` or `tutorialDone`.
- Continue leaves `tutorialDone` out of its call when it is already `true`, and sends `false` otherwise.
- With a rejected `save_settings`, step 2 stays open and the tutorial does not start.
- The tutorial does not start with no AI connected, and starts once one is.
- A card whose anchor is missing is shown centred.

**Rust (`engine/tests/engine.rs`).** `experience` and `tutorialDone` survive a `save_settings` patch that does not
mention them.

**Screen (in `testkit/src/screen/setup.rs`).** The ported C5.1–C5.5 and R5.5, which now also pass through step 2,
plus:

- Clicking Oficina changes `--bg` at once.
- Large text survives a theme switch.
- Continue lands on s1.
- A restart skips the first run.
- A data folder with a legacy appearance and saved conversations opens at step 2 once.
- A beginner sees three tutorial cards and an experienced user two. Skipping persists.
- **Reconnecting.** After a finished first run, open Settings > Connected accounts and reconnect an account. No step 2
  appears, and `appearance`, `experience` and `tutorialDone` are unchanged.
- **"None" to the end.** Choose "None", finish step 2 and land on s1. It shows the no-AI state and no tutorial. Connect
  an account in `manage` mode and return to s1: the tutorial starts.
- **Keyboard only.** Complete step 2, the beginner tutorial and an Esc-skip using Tab, Shift+Tab, Space, Enter and Esc
  only. Esc on steps 1 and 2 does nothing. Focus lands on the composer afterwards, and Ctrl N does nothing while a
  card is open.
- **Missing anchor.** At 800 px wide the sidebar is behind its button, so the Projects card is centred with no
  pointer.

### Files touched

- **New:** `ui/Tutorial.tsx` and `tests/ui/tutorial.check.ts`.
- **Changed:**
  - `ui/Setup.tsx` (the `mode`, restyled as step 1);
  - `ui/setup.ts`;
  - `ui/App.tsx` (the global shortcuts, the first-run flow, the tutorial start);
  - `ui/api.ts`;
  - `ui/locales/*.ts`;
  - `engine/src/model.rs`;
  - `engine/src/engine.rs`;
  - `engine/tests/engine.rs`;
  - `testkit/src/screen/setup.rs`.

## B8. Settings

### Requirement

Settings has an Appearance section with themes and several colours [D141]. The mock's s5 is its design [mock, notes].

### Components

`ui/Settings.tsx` is reorganized into the mock's sections [mock] (AI-21):

**General.**

- Language, as today [code].
- The switch "Projects run my own CLI hooks". It moves here from the former Projects section [code].

**Appearance (s5)** [mock, notes].

- **Choice controls:**
  - the six preset cards, in the order Sala, Oficina, Bancada, Manhã, Alto contraste, Cobalto [tokens];
  - five accent swatches, plus a custom colour (native colour input and a hex field);
  - the ground (5 options);
  - the text font (3);
  - the heading font (4);
  - the size slider (13–18 in steps of 0.5);
  - density (2);
  - corners (3);
  - motion (3).
- **Locked colours.** A read-only row shows the three locked meaning colours with their shapes and words, as Erick decided
  [R77].
- **Files.** "Save as file" and "Open file" (B3).
- **Preview panel**, labelled "Not applied yet":
  - a scaled-down live chat;
  - the measured contrast of text, faint text and accent;
  - the three effect switches.
- **Actions.**
  - "Back to default" resets the draft to the factory look: Sala, the theme's sizes and the system's motion (AI-21).
  - "Use this look" applies the draft (B3). It saves first, and applies only after the save succeeded.

This replaces B3's simple Appearance section, which saved each choice at once.

**Connected accounts.** Today's subscriptions section [code]. Its button opens `Setup` in `manage` mode (B7): only the
account cards, with a Close button, and no first-run steps. It cannot touch the appearance, the experience answer or the
tutorial.

**Usage limits.** The quota readings that leave the header: one per vendor and window, with the reset time. They come
from `AppState.quotas` (B5), so they are the same after the window is closed and reopened. A reading whose reset time
has passed, or a vendor with no reading yet, shows "—".

**Privacy.** Three plain statements [inspiração] (AI-21):

- "Telemetry: off."
- "History: only on this PC."
- "Your Claude and Codex logins: Faber never reads them."

### Data flow

As in B3, steps 3 to 5, including the ordering of previews. Settings opens with Ctrl+, and closes with Esc, as today
[code].

### Error handling

As in B3, including a failed save: the draft stays, `:root` keeps its look and the error shows.

### Tests

**UI checks.**

- Choosing a preset keeps `textSize`, `density` and `motion` and clears the rest.
- A rejected `save_settings` after "Use this look" leaves `:root` and the draft as they were.
- Previews: the answer to an older request that arrives last does not replace a newer preview.

**Screen.**

- The draft does not change `:root` until "Use this look".
- An accent near amber (#f0a030) shows the refusal sentence, and "Use this look" stays as it was.
- "Back to default" returns the preview to Sala.
- The Privacy section shows the three statements.
- Connected accounts opens the account cards only. Closing it returns to Settings, with the appearance unchanged.
- Usage limits keeps its readings after the page is reloaded.

### Files touched

`ui/Settings.tsx`, `ui/api.ts`, `ui/locales/*.ts` and `testkit/src/screen/app.rs`.

## B9. CLI download straight to disk

### Requirement

Today the CLI installer download holds the whole file in memory while hashing it, and copies it once more: about 200 MB
for `claude.exe` [estado, A12; code: `default_fetch` ends in `res.bytes()` and `.to_vec()`]. The payload must go to a
file while it is hashed. The small metadata documents are a different case: the Claude version text, the Claude
manifest and the Codex release JSON must be parsed, so they stay in memory, under a hard size cap.

### Components

`engine/src/install.rs`, and its public wrapper `Cli::install_windows` in `engine/src/cli.rs`:

- **Two kinds of fetch**, where there is one today (`Fetch`, returning `Vec<u8>`):
  - **`Fetch` (metadata)** keeps its signature and its use for the three small documents. `default_fetch` now stops
    after 4 MiB and returns an error beyond that, so a bad server cannot fill memory.
  - **`Download` (payload)** is new: `Fn(&str, &Path) -> io::Result<String>`. It streams the body into the file at the
    given path, one received chunk at a time, hashes each chunk as it goes (SHA-256), and returns the hash in lowercase
    hex. The file is closed, with its handle dropped, before the function returns. `default_download` is its network
    form and keeps the URL checks and the loopback rule [code]. The caller never holds the body. It is used for
    `claude.exe` and for the Codex archive, and for nothing else.
- **Where files live.** All under `<data>/install-tmp/`, which the cleanup guard empties at the end [code]:
  - a download goes to `install-tmp/download/<name>.part`;
  - extraction goes to `install-tmp/codex-staging/`, and the publish step copies that whole folder to the destination
    [code: `publish_staging_to_destination`]. No download is ever inside `codex-staging`, so an archive can never be
    installed as if it were release content.
- **Claude path.** What runs does not change. After the hash matches the manifest's checksum, the verified file is
  renamed to `claude-{version}-{platform}.exe` in `install-tmp`, and the official binary is run with `install`,
  unmodified, as today [code]. Faber unpacks nothing for Claude. Because Windows refuses to start a file that is still
  open for writing, the download's handle is already closed (see `Download`) when the installer is spawned.
- **Codex path.** After the hash matches the release digest, `extract_zip` and `extract_tar_gz` read the `.part` file
  (they take a `File`, not a byte buffer) into `codex-staging`. Then the existing identification, publish and PATH steps
  run as they do today.
- **Signatures.** `Cli::install_windows` in `cli.rs` (today it requires `Fn(&str) -> io::Result<Vec<u8>>` [code]),
  `install::install_windows`, `default_fetch`, the new `default_download`, the call in `src-tauri/src/setup.rs`, and
  every typed adapter in `engine/tests/install.rs` (its own `Fetch` alias and the two `install_windows` shims) all take
  both `Fetch` and `Download`.
- **Unchanged.** The URL checks and the loopback rule stay as they are [code].

### Data flow

1. Step 1 of the first run asks to install a CLI.
2. `install_windows` checks each URL before using it.
3. The metadata documents are fetched into memory, 4 MiB at most each, and parsed.
4. The payload streams into `install-tmp/download/<name>.part` while it is hashed.
5. The hash is compared. Claude: rename, then run the official installer. Codex: extract from the file into
   `codex-staging`, then publish as today.
6. The guard empties `install-tmp`.

### Error handling

- **Hash mismatch.** The `.part` file is deleted at once, the guard cleans the rest, and the same error key as today is
  returned (`error.setup.checksum`).
- **Interrupted download, or a failed write** (a full disk, say). The partial file is deleted. The key is
  `error.setup.download` for a network failure and `error.setup.install` for a write failure.
- **Metadata over the cap.** `error.setup.download`.
- **A file that cannot be closed or renamed.** `error.setup.install`.

### Tests

**Rust (`engine/tests/install.rs`):**

- A streamed install produces the same files as before, for both vendors. Only the release's contents reach the
  destination: neither a `.part` file nor the archive is among them.
- A wrong hash leaves no file behind.
- A fetch that fails halfway leaves no file behind.
- **Bounded memory, file progress.** The test serves a generated 64 MiB body from a loopback server, in 8 MiB steps
  with a pause after each. While the body is still arriving, the test reads the size of the `.part` file. Before the
  last step is sent, the file must already hold all the earlier steps, and at the end its size and hash must match
  what was sent. An implementation that buffers the body, as `res.bytes()` does today, leaves the file empty until the
  end and fails. This does not catch an implementation that writes each chunk and also keeps it, so the next test
  exists.
- **Bounded memory, process memory** (AI-25). This test sits alone in `engine/tests/install_memory.rs`, so no other test shares
  its process. The workspace forbids `unsafe_code`, so it counts no allocations. It reads the resident memory of its own
  process with the `sysinfo` crate (a dev-dependency of `engine`), which works on all three systems.
  - The loopback server makes a body of 32 MiB and then one of 160 MiB. It writes 64 KiB at a time from a counter and
    never holds the body, and it hashes as it goes.
  - For each body, a sampler task reads the process memory every 10 ms while `Download` runs. The test keeps the
    baseline taken just before the call and the highest reading during it.
  - It asserts that the highest growth for the 160 MiB body is at most 24 MiB, and that the highest reading for 160 MiB
    exceeds the one for 32 MiB by at most 16 MiB. The first bound catches a full buffer. The second shows that memory
    does not grow with the payload. An implementation that writes each chunk and also keeps it grows by about 128 MiB
    between the two runs and fails. The returned hash must equal the server's.
  - The margins are chosen to leave room for allocator slack. If the first CI run on the three systems shows that
    they are too tight or too loose, only the two numbers change.
- **Write failure.** A destination that fails after 1 MiB makes `Download` return an error and leaves no `.part` file.
- **Metadata cap.** A metadata document of 4 MiB plus one byte is refused.
- **Handle release (Windows).** Right after `default_download` returns, the file can be renamed and deleted, which
  Windows refuses while a handle is open. When the stand-in for the Claude installer is started, it can open the
  staged file for writing exclusively, which works only if Faber has closed its own handle.

**Screen.** C5.2 (install from loopback) still passes.

### Files touched

- `engine/src/install.rs`;
- `engine/src/cli.rs` (the public `install_windows` wrapper);
- `engine/tests/install.rs` (its adapters and the new tests);
- `engine/tests/install_memory.rs` (new) and `engine/Cargo.toml` (the `sysinfo` dev-dependency);
- `src-tauri/src/setup.rs` (call sites).

## B10. Memory checks

Every scenario measures the private working set of Faber's whole process tree, with no CLI running. Each must stay at
or under 250 MB [AGENTS]. These are `screen` scenarios, so they run on Windows, as today's memory tests do [code]; the
numbers come from WMI (B2):

| # | Scenario | Source | Added by |
| :--- | :--- | :--- | :--- |
| M1 | Idle on s1 in Sala with all effects on, after 10 s, with the dust canvas running | Updated from today's idle test [code] | B3 |
| M2 | After applying each of the six looks in turn, which loads every font family, then idle on s1 | New | B3 |
| M3 | A conversation of 1,000 messages | Today's test [code] | B2 (ported) |
| M4 | The first run open on step 2, with no conversation | Updated from C5.4 [code] | B7 |

The runner prints each measured value, so growth stays visible between commits. A new piece that adds a scenario adds
it here. The CLI download is covered by the bounded-memory test in B9, because it does not run at idle.

## B11. Out of Phase 1

Each item below is out of Phase 1. The phase comes from R45, or from AI-1 where R45 is silent.

| Item | Phase | Source |
| :--- | :--- | :--- |
| Visual answers (about 10 pieces, saved state) | 2 | [R73, R39], AI-1 |
| Auto learning from approvals; draft routed at most twice per message | 2 | [R21, R22] |
| Asking Faber about Faber | 2 | [R18], AI-1 |
| Search; moving a chat to a project; the composer's project picker | 2 | [R19, mock], AI-1 |
| The rest of "full Chat" (features beyond today's) | 2 | [R45, D18] |
| Benchmark and Erick's OK before each version | 2 | [R23, R24], AI-1 |
| Progress view and part detail (s3, s4); percentage and "stopped" tails in the sidebar | 3 | [R52], AI-1 |
| Decision list | 3 | [R16], AI-1 |
| Lead agent, toolbox, 15 powers, modes, guardrails, big-work rules | 3 | [R8, R47, R48, D63, R36, R37] |
| Learning, global memory promotion, `.faber/` folder | 3 | [R15, R35, R51], AI-1 |
| Catalog screens and the catalog step in the first run | 3 | [R33, D94–D100], AI-1 |
| Live manifest and nightly CLI version tests | 4 | [R34], AI-1 |
| Shared MCP per project, RAM budget for parallel workers | 4 | [estado] |
| First public version | 5 | [R45] |
| Creative drafts and blind judging; comparison views | 6 | [R41, R42] |
| Phone remote control, messaging channels, routines | 7 | [R13, R14] |
| Curioso animations | Later, not planned | AI-13 |
| Community themes listed in the catalog | Not planned | [D143]; sharing a theme file directly covers R60, AI-1 |
| Code signing and the Microsoft Store | Not planned | [R43] |
| Reporting failed writes of conversation and project files (Phase 1 covers settings only) | 2 | AI-1; review log 6 |
| The engine side of A19: permission mode, model and effort on every start; the interrupt wait and silence watchdog; result and reason mapping | 2 (rules 2, 3, 5, 6); rules 4 and 7 in 3 | A19, AI-22 |

## B12. Phase 1 is done when

1. **Proof commands.** All proof commands in B2 pass locally on Windows, including `cargo t --all`. The CI run on
   Windows, Linux and macOS (the `rust` and `ui` suites on all three, `screen` on Windows) is not part of this list:
   it needs a push, so it is the gate for merging `phase-1-base` after Erick approves the first push (item 11).
2. **Old suite gone.** No Node test file and no `playwright-core` remain.
3. **License.** The license is `FSL-1.1-ALv2` [R79] in `LICENSE`, `package.json`, `package-lock.json`, `Cargo.toml`, `README.md` and
   `AGENTS.md`, in one commit of its own.
4. **Documents.** `docs/roadmap.md` and `docs/design.md` match this document.
5. **Looks.** The six looks render s1, a chat thread, Settings and the first run, and every one passes the twenty-six
   contrast pairs.
6. **First run and tutorial.** The first run has three steps, and the tutorial follows R18.
7. **Running list and tray.** The running list follows R53 and R54. The tray follows R74.
8. **Icons.** The new icon shows in the taskbar, the tray, the installer and the window.
9. **Memory.** M1–M4 stay within 250 MB.
10. **Hand check.** Erick has seen it in "Faber (ao vivo)" [D180]. The tray and notification hand check was done on a
    local installed build (B6). No public release is made (AI-3).
11. **Branch.** All of this sits on `phase-1-base`, which is not merged and not pushed until Erick approves (AI-24). The first push, which he approves, runs `cargo t --all` in CI on three systems, and the
    merge waits for that run to pass.

---

# AI choices made in this spec

Each item fills a gap the sources leave open. Each is marked in the text above as "(AI proposal, AI-n)".

1. **AI-1. Phase placement where R45 is silent.**
   - Phase 2: the benchmark, visual answers, asking Faber about Faber, search, and moving a chat to a project.
   - Phase 3: the progress view, the decision list, the catalog and learning.
   - Phase 4: nightly CLI tests and the live manifest.
   - Not planned yet: community themes listed in the catalog [D143].
2. **AI-2. macOS updates.** macOS self-updates under R50. It falls back to D160's notification only if a hand test
   shows that an unsigned update cannot install.
3. **AI-3. No public release in Phase 1**, because R23 needs the benchmark first. Erick checks Phase 1 in the live app
   and through a local installer build.
4. **AI-4. Phase 1's contents and order.** B1 to B9, including B9 (download to disk), which comes from estado's pending
   list.
5. **AI-5. The test runner's design.**
   - A `testkit` crate and `cargo t`.
   - Three suites, chosen by changed paths.
   - TypeScript checks run inside the hidden Faber, which keeps them away from Node. The app reports their results to
     the runner itself (a `test-hooks` feature), so the `ui` suite runs on Windows, macOS and Linux.
   - Elements are found through `__faberText`.
   - Memory is read through WMI, in the `screen` suite only, so the launcher and the audit run on every OS: Windows
     takes process identity from WMI creation times, macOS and Linux from `sysinfo`.
   - Only `screen` runs on Windows alone.
   - The test build uses a test-only Tauri config that removes the build hook, so it never goes through `npm` or
     writes `dist/`.
   - Every launch uses the full sandbox contract and audit of today's setup test.
6. **AI-6. Theme logic in the Rust shell.**
   - `look.rs` and `themes.json` hold it, while the engine keeps the appearance opaque.
   - The first paint uses the theme's frame colour.
   - A legacy or invalid saved appearance falls back to Sala.
   - When a theme file changes `bg`, the meaning colours come from Sala or Manhã, whichever contrast reads better.
   - Twenty-six required pairs, not nine, so every ink is checked on every surface where it appears.
   - Factory looks keep their own accent inks. The pure black or white rule is for accents the user picks.
   - The hue block uses the HSL hue, and the bands of Claude and amber join.
7. **AI-7. English for the theme file.** Its keys are English, the extension is `.faber-theme.json`, the effect
   switches are booleans, and `name` is limited to 80 characters. The draft used Portuguese keys.
8. **AI-8. Translated theme names.** Theme and preset names are translated per language. In English they are Dark
   Room, Workshop, Workbench, Morning, High contrast and Cobalt; pt-BR keeps R60's names.
9. **AI-9. No light/dark switching that follows the system.** The light look exists only as the Manhã preset [R59].
10. **AI-10. Fonts.** Fonts come from Fontsource, latin and latin-ext subsets only. Font packages left unused are
    removed.
11. **AI-11. Screen details the mock leaves open.**
    - The display heading is clamped to `9vw`.
    - The reading column is 660 px at most. Below 1,100 px the running column overlays the stage, and below 900 px the
      sidebar goes behind a button.
    - Bancada's flow line names only the connected vendors.
12. **AI-12. The wordmark font is Rubik Bold**, converted to outlines.
13. **AI-13. The mark inside the app.**
    - It always uses the ink colour, never lime.
    - The titlebar shows the lockup in every theme.
    - Curioso appears static in the empty states, not on s1, and has no animation in Phase 1.
14. **AI-14. Icon build.** `tauri icon`, plus the hand-tuned 16 and 24 px frames placed in the ICO. Tray PNGs at 16,
    24 and 32 px.
15. **AI-15. Content of s1 and the sidebar footer.**
    - The greeting and the sidebar footer show no user name.
    - Four suggestions fill the composer without sending.
16. **AI-16. The Phase 1 sidebar.**
    - No search.
    - Projects expand.
    - Chats are grouped as Today, This week and Earlier.
    - Project tails show "{m} needs you" in amber, or a running dot.
    - The quota summary moves from the header to the footer and to Settings. The engine keeps the latest reading of
      each window, with its reset time, so the summary survives a rebuilt window.
17. **AI-17. Running list mechanics.**
    - The engine builds and orders the list, in a stable order, with a revision that the screen and the tray use to
      ignore older lists.
    - An item leaves the list when its turn ends. A reply still waiting for Auto's pick is listed too, with no vendor
      yet.
    - The column auto-opens 1.5 s after the last keystroke, once per question per window, including questions
      already waiting at launch. A question is identified by conversation, turn and request id together.
    - Auto-opening never moves focus, at any width. At the overlay widths it opens non-modal and ends above the
      composer. Opened by the user, the overlay is modal.
    - With no items, the button reads "Nothing running".
    - Opening a row sets the area, the project and the conversation together.
    - `answer` carries the turn id, reports a stale or undelivered request, and a permission leaves the list only after
      the session confirms that the answer was written to the CLI (AI-25).
18. **AI-18. Tray badge and notifications.**
    - The badge is an amber circle with 1–9 or "+".
    - The tooltip carries the words.
    - Notifications go out only while the window is unfocused or closed, once per question (the same three-part key as
      the running list).
    - Faber does not rely on clicking a notification.
    - `set_shell_texts` carries `{one, other}` templates.
    - The delivery, identity, focus and closed-window checks are made by hand on an installed build.
19. **AI-19. First-run flow.**
    - Skip and "None" save at step 1.
    - Upgraded installs see the Appearance step once.
    - An unanswered experience question counts as beginner.
    - `Setup` has a first-run mode and an account-management mode. Reconnecting an account never reopens the first run
      or resets `tutorialDone`.
    - Esc and Ctrl N do nothing on the first-run steps.
20. **AI-20. Tutorial content.** Beginners get three cards (Chat, Projects, Follow along), adapted from the
    inspiração proposal. Experienced users get two (Auto and the running list). The cards are modal, work by keyboard
    alone, stay centred when their anchor is missing, and wait while no AI is connected.
21. **AI-21. Settings layout.**
    - The hooks switch moves to General.
    - Privacy shows three plain statements.
    - "Back to default" means the factory look.
22. **AI-22. Rules for driving the CLIs (A19).** The nine rules come from the 2026-10-11 research and the model-use
    notes. Wherever the research marks a point as inference, the mapping to Faber's behaviour is an AI proposal, and
    each rule is re-measured when a CLI version changes. The known legacy exceptions in today's engine and the phases
    that close them (2 for rules 2, 3, 5 and 6; 3 for rules 4 and 7) are placed by the AI. Rule 7 follows R37 exactly:
    a warning and economy mode, never a kill for budget. The cache effect of an effort change is stated per the Claude
    Code documentation, and the rule against needless changes stays.
23. **AI-23. The build team's UI tools (B0).** `redesign-existing-projects` audits a screen before a piece changes it.
    `design-taste-frontend` is the style guide and `impeccable` the judge, both already chosen by Erick for design
    [R56]; the AI only fixes when in the process each is used. All three are development tools and never ship.
24. **AI-24. Decided questions on an unmerged branch.** Open questions 1 and 2 are Erick's and are decided [R77, R79].
    Phase 1 builds on the local branch `phase-1-base`, which is not merged or pushed until Erick approves. The license
    change is its own small commit. The colour lock lives in three places only.
25. **AI-25. Mechanics added after the second review.**
    - `answer` takes the turn id, and the session task confirms each answer after writing it. `Engine::answer` is the
      asynchronous owner and waits for the confirmations for at most 5 s, while `Core` stays synchronous (B5a).
    - `save_settings` returns `{settings, look}` (B3).
    - B5a also ports the one old caller of `answer`, so that it passes alone (B5a).
    - The test launcher takes process identity from WMI creation time (microseconds) on Windows and from `sysinfo` on
      macOS and Linux, and kills the app it started through its `Child` handle (B2).
    - The bounded-memory test for downloads reads its own process memory over a 32 MiB and a 160 MiB body, with margins
      of 24 MiB and 16 MiB that the first CI run may adjust (B9).
    - The failed-save test blocks the temporary file and leaves a valid, non-default settings file in place (B3).

There are 25 AI choices in total.

# Open questions for Erick

Both questions are decided by Erick [R77, R79]. The branch rule of AI-24 still applies.

1. **Locked meaning colours, or full freedom?** Decided: the three meaning colours stay locked [R77]. Amber ("needs
   you"), Claude coral and Codex violet are fixed, and the design refuses accents close to them. The lock lives in the
   hue block of the validator, the `locked` section of `themes.json` and the read-only row in Settings (A13, B3, B8).
2. **License identifier.** Decided: FSL-1.1-ALv2 [R79], replacing the AGPL-3.0 choice of R25. Faber is source-available,
   not open source. Each version becomes Apache-2.0 two years after its release (A15, B1).

---

# Appendix: Review log

## Round 1

The cross-vendor review of this spec gave 30 findings. Each was checked against the code it cites before the spec
changed. All 30 hold. Result: 29 accepted, 1 partly accepted, 0 rejected.

1. **Accepted.** R76 is cited. Erick-authored decisions outrank AI proposals. The two reserved questions are built as
   reversible defaults on the unmerged, unpushed branch `phase-1-base`. The license is its own commit (AI-24, Open
   questions, B0, B1).
2. **Accepted.** The test sandbox keeps the whole contract of today's setup test: `USERPROFILE`, `HOME`, `APPDATA`,
   `CODEX_HOME`, removed provider variables, a dead proxy, an empty `PATH`, and the audit of writes outside the sandbox
   (B2).
3. **Accepted.** B3 ports `App.tsx`, `Settings.tsx` and the appearance scenario in the same commit as the new saved
   shape. Later layout changes stay in B5b and B8 (B0 commit order, B3).
4. **Accepted.** The test build uses `src-tauri/tauri.test.conf.json`: no build hooks, no bundling, output in
   `dist-test/` (B2, AI-5).
5. **Accepted.** B9 migrates the public wrapper in `engine/src/cli.rs` and every typed test adapter with the new
   signatures (B9).
6. **Accepted.** `Store::save_settings` returns `io::Result`. The engine returns `error.settings.saveFailed`. The shell
   passes it on, the UI awaits saves, and tests cover failed writes (B3, B7, B8).
7. **Accepted.** Factory looks keep their own accent inks (Sala `#0e0e10`, 16.93:1). The pure black or white rule
   applies only to accents the user picks. The reference table and tests agree (B3, AI-6).
8. **Accepted.** Twenty required pairs replace the nine, including `ink`, `ink-2` and `ink-3` on `bg`, `s1`, `s2` and
   `s3`. Every factory look was computed and passes; the lowest is 4.59 (B3, A13, AI-6). Round 2, item 2, extended this
   to twenty-six pairs.
9. **Accepted.** The running list also holds a reply still waiting for Auto's pick, with no vendor yet. Tests cover
   completion, cancellation and routing failure (B5a, AI-17).
10. **Accepted.** A question is identified by conversation, turn and request id together. Tests cover equal ids from
    both areas and after a server restart (B5a, B6, AI-17).
11. **Accepted.** `answer` reports stale and undelivered requests. A permission leaves the list only after the session
    took the answer, and `Session::answer` returns `bool`. Both copies of the card are tested (B5a, B5b). Round 2,
    items 3 and 10, replaced the `bool` with an acknowledgement and added the turn id.
12. **Accepted.** Opening a row sets area, project and conversation in one step. Tests move from Chat to another
    project's running conversation and send there (B5b, AI-17).
13. **Accepted.** The snapshot and the events carry a revision. The screen subscribes first, then reads the snapshot,
    and drops anything older. A test covers a permission arriving while the window is rebuilt (B5a, B5b).
14. **Accepted.** `AppState.quotas` holds the latest reading of each window with its reset time. Unknown and expired
    readings are defined, and a test closes to the tray and reopens (B5a, B5b, AI-16).
15. **Accepted.** `Setup` has `firstRun` and `manage` modes. Reconnecting an account never reopens the first run and
    never resets `tutorialDone` (B7, B8, AI-19).
16. **Partly accepted.** The finding says the current path hides the composer. The code renders it but it cannot send
    when no AI is connected. The gap is real anyway: the tutorial and the no-AI state were undefined. The spec now
    defines that the cards wait while no AI is connected, stay centred when an anchor is missing, and a test covers
    "None" through tutorial completion (B7, AI-20).
17. **Accepted.** The reading column is a maximum of 660 px. The width table covers 1,100 and above, 900 to 1,099, and
    720 to 899, and tests run at 720 px, around the breakpoints and with large text (B5b, AI-11).
18. **Accepted.** Every overlay says whether it is modal, moves focus in, returns it, and has defined Esc and Ctrl N
    behaviour. Coach cards use `data-tour` anchors. Onboarding is tested by keyboard alone (B0 rules, B7).
19. **Accepted.** The `ui` suite stays portable. The app reports its TypeScript checks itself through a `test-hooks`
    feature, so macOS and Linux still run them without Node or a DevTools port. Only `screen` is Windows-only (B2,
    AI-5).
20. **Accepted.** B2 ports the appearance scenario unchanged. B3 replaces it in the commit that changes the shape
    (B0 commit order, B2, B3).
21. **Accepted.** B1 keeps the roadmap's table of 12 powers as it is and adds rows 13 to 15 from R48 (B1).
22. **Accepted.** Downloads go to `install-tmp/download/<name>.part`, apart from the extraction tree
    `install-tmp/codex-staging`. A test checks that neither the `.part` file nor the archive reaches the destination
    (B9).
23. **Accepted.** B9 describes both paths. Claude renames the verified binary and runs the official installer with
    `install`, after Faber closes its file handle. Codex extracts the archive (B9).
24. **Accepted.** `Fetch` stays for metadata with a 4 MiB cap. A new `Download(&str, &Path) -> io::Result<String>`
    streams payloads. File ownership, paths and cleanup are defined (B9).
25. **Accepted.** A generated 64 MiB download is served in steps. The test checks that the `.part` file grows while the
    body is still arriving, so a full buffer fails. Write-failure cleanup and Windows handle release are tested too
    (B9, B10).
26. **Accepted.** Notification delivery, Faber identity, focus suppression and closed-window behaviour are checked by
    hand on an installed build (B6, AI-18).
27. **Accepted.** Each `preview_look` request carries a growing number and only the latest answer is applied. A test
    delivers two answers in reverse order (B3, B7, B8).
28. **Accepted.** `rename` of a running conversation emits `Running` with the new title, and the recompute list
    includes `rename` (B5a).
29. **Accepted.** The hue block uses the HSL hue, measured the short way round. The bands of Claude and amber join, so
    the allowed hues are 64 to 221 and 269 to 350. Exactly 24 passes. Tests cover the boundary, the overlap and the
    wrap at 0 (A13, B3, AI-6).
30. **Accepted.** A9 says the conversation history of both areas, Chat and Projects, stays private inside Faber. Only
    the named project-memory files live in `.faber/` [R35] (A9).

## Round 2

The reviewer checked the revision and gave 13 more items. Each was checked against the code, the decision ledger or the
documentation it cites before the spec changed. All 13 hold. Result: 12 accepted, 1 partly accepted, 0 rejected.

1. **Accepted (blocker).** B3 asked for greeting checks, but the greeting is built in B5b, and `ui/App.tsx` still
   renders the old empty state at that point. B3 now styles and checks today's empty-state heading and mounts the
   ambient effect there. B5b replaces it with the greeting and carries the greeting's checks. B4 got the same repair:
   it gives Curioso only to the empty states that exist when it lands, and later pieces add the rest (B3, B4, B5b, B0
   commit order).
2. **Accepted.** The `.amber-tag` sits on `s3` in the mock, so every meaning ink is now checked on `bg`, `s1`, `s2` and
   `s3`: twenty-six pairs. All six factory looks pass (lowest meaning-ink ratio 5.39, Cobalto `claude-ink` on `s3`).
   The reviewer's counterexample is a test: white plain inks with `s3` at `#666666` pass the plain pairs (5.74) and fail
   `amber-ink` at 3.68 (B3, AI-6, B12).
3. **Accepted.** Queue submission is not delivery. `SessionCommand::Answer` carries an acknowledgement that the session
   task sends after `write_line` has written and flushed (`write_line` now returns an `io::Result`). A permission leaves
   the list only on `Delivered`, and tests use a stdin that fails every write (B5a).
4. **Accepted.** The launcher and the audit use `sysinfo` for the process tree on macOS and Linux (Windows identity was
   refined in round 3, item 3). WMI and the
   `wmi` crate are Windows-only, used by the memory scenarios of `screen`, and declared under `cfg(windows)`. The sandbox
   covers `TMPDIR` and `XDG_*` on macOS and Linux, and the `ui` suite always turns CLI discovery off (B2, B10, AI-25).
5. **Accepted.** A second test, in its own test binary, reads its process memory while `Download` fetches a 32 MiB and
   a 160 MiB body, and asserts both a bound and no growth with size. A writer that also keeps every chunk fails it. The
   workspace forbids `unsafe_code`, so it counts no allocations (B9, AI-25).
6. **Accepted.** `save_settings` returns `SavedSettings` `{settings, look}`. The Rust command, `ui/api.ts` and the two
   `api.saveSettings` calls in `ui/App.tsx` move together in B3 (B3, AI-25).
7. **Accepted.** R53 says the column does not move the cursor. The column opens non-modal at every width when it opens
   by itself: focus stays, nothing goes inert, it ends above the composer, and Esc acts only from inside it. Opened by
   the user, the overlay is modal. The overlay rule, the width table and the tests say so (B0 rules, B5b, AI-17).
8. **Accepted.** R37 says that passing the estimate by 50 percent only warns and moves to economy mode, and that "stop"
   only pauses. A19 rule 7 now says exactly that and no longer lets Faber kill a process for budget (A19).
9. **Accepted.** A19 now has a scope paragraph and a table of known legacy exceptions in today's engine (rules 2, 3,
   4, 5, 6 and 7), each with its owner phase. A17 and B11 carry the same phases. New code, including Phase 1's, follows
   the rules (A19, A17, B11, AI-22).
10. **Accepted.** `answer` takes the turn id and the engine checks it against the live turn before anything is sent. A
    delayed answer to request `0` of turn 1 cannot reach request `0` of turn 2. B5a also ports the one old caller so the
    commit passes alone, and a test covers the reused request id (B5a).
11. **Partly accepted.** The contradiction is real: CI runs only for `main` and pull requests, and the branch is not
    pushed. The reviewer's fix, a review ref, is not taken, because pushing waits for Erick. B2 is done on local Windows
    proof. The three-system CI run is the gate for merging `phase-1-base` after he approves the first push. The macOS and
    Linux `ui` claims stay unproven until then, and the spec says so (B2, B12).
12. **Accepted.** Checked against the Claude Code prompt-caching page. A change of model, tools or system prompt breaks
    the cache. A change of effort keeps it on Opus 5.5, Sonnet 5.5, Haiku 5.5 and Fable 5.1 with an API key or a
    subscription, and not on other models, Bedrock, Google Cloud's Agent Platform, a Claude apps gateway, with
    `CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS`, or under HIPAA. R76's rule against needless changes stays (A19 rule 3).
13. **Accepted.** `Store::settings()` returns the defaults for an unreadable file, so a directory at `settings.json`
    proved nothing. The test now keeps a valid, non-default `settings.json` and blocks the temporary path, then checks the
    bytes, the stored and live settings and that no idle CLI restarted. A failed rename is tested on the store's write
    function alone (B3).

## Round 3

The reviewer checked the second revision and found three more majors. All three hold against the code. Result: 3
accepted, 0 partly accepted, 0 rejected.

1. **Accepted.** `Engine` owns the mutex and `Engine::with` lends `Core` through a locked guard, so an `async` `Core`
   method cannot release the lock. `Engine::answer` is now the `async fn`: `Core::prepare_answer`, an unlocked wait,
   then `Core::settle_answer`, which re-checks that the conversation, turn and permission still exist (B5a).
2. **Accepted.** Both session handlers removed their pending entry before writing, so a retry after a failed write would
   find nothing and the engine would drop the card. They now remove it only after a successful write and flush. The
   tests run through the real Claude and Codex handlers with a stdin that fails once and then works, and check one
   written line (B5a).
3. **Accepted.** `sysinfo` start times are whole seconds. On Windows, identity is now the `Win32_Process` creation time
   to the microsecond, read through `wmi` as today's test does, and the app the runner started is killed through its
   `Child` handle. On macOS and Linux it is the `sysinfo` start time plus the name, and the spec states the ceiling:
   reuse inside one second needs a full wrap of the id space. A test covers equal ids in the same second with
   timestamps 5 ms apart (B2).
