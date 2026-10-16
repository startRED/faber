# Faber rework: an AI that leads other AIs and gets better by working

Date: 2026-10-12. Status: master design proposal. Nothing here is implemented. It authorizes no build except the small
additions marked **MVP+** in section 10, and those reach the MVP session only after this rework is done (Erick, ER4).

Read with: [self-development MVP spec](2026-10-12-self-development-mvp-design.md) and
[plan](../plans/2026-10-12-self-development-mvp.md) (unchanged), [Kits](2026-10-12-tool-selection-rework-design.md) and
[generated pipelines](2026-10-12-generated-pipelines-design.md) (their status notes say which sections still hold),
[auto quota pacing](2026-10-11-auto-quota-pacing-design.md), [UX sentinel](2026-10-12-ux-sentinel-design.md) and the
[TL-ADE pipeline map](../../research/2026-10-12-tl-ade-pipelines.md). This document repeats none of them; it cites them
by section. It does not edit `docs/roadmap.md`; section 12 lists the roadmap lines a later commit must change.
"r1" to "r6" name the six research notes of the 2026-10-12 rework session (official Claude and Codex guidance,
community practice, creative and software catalogs, quality checks); they live outside the repository, so every
outside rule here also carries its primary URL (section 9, appendix C).

**Authorship.** Erick decides direction, taste, money and legal. His four decisions from the 2026-10-12 card are ER1 to
ER4; his earlier directions E1 to E18 (Kits and generated pipelines specs) still bind. Every other choice is tagged
`RW-n`: an AI decision, Erick delegated, not reviewed. Research claims were checked against primary pages on 2026-10-12
(Claude Code 2.1.296, Codex CLI 0.162.1); appendix B lists what changed after that check.

**Revision 2 (same day).** Erick answered the six questions (ER5 to ER10, section 12), and the claude.dev briefing
on Claude 5.5 practice is folded in: typed routes and a held-out gate for scars, routes and skills (§1.1, §1.2, §4.2),
the opt-in route upload (§1.5), stop rules for unattended workers (§2.2, §6.2), judges that report reproducible
blockers (§6.4), cache pricing (§6.5) and pinned model and effort (§6.6). claude.dev posts are cited by key (S55,
O55, EFF, COST, CTX, EVAL, AUTO, CLOUD, FAST; URLs in appendix C). Their evidence is Claude-only, and the briefing
could not confirm who runs the site, though its prices match anthropic.com.

## 0. What changes, in one page

| # | Before (Kits + generated pipelines) | After (this rework) |
| :--- | :--- | :--- |
| 1 | Each request compiles a fresh plan and throws it away; recipes are hints | Faber accumulates: **routes** per task shape, **scars** from failures, **ghosts** of the best run, first-party tools. The compiler still plans every request from the request itself (E9), but seeded by the nearest route; a new shape gets a **tutorial level**. Changes to routes, scar rules and skills pass a **held-out gate** |
| 2 | Checks run at the end of steps | Every route names its **eyes** (fixed renders, measures, listening) and a polish order; the worker looks after every pass and keeps a pass only if it is better. Quality first, then speed, then cost (ER1) |
| 3 | The lead gets a small kit; models learn Faber from briefs | One **persona** (about 330 tokens) in every Chat and Projects session through the flags Faber already passes; knowledge sits at addresses read on demand |
| 4 | Tools install when a gap appears | At setup the person **picks areas**, and Faber installs those areas' recommended tools from source into the Tool store (ER3, ER6); heavy media tools wait for a job and a yes; a session gets only what its step needs; hot, warm and cold tiers plus the user's own shelf |
| 5 | Workers exchange free-text packets | **Typed envelopes** written by the engine; requirements, checks and user quotes travel as IDs expanded verbatim; terse style only in the `note` field (ER2); a distiller with a drop-ledger; receipts for every "done" |
| 6 | Missing tools are searched for, then the floor | Missing tools are searched in a fixed order, then **crafted** on a bench: cited brief, prototype tests, no-skill placebo, blind judging, promotion and decay, on both engines |
| 7 | Process skills are borrowed | The **Faber Method**: one development framework, enforced in engine code, for user code and for Faber building itself |

## 1. Core idea: New Game Plus

### 1.1 Terms

- **Shape.** What kind of job this is, computed by code from the labels, fingerprint and facets of Kits §4.1, §4.2 and
  §4.5: `{deliverable, domain, stack, purpose}`, where purpose (E17) also carries the risk group. Example:
  `{sprite-sheet, pixel-art, aseprite, sell}`. Models label; code builds the key (generated pipelines GP-3: models
  judge, code weighs).
- **Route.** The best known way to make one shape: steps (blueprint nodes, generated pipelines §5.3), each with role,
  model, effort and kit; the bar and its proofs (generated pipelines §6); the eyes and polish order (section 1.4); the
  scars; the ghost; and the runs it was minted from, with hashes. A route seeds the compiler; it never replaces it.
  Routes hold typed fields only (ids, enums, numbers, catalog tool and check ids), never free text from the web, tool
  output or traces, because notes kept between runs can carry planted instructions (AUTO) (RW-27).
- **Scar.** One permanent regression check born from a real failure, plus an optional one-line rule. It carries the
  labels where it applies, so a spliced route inherits only the scars that fit. The check always ships; the rule reaches
  briefs only after it beats no rule on held-out probes (section 1.2, step 7).
- **Ghost.** The best run's cost per step: quota share per vendor, minutes, tokens. The start card's estimate comes from
  it, and a run far above it is flagged.
- **Tutorial level.** The first run of a shape: cheap read-only scouts go first to find what is hard, the generated
  pipelines compiler builds the bar and steps (its §3 to §11), extra gates run, two other-vendor judges check, and the
  full trace is kept.

```json
{ "shape": {"deliverable": "sprite-sheet", "domain": "pixel-art", "stack": "aseprite", "purpose": "sell"},
  "version": 3, "status": "proven", "minted_from": ["run_0412", "run_0420"],
  "steps": [{"id": "S2", "role": "writer", "kit": ["faber-grid-snap@1.2"], "checks": ["C-grid32", "C-palette16"],
             "eyes": "contact sheet at 1x and 4x after every pass"}],
  "polish": ["grid", "palette", "light and outline", "timing", "pivot", "atlas"],
  "scars": ["SC-14"], "ghost": {"quota_pct": {"claude": 1.2, "codex": 0.8}, "minutes": 14} }
```

```json
{ "id": "SC-14", "check": "C-aa", "applies": {"domain": "pixel-art"}, "origin": "run_0412", "fired": 3,
  "rule": "Snap to the 32 px grid before any palette work.", "rule_state": "passed-held-out" }
```

`C-aa` (colours used by at most 2 pixels = 0) is a catalog check. The bench writes `rule` from typed failure fields,
never from tool output, web text or failure logs (section 4.2).

### 1.2 Life of a route

1. **Match.** Code compares the request's shape with the route book. Distance is a weighted mismatch (deliverable 0.4,
   domain 0.3, stack 0.2, purpose 0.1). Distance 0: follow the route. Up to 0.3: follow it and re-check the bar for the
   differing part. Above 0.3 the shape is new and gets a tutorial level, which starts from a splice (step 4) when some
   piece has a route within 0.6, and from the compiler alone otherwise (RW-1).
2. **Follow.** The route seeds this request's plan; it is not a rail. The compiler still writes the deliverable sheet
   and the bar from the request itself (E9; generated pipelines §3) and keeps a route step only where it feeds a check;
   the route adds its steps, kits, models, eyes and scars, so the compile is mostly code and is cheaper. A plan leaves
   the route only when the bar would fail, when the request goes past the route, or when the person asks; each
   departure is logged as a decision with its reason, and a departure that wins becomes the next route version (RW-2).
   This keeps "powers, not tracks" and E11's "no line run as a fixed pipeline": routes are learned per shape, never
   hand-written, and every request is still compiled. It changes the roadmap's "recipes are hints" only in that a
   matching route is the default seed.
3. **Mint.** Hard proofs mint routes; judge votes alone never do (the trap in the brief). A run becomes a *provisional*
   route when every program check passed, the other-vendor judges passed it in both orders, no mandatory check was
   "not run", and the person kept the result. It becomes *proven* after a second run on different inputs. A domain with
   no program check yet keeps a *draft* route, used as a hint only (RW-3).
4. **Splice.** For a new shape, code takes each piece (generated pipelines §8 pieces) from its nearest route and the
   union of the scars whose labels apply. Example: a Stardew Valley mod splices "C# code change", "game mod load test"
   and "pixel sprite sheet".
5. **Improve.** A change (a route version, a scar rule, a step kit) is kept only if it is better on both the train and
   the held-out test split of the shape's probe bank, by more than the noise of repeated runs: program metrics not
   worse and blind other-vendor pairwise wins. Train up with test flat is reverted as overfitting; a gain inside the
   noise is "do not merge". One change per round, aimed at one root cause (EVAL). At equal quality, a version at most
   0.8 of the ghost's cost wins: cost only breaks ties (ER1, RW-4).
6. **Challenge.** Near a window reset, with spare quota only (auto quota pacing §3, the shared-account pause, never the
   daily grant), a challenger runs probes of the shape (Kits §15.3 probe bank) without the route or with one step
   changed. It replaces the route only through the same gate (RW-5).
7. **Scar.** Every escaped defect (found later by the person, a reviewer, the UX sentinel or a proof) adds a scar's
   check to the route's bar at once: a check is a test, not prompt text. Its rule line enters briefs only after a
   held-out probes show the rule beats no rule; failure text never enters a route, scar, skill or persona.
   Briefs carry at most 5 scar rule lines, and a rule that stops helping on re-test drops out while its check stays
   (RW-6). Within one run a writer still sees its failing proof lines (section 6.3); that context dies with the
   session.

**Quality first, concretely (ER1).** For each step the route records the model and effort that produced its best proven
result. Faber uses that pair; a cheaper pair replaces it only through the held-out gate of step 5. With no evidence,
the step gets the strongest pair the pace allows, because spare quota buys quality (auto quota pacing §3). This narrows
the roadmap power "Right-sized": the bar is the best known result, not the first pass (RW-7). Model and effort are
pinned on every spawn and escalate on a failed proof, never down by pace alone (section 6.6).

**First code step.** `engine/src/book.rs`: types `Shape`, `Route`, `Scar`, `Ghost`; `fn distance(a, b) -> f32`;
`fn nearest(shape, routes) -> Vec<(RouteId, f32)>`; load and save through `store.rs`. Tests in `engine/tests/book.rs`
with JSON fixtures. Data: `<faber data>/book/routes/<shape>/<version>.json`, `book/scars.jsonl`; shipped routes are
read-only app resources, copied on write. Not `route.rs`, which stays the Auto model table.

### 1.3 Day one

Before the first public version, the maintainer plays about 60 shapes across all areas, spread over the weeks before
release (Erick, ER7), as tutorial levels on both engines: the 40
requests of generated pipelines §14.1 plus about 20 niche shapes (section 8.2). Routes and first-party skills ship as
data under `catalog/routes/` and `catalog/skills/faber-*`; third-party tools appear only as catalog ids, downloaded from
their source (AGENTS.md). Routes are written by Faber's own runs, so they hold no third-party text (RW-8).

### 1.4 Look and polish while making

Erick's example (ER1): a 3D model is rendered from six angles while it is built, then polished. The rule for every
route (RW-9), from the order the research supports (r6 §0; MT-Bench, https://arxiv.org/abs/2306.05685):

1. **Make one pass** in the domain's polish order.
2. **Look** with fixed eyes: the same viewpoints, sizes, lighting and crops every time, so passes compare.
3. **Measure** with programs first. They are repeatable and cannot be talked into a pass.
4. **Judge** only what programs cannot: binary, local questions to an other-vendor vision or audio model, pairwise in
   both orders, ties when the orders disagree. Vision models are weak at counting and alignment, so code computes those.
5. **Keep the pass only if it is better** (Kits §7.1 keep rule); otherwise revert and try the next idea or stop.
6. **Stop** when the bar passes and the last pass gained nothing, or at the budget. Eyes artifacts go into the receipt.

| Domain | Eyes after every pass | Program checks first | Judge asks (binary, local) | Polish order |
| :--- | :--- | :--- | :--- | :--- |
| 3D and Blender | Six orthographic views on a fixed rig (front, back, left, right, top, bottom), grey clay first, then a turntable sheet | Blender 3D Print Toolbox "Check All" (solid, intersections, degenerate, distorted, thickness); glTF-Validator; asset auditor defaults (100,000 triangles, 5 materials, power-of-two textures); silhouette overlap with the six reference views computed in code | Floating parts? Interpenetration? Reads at thumbnail size? Seams? | Scale and origin, topology, silhouette, UVs, materials, lighting |
| Game assets | LOD0 and LOD1 side by side at 1 m and 10 m | Frame time (16.7 ms at 60 fps), texture memory, LOD chain present, power-of-two | Silhouette holds at distance? | Profile, draw calls and memory, LODs, silhouette triangles |
| Pixel art | Contact sheet at 1x and integer scale, animation played as frames | Palette size, grid snap, colours used by 1 or 2 pixels, constant frame size and pivot, atlas padding | Light direction and outline same in every frame? Reads at 1x? | Grid, palette, light and outline, timing, pivot, atlas |
| Icons and 2D | Each icon at 16, 24, 32, 48 px, greyscale and dark | 3:1 non-text contrast (WCAG 1.4.11), 24 px targets (2.5.8), grid and live area | Same family on one sheet? Detail lost at 16 px? | Grid, optical alignment, stroke, drop detail, colour |
| Logo | 16 and 32 px squint render, one-colour and greyscale, mock-ups (tab, avatar, letterhead) | Vector validates; favicon square, larger than 48 px recommended; Android 108/66 dp safe zone | Recognisable without colour? | Simplify, small sizes, one colour, type, mock-ups |
| Photo and thumbnail | Phone-size feed mock beside real competitors | Clipped pixels under about 1%, 16:9, at least 640 px wide, 2 MB on mobile | Subject and text readable in about one second? | Exposure, white balance, subject, text, sharpen |
| UI and frontend | Screenshots at 320, 375, 768, 1024, 1440 px, light and dark, 400% zoom | Core Web Vitals p75 (LCP ≤ 2.5 s, INP ≤ 200 ms, CLS ≤ 0.1); WCAG 2.2 AA contrast, targets, reflow; `npx impeccable detect --json` (59 rules, no model); pixel diff (Playwright threshold 0.2) | Overlap? Truncation? Primary action obvious? | Layout breaks, accessibility, performance, visual polish |
| Backend and API | Rendered docs and sequence diagram | OpenAPI validation; object-level authorization matrix per role (OWASP API1:2023); k6 thresholds; N+1 count; secrets | Docs match the requirements? (reviewer, not gate) | Auth and validation, status codes, data access, load test, docs |
| Desktop app (Faber itself) | UX sentinel walkthroughs at the shell widths and themes (UX sentinel §4) | Proof commands of AGENTS.md; idle RAM ≤ 250 MB test; window audit; UX sentinel detectors; four timed journeys (launch to typeable, start chat, load chat, send) against checked-in ceilings that CI only lets fall (FAST) | UX sentinel rubric, two vendors (its §8) | Crashes, scaling, keyboard and focus, accessibility names, copy |
| Video and animation | A frame every 2 s at 1080p and phone size; key poses as a contact sheet | Flashes (WCAG 2.3.1); loudness; captions (Netflix English: 2 lines, 42 characters per line, 20 per second); FFmpeg `blackdetect`, `freezedetect`, `silencedetect` | Text inside the safe area? Face stays the same between shots? | Cut, audio, captions, colour, motion, encode |
| Audio, voice, SFX | Spectrogram images; speech-recognition round trip against the script | BS.1770 loudness and true peak against the target (Apple Podcasts −16 LKFS ±1 and ≤ −1 dBFS; Spotify −14 LUFS; ACX RMS −23 to −18 dB, peaks ≤ −3 dB, noise ≤ −60 dB, 1 to 5 s room tone); clipping and click counts | Mispronounced name? Sibilance? (an audio model cannot judge a mix: say so) | Clipping and clicks, noise, levels, EQ and de-ess, loudness, metadata |
| Music | A/B against a reference matched to the same LUFS | Loudness and true peak after encoding; mono fold-down drop | Masking or muddy low mids on the spectrogram? | Arrangement, balance, low end, dynamics, master, encode |
| Game mods | Same save and camera path with the mod on and off, pixel diff | Loads with zero errors (LOOT for Bethesda games; Minecraft tick ≤ 50 ms); frame time and memory delta; crash count over N launches | Seams, z-fighting, missing assets in changed regions? | Load order, dirty plugins, hot path, allocations, cache |
| Text and scripts | Text-to-speech read-through | Facts and citations; readability for the audience (Flesch); AI-tell phrase scan (Wikipedia list, one sign proves nothing); repeated n-grams | Claim not supported by the brief? | Facts, structure, rhythm, word choice, formatting |
| Slides and documents | Contact sheet of rendered pages | Text inside its frame (from the rendered PDF); 4.5:1 contrast; alt text; heading order | One message per slide? | Message, overflow, grid, contrast, notes |
| Data and spreadsheets | Each chart at its use size | Table Schema validation; totals reconcile; zero error values; computed twice | Axis or unit misleading? | Schema, cleaning, reconciliation, aggregation, charts |
| Research reports | Rendered PDF | Every DOI resolves at doi.org; Crossref `update-to` for retractions; every quote found in its source | Hedging matches the evidence? | References, claims, dates, retractions, hedging |

Threshold sources are in appendix C. Vendor and forum figures are starting values a route may tune, not standards.
**First code step:** `engine/src/eyes.rs` holds the per-domain eyes as data (`Eyes { views, sizes, cadence }`) and the
keep rule; the first domain is the desktop app, which reuses the UX sentinel's walkthroughs.

### 1.5 What stays on the PC, and the opt-in upload (ER8)

- **Default.** Routes, scars and ghosts live on the person's PC. Anything useful only to one person (a shape with a
  project-specific stack or purpose, a scar whose check is not a catalog check) never leaves it.
- **Opt-in upload, after the MVP.** A person may share general routes and scars anonymously, so Faber improves faster
  for everyone. It is off until they say yes, so AGENTS.md's "telemetry stays off by default" holds (RW-33).
- **What is sent, exactly:** the shape key in catalog words; step kinds; catalog tool ids and versions; the model and
  effort pair per step; check ids with pass or fail; rounded ghost numbers (quota share to 0.1%, whole minutes); the
  Faber version. **Never sent:** files, prompts, the person's words, names, paths, project or repository names, free
  text of any kind. Code builds the payload from typed fields and drops any string outside the catalog vocabulary.
- **The ask,** once, right after the first route is minted, when the benefit is concrete. The card says what Faber just
  learned and what sharing sends, with "Show exactly what is sent" opening the real payload. Two buttons of equal
  weight, "Share anonymously" and "Keep on this PC"; nothing pre-selected; no second ask; no feature tied to the
  answer; a Settings switch to stop at any time. It says plainly that anonymous data cannot be traced back, so it
  cannot be deleted later. Draft English: "Faber just learned a better way to [job]. Share it anonymously so Faber
  gets better at jobs like this for everyone? Only the kind of job, the steps, the tools and models used, which checks
  passed, and rounded time and quota are sent. Never your files, your words or your project names." The screen text
  goes into `ui/locales/`, hand-written per language.
- **Minimum server (Phase 5, before the first public version).** One HTTPS endpoint, `POST /v1/routes`, accepts JSON up
  to 64 KB, validates it against the published schema and appends it to object storage. No account, no device id, no
  stored IP (no access logs; rate limits in memory only). Uploads never reach other users directly: the maintainer's
  monthly job aggregates them, re-runs candidates through the held-out gate on probes, and ships winners in the signed
  catalog with a normal release, so a poisoned upload can only propose. Host and monthly cost cap are Erick's money
  decision, and the privacy text is his legal one; both are asked when the server is built (section 13).
- **First code step (Phase 3):** `engine/src/share.rs`, pure: `fn payload(route, scars) -> Option<Upload>` returns
  `None` for anything not general, with a test that a planted path or free-text string never reaches the payload.

## 2. The Faber persona

### 2.1 What it is

One short text, the same in Chat and Projects, that tells whichever model runs inside Faber that it is Faber, the
contract it works under, and where to find more. Draft in appendix A: about 240 words, about 330 tokens (RW-10). It
holds no knowledge: playbooks, routes and tool lists are pulled when needed, which follows progressive disclosure
(https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) and just-in-time retrieval
(https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents).

### 2.2 How it is delivered (existing flags only)

| Engine | Lever today | Change |
| :--- | :--- | :--- |
| Claude Code | `--append-system-prompt` with the user's name (`engine/src/claude.rs:72`) | `--append-system-prompt` with the persona, on the `claude_code` preset, every session except side questions |
| Codex | `developerInstructions` on `thread/start` and `thread/resume` (`engine/src/codex.rs:323`) | the persona as `developerInstructions`, every thread except side questions; Chat keeps its `baseInstructions` |

- **Why append, not replace.** Anthropic's guidance is to append product rules to the preset and to replace the prompt
  only for a different surface or identity, because replacing drops the tool guidance and safety instructions
  (https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts). Faber gives a new identity but keeps the CLI's
  own prompt. **Departure:** a new identity by appending. Reason: Faber runs the official CLI for coding work with a
  person watching, which is the preset's job; the persona adds a name and a contract, and says which model runs
  underneath when asked, so nothing is hidden.
- **Why developer instructions on Codex, not AGENTS.md.** AGENTS.md belongs to the user's repository and is capped at
  32 KiB (https://learn.chatgpt.com/docs/agent-configuration/agents-md); Faber never writes into it.
  `developer_instructions` is the session-level field (https://learn.chatgpt.com/docs/config-file/config-reference.md).
- **Cache.** The persona is static per Faber version; the name and language are fixed per conversation; nothing time
  based goes in it, so it stays in the cached prefix (https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus).
  Claude Code records the system prompt at the first request, so a new persona reaches new conversations or the next
  compaction only; the conversation file stores the persona version it started with.
- **Workers** get a worker contract instead of the persona, since they never talk to the person (RW-11). About 200
  tokens: the finish line is the envelope's check IDs; keep a checklist file of those IDs and their states; list your
  assumptions in the reply; stop only when blocked or before a destructive or out-of-scope action, and then reply
  `Blocked`; receipts; verbatim IDs; no credentials; no windows (O55). Opus 5.5 sometimes ends a turn with a summary
  and an offer to continue; the engine treats that as a stall (section 6.2).
- **Hard rules are not prompt rules.** The persona states the contract, but every rule that must hold is enforced in
  code, permissions or blocking hooks (https://code.claude.com/docs/en/hooks; the MVP's execution boundary, its §5).

### 2.3 The address map

| Address | What it holds |
| :--- | :--- |
| `faber:playbook/<name>` | How to do a kind of work: `delegate`, `ask`, `look-and-polish`, `receipts`, `faber-method`, one per domain |
| `faber:route/<shape>` | The route for a shape, with its scars and ghost |
| `faber:tools` | This session's kit, one line per tool (the hot shelf, section 3.2) |
| `faber:receipt/<id>` | A receipt: checks, results, evidence paths |

Playbooks are first-party Markdown files in SKILL.md format (frontmatter `name` and `description`, body under 500
lines, references one level deep, a contents list above 100 lines), so the same file can later load as a skill.
Resolution, in order of build:

1. **MVP:** Projects workers and the lead read addresses through the envelope `read` action (MVP spec §5): the engine
   maps the address to a read-only file. No new MCP server, which the MVP forbids in its first slice.
2. **Chat (Phase 2):** addresses are plain files under `<faber data>/library/`, readable through `--add-dir` on Claude
   Code and the sandbox's read access on Codex. Both are day-one probes (`--add-dir` under `--restricted` is unverified).
3. **Phase 3:** Faber tools `pull`, `receipt`, `find_tool` and `delegate`. On Codex they arrive as experimental
   `dynamicTools` answered through `item/tool/call` (seen in the app-server schema of 0.162.1), which needs no extra
   process; on Claude Code, a small stdio MCP server `faber-mcp` passed with `--mcp-config` and `--strict-mcp-config`.
   It is a separate binary because the app's `main` takes the single-instance lock (`src-tauri/src/main.rs:395`).

**First code step:** `engine/src/persona.rs` with `fn persona(p: &PersonaInput) -> String` and `fn worker_contract()`,
replacing `name_instructions` (`engine/src/model.rs:234`) in both drivers; a test that the persona stays under 1,800
characters and that both drivers pass it whenever `instructions` is `None`.

## 3. Tool catalog and shelves

### 3.1 Setup installs what the picked areas need (ER3, ER6)

- Setup asks which areas the person will work in (software, images and thumbnails, 3D, video, audio and voice, pixel
  art, game mods, documents, business). Faber installs those areas' recommended tools from their original source into
  the Tool store (Kits §9), in the background, hidden and resumable. Areas can be added later in Settings (Erick, ER6,
  refining ER3).
- **Paid things (Erick, ER5).** Free tools by default. A tool for a bought app (Aseprite, DaVinci Resolve Studio,
  Photoshop) is used only when Faber finds the app already installed, and its MCP server installs then (RW-12).
  Credits (ElevenLabs, Meshy, Adobe) are spent only after a yes for that task.
- **Heavy media tools on demand (Erick, ER6).** Voice and audio engines, local image models and other large weights are
  never part of setup. When a job needs one, a card offers the options with size, drive and license, and nothing
  downloads without a yes (RW-13).
- **Image generation uses Codex's built-in tool (Erick, ER6),** so ComfyUI is no longer a default. Checked on Codex CLI
  0.162.1: the `image_generation` feature is stable and on; the app-server streams an `imageGeneration` item
  (`status`, `result`, `revisedPrompt`, `savedPath`, `transparentBackground`, and `failure` with `usageLimitExceeded`,
  `limitId`, `resetsAt`); `modelProvider/capabilities/read` says whether image generation is available. There is no
  image subcommand: Faber asks for an image in an ordinary turn on its existing app-server, and `codex exec` can do the
  same headless. `savedPath` points into the Codex home folder, which Faber never opens, so Faber takes the image from
  the item's `result` (expected to hold the image data; a day-one probe confirms) or has the turn write the file into
  the workspace. Secondary sources say it runs gpt-image-2 and uses plan limits 3 to 5 times faster than text; the
  official CLI page read does not cover image generation, so the quota cost is measured, not assumed. A
  `usageLimitExceeded` failure feeds the pacer with its `resetsAt`. Without Codex connected, image generation is an
  on-demand route (shinpr/mcp-image with the person's own key, or stable-diffusion.cpp with local weights after a yes;
  ComfyUI only on request) or the step parks (RW-34).
- Recommended tools are pinned: the store's 30-day cleanup (Kits §9) applies only to tools installed for a task.
- Installed tools are files. They use no memory until a session mounts them, and MCP servers start and stop with
  their session (strict MCP config), so the 500 MB idle budget is untouched.
- **Telemetry off for tools too.** Faber sets every documented opt-out in the tool's launch environment
  (`DISABLE_TELEMETRY=true` for mcp-for-blender, godot-ai and Coplay's Unity MCP, plus `UNITY_MCP_DISABLE_TELEMETRY` and
  `MCP_DISABLE_TELEMETRY` for Coplay's Unity MCP; the pick for Unity 6+ is CoderGamester's mcp-unity, which has none). Read in
  the repos on 2026-10-14 and 2026-10-16, these are on by default too and get their switch: HyperFrames
  `HYPERFRAMES_NO_TELEMETRY=1`, Impeccable `IMPECCABLE_NO_TELEMETRY=1`, gh `GH_TELEMETRY=false`, the skills CLI
  `DISABLE_TELEMETRY=1`, Chrome DevTools MCP `--no-usage-statistics --no-performance-crux` and
  `CHROME_DEVTOOLS_MCP_NO_UPDATE_CHECKS=1`, Windows-MCP `ANONYMIZED_TELEMETRY=false`, OpenSpec `OPENSPEC_TELEMETRY=0`,
  the Context7 CLI `CTX7_TELEMETRY_DISABLED=1`, Maestro `MAESTRO_CLI_NO_ANALYTICS`, mobile-mcp
  `MOBILEMCP_DISABLE_TELEMETRY`, and `ilspycmd` `--disable-updatecheck`; `DO_NOT_TRACK=1` is set for every tool as well
  ([round-2 catalog re-curation](../../research/2026-10-16/tool-catalog-recuration.md), which builds on [round
  1](../../research/2026-10-14/tool-catalog-recuration.md)). A tool with default telemetry and no opt-out is not
  recommended (RW-14).

### 3.2 Shelves

| Tier | What sits there | Who sees it |
| :--- | :--- | :--- |
| Hot shelf | At most 8 one-line cards: the session's kit, rendered under `faber:tools` after the persona | The model, every turn (stable prefix) |
| Warm aisle | Everything in the Tool store | The assembler and picker at each step boundary; `find_tool` from Phase 3 |
| Cold archive | Catalog items not installed, and registry results | The gap pipeline (Kits §10) and the search order of section 4.1 |
| Your shelf | The person's own skills, MCP servers and plugins, as each CLI reports them | Ranked like the others with the user prior (Kits §9); labelled "yours"; never removed or changed outside Faber without asking |

- **User tools.** The inventory comes only from what the CLIs report (Claude Code `system/init`: tools, MCP servers,
  skills, plugins; Codex `skills/list` and `mcpServer` status). Faber never opens `~/.claude` or `~/.codex`. A card is
  written from the reported description, which is treated as data (Kits §9 card writer). The person can mark a tool
  "always in Faber" or "never in Faber". A tool added by URL goes through the vetting bounds of Kits §12.
- **Per-step kit.** The assembler still runs at every step (E1: no hand-written kits). A route's step kit, assembled in
  the run that minted it, enters as the strongest prior, and the assembler validates it (jobs, conflicts, budget, risk,
  pins; Kits §6). The model pick of Kits §6.2 is needed only for a step with no route kit, a route tool that is missing,
  or a probe. Delivery and attestation stay as Kits §8, with one correction: Codex
  `skills/extraRoots/set` adds the store as a skill root for the whole app-server process (not persistent), so per-thread
  selection still needs thread config or inline delivery (RW-15).

**First code step (Phase 3):** `engine/src/shelf.rs`: turns the tools each CLI reports at session start into cards and
tiers, and renders the hot shelf lines for `faber:tools`; pure, fed by the drivers' existing event parsing.

### 3.3 Recommended default kit per domain (verified 2026-10-16)

Every tool below was read again in its own repository in the second re-curation round (2026-10-16): file tree, LICENSE
file, real SKILL.md or server code, activity and telemetry. That round also widened the candidate search. The per-tool
rows, evidence paths, the cross-area rules and the new candidates are in the [round-2 catalog
re-curation](../../research/2026-10-16/tool-catalog-recuration.md), which supersedes the [round-1
document](../../research/2026-10-14/tool-catalog-recuration.md) for verdicts. This table is the curated list: one
winner per job, and a second tool only for a different real need, named in a few words. The second column installs at
setup for the areas the person picked; the third needs something the person provides, or downloads on demand after a
yes (ER5, ER6). Nothing is bundled. Telemetry switches are in section 3.1. Tags: Erick's decisions keep their tag;
**(AI)** marks a change from the 2026-10-14 table, made by the AI and not reviewed.

| Domain | Store at setup (for the picked areas) | Ready when the person has it, or on demand after a yes | Not recommended (why) |
| :--- | :--- | :--- | :--- |
| Every code writer | ponytail (MIT) with Erick's cold-start prior for writers (E18), provisional (section 5.3); Faber installs its skills and the plain hook file, not the optional Claude Code band added in v5.3 **(AI)** | — | caveman as the note style (R33; its CLI also sends telemetry by default) |
| Software: process, code navigation, docs lookup, skill discovery (area 1) | superpowers (MIT): only `systematic-debugging` and `test-driven-development` as warm cards (section 7.3); gh CLI (MIT); ast-grep CLI (MIT); Context7 (MIT), CLI plus skill form, pinned, after a one-time "sends your query and IP" notice **(AI)**; the `npx skills` CLI (vercel-labs/skills, MIT) for skill search and install | mattpocock/skills: `grill-me` and `improve-codebase-architecture` only **(AI)**; OpenSpec for big builds (telemetry env off), spec-kit only for greenfield builds with gated artifacts; open-code-review in delegate mode (code review is otherwise each CLI's own); knip (JS/TS) and jscpd; Claude Code LSP plugins for symbol navigation, Serena for Codex or symbol edits (GPL app, heavy; one navigation tool per session; its memory cost is measured when the catalog is built) **(AI)**; `ck` to search code by meaning **(AI)**; mcp-debugger **(AI)**; docs-mcp-server for offline docs (telemetry off) and DeepWiki for a public repo's design (sends names and questions to Cognition) **(AI)**; glab for GitLab **(AI)**; `gh skill` for people already signed in to gh (preview feature) **(AI)**; skill-creator (Claude Code only; spawns sessions that spend quota) with agnix to lint what it writes, and mcp-builder (anthropics/skills, Apache-2.0 per LICENSE.txt) **(AI)**; code-modernization plugin (Claude Code only) **(AI)**; stack packs Laravel Boost and Tauri MCP (DBHub is under payments, data and deploy) | `using-superpowers` (conflicts with Faber's kit control); GitHub MCP server (gh covers it without a token); reference Memory, Sequential Thinking, Git and Filesystem servers (Faber keeps its own memory; the CLIs already have these tools); codebase-memory-mcp and the other code-graph MCPs (Serena and the LSP plugins cover them), GitNexus and jCodeMunch (non-commercial licences) **(AI)**; repomix (the CLIs read files themselves) **(AI)**; find-skills (Faber's selector covers it) **(AI)**; BMAD, GSD, task-master, beads, planning-with-files and the addyosmani and compound-engineering bundles (repeat superpowers) **(AI)**; gstack and ruflo (take over the session) **(AI)**; the Codex plugin for Claude Code and Anthropic's review plugins (repeat the CLIs' own review) **(AI)**; rtk (CVE-2026-54555, hook bypass) **(AI)** |
| Frontend, web testing and diagrams (area 2) | Impeccable (Apache-2.0) and its detector, pinned, plugin hooks off; Taste-Skill v2 `design-taste-frontend` (MIT) for public web pages, as the base pass before Impeccable (Erick, R33 and E18; it is art direction for landing and marketing pages only, while Impeccable covers app screens, native apps and refinement); playwright-cli (Apache-2.0) with `run-code` denied, the one flow driver, replacing Playwright MCP; Chrome DevTools MCP (Apache-2.0), the one measurement tool for traces, memory, Lighthouse and accessibility, mounted only when measuring, with its usage-statistics, CrUX and update-check switches off **(AI)**; diagrams: Mermaid text in Markdown, nothing to install **(AI)** | `@google/design.md` to lint a DESIGN.md **(AI)**; emilkowalski/skills for motion, gsap-skills only with a `gsap` dependency **(AI)**; Figma's official remote MCP for Figma users, with Framelink only on a Starter plan or View seat, Penpot MCP for Penpot users and Stitch skills for Google Stitch users **(AI)**; by stack: shadcn MCP (React), Storybook addon MCP, and the Svelte, Angular and Next.js official MCPs **(AI)**; Playwright test agents for projects on Playwright Test; other Taste variants only when asked; the draw.io skill, skill folder only, for editable diagrams, and mermaid-cli to render an image **(AI)** | frontend-design (a weaker subset of Impeccable plus Taste; its folder is Apache-2.0, so the reason is quality) **(AI)**; Playwright MCP (`browser_run_code_unsafe` on by default) **(AI)**; UI UX Pro Max, Hallmark and similar look skills (duplicates); Adobe Stardust (bot-detection evasion) **(AI)**; Vercel Web Interface Guidelines (Impeccable's `audit` covers it; it was setup on 2026-10-14) **(AI)**; agent-browser, Lighthouse and axe-core as separate tools (Chrome DevTools MCP covers them) **(AI)**; figma-console (the official server now writes) **(AI)**; Playwriter, browser-harness and other tools that attach to the person's real signed-in browser **(AI)**; the draw.io plugin's hosted `mcp.draw.io` and the window it opens by default **(AI)**; Taste-Skill on app screens or dashboards; two Taste variants in one session |
| Security and reverse engineering (area 3) | OSV-Scanner (Apache-2.0); Betterleaks (MIT, validation off); trailofbits/skills (CC-BY-SA-4.0, download only) as five review lenses: differential-review, insecure-defaults, sharp-edges, variant-analysis, fp-check; REA (MIT) when the person picks mods and reverse engineering (Erick, R149), pinned from npm and registered through each CLI's own MCP command | Opengrep, zizmor, Varlock (opt-out first), cargo-deny, GuardDog (Windows only through Docker) and Checkov **(AI)**; Cisco skill-scanner, static mode only, to vet third-party skills before install **(AI)**; `security-guidance` and `claude-security` (Claude Code), Codex Security (Codex), all spending plan quota **(AI)**; reverse engineering on the person's own or legally analysable binaries: Ghidra 12.1.4 (the person installs it), then GhidraMCP as the one alternative to REA's own Ghidra bridge (Erick, R149; never both), IDA MCP for the edition owned (HexRaysSA/ida-mcp for Pro, ida-free-mcp for Free), `ilspycmd` (ILSpy's official CLI) for C# source from .NET, because REA reads metadata and CIL only **(AI)**, capa, x64dbg-MCP, mcp-windbg for crash dumps, mitmproxy, Wireshark-MCP, Apktool, and Binwalk or Unblob through REA on Linux or WSL **(AI)** | Semgrep MCP (archived; its successor inside Semgrep traces to semgrep.dev by default) and Semgrep's registry rules (not open source) **(AI)**; gitleaks (superseded by Betterleaks) **(AI)**; TruffleHog (AGPL, duplicate) **(AI)**; Trivy (2026 supply-chain compromise); CodeQL CLI (the licence limits free use to open-source code) **(AI)**; Snyk and other hosted-account security MCPs (account and token) **(AI)**; the community ILSpy-MCP servers (duplicates of `ilspycmd`; they were R149 alternatives) **(AI)**; other Ghidra bridges, radare2-mcp and Frida MCPs (duplicates of REA) **(AI)** |
| Documents, research and productivity (area 4) | MarkItDown (MIT); Pandoc (GPL-2.0-or-later, run as its own program); Typst (Apache-2.0) with the lucifer1004 Typst skill (MIT); OfficeCLI (Apache-2.0, release binary, auto-update off); K-Dense `paper-lookup` (MIT; 18 scholarly APIs, no key, no server) for research **(AI)**; both CLIs have web search built in | In Claude Code only: the docx, pdf, pptx and xlsx skills, turned on through Anthropic's own plugin system and never copied into Faber's store (Erick, Q7); Docling for scans and hard layouts, pdfcpu, OCRmyPDF, Tectonic (only for a LaTeX class), Marp CLI for slides from Markdown, trafilatura **(AI)**; search beyond the built-in: SearXNG MCP (free), Brave MCP (the person's own paid key) **(AI)**; Weizhena Deep-Research-skills, and K-Dense `citation-management`, `literature-review` and `scientific-writing` **(AI)**; Zotero, hosted Notion, Obsidian skills (kepano), Todoist, rclone and Microsoft 365 (read-only first) for their users; Google Workspace through the community `google_workspace_mcp` for now (Google's own servers are in preview and are revisited at GA) **(AI)**; mcp-local-rag to search the person's own folders (Everything's `es.exe` for file names on Windows) **(AI)**; sbroenne's Excel MCP only when desktop Excel is installed, OfficeCLI otherwise **(AI)** | A copy of Anthropic's docx, pdf, pptx, xlsx skills in Faber's store, or on Codex: their LICENSE.txt is proprietary and forbids copies outside Anthropic's services and derivatives (Erick, Q7); gws (stalled since March); Crawl4AI (licence adds a mandatory attribution clause); Exa, the local server and the hosted endpoint (analytics to Agnost with no opt-out, RW-14; built-in search plus SearXNG and Brave cover the need) **(AI)**; hyperresearch (hard dependency on Crawl4AI and pymupdf, hooks written into the project) **(AI)**; Tavily, Perplexity, Firecrawl and Jina (paid keys, duplicate search) **(AI)**; gpt-researcher (paid keys); academic-research-skills (non-commercial licence) **(AI)**; arXiv and other single-source paper servers (`paper-lookup` covers them) **(AI)**; MCPVault, qmd and haypile (duplicates; haypile is a resident AGPL daemon) **(AI)**; the Word and Excel MCPs (duplicates of OfficeCLI) **(AI)** |
| Business, marketing, SEO and writing (area 5) | marketingskills (MIT, 50 skills; the `skills/` folder only, its API CLIs never wired); humanizer (MIT; tuned for English) | claude-seo and claude-ads (heavy; after a yes), Google Analytics MCP, Google Ads MCP (official, read-only analytics) **(AI)**, mcp-gsc, pm-skills, Anthropic knowledge-work sales, finance and small-business skills and financial-services skills (skills only, not their hosted connectors); anthropics/claude-for-legal plugins, on request (Apache-2.0; output needs a lawyer's review); humanizer-pt-br for Portuguese text **(AI)**; Vale with vale-ai-tells, a deterministic lint for docs and commit messages that the software area may use too **(AI)** | claude-youtube, tapestry youtube-transcript (stale), feiskyer skills (reads browser cookies); baoyu-youtube-transcript (duplicate of mcp-youtube-transcript) **(AI)**; Product-Manager-Skills (non-commercial licence), stop-slop, claude-blog, seomachine, geo-skills and the big skill collections (duplicates) **(AI)**; Postiz and the social-posting automation skills (AGPL and heavy, or a third party holds the accounts) **(AI)** |
| Image, pixel art and vector (area 6) | Codex built-in image generation (no install, ER6); ImageMagick; vtracer (MIT) and svgo (MIT); proper-pixel-art (`ppa`, MIT) to fix fake pixels in AI pixel art **(AI)** | Codex not connected: shinpr/mcp-image with the person's own key, or offline on a GPU stable-diffusion.cpp with Z-Image-Turbo weights (Apache-2.0), FLUX.2 klein 4B when edits need reference images **(AI)**; ComfyUI with comfyui-mcp only on request, never a default (Erick, ER6); rembg with BiRefNet; Upscayl's ncnn CLI (AGPL, own process); resvg and satori; aravindev/inkscape_mcp with Inkscape; logo-design-skill; Anthropic's `canvas-design` (Apache-2.0) **(AI)**; GIMP MCP (GIMP 3.2; it has a run-anything tool, so it asks first), Photoshop, Illustrator and Lightroom MCPs for their owners, adb-mcp for InDesign and After Effects owners; Adobe hosted MCP (Creative Cloud; credits after a yes); diivi/aseprite-mcp (MIT) only when Aseprite is installed (ER5); Piskel MCP when it is not; PixelLab hosted (token, no licence) | ComfyUI as a default (heavy, ER6); sandraschi/inkscape-mcp (sprawling, metrics port on by default); Comfy-Org/comfy-mcp (AGPL or commercial licence; duplicate) **(AI)**; the other GIMP, Aseprite, Krita and image-API MCPs (duplicates or stale) **(AI)**; Real-ESRGAN-ncnn (unmaintained); FLUX.3 klein (gated, custom licence unread) **(AI)** |
| 3D, game engines and game mods (area 7) | glTF-Transform (MIT; its `validate` runs the Khronos validator) **(AI)**; gamedev-skills/awesome-gamedev-agent-skills (Apache-2.0, 74 skills for Godot, Unity, Unreal, Phaser, PixiJS, three.js and more, mounted per step, not all at once) **(AI)** | Blender 5.1+: the official Blender Lab MCP (GPL-3.0, no telemetry found); older Blender: mcp-for-blender (MIT, `look` tool renders views; telemetry on by default, switched off; safe mode) with cc-blender-skill; build123d-mcp for CAD (OpenSCAD MCP only if OpenSCAD is already used) **(AI)**; Meshy MCP (key, Pro plan); Unity 6+: CoderGamester/mcp-unity (MIT, no telemetry found), and Coplay's Unity MCP (telemetry switched off) only for Unity before 6; Unity's agent plugin skills (not its `unity-cli`); Godot 4.7+: godot-ai (MIT, telemetry switched off), and Erodenn/godot-mcp-runtime for older Godot or no addon **(AI)**; ChiR24 Unreal_mcp (MIT, token) with quodsoler's Unreal skills for C#-free depth **(AI)**; Epic and Roblox built-in MCPs as guides; minecraft-dev-mcp with the Jahrome907 skills; BepInEx, MelonLoader, SMAPI, tModLoader and UE4SS guides; for the person's own games Cpp2IL, UnityPy, AssetRipper and gdsdecomp **(AI)**; universal-modder as skills only (Erick): Faber installs only the skill folders that do not tell the agent to add other MCP servers, and leaves out its `reverse-engineering` skill **(AI)** | TRELLIS and TRELLIS.2 (MIT, but no agent interface and a 24 GB GPU); Hunyuan3D (licence excludes EU, UK, South Korea); Stable Fast 3D (revenue cap); IvanMurzak Unity-MCP (its default path needs a hosted account) **(AI)**; Tripo's agent tools (no public source) **(AI)**; mcmodding-mcp and the younger Blender and Godot MCPs (duplicates) **(AI)**; Claude-Code-Game-Studios (takes over the project) **(AI)**; Il2CppDumper, Il2CppInspector and AssetStudio (stale) **(AI)** |
| Video, audio and voice (area 8) | heygen-com/hyperframes (Apache-2.0; HTML to MP4 rendered locally with headless Chrome and FFmpeg; agent skills including transcription and captions; Windows in its CI) (Erick, R105); Faber sets its telemetry switch and denies the HeyGen sign-in paths of its `media-use` skill **(AI)**; FFmpeg from its official builds, with `ebur128` and `loudnorm` as engine checks; auto-editor (Unlicense) **(AI)** | On demand after a yes (ER6): HyperFrames `tts` (Kokoro, local; Brazilian Portuguese voices; model downloads on first use); debpalash/VoiceStudio (AGPL-3.0 desktop app: local cloning, voice design, dubbing; local HTTP API and MCP server), with Chatterbox (MIT weights) when the audio will be sold, because VoiceStudio's default engine has non-commercial weights **(AI)**; whisper.cpp to transcribe a file on its own, faster-whisper without a whisper.cpp Windows build, WhisperX for speaker labels **(AI)**; python-audio-separator; ACE-Step 1.5 (MIT weights) for music; xDarkzx/Audacity-MCP with Audacity 3.x only; ElevenLabs hosted MCP and skills (credits; never its key-reading setup skill); KyaniteLabs/kinocut (203 MCP tools, so only with deferred tool loading); Manim; davinci-resolve-mcp (Resolve Studio), leancoderkavy Premiere MCP and capcut-cli for editors the person owns; jkawamoto/mcp-youtube-transcript, with yt-dlp to download the media itself (never `--cookies-from-browser` or `--exec`) **(AI)** | Remotion skills as default (no licence file; engine licence limits companies over 3 staff); browser-use/video-use (key handling; duplicate of HyperFrames) **(AI)**; hetpatel Premiere MCP (telemetry on by default to its own server); after-effects-mcp (run-anything tool on) **(AI)**; PySceneDetect (FFmpeg's `scene` filter covers it) **(AI)**; kokoro-onnx, KittenTTS and Piper (the same job as HyperFrames `tts`) **(AI)**; HeartMuLa (duplicate of ACE-Step) **(AI)**; the OpenAI speech and transcribe skills (a separate paid key) **(AI)**; ElevenLabs local MCP (archived); kokoro-tts-mcp (stale); Suno wrappers (session cookie and a paid CAPTCHA service); OmniVoice, F5-TTS, MusicGen (non-commercial weights); Buzz (telemetry by default; covered) |
| Mobile (area 9) | — | Maestro with its MCP (telemetry switched off), with mobile-mcp as the second for device logs, crash reports, screen recording and GPS override **(AI)**; XcodeBuildMCP and the AvdLee Swift skills (macOS only) **(AI)**; Expo skills and EAS CLI; callstack agent-skills for bare React Native; Dart and Flutter MCP with flutter/skills; android/skills **(AI)** | Appium MCP, agent-device and the other adb and simulator MCPs (duplicates of Maestro or mobile-mcp) **(AI)**; swift-ios-skills (PolyForm Perimeter licence) **(AI)** |
| Payments, data and deploy (area 9) | — | Stripe: four stripe/ai skills (best practices, upgrade, docs, connect), the Stripe MCP in test mode with a restricted key the person types, and the Stripe CLI (test mode; `--live` blocked) **(AI)**; Paddle, PayPal and Mercado Pago MCPs, each only for a project on that processor **(AI)**; DBHub for SQL (SQLite, Postgres, MySQL; read-only), with Postgres MCP Pro for tuning, MotherDuck for DuckDB, MongoDB MCP for MongoDB and MCP Toolbox for Google Cloud databases **(AI)**; Supabase (supabase/mcp, `read_only=true`), Neon, Firebase and Prisma skills; deploy through the host's own CLI or MCP: Cloudflare skills and MCP, Vercel CLI, Netlify CLI, Render, Fly, Heroku, DigitalOcean, and AWS, Azure or Google servers in read-only mode only **(AI)**; auth skills for Clerk (no LICENSE file; link only, never copy), Auth0 and WorkOS; Sentry plugin skills and hosted MCP, Grafana MCP (`--disable-write`), PostHog skills and hosted MCP, Datadog hosted MCP for existing customers **(AI)**; Jupyter MCP (sandboxed notebooks), Power BI (separate EULA) **(AI)**. Tool installers (Wrangler, the Stripe plugin) are blocked from writing into the agents' folders | Postgres reference server (archived); Prisma remote MCP (repo archived), better-auth skills (no licence), Netlify MCP (no LICENSE file; the CLI covers it), Vercel plugin and deploy skills (hooks, default telemetry, no licence), Railway skills (the agent creates accounts); the Stripe skills that send money, sign up for services or take over lookups, and Auth0's beta admin MCP that returns secrets **(AI)**; PlanetScale's hosted MCP (partial source; its CLI covers it), Turso MCP (duplicate of DBHub), and the archived or unlicensed Honeycomb, Axiom and New Relic MCPs **(AI)** |
| Desktop automation and local models (area 10) | — | Windows-MCP (MIT; pinned 0.8.7 because 0.8.8 has an open control-gate bug, telemetry off, allowlist without its PowerShell, Registry, FileSystem, Process and App tools; the person says yes each session); Peekaboo on macOS only **(AI)**; Ollama with `OLLAMA_NO_CLOUD=1`, or llama.cpp when Ollama is too heavy; LM Studio only if the person already runs it, and Lemonade only for AMD Ryzen AI NPU hardware **(AI)**; sandbox-runtime on Windows is alpha and needs the person's own one-time administrator approval, which Faber never runs silently **(AI)** | DesktopCommanderMCP (open guard-bypass reports; duplicates the CLIs' own tools); macos-automator (archived, macOS only); the other Windows control, screenshot, clipboard and shell MCPs (duplicates of Windows-MCP or the CLIs' own tools, with a weaker approval flow) **(AI)**; desktop-touch-mcp and terminator (run-anything tools on or stale) **(AI)**; vLLM, SGLang, Jan, llamafile, LocalAI and Foundry Local (duplicates, no native Windows, or licence terms) **(AI)**; LiteLLM and claude-code-router (they hold provider keys) **(AI)** |

**What changed and why (AI).** Round 2 widened the candidate search, because round 1 ran out of search quota and missed
REA, and it applied one tool per job: a second tool stays only for a different real need, and a duplicate sits in the
last column beside the tool it repeats. Setup still holds only tools that need nothing from the person. Chrome DevTools
MCP joined it as the single measurement tool, and the Web Interface Guidelines and draw.io left it (the Guidelines
repeat Impeccable's `audit`; draw.io's plugin ships a hosted MCP and opens a window by default, so Mermaid in Markdown
is the default and the draw.io skill is on demand). Other switches: `npx skills` stays at setup and `gh skill` is on
demand; mcp-unity replaces Coplay as the Unity pick; the official `ilspycmd` CLI replaces the community ILSpy-MCP for C#
source; Exa is dropped, the hosted endpoint included; mcp-local-rag is the one search over the person's own files;
OfficeCLI owns Office files and sbroenne's Excel MCP waits for an installed Excel. Three rules now apply everywhere:
an official vendor-hosted connector is on demand only after a one-time "this sends your data to the vendor" notice and
only when the vendor publishes docs or a repo; tool installers never write into the agents' skill folders, because
only Faber's store places skills; and a catalog item declares the runtimes it needs, which Faber installs from their
official source only after the same yes. Erick's recorded choices are unchanged: ponytail in and caveman out (R33,
E18), Taste-Skill before Impeccable on public pages (R33, E18, R56), HyperFrames as the video default (R105), free
tools first and paid apps only when installed (ER5), Codex images and no ComfyUI default, heavy audio and VoiceStudio
on demand (ER6), universal-modder in the catalog as skills only (Erick), REA in the catalog (R149).

**Erick's two names, checked.** "Hyperframes" is HeyGen's open-source HyperFrames (Apache-2.0, 61.5k stars, pushed
2026-10-12): HTML compositions rendered to MP4 locally (Node 22+, FFmpeg, headless Chrome), with optional HeyGen
cloud or Lambda rendering, local Kokoro-82M text-to-speech, local transcription (whisper.cpp or Parakeet through sherpa-onnx) and published agent skills. It fits Animated video and the
TL-ADE's "HyperFrames way", and replaces Remotion as the video default; its CI runs on Windows (v0.8.164, repo read 2026-10-16), and a day-one
probe still renders on Erick's PC. "Voice Studio" is ambiguous. The one that fits is debpalash/VoiceStudio, formerly OmniVoice
Studio (AGPL-3.0, 59.8k stars, pushed 2026-10-12): a local Electron app for voice cloning from about 3 s of audio,
voice design, dubbing and audiobooks, with a local HTTP API and an MCP server and analytics only with consent. HeyGen
has no product of that name (its voices live in a Voice Library), and ImagineArt's Voice Studio is a hosted web tool.
Faber never runs VoiceStudio's `irm https://voicestudio.sh/install | iex` install line: it offers the release installer as a needs-you step (an
app install is never automatic, Kits §12), then talks to the local API. Cloning needs the voice owner's consent, and
the person checks the model licenses (shown in its Model Manager) before commercial use.

FFmpeg itself is downloaded from its official builds at setup, never bundled. The catalog records, per item, the
engine it was verified on (r5 gap 9). **First code step (Phase 3):** `catalog/manifest.json` schema and
`engine/src/catalog.rs` (types, validation, no network); MVP has no catalog (MVP plan, calendar).

## 4. Finding and creating tools

### 4.1 Search order

1. The catalog (local, vetted) and the person's shelf.
2. `npx skills find <query>` (https://github.com/vercel-labs/skills), run from the store by its resolved executable,
   never through a shell. Its own ranking prefers 1K+ installs and official owners and distrusts under 100
   (https://github.com/vercel-labs/skills/blob/main/skills/find-skills/SKILL.md). The skills CLI is installed at setup;
   `gh skill` is the on-demand alternative for people already signed in to gh (preview feature).
3. The official MCP Registry, `GET https://registry.modelcontextprotocol.io/v0.1/servers?search=<term>` (v0 also
   answers). It is metadata only, with no security review field, so a hit is never treated as vetted.
4. Smithery (`smithery mcp search`, `smithery skill search`), discovery only.
5. A read-only research worker on GitHub and vendor organizations (Kits §10, G1).

Every candidate is scored by code on license (SPDX), last push within 12 months, telemetry and opt-out, data sent, host
OS, engines it runs on, and runs-code or text-only; then the bounds of Kits §12 apply. Departure from find-skills:
install counts and stars are only a tie-breaker, because outcomes on probes rank tools (Kits §15) (RW-16).
**First code step (Phase 3):** `engine/src/find.rs`, a pure scoring function over the JSON that the skills CLI and the
registry return; the research worker makes the network calls.

### 4.2 The crafting bench (skills, MCP servers, plugins)

Triggers: the same hand procedure (three or more tool calls of the same shape, by normalized command and file kind)
appears in three runs of a shape with no kit tool covering it; a gap that search did not fill; a scar that keeps
firing; or the person asks. Example: Sonnet cleans fake pixels by hand three times, so the bench drafts `grid-snap`.

| Stage | What happens | Source of the practice |
| :--- | :--- | :--- |
| 1. Brief | Code writes a typed trace summary (tool names, command kinds, file kinds, counts; never raw tool output or failure text). A research worker with no repo access writes a cited brief from primary docs; code fetches every cited URL; the brief is data to the drafter and is scanned like an unvetted skill (Kits §12) | Kits §10 G2; skill-creator "capture intent"; AUTO |
| 2. Draft | Bundled `skill-creator` on Claude Code or `$skill-creator` on Codex drafts SKILL.md with the six portable frontmatter fields, a third-person description with triggers first, body under 500 lines, scripts for deterministic steps. MCP servers follow `mcp-builder`; a skill with a script is preferred unless the job needs a live app connection | https://github.com/anthropics/skills/tree/main/skills/skill-creator; https://learn.chatgpt.com/docs/build-skills; https://code.claude.com/docs/en/costs |
| 3. Evals first | 3 or more realistic cases from real traces with assertions (program checks where possible) in skill-creator's `evals/evals.json` shape; 20 trigger queries, 60/40 split | skill-creator |
| 4. Prototype tests | XS and S probes (Kits §15.3) split at random into train and held-out test; three arms: with the skill, no-skill placebo, best existing tool; 3 runs each; both engines; the model tiers that will use it. Before the run, code prints cases × repeats × models and the estimated quota | skill-creator baseline; EVAL; Faber adds the third arm and the second engine |
| 5. Blind judging | Program graders first; judged parts by the other vendor as yes/no claims, random order, run twice (section 6.4) | r6 §0; EVAL |
| 6. Prototype tier | Stored as `bench/<id>/v<n>`; used only in probes and challengers, never in a delivered result | brief, Erick's idea 1 |
| 7. Promotion | Better than both other arms on train and on held-out test, by more than the noise (a tie must also be cheaper), and trigger accuracy at least 90% on the held-out queries; then pinned and hashed. Train up with test flat is overfitting and is reverted | Kits §15.4; EVAL |
| 8. Decay | Re-run against placebo after every major CLI or model update and every 50 uses; demoted after two losses | Kits §15.4 |

At most 5 iterations per skill (skill-creator), one change per iteration aimed at one root cause. After 2 or 3 flat
rounds the bench stops editing, sorts the remaining failures by cause and audits its graders first, since broken
graders often explain a stall (EVAL). Failure text never goes into the skill. Codex bundles `$skill-creator` but no evaluation harness, and
`claude plugin eval` covers Claude Code only, so Faber's own harness runs both (r5 gap 1). Plugins are only generated
wrappers (a kit plugin folder, Kits §8); a first-party plugin hook may only block (exit 2), never add behaviour.
Bench spend comes from spare quota under the probe lane caps (Kits §15.3) (RW-17).

**First code step (Phase 3):** `testkit/src/bench.rs`: reads `evals/evals.json`, runs each arm in the existing
`testkit/src/sandbox.rs`, writes `grading.json`. Trigger detection: `engine/src/bench.rs`,
`fn repeated_procedures(traces) -> Vec<Candidate>`, pure.

### 4.3 Strong first-party tools on day one

The day-one shapes (section 1.3) feed the bench, so first-party skills come from traces, not from hand-writing (the
"kits on the fly" rule). Expected candidates, each only if it beats placebo: `faber-six-views` (Blender render rig as a
script), `faber-grid-snap`, `faber-loudness` (two-pass FFmpeg measure and normalize), `faber-a11y-scan` (axe-core, r5
gap 7), `faber-safe-sql` (read-only, gap 2), `faber-semgrep` (wraps the binary, gap 3), `faber-open-office` (open
libraries, gap 4), `faber-deep-research` (gap 5), `faber-phone-feed` (places a thumbnail and title among real ones).

## 5. Talk between AIs

### 5.1 Typed envelopes

The engine, not the model, writes one JSON envelope per dispatch and validates every reply (RW-18):

```json
{ "v": 1, "id": "env_7f2a", "ping": "Order", "from": "lead", "to": "writer:w3", "goal_ref": "G1",
  "requires": ["R3", "R4"], "checks": ["C7"], "quotes": ["Q2"],
  "inputs": [{"path": "engine/src/budget.rs", "sha256": "…"}], "forbidden": ["src-tauri/**", "**/.git/**"],
  "budget": {"turns": 12, "quota_pct": 1.5}, "reply_to": "env_7f2a", "note": "pace calc wrong at reset; keep API" }
```

- `ping` is one of `Order`, `Found`, `Blocked`, `NeedEyes`, `DoneReady`, `Disagree`.
- **IDs expanded verbatim.** When the engine renders the prompt it writes each ID with its exact stored text: "R3 (the
  person's words): …". Models refer to IDs and never restate them, so paraphrase drift cannot happen by construction.
- **Replies** carry `ping`, `receipt` (check IDs with results and evidence hashes), `artifacts` (path and hash),
  `claims` (each tied to a check ID), `assumptions` (what the worker decided without asking; the verifier checks each,
  because higher effort assumes more silently, EFF), `unconfirmed` (what it could not check) and `note`. Codex turns enforce the shape with `outputSchema`; bounded Claude Code
  runs with `--json-schema` (its `format` keyword is not enforced); live sessions by code validation.
- **Malformed replies bounce** with an `error.*` key and no model call; after two bounces the step is `Blocked`.
- **Envelope hash** is the cache key for partial reruns (generated pipelines §9).
- `Disagree` goes straight to an other-vendor judge without waking the lead.
- **Fit with the MVP:** these are fields of the MVP's existing worker envelope (actions `read`, `plan`, `amend`,
  `patch`, `proof`, `decision`, `complete`; MVP plan Task 6), not a new channel.

The brief's estimate of 65 to 75% fewer tokens per hand-off is unmeasured; section 10 measures it. **First code step
(MVP+2):** the new fields, the ID renderer and the bounce in `engine/src/project/worker.rs`.

### 5.2 Terse notes only (ER2)

Only `note` and pings may be terse. Requests, requirements, checks and the person's words always travel verbatim as
IDs. The style is one rule in the envelope renderer ("note: at most 60 words; fragments allowed; never restate a
requirement") plus a schema `maxLength`, not the caveman skill (RW-19). Reasons from verification: caveman's own
benchmark shows its skill about 3% shorter than a plain "be concise" instruction (ultracave 35%), the skill costs about
1,160 input tokens per turn, and its CLI sends telemetry by default (https://github.com/JuliusBrussee/caveman). Erick
named caveman as a cold-start candidate (E18), so the skill stays one: it replaces the rule only if it beats it on
probes (section 4.2). This narrows Kits §7.2's caveman row to the note field, as ER2 requires.

### 5.3 Ponytail for code writers

Verification supports it only in part: the −53% code, −26% cost, −45% tokens figures are the author's own, on 39 tasks,
Claude Code with Opus 5.5 only (https://github.com/DietrichGebert/ponytail). Decision (RW-20):

- **Prior:** Erick's cold-start prior for code writers (E18) stays, on both engines; it is a prior, so the assembler
  can still leave ponytail out of a step.
- **MVP:** Faber installs nothing before Phase 3 (Kits §17.1), so ponytail joins a writer only where the person already
  has it and the CLI reports it (on this PC, Claude Code), loaded natively.
- **Phase 3:** from Faber's store copy; on Codex inline (its ladder, about 200 tokens), which avoids the plugin hooks
  Codex makes the person trust by hand.
- **Evidence:** the first bench experiment after G8 runs 10 Faber code tasks with and without it on both engines,
  judged blind by the other vendor plus the proof set. On an engine where it does not help, its prior drops to zero.
- The Faber Method outranks it: tests first and the software bar are never "simplified away".
- **First code step:** the placebo run as the first `testkit/src/bench.rs` case; delivery then uses the Kits §8 inline path.

### 5.4 The distiller and its drop-ledger

When cheap gatherers (scouts, research workers) return more than 6,000 tokens for an expensive consumer, a small
model writes a brief of at most 1,500 tokens (the 1,000 to 2,000 range of the context engineering post) plus a
drop-ledger: one row per left-out part `{ptr: "file#L10-40" | "url", why, tokens}`. The consumer expands a pointer with
the envelope `read` action. Code checks that every requirement ID and quote in the inputs is kept, and that kept plus
dropped covers all input bytes (RW-21). **First code step (Phase 3):** `engine/src/distill.rs` (ledger and coverage,
pure); the model call reuses the drivers' side-question launch (`instructions` set: no tools, no session).

### 5.5 Receipts

A receipt is `{id, checks: [{id, kind: program | judge | gate, result, evidence, hash}], artifacts, model, cost}`.
On screen, a "done" with no receipt id is greyed out; a receipt with no program check, in a domain that has one, shows
"unverified" (RW-22). The lead keeps receipts, not outputs. **First code step (MVP+3):** a `Receipt` type in the MVP's
`engine/src/project/proof.rs`.

## 6. Orchestration

### 6.1 One agent or many: the r3 dispute, resolved

| Work | Shape | Sources |
| :--- | :--- | :--- |
| Coupled code (shared files or contracts) | One writer, in order; others read only | Cognition, https://cognition.com/blog/dont-build-multi-agents and https://cognition.com/blog/multi-agents-working; Anthropic, https://www.anthropic.com/engineering/multi-agent-research-system ("most coding" fits poorly) |
| Disjoint areas behind a frozen contract | Parallel writers, one worktree and lease each, once qualified | MVP plan Task 15; https://claude.com/blog/how-and-when-to-use-subagents-in-claude-code (independent pieces); Codex worktrees, https://learn.chatgpt.com/guides/best-practices |
| Breadth: research, scouting, test proposals | 3 to 5 parallel read-only workers | Anthropic research system (90.2% over single agent on its internal eval) |
| Review and judging | Clean context, other vendor | Cognition 2026; https://code.claude.com/docs/en/best-practices |
| Creative divergence | Isolated drafters with no shared context, then anti-median | TL-ADE Ideas line; section 6.4 |
| A simple question | One agent, no orchestration | https://www.anthropic.com/engineering/building-effective-agents |

Cognition calls the single agent the simplest reliable choice, not a law, and its 2026 post lists what multi-agent work
does well; Anthropic says the same about coupled code. The table follows both. Caps: at most 5 parallel scouts,
explicit tool-call budgets in every envelope, because early research agents spawned 50 subagents for simple queries
(RW-23). **First code step:** the MVP plan validator (`engine/src/project/lead.rs`, Task 5) already rejects overlapping
writers; it gains the scout cap and a required budget per node.

### 6.2 Boss and workers

- **Lead (boss):** one live session per mission; keeps the goal, talks to the person, writes the plan graph, never
  writes source (MVP §6). Engine code schedules, integrates and pays.
- **Workers:** scouts (cheap, read-only), test proposers, writers, reviewers and judges (other vendor), ghost users,
  the distiller. A worker cannot spawn peers or enlarge its grant (MVP Task 6).
- **Reflexes in code** stop a worker before the lead is woken: three identical failing proofs, or three patches on the
  same hunk with no proof progress, return `Blocked` (Codex cookbook: stop looping without progress; Huntley,
  https://ghuntley.com/ralph/). A retry of the same step gets half the previous budget, so loops starve (RW-24).
- **Stalls.** A turn that ends with neither `DoneReady` nor `Blocked` is a stall, not an answer: the engine sends one
  "continue" in the same session and logs it; a second stall in the same step returns `Blocked` with `reason.stall`
  (O55; RW-29).
- **Pre-start check.** Before a worker's first request, the engine compares the tools, MCP servers with their status,
  skills and plugins the CLI reports with the kit. A server that is down or missing means `Blocked` with
  `reason.kit_mcp_down`, never a tool-less run, because an unreadable source is not an empty one (AUTO). User plugins
  and mods reported in `system/init` are recorded, and an unexpected one fails attestation (Kits §8) (RW-30).
- **Side effects in three states.** Pushes, PRs and posts are recorded as `posting`, `posted` or `maybe_posted`; on
  `maybe_posted` the engine re-reads the live state first and changes nothing it cannot confirm (AUTO; the MVP git
  broker already compares and swaps refs).
- **No cloud sessions or routines** for workers: they share the same plan limits, store transcripts remotely, and do
  not document Faber's launch flags (CLOUD).
- **Milk run:** one cheap model batches micro-tasks (labels, link checks, summaries). It never writes `ui/locales/`,
  which stays hand-written (AGENTS.md).

### 6.3 Context engineering rules

1. Persona, worker contract and envelope header form a stable prefix; dynamic parts come last (Manus; Claude Code
   `--exclude-dynamic-system-prompt-sections`, already passed).
2. A session's tools never change (Kits TS-2); Manus masks tools rather than removing them for the same cache reason.
3. Load by address or pointer, never by dump (context engineering post).
4. Workers return at most 1,500 tokens; bulky output goes to files.
5. Workers get one unit of work per fresh session (https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents);
   the long-lived lead compacts at 60% use (HumanLayer keeps 40 to 60%,
   https://github.com/humanlayer/advanced-context-engineering-for-coding-agents/blob/main/ace-fca.md).
6. Failing proof output returns to the writer cut to the failing lines and their context (Anthropic costs page:
   filter output before it enters context). That repair context dies with the session; failure text never enters a
   route, scar, skill or persona (EVAL).
7. Progress lives in files: the MVP journal and the worker's checklist of check IDs and states.
8. **Rule budget.** Persona, worker contract, scar rules and hot shelf stay under 60 lines per brief. Every clause is
   placebo-tested on Faber's own flows before it stays, and lines like "think carefully" or "do not be lazy" are not
   used: the Claude Code team cut over 80% of its system prompt for the Claude 5 models with no measurable loss on its
   coding evals (CTX). Tool usage text lives in the tool card only (RW-35).

### 6.4 Judges

- Writer vendor ≠ judge vendor, always (MVP: other-vendor review is mandatory), and a judge never shares a session
  with the writer. The engine logs the model that actually answered each judge turn: Opus 5.5 can move a flagged turn
  to Opus 4.8 for the rest of a session (O55), and Codex reports reroutes with `ModelReroutedNotification` (reason
  `highRiskCyberActivity`, in the 0.162.1 schema). A verdict from a fallback model of the writer's vendor is marked
  "not independent".
- **Blockers only, with a repro.** Reviewers and judges report only problems that block acceptance, each with its
  place (file and line, region, timestamp or frame), the reason, and how to show it fails; they also list what they
  could not confirm. Code rejects a finding without a repro (O55).
- **Yes/no claims, random order, twice.** Rubrics are checkable yes/no claims; pairwise inputs carry no baseline label;
  each judgment runs twice, once in each order, and a flip counts as a tie; timeouts, API errors and truncated
  answers are counted apart from verdicts (EVAL). The original wins ties (TL-ADE edit rule). In MT-Bench only GPT-4
  stayed consistent under swaps in more than 60% of cases (https://arxiv.org/abs/2306.05685), and judges favour longer
  answers (https://arxiv.org/pdf/2310.10076), so code compares lengths, and rubrics ask for a short justification,
  never "show your reasoning" (RW-32).
- **Anti-median** (creative work only): when there are sibling drafts, a judge names the most generic one and it loses
  a point; finalists are also compared with human-made exemplars from the person's taste or licensed references.
- **Calibration:** planted defects (UX sentinel §8) keep proving each judge; a judge that misses them loses its slot.
- **Placebo:** every tool, skill and persona section is periodically run against a no-tool placebo on probes and kept
  only where it measurably helps (Kits §15.3, §15.4).
- **First code step (Phase 3):** `engine/src/judge.rs`, pure: pairs in both orders, the tie rule, length comparison and
  the anti-median penalty over judge outputs.

### 6.5 Quota economy and cache

Unchanged in rules: auto quota pacing (R88 daily pace, R90 own and account ledgers, R91 start every eligible node,
spare quota buys quality, the shared-account pause). New inputs: ghosts give the start card's estimate; probes,
challengers and the bench run only on spare quota near a reset, never on the daily grant.

**Price the cache, not just tokens (COST).** The API-equivalent meter splits input, cache read, cache write (5-minute
and 1-hour) and output, thinking included. On Opus 5.5 a cache read costs 1/20 of input, and the post's example
session is 91% cache reads. Claude Code keeps the cache 1 hour on a subscription, and 5 minutes on an API key or once a
subscription draws usage credits, so the start card shows the account's cache lifetime and the meter never assumes
one constant. A model switch, a fast-mode toggle or a tool change mid-session forces a re-cache (about US$0.60
API-equivalent at 120K tokens on Opus 5.5); compaction at 150K costs about US$0.25. Rules (RW-31):

- Never switch model, fast mode or tools mid-session; escalation is a new session with a hand-off (section 6.6).
- A worker idle past its cache lifetime resumes as a fresh session from its checklist when that costs less than a
  full re-read.
- The pacer pauses work at step boundaries, not mid-step, since an idle worker pays a full re-cache.
- Whether an effort change clears the cache is disputed between two claude.dev posts (S55 and COST), so it is measured
  first, as auto quota pacing §6 already requires for the cache. Codex cache costs are unmeasured; the same buckets
  are logged from its usage reports.

### 6.6 Model and effort on every spawn

- **Pinned.** Every spawn passes an explicit full model id and effort (`--model` and `--effort` in `claude.rs`; the
  thread model and the turn `effort` in `codex.rs`). Aliases such as `sonnet` or `default` are never passed, because
  their target moves (`sonnet` means Sonnet 5.5 from Claude Code v2.1.284), and an unset effort silently changes the
  rung (S55, O55). The engine logs the model, effort and CLI version the session reports.
- **Defaults by step kind (EFF, S55).** Low: sketches and mechanical edits. Medium: regular features. High: bug fixes
  and verification. xhigh and max: only after a measured gain on that step kind, and never max on Sonnet (Erick's
  standing rule). Sonnet 5.5 takes well-scoped work with a spec and a check, bug fixes included; Opus 5.5 takes
  judgment-heavy, long-horizon work. A route's best proven pair overrides these defaults (section 1.2).
- **Escalate on a failed proof, not by pace.** Pace may delay a step but never pushes it below the route's best proven
  pair. A failed proof first raises effort one level, then moves up a model, each as a new session with a hand-off
  (on Codex an effort raise can stay in the thread, since effort is set per turn): no other model reads Sonnet 5.5's
  thinking blocks, and a mid-session switch re-caches. Higher effort finds more edge
  cases but assumes more silently (EFF: in 370 replays missed bugs fell from 40 to 14 while wrong readings rose from 25
  to 47), so a misreading goes back to the ask step, not up the ladder (RW-28).
- **First code step (MVP+7):** a spawn record in `engine/src/project/worker.rs` with the requested and reported model,
  effort and CLI version, and the escalation rule as a pure function.

## 7. The Faber Method

### 7.1 Scope

Every code task Faber runs: the person's projects and Faber building itself. It is enforced by the engine (plan
validator, roles, proofs, queue), not by a skill the model may skip (RW-25).

### 7.2 Steps

1. **Risk first.** At most 3 questions with defaults, answers saved; they switch check groups (generated pipelines
   §15.1.1, Software line).
2. **Scouts map callers.** Affected callers and public shapes are listed before any code (software bar).
3. **Contract.** Owner, boundary, examples, proof, dependencies (MVP §6 contract fields).
4. **Test first, by someone else.** A read-only test-proposal worker writes a test that fails on the base and passes
   only when the request is done (MVP role; TL-ADE Software line).
5. **Write.** One writer per lease. Tests are never edited or removed to get green
   (https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents; Beck,
   https://kentbeck.com/summaries/augmented-coding-beyond-the-vibes/).
6. **Prove.** The AGENTS.md proof commands, `cargo t`, reflexes of section 6.2.
7. **Look.** A screen change runs the real app at the shell widths and themes (UX sentinel).
8. **Other-vendor review** of the exact composed tree, asked only for correctness and requirement gaps, because a
   reviewer asked for gaps invents some (https://code.claude.com/docs/en/best-practices). Findings follow section
   6.4: blockers only, each with a repro.
9. **Integrate.** Serial queue, one commit per coherent change, receipt.
10. **Scar.** An escaped defect adds a scar: the rule to the route, the check to the bar.

The writer never approves its own work; the lead never writes source; the person decides direction, taste, money and
legal; small tweaks take the fast path (TL-ADE Software line). **First code step:** the MVP plan validator requires,
for feature and bug-fix nodes, a test-proposal node from a different worker before the writer node.

### 7.3 Existing process skills

| superpowers skill | Fate | Why |
| :--- | :--- | :--- |
| brainstorming, writing-plans, executing-plans | Replaced | The deliverable sheet, bar and plan graph do this in engine code |
| subagent-driven-development, dispatching-parallel-agents | Replaced | Engine dispatch, R91 |
| using-git-worktrees, finishing-a-development-branch | Replaced | MVP git broker and serial queue |
| verification-before-completion, requesting-code-review | Replaced | Receipts, proofs and mandatory other-vendor review |
| test-driven-development | Warm card for test-proposal workers | Kept only if it beats placebo |
| systematic-debugging | Warm card for bug-fix writers | Kept only if it beats placebo |
| receiving-code-review | Warm card for writers repairing review findings | Kept only if it beats placebo |
| using-superpowers | Never loaded | Its "must invoke" rule fights the kit; skill-creator warns against heavy MUST wording |

### 7.4 Self-development

A twin build must pass the stable install's proof commands before it swaps in (MVP G6 canary, last good kept). The
route book has one shape family at first: `faber.code-change` × {feature, bug fix, refactor, dependency} × {engine, ui,
shell}. The person's frustrated sentences are its first failing acceptance scenarios (section 10.3).

## 8. Beating the TL-ADE

### 8.1 All 26 lines

| Line | TL-ADE's best move | Faber's concrete edge |
| :--- | :--- | :--- |
| Quick question | Each fact tied to its passage | Each quote is an ID whose text code finds verbatim in the stored page; a follow-up reuses stored sources through the drop-ledger |
| Translation | Protects names, numbers, links | Protected spans are quote IDs restored by code after translation, so no model can alter them |
| PC tasks | Logs every action | Runs with no visible window; every action is an idempotent receipt; a purchase or send stays a needs-you card |
| Learning | Graded exercises | A program grades where it can (a learner's `.blend` passes the mesh checks); the route remembers what the person knows |
| Title | Tested with the thumbnail at phone size | The phone feed is rendered by code at three widths; title and thumbnail are judged as a pair; anti-median drops the generic title |
| Edit image | Edit stays only if it beats the original | A protected-region pixel diff (face, logo, text masks) runs before any judge; a free GIMP route when there is no Photoshop |
| New image | 3 drafts, text set separately | Drafts come from Codex's built-in image generation; text is never drawn by the image model: it is typeset as a vector layer; drafts are judged at their real placement size |
| Audio and voice | Says when it could not listen | Measures first (BS.1770 LUFS, true peak, clipping) against the delivery target; speech-recognition round trip for pronunciation |
| Animated video | Scene by scene | Each scene is a HyperFrames composition with an owner; the flash test (WCAG 2.3.1) and caption checks run by code on the export |
| Research | Copies count as one; no over-claiming | Every DOI resolved and Crossref `update-to` read for retractions; every quote found in its source |
| Ideas | 6 to 12 isolated AIs and a tournament | Anti-median penalty plus a real lookalike search per finalist; the person writes their ideas first |
| Purchase and price | Reads the whole listing | Compatibility and total cost computed by a program; Faber never buys |
| Text and script | Removes the "written by AI" feel | AI-tell scan and readability as program checks; a text-to-speech read-through for rhythm |
| Data and spreadsheets | Math by program, redone | Table Schema validation, totals reconciled, zero error values; the recomputation runs on a second code path |
| Documents and slides | Every page seen as an image | Overflow found by code in the rendered PDF; contrast and alt text checked; an open-format route when Office skills' license blocks |
| Video idea | Ideas from real outliers | Outliers computed by code (views against the channel median); niche data kept with its fetch date |
| Thumbnail from scratch | Ugly sketches tested before Blender | Same, plus six-view checks after every Blender pass and a 1x phone-feed legibility test; taste saved |
| Video editing | Redo only the changed piece | Content-addressed receipts rerun exactly one segment and its captions; FFmpeg black, freeze and silence detection |
| 3D model | Six-side drawings and silhouette match | Silhouette overlap computed by code between six renders and the drawings; Blender toolbox and glTF-Validator on export |
| Brand and logo | Tested small and in one colour | 16 and 32 px, greyscale and one-colour versions generated by code; favicon and Android icon specs checked; trademark left to the person |
| Software | Test first; writer never approves | The complete software bar (generated pipelines §15.1.1) plus scars as permanent checks and receipts; the head-to-head of generated pipelines §14.8 |
| Full YouTube video | Parts made separately | Fact-level reruns from the fact sheet; idea, script and shot list delivered before the recording arrives |
| Marketing campaign | Never invents a price or testimonial | Every price and claim is a fact ID expanded verbatim; drafts only; the person publishes |
| Product launch | A size change redoes only the 3D | Spliced routes with inherited scars; the plan graph invalidates only dependents |
| Data to presentation | A corrected number updates everything | Slide numbers are references to computed cells; code checks each slide claim against the numbers |
| Client website | Drafts before code; customers try it | Object-level authorization tested per role (OWASP API1:2023); Core Web Vitals and WCAG 2.2 by program; staging only, the person releases |

Where fixed lines fail (TL-ADE map, last section), Faber's answer: shapes no line covers get a tutorial level and a
splice; an improvement to one route's scar reaches every route it applies to; tools come from what the person has and
what wins probes, not from a fixed station.

### 8.2 Niche tasks met for the first time

The generic first run: shape key → a new shape (distance above 0.3) → a seed spliced from the nearest routes within
0.6, if any → 2 or 3 scouts read official docs and
"how experts check this" → the other vendor drafts the bar with grounds (generated pipelines §6.1) → at most 3
questions with defaults → tool search (section 4.1) → run with eyes after every pass and two judges → hard proofs pass
→ a provisional route and its scars; bench candidates logged.

| Task | Spliced from | Hard proofs that can mint it | Eyes |
| :--- | :--- | :--- | :--- |
| Stardew Valley mod (SMAPI, C#) | C# code change, game mod, pixel sprites | `dotnet build` passes; SMAPI log has zero errors and warnings for the mod; sprites on the 16 px grid with the game's palette | Sprite contact sheet; an in-game screenshot only in the hidden runner. If the game cannot run hidden, the launch test becomes a needs-you step, never a visible window |
| Isometric pixel-art tileset | Pixel art, sprite sheet | 2:1 tiles exact; tiled 4×4 wrap diff shows no seams; palette size; no anti-aliased pixels | Tiled map preview at 1x and 3x |
| Blender character rig | 3D model, game asset | Bone names mirrored (L/R); every vertex weighted, at most 4 influences; no non-manifold after deformation | Six views in 3 test poses |
| Podcast cleanup with chapters | Audio, text | −16 LKFS ±1 and true peak ≤ −1 dBFS (Apple Podcasts); zero clipped samples; transcript-based chapters match timestamps | Spectrogram before and after; speech-recognition round trip |
| Minecraft Fabric mod | Java code change, game mod | Gradle build; GameTest passes; tick time ≤ 50 ms with the mod | Same seed and camera, mod on and off |
| Godot 2D prototype | Software, game asset | GUT tests headless; scene loads without errors | `editor_screenshot` (godot-ai) at fixed camera |
| Skyrim load-order fix | Game mod | LOOT reports zero errors; dirty plugins cleaned dependencies first, after reading author notes | Same save, mod list on and off |
| Wedding invitation for print | New image, documents | Names and dates as quote IDs, verbatim; text inside its frame in the rendered PDF; contrast | Rendered page at print size and phone size |
| Spreadsheet macro (VBA) | Data and spreadsheets | Runs on a copy only; zero error values after; totals reconcile; macros never enabled on the original | Before and after tables |

Thresholds not in r6 (bleed sizes, influence limits) are rules of thumb and are labelled as such in the bar.

## 9. Best-practice compliance

| Mechanism | Source | Departure and reason |
| :--- | :--- | :--- |
| Persona appended to the preset | https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts | New identity without replacing: keeps tool and safety guidance; the model underneath is named when asked |
| Session instructions on Codex | https://learn.chatgpt.com/docs/config-file/config-reference.md | None; Faber never writes AGENTS.md in a user's repo |
| Short always-on text, knowledge on demand | https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices; https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | None |
| Hard rules in code, permissions and blocking hooks | https://code.claude.com/docs/en/hooks; https://code.claude.com/docs/en/best-practices | None |
| Skill format and listing budgets (Claude 1% of context; Codex 2% or 8,000 characters) | https://code.claude.com/docs/en/skills; https://learn.chatgpt.com/docs/build-skills | Contents list above 100 lines (platform page), not 300 (skill-creator): the stricter one |
| Skill evals with and without the skill; trigger queries | https://github.com/anthropics/skills/tree/main/skills/skill-creator | Adds a best-existing-tool arm, the second engine and blind other-vendor judging |
| Few, non-overlapping, consolidated tools | https://www.anthropic.com/engineering/writing-tools-for-agents | None |
| CLIs and scripts before MCP | https://code.claude.com/docs/en/costs | MCP only for live app connections |
| Strict per-session MCP; check `system/init` | https://code.claude.com/docs/en/cli-reference; https://code.claude.com/docs/en/headless | None |
| Codex MCP `enabled_tools`, `required` | https://learn.chatgpt.com/docs/extend/mcp.md | None |
| Orchestrator and workers; 4x and 15x tokens | https://www.anthropic.com/engineering/multi-agent-research-system | Used for breadth and judging only |
| One writer for coupled code | https://cognition.com/blog/dont-build-multi-agents; https://cognition.com/blog/multi-agents-working | Parallel writers only on disjoint, qualified areas |
| Complete briefs: objective, format, tools, boundaries | Anthropic research system post | The envelope enforces them by schema |
| Summaries of 1,000 to 2,000 tokens; notes in files | https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | None |
| Stable prefix and cache | https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus | None |
| Context use 40 to 60% | https://github.com/humanlayer/advanced-context-engineering-for-coding-agents/blob/main/ace-fca.md | 60% applies to the lead; workers are one-unit sessions |
| Runnable checks; evidence, not claims | https://code.claude.com/docs/en/best-practices | None |
| Goals end on evidence | https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex | The engine owns stop conditions; `thread/goal/set` waits for a probe |
| Never edit tests to pass | https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents | None |
| Reviewers report only correctness and requirement gaps | https://code.claude.com/docs/en/best-practices | None |
| Judge order and length bias | https://arxiv.org/abs/2306.05685; https://arxiv.org/pdf/2310.10076 | None |
| Evals from real failures; pass@k and pass^k | https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents; https://hamel.dev/blog/posts/evals-faq/why-is-error-analysis-so-important-in-llm-evals-and-how-is-it-performed.html | None |
| Effort set per task; max only after a test | https://code.claude.com/docs/en/model-config; https://learn.chatgpt.com/guides/best-practices | Sonnet never at max (Erick's saved rule) |
| Untrusted input never triggers outward actions | https://simonwillison.net/2025/Jun/16/the-lethal-trifecta/; https://arxiv.org/pdf/2506.08837 | None |
| Audit third-party skills; trusted sources | https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills | Adds quarantine, canary token, pinned hashes (Kits §12) |
| Skill search ranking | https://github.com/vercel-labs/skills/blob/main/skills/find-skills/SKILL.md | Probe outcomes rank; installs and stars only break ties |
| Codex sandbox `workspace-write`, `on-request` | https://learn.chatgpt.com/docs/sandboxing.md | None |
| `--bare` for scripted runs | https://code.claude.com/docs/en/headless | Not used: it needs an API key and ignores subscription sign-in; every needed flag is passed explicitly |
| Small, inspectable steps with a person in charge | Karpathy via https://www.latent.space/p/s3 (secondary) | None |
| Pinned model and effort; effort by step kind; escalate on failure | https://claude.dev/blog/building-with-claude-sonnet-5-5; https://claude.dev/blog/getting-the-most-out-of-opus-5-5; https://claude.dev/blog/spending-your-effort | Claude-only evidence, applied to Codex too |
| Held-out gate; one change per round; no failure text in prompts | https://claude.dev/blog/automating-eval-design-and-hillclimbing | Adds the second engine and the placebo arm |
| Blockers with a repro; finish line and stop rules for unattended work | https://claude.dev/blog/getting-the-most-out-of-opus-5-5 | None |
| Silent-failure guards: down tool servers, three-state side effects, planted notes | https://claude.dev/blog/building-effective-agent-automations | None |
| Price cache reads and writes; no mid-session switches | https://claude.dev/blog/what-a-task-costs-on-opus-5-5 | The effort-change cache effect is measured first (posts disagree) |
| Fewer, measured instructions | https://claude.dev/blog/the-new-rules-of-context-engineering-for-claude-5-generation-models | Every clause placebo-tested on both engines |
| Cloud sessions | https://claude.dev/blog/claude-code-in-the-cloud | Not used for workers yet: flags and storage unverified |

## 10. Optimized route to the self-development MVP

### 10.1 What goes in now (MVP+), and nothing more

The MVP keeps its scope, calendar (13 to 22 October) and gates G1 to G8, plus what Erick's E1 and E10 already add:
on-the-fly kits with gate G9 (Kits §17.1) and the complete software bar with gate G10 (generated pipelines §15.1).
The rework adds only the seven pieces below. They fit inside tasks the MVP already has, add no catalog, no tool
server, no install and no design overhaul, and total about 2 days (RW-26):

| Mark | Addition | MVP task and file | Proof |
| :--- | :--- | :--- | :--- |
| MVP+1 | Persona and worker contract replace the name note | Task 5 and 6; `engine/src/persona.rs`, `claude.rs:72`, `codex.rs:323` | Unit tests on the arguments both drivers build; persona ≤ 1,800 characters; the second turn's cache-read share not lower than before (Claude usage reports it) |
| MVP+2 | Envelope fields `ping`, `requires`, `checks`, `quotes` expanded verbatim; reply fields `assumptions` and `unconfirmed`; `note` ≤ 60 words; bounce on malformed replies | Task 6; `engine/src/project/worker.rs` | The rendered prompt holds the exact bytes of each requirement; a malformed reply is bounced with no model call |
| MVP+3 | Receipt type; every completion cites a receipt id; "unverified" label | Task 7 and 9; `engine/src/project/proof.rs`, Projects screen | G8 run: 100% of completions carry receipts; zero "done" without one |
| MVP+4 | Reflex loop stops (three identical failing proofs or three patches on one hunk) and the stall rule (one "continue", then `Blocked`) | Task 6; `worker.rs` | A looping fixture worker is stopped with `Blocked` inside its grant (G2); a fixture turn ending with an offer to continue gets one "continue", then `reason.stall` |
| MVP+5 | Scar store: `engine/src/book.rs` with `Scar` only; in the MVP, scars add checks to the bar and no rule lines, because the held-out gate needs the Phase 3 probe bank | Task 5 and 13 | The four owner findings enter as scars with their UX sentinel checks |
| MVP+6 | Record the G8 run as route v0 of `faber.code-change` (typed data only, not used to plan) | Task 13 | File written with steps, roles, models, costs and receipt ids |
| MVP+7 | Pinned full model id and effort on every spawn; reported model, effort and CLI version logged; a failed proof escalates effort, then model, in a new session | Task 2 and 6; `worker.rs`, `claude.rs`, `codex.rs` | A spawn without an explicit model or effort fails in tests; the G8 log names the model that answered every turn |

Persona text also reaches Chat, because both areas share the driver function; that is a one-line behaviour change with
fixture tests, not a Chat feature.

### 10.2 What waits for the first public version

Planning from routes, tutorial levels in all domains, splicing, challengers, the crafting bench and its harness, the
catalog and setup install, the Tool store shelves, `faber-mcp` and Codex `dynamicTools`, the distiller, anti-median,
the placebo lane, the 60 day-one shapes, the TL-ADE head-to-head and the niche tasks. Order: Phase 3 brings the book,
catalog, shelves, bench harness and Faber tools; Phase 5 brings the day-one shapes and the 40-request benchmark
(generated pipelines §15.3). After software, the next area is thumbnails from scratch (Erick, ER9): Codex image
generation, Blender and image editing, with the six-view and phone-feed eyes. The opt-in route upload gets its payload
code in Phase 3 and its server in Phase 5 (section 1.5).

### 10.3 First failing acceptance scenarios: the person's own sentences

AS-1 to AS-4 are proven red on today's build before any fix (UX sentinel §6). AS-5 to AS-9 are written as failing
tests before the code that makes them pass (test first). Each becomes a scar once it passes.

| ID | The sentence | Given / when / then | Check |
| :--- | :--- | :--- | :--- |
| AS-1 | "A panel teleports" | Any panel opens or closes; at least one intermediate frame is drawn | UX sentinel M1 |
| AS-2 | "The composer does not grow" | Three lines are typed; the composer height grows up to its cap | UX sentinel I1 to I5 |
| AS-3 | "Mojibake in the breadcrumb" | The breadcrumb renders in all five languages; no `â€º` or other mojibake | UX sentinel T1 |
| AS-4 | "The last message sits on the composer, so people try to scroll" | The thread is at its end; the gap under the last message is at least 24 px | UX sentinel L4, L5 |
| AS-5 | "Never open windows on my PC; they steal focus from my games" | A full mission runs; zero visible windows or consoles | Window audit (G7) |
| AS-6 | "Rust build folders filled my E: drive" | After the canary and promotion, no stale build folders remain outside the build slots | New disk check in Task 11's build slots |
| AS-7 | "The plan is still too generic" | The G8 lead plan is validated; every node names a check id and a file path, or it is rejected | Plan validator (Task 5) |
| AS-8 | "Never trust one model blindly" | A change is accepted; proofs and an other-vendor review exist for it | G4 |
| AS-9 | "Ask me only what truly needs me" | The G8 run ends; every question asked was about direction, taste, money or legal | Question log, counted by class |

AS-1 to AS-4 are the owner reports in UX sentinel §6. AS-5 to AS-9 are Erick's standing instructions and the rework
brief, paraphrased.

### 10.4 First measurements

Tokens per hand-off before and after MVP+2 (baseline from the first worker runs); quota per accepted change against the
route v0 ghost; first-try proof pass rate; review findings per change; questions per mission by class; bounce rate;
idle RAM (≤ 250 MB test). Spend stays inside the daily grant; extra comparisons stay under the US$25 test envelope.

## 11. Risks and what to measure

| Risk | Guard | Measure |
| :--- | :--- | :--- |
| Routes freeze mediocrity | Hard proofs to mint, challengers, anti-median | Challenger win rate per route; above 30% means the routes are weak |
| Scar rules bloat briefs or overfit | Rule lines only after the held-out gate, at most 5 per brief; 60-line rule budget | Brief tokens from rules; held-out lift per rule |
| Persona conflicts with the CLI prompt or adds refusals | Append only; 20 persona behaviour cases on both engines | Identity compliance; refusal rate before and after |
| Persona cuts cache reuse | Static text, fixed per conversation | Cache-read share of second turns |
| Install-all fills the disk or slows setup | Pinned tools only, weights on first use, size shown | Store size; setup minutes; free space on the chosen drive |
| A third-party tool is hostile | Kits §12 bounds, quarantine, canary token, pinned hashes, OSV | Escape tests; network destinations per tool in the runner |
| Tool telemetry | Documented opt-outs set; no opt-out means not recommended | Outbound hosts per tool in the runner logs |
| Judges are noisy or biased | Both orders, other vendor, planted defects | Agreement with program checks and with the person's picks |
| Placebo and challengers cost too much | Spare quota only, probe lane caps | Quota per kept finding |
| Envelopes are too rigid | `note`, `Disagree`, two bounces then `Blocked` | Bounce rate under 5%; blocked rate |
| Experimental Codex APIs change (`dynamicTools`, goals, skill roots) | Probed on every CLI update; inline fallback | Lever probe results per version |
| Vision judges miscount | Code computes measurable parts | Judge against program disagreement rate |
| Self-development breaks Faber | MVP G6 canary and last good build | Rollback drills passed |
| Ponytail cuts needed code | Faber Method outranks it; placebo | Escaped defects with and without it |
| The route upload leaks private data | Typed payload from catalog words only; planted-string test; the exact payload shown before consent | Leak tests; strings dropped per upload |
| Poisoned uploads | Uploads only propose; the maintainer re-runs the held-out gate before shipping | Candidates rejected by the gate |
| A cheap retry costs more than one strong pass | Escalation on failed proof; finish rate tracked per model and effort pair | Retries and quota per accepted step |
| Codex image generation drains quota | Image turns metered apart; `usageLimitExceeded` feeds the pacer | Quota per image; limit hits |

## 12. Decisions

**Erick (2026-10-12 card):** ER1 quality first, speed and cost second; ER2 terse style only in notes between AIs,
everything else verbatim; ER3 setup installs all recommended tools from source into Faber's store, activated only when
useful; ER4 the new plans go to the MVP session only after this rework.

**Erick's answers (2026-10-12, second card):** ER5 free tools by default; paid apps only when already installed;
credits only after a yes per task. ER6 the person picks areas at setup and Faber installs those areas' recommended
tools (refines ER3); image generation uses Codex's built-in tool, so no ComfyUI by default; heavy audio and voice tools
download only when needed and wanted, with options such as HyperFrames or Voice Studio. ER7 about 60 kinds of jobs
across all areas, spread over the weeks before release. ER8 anything useful only to one person stays on their PC; an
opt-in anonymous upload of routes and scars, asked convincingly and honestly (off until yes, no dark patterns, says
exactly what is sent), with a minimum server after the MVP; telemetry stays off by default. ER9 thumbnails from
scratch is the next area after software. ER10 Faber talks calmly and briefly, like a senior colleague.

**Earlier directions this design leans on:**
E1 (no hand-written kits, MVP included), E9 (every request gets its own plan), E10 (complete software bar in the MVP),
E11 (no fixed-line runs), E17 (purpose facets), E18 (ponytail, caveman, Taste-Skill, Impeccable as cold-start priors).

**AI (Erick delegated), not reviewed:**

| Tag | Decision |
| :--- | :--- |
| RW-1 | Shape distance weights and the 0, 0.3, 0.6 thresholds |
| RW-2 | Routes seed every compile, never replace it; departures are logged decisions; winning departures become versions |
| RW-3 | Minting rules: provisional, proven, draft; judge votes alone never mint |
| RW-4 | Held-out gate (train and test both better, beyond noise) for route versions, scar rules and step kits; cost breaks ties at 0.8 of the ghost |
| RW-5 | Challengers on spare quota replace a route only through the held-out gate |
| RW-6 | Scar checks ship at once; scar rule lines only after the held-out gate, at most 5 per brief |
| RW-7 | Per step, the best proven model and effort; cheaper only after equal-quality probes |
| RW-8 | Day-one routes and first-party skills ship as data from Faber's own runs |
| RW-9 | The look-and-polish loop and the per-domain eyes table |
| RW-10 | The persona: about 330 tokens, appended on Claude Code, developer instructions on Codex |
| RW-11 | Workers get a worker contract, not the persona |
| RW-12 | Host-app MCP servers install when the app is found |
| RW-13 | Heavy media tools and weights download only on demand, after a yes (ER6) |
| RW-14 | Tool telemetry switched off by documented opt-outs; no opt-out means not recommended |
| RW-15 | Route kits are the strongest prior for the per-step assembler; Codex store skills via extra roots plus selection |
| RW-16 | Search order; probe outcomes rank candidates; installs and stars break ties |
| RW-17 | The crafting bench stages, thresholds and spend source |
| RW-18 | Typed envelopes with verbatim ID expansion and bounces |
| RW-19 | Terse notes by a renderer rule; the caveman skill replaces it only if it wins on probes (E18 candidate) |
| RW-20 | Ponytail keeps the E18 writer prior; in the MVP only from the person's shelf; placebo on both engines decides |
| RW-21 | Distiller threshold 6,000 tokens, brief 1,500, coverage checked by code |
| RW-22 | Receipt display rules: greyed "done", "unverified" |
| RW-23 | Topology table and caps (5 scouts, budgets in every envelope) |
| RW-24 | Reflex stops and halving retry budgets |
| RW-25 | The Faber Method is enforced by the engine; superpowers mostly replaced |
| RW-26 | The seven MVP+ additions and nothing more |
| RW-27 | Routes and scars hold typed fields only; bench briefs get a typed trace summary, never raw output |
| RW-28 | Pinned model and effort per spawn; defaults by step kind; escalation on a failed proof, by new session |
| RW-29 | Stall rule: one "continue", then `Blocked` |
| RW-30 | Pre-start tool and MCP status check; plugins recorded; side effects in three states |
| RW-31 | Cache-priced meter; no mid-session switches; pauses at step boundaries |
| RW-32 | Judges: blockers with a repro, yes/no claims, run twice, answering model logged, fallback marked not independent |
| RW-33 | Opt-in upload: typed payload, one honest ask, minimum server, maintainer gate |
| RW-34 | Image generation through Codex's built-in tool on the app-server; HyperFrames as video default; VoiceStudio on demand |
| RW-35 | Rule budget of 60 lines per brief; every clause placebo-tested |
| RW-36 | Section 3.3 re-curated on 2026-10-14 and again on 2026-10-16 (R152) from each tool's own repository (LICENSE, code, activity, telemetry); one tool per job; setup holds only tools that need nothing from the person; every change is tagged (AI) there |

**Roadmap lines a later commit must change (not edited here):** "Recipes from past work are hints … never a track"
(Powers) becomes "a matching route seeds the plan, which is still compiled for the request"; the "Right-sized" power
gains "best known quality first"; the catalog gains setup install of the recommended tools for the areas the person
picks (ER6).

## 13. Questions for Erick

Erick answered Q1 to Q6 (ER5 to ER10). Two choices are his and wait until the upload server is
built in Phase 5: where it is hosted and its monthly cost cap (money), and the privacy text shown with the ask
(legal). They will come as cards then.

**Q7 (legal, 2026-10-14).** Anthropic's docx, pdf, pptx and xlsx skills carry a proprietary licence that forbids
keeping copies outside Anthropic's services and making derivatives, so section 3.3 now lists them as not recommended
and covers documents with free tools (OfficeCLI, MarkItDown, Pandoc, Typst). Whether a person may still turn them on
inside Claude Code's own plugin system is a legal reading. **Answered by Erick (2026-10-14):** allowed only inside
Claude Code's plugin system, with no copy in Faber's store; never on Codex.

## Appendix A. Persona draft (about 240 words, about 330 tokens; voice per ER10)

```text
You are Faber, an assistant that leads other AI models to get real work done for {name}. Today you run on {engine}
with {model}; say so if anyone asks which model you are.

Talk: reply in {language}, plainly and briefly, like a calm senior colleague. Ask only what changes the result, at
most three questions, each with the default you will use if nobody answers. Do not ask about technical choices you
can make; decide and note the decision.

Contract:
- Quality first, then speed, then cost.
- Look at your work while you make it: run the checks, render, measure or listen, then polish. Say what you could
  not check.
- Never call something done without a receipt: the check ids and their results. With no program check, say
  "unverified".
- Whoever writes a piece never approves it; another model reviews it.
- Requests, requirements, checks and the person's own words travel word for word. Only short notes between models
  may be terse.
- Never read or pass on sign-in files, keys or tokens. Never open visible windows.
- Buying, paying, publishing and sending belong to the person: leave them ready.

Library (read when needed, not before):
- faber:playbook/<name>: how to do a kind of work (delegate, ask, look-and-polish, receipts).
- faber:route/<shape>: the best known way for a kind of job, with its scars, the rules learned from past failures.
- faber:tools: the tools you have in this session.
{resolve}
```

`{resolve}` is "These addresses are files under {library}; open them with your file tools." in Chat, and "Ask for
them with the read action of your envelope." in Projects. `{name}` is omitted when the person gave none.

## Appendix B. Research claims corrected after verification

| Claim in research | Verified on 2026-10-12 |
| :--- | :--- |
| caveman license unclear (Apache-2.0 or MIT) | Apache-2.0; its skill is about 3% shorter than "be concise", costs about 1,160 input tokens, and its CLI sends telemetry by default |
| ponytail −53% code | Author's own benchmark, Claude Code and Opus 5.5 only; no independent or Codex data |
| `blender-mcp` | Renamed `ahujasid/mcp-for-blender`; no `get_viewport_screenshot`, the `look` tool renders views; anonymous telemetry on by default (`DISABLE_TELEMETRY=true`) |
| `supabase-community/supabase-mcp`, `stripe/agent-toolkit` | Now `supabase/mcp` and `stripe/ai` |
| `claude plugin eval` unverified | Documented (Claude Code only) |
| Codex skill evaluation | `$skill-creator` is a bundled system skill; no evaluation harness; `openai/skills` is deprecated for `openai/plugins` |
| Codex `-c` overrides are secondary | Official |
| App-server goals unknown | `thread/goal/set`, `get`, `clear` exist; experimental `dynamicTools` with `item/tool/call`; `skills/extraRoots/set` is per process and not persistent |
| Some Claude Code flags missing | `--append-system-prompt-file`, `--max-turns`, `--append-subagent-system-prompt` are documented though absent from `--help` |
| One skill listing budget | Claude Code 1% of context (1,536 characters per entry); Codex 2% or 8,000 characters |
| Netflix 42 characters and 20 per second unverified | Verified in the English (USA) Timed Text Style Guide; durations (5/6 s to 7 s) are on the general page |
| ACX room tone 0.5 to 1 s at the head | 1 to 5 s at head and tail |
| YouTube thumbnail 2 MB | 2 MB is the mobile limit; desktop allows 50 MB; 3840×2160 recommended, 640 px minimum width |
| EBU R 128 true peak −1 dBTP | −23 LUFS confirmed; −1 dBTP not confirmed on the pages read (the PDF did not extract) |
| godot-ai needs Godot 4.3+ | 4.7+ for v4; telemetry opt-out `DISABLE_TELEMETRY` |
| Unity MCP telemetry | On by default; three documented opt-out variables |
| Audacity MCP | Audacity 3.x only; Audacity 4 removed mod-script-pipe |
| Context7 data | Sends query, library id, client name and, over HTTP, the IP; key optional |
| Impeccable detector | 59 deterministic rules, `npx impeccable detect <path or url> --json`, no model, no key |
| MCP Registry path in doubt | Both `v0` and `v0.1` return JSON |
| Cognition "default to one agent" | "Simplest" reliable choice; the 2026 post lists working multi-agent patterns |
| "Never run parallel subagents on one file" (Anthropic blog) | No absolute "never" in the post |
| Beck "agents delete tests" | Not in Beck's essay; the Anthropic harness post is the source for "never edit tests" |
| Karpathy "verification is the bottleneck" quote | Not found; "keep the AI on a short leash" comes via the Latent Space write-up |
| Reference-file contents list | Platform page says over 100 lines; skill-creator says 300 |
| `--bare` for scripts | Needs an API key; not usable with subscription sign-in |
| CLAUDE.md "under 200 lines" | Stated in the claude.dev cost post, with no reason given; the memory docs page was not read in full |
| "Hyperframes" (Erick) | HeyGen's open-source HyperFrames: Apache-2.0, 61.5k stars, pushed 2026-10-12; local render with Node 22+, FFmpeg and headless Chrome; local Kokoro-82M text-to-speech; opt-out PostHog telemetry (switch to confirm); Windows support not stated |
| "Voice Studio" (Erick) | debpalash/VoiceStudio (formerly OmniVoice Studio): AGPL-3.0, 59.8k stars, pushed 2026-10-12; local Electron app with HTTP API and MCP server; analytics only with consent; HeyGen has no product of that name |
| Codex image generation | `image_generation` stable and on in 0.162.1; app-server `imageGeneration` items and `modelProvider/capabilities/read` confirmed in the generated schema; gpt-image-2, the `$imagegen` skill and the 3 to 5 times quota rate come only from secondary sources |
| claude.dev notes | "Up to 30% less per task" reads "up to 30% less for most work"; the effort-change cache effect is disputed between posts; "72% vs 56%" is not on the blog; the mods docs page returned 404 |

## Appendix C. Threshold and tool sources

Tool names written as `owner/repo` are GitHub repositories at `https://github.com/owner/repo`.

| Area | Primary source |
| :--- | :--- |
| Blender 3D Print Toolbox | https://docs.blender.org/manual/en/4.1/addons/mesh/3d_print_toolbox.html |
| glTF validation and asset auditor defaults | https://github.com/KhronosGroup/glTF-Validator; https://github.com/KhronosGroup/gltf-asset-auditor |
| No fixed triangle budget (profile instead) | https://dev.epicgames.com/documentation/unreal-engine/nanite-virtualized-geometry-in-unreal-engine |
| Atlas padding; indexed colour | https://docs.unity3d.com/450/Documentation/ScriptReference/Sprites.AtlasSettings-paddingPower.html; https://aseprite.org/docs/color-mode/ |
| WCAG 2.2 contrast, non-text contrast, targets, reflow, flashes | https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html; https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html; https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html; https://www.w3.org/WAI/WCAG22/Understanding/reflow.html; https://www.w3.org/WAI/WCAG22/Understanding/three-flashes-or-below-threshold.html |
| Icons, favicon, Android adaptive icons | https://m2.material.io/design/iconography/system-icons.html; https://developers.google.com/search/docs/appearance/favicon-in-search; https://developer.android.com/develop/ui/views/launch/icon_design_adaptive |
| YouTube thumbnails and encodes | https://support.google.com/youtube/answer/72431; https://support.google.com/youtube/answer/1722171 |
| Core Web Vitals | https://web.dev/articles/defining-core-web-vitals-thresholds |
| Screenshot diff defaults | https://playwright.dev/docs/api/class-pageassertions |
| UI anti-pattern detector | https://github.com/pbakaus/impeccable |
| API authorization and load thresholds | https://owasp.org/API-Security/editions/2023/en/0x11-t10/; https://grafana.com/docs/k6/latest/using-k6/thresholds/ |
| Captions | https://partnerhelp.netflixstudios.com/hc/en-us/articles/217350977-English-USA-Timed-Text-Style-Guide; https://partnerhelp.netflixstudios.com/hc/en-us/articles/215758617 |
| Video QC and loudness filters | https://ffmpeg.org/ffmpeg-filters.html; https://github.com/Netflix/vmaf |
| Loudness targets | https://tech.ebu.ch/publications/r128; https://podcasters.apple.com/support/893-audio-requirements; https://support.spotify.com/artists/article/loudness-normalization/; https://help.acx.com/s/article/acx-audio-submission-requirements |
| Mods and game ticks | https://loot.readthedocs.io/en/latest/app/usage/plugin_editor.html; https://ck.uesp.net/wiki/TES5Edit_Cleaning_Guide_-_TES5Edit; https://minecraft.wiki/w/Tick; https://docs.neoforged.net/docs/misc/gametest; https://github.com/bitwes/Gut |
| Text | https://support.microsoft.com/en-us/word/about-flesch-kincaid-readability-and-level-statistics; https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing |
| Data and references | https://specs.frictionlessdata.io/table-schema/; https://api.crossref.org/works |
| claude.dev posts | S55 https://claude.dev/blog/building-with-claude-sonnet-5-5; O55 https://claude.dev/blog/getting-the-most-out-of-opus-5-5; EFF https://claude.dev/blog/spending-your-effort; COST https://claude.dev/blog/what-a-task-costs-on-opus-5-5; CTX https://claude.dev/blog/the-new-rules-of-context-engineering-for-claude-5-generation-models; EVAL https://claude.dev/blog/automating-eval-design-and-hillclimbing; AUTO https://claude.dev/blog/building-effective-agent-automations; CLOUD https://claude.dev/blog/claude-code-in-the-cloud; FAST https://claude.dev/blog/how-we-made-claude-ai-faster |
| Video and voice tools | https://github.com/heygen-com/hyperframes; https://github.com/debpalash/VoiceStudio |
| Codex image generation (secondary) | https://codex.danielvaughan.com/2026/04/27/codex-cli-image-generation-gpt-image-2-visual-development-workflows/ |

## Appendix D. Build map

| Mechanism | First code step | When |
| :--- | :--- | :--- |
| Persona and worker contract | `engine/src/persona.rs`; `claude.rs:72`, `codex.rs:323`; remove `model.rs:234` | MVP+1 |
| Envelope fields, verbatim IDs, bounces | `engine/src/project/worker.rs` | MVP+2 |
| Receipts | `engine/src/project/proof.rs` | MVP+3 |
| Reflex stops | `engine/src/project/worker.rs` | MVP+4 |
| Scars | `engine/src/book.rs` (`Scar` only) | MVP+5 |
| Route v0 record | `engine/src/book.rs` writer, called at Task 13 | MVP+6 |
| Route book: match, mint, splice, challenge | `engine/src/book.rs`, `engine/tests/book.rs` | Phase 3 |
| Eyes and keep rule | `engine/src/eyes.rs` | Phase 3 (desktop app first) |
| Shelves and hot shelf | `engine/src/shelf.rs` | Phase 3 |
| Catalog and setup install | `catalog/manifest.json`, `engine/src/catalog.rs` | Phase 3 |
| Search and scoring | `engine/src/find.rs` | Phase 3 |
| Bench harness and triggers | `testkit/src/bench.rs`, `engine/src/bench.rs` | Phase 3 |
| Distiller and drop-ledger | `engine/src/distill.rs` | Phase 3 |
| Judges and anti-median | `engine/src/judge.rs` | Phase 3 |
| Faber tools | Codex `dynamicTools` in `codex.rs`; `faber-mcp` binary in the engine crate | Phase 3 |
| Spawn record and escalation | `engine/src/project/worker.rs` | MVP+7 |
| Stall rule | `engine/src/project/worker.rs` | MVP+4 |
| Cache-priced meter | `engine/src/usage.rs` (adds the 5-minute and 1-hour write split and the cache lifetime) | With MVP gate G2 |
| Held-out gate | `testkit/src/bench.rs` (train and test split, noise rule) | Phase 3 |
| Opt-in route upload | `engine/src/share.rs`; server | Phase 3; server Phase 5 |
| Codex image generation | `imageGeneration` items in `codex.rs` | Phase 3 (thumbnails) |
| Day-one shapes and benchmark | `catalog/routes/`, generated pipelines §14 | Phase 5 |
