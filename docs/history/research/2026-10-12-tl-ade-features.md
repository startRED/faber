# TL-ADE feature dossier: what Faber must match and beat

Date: 2026-10-12. Read-only study of `E:\Documentos\ProjetosIA\TL-ADE` (main, last commit 2026-10-10, "estado: alfa").
Sources: five Haiku code reads, one station-by-station inventory, and (section 9) two GPT-6 Luna cross-checks. Paths
below are relative to the TL-ADE repo. Numbers marked [calc] are aggregates of the 69 mission journals in
`.ade/missions`; [inf] marks an inference. Faber may reuse ideas, never code (AGENTS.md).

The pipeline map itself is in [2026-10-12-tl-ade-pipelines.md](2026-10-12-tl-ade-pipelines.md).

## 1. The Software line, station by station

Erick: the Software line was complete; it is the only line the TL-ADE actually ran.

| # | Station | Status in code | Where |
| :- | :--- | :--- | :--- |
| 1 | Request | working | src/intent/compiler.ts |
| 2 | Questions that switch check groups, saved | working; 4 fields (not 3), saved in `.ade/destino.json`; interview up to 5 questions, median 4 | src/intent/destination.ts:28-30, destination-store.ts:32, interview.ts:172, compiler.ts:331 |
| 3 | Affected parts | partial | src/intent/software-profile.ts:177 |
| 4 | Test first (red proof) | working | src/software/red-proof.ts:97 |
| 5 | Code; new library check | partial: package age ≥30 days and ≥100 downloads, checked after the write, not before install | src/software/maker-call.ts:71, src/contain/packages.ts |
| 6 | Quick tests and secret scan | working | src/software/proof-gates.ts:26, src/contain/secrets.ts:23 |
| 7 | Other-vendor review | working; checker never from the writer's family, parks if none left | src/software/checker-family.ts:7-28, review-dispatch.ts:91-124 |
| 8 | Screen check | partial: D1–D7, axe, light and dark; no keyboard or speed check | src/visual/gates.ts:24, src/software/visual-pass.ts:16 (max 6 visual rounds) |
| 9 | Full suite | working, in parallel with the review | src/software/review-with-suite.ts:22 |
| 10 | Launch check | partial: SEO, cookies, cross-user access, journeys; no speed or broken links; solo projects get none; needs undocumented `ADE_LAUNCH_URL` | src/gates/launch.ts:33-34, :96; launch-published.ts:94; launch-access.ts:60 |
| 11 | Receipt | working: certificate over tree, base, contract, proofs and a different-family reviewer; a base move invalidates it | src/software/certificate.ts:25-55, recertify.ts:288-300 |
| ↺ | Back edges | working: failures loop to the maker; rounds 1/2/3 by risk, 2 resumes, recertify at most 3 per attempt | src/software/run-story.ts, rounds.ts:112, engine/ladder.ts:23-128, recertify.ts:21 |

Recipes: "ajuste-rapido" (fast path) vs "alteração", chosen by risk words and a time gate (src/intent/software-recipe.ts:30-90).
No Client website line exists in code.

## 2. Engine

| Feature | Status | Where |
| :--- | :--- | :--- |
| Mission / story / plan model; `.ade/missions/<id>/` with plan.json, stories/, briefing.json, journal.jsonl | working | src/mission/plan-mission.ts:116, engine/control.ts:82 |
| Journal: JSONL, hash chain, fsync, torn-tail repair; state folded from it | working | src/journal/journal.ts:39, :288, :326 |
| Crash resume with lease, engine version stamp, approval and budget re-check | working | src/engine/resume.ts:122-160, lease/lease.ts:299 |
| Park reasons mapped to recovery policies | working | src/engine/recovery.ts:23 |
| Two-phase side effects (intent, then result), reconciled after a crash | working | src/software/deliver.ts:65-86 |
| Worktree per story (`.ade/wt/`) | working | src/software/prepare.ts:102, :263 |
| Commit, then local fast-forward merge into base; the engine is the only merger | working | src/git/gitport.ts:368, :669 |
| Push, PR, remote merge | partial: reconcilers, no executor [inf] | src/step/reconcile.ts:250, reconcile-forge.ts |
| External effects denied unless the approved plan lists them | working | loop.ts:14-19, compiler.ts:631 |
| Per-part budgets: 3–12 model calls per story, mission cap, 8 h wall clock, turns per phase (proof 14, implementation 30, correction 20, review 10) | working | src/engine/plan-load.ts:64-80, budget.ts:13, :144 |
| 2 manual retries per part ("Tentar de novo") | working | src/engine/unit-retry.ts:112 |
| Learning and memory | partial: memory, choice book, taste choice exist; `calibrate()` never applies; no lessons store or scoreboard | src/memory/memory.ts:13, intent/choice-book.ts:27, engine/taste-choice.ts:45, harness.ts:176 |
| Containment: scope, secrets, package age | partial, after the fact; no egress fence; no OS sandbox for claude/agy on Windows | src/contain/contain.ts, docs/security/README.md:250-262 |
| Worker environment: strips ANTHROPIC_, OPENAI_, GEMINI_, GOOGLE_, AWS_, GH_, GITHUB_, NPM_TOKEN, CLAUDE_CODE_OAUTH; sets DO_NOT_TRACK=1, CLAUDE_CODE_DISABLE_AUTO_MEMORY=1 | working | src/runner/spawn.ts:45 |
| Updater | missing: stale engine versions are refused, not upgraded | src/journal/stamp.ts |
| Telemetry | local only [inf] | src/telemetry/cost.ts:76, delivery.ts:36 |
| Unattended night (`ade run --unattended`) | partial, gated; never actually ran (zero `unit_parked` events) | src/cli/run.ts:340 |
| Options `fast_lane`, `visual_gate` | stored, never read | src/mission/options.ts:23-24 |

## 3. Models, routing and quota

| Feature | Status | Where |
| :--- | :--- | :--- |
| Adapters: Claude, Codex, agy (Google) | working | src/adapters/{claude,codex,agy} |
| Model catalog: intelligence, cost per task, time, terminal score; plan tiers and allow-lists | working | src/models/catalog.ts:58-150 |
| Per-role queues (writer, checker, fix, planner), effort per role, max effort never routed | working | src/models/route.ts:21-43, chains.ts:269-355 |
| Score = quality + speed − cost × quota price + preference; measured approval and minutes per model after 5 reviews | working | src/models/chains.ts:198-249 |
| Official quota from each CLI (Claude event, Codex session file, agy /quota); receipts stale after 24 h or renewal; refresh costs a Haiku call; panel refresh every 3 min | working | src/adapters/local/official-quota.ts, engine/quota-receipt.ts:99 |
| Pacing: quota price 1/slack², day brake, 15% operator reserve | working | src/models/chains.ts:99-130, adapters/local/quota.ts:5 |
| Quota hit: pause, keep session, retry after renewal (no round burned); exhausted or logged-out family leaves the queues | working | src/engine/quota.ts:27-120, software/maker-call.ts:313-321 |
| USD cap | informative only (`checkUsdCap` has no caller) | src/engine/budget.ts:79 |
| Lanes: default 1 (`ADE_MAX_LANES`), mission option up to 3; pools of 6 model calls and 2 CPU jobs | partial | src/engine/lanes.ts:77-101, model-slots.ts:42, cpu-slots.ts:11 |
| Prompt cache: session resume only, max 2; stable-prefix design not built | partial | src/engine/ladder.ts:128-136 |
| Economy mode, cross-subscription balancing beyond quota price, shared-account handling | missing | — |

Real numbers [calc]: about 222 delivered stories; median cost US$2.91 list-price equivalent (p90 7.91); median 25 min
per story (p90 79); 117 of 222 passed in one round; 604 approvals vs 386 change requests; cache reads 89–100% of input;
16 of 69 missions had 2–3 stories open at once; measured US$ per 1% of weekly quota: Claude about 2.5, Codex about 0.31,
Google about 2.4 (the code's static guess was 1.3 / 0.7 / 3.0). Dogfood D1: 27 s, US$0.28.

## 4. Skills, tools and catalog

| Feature | Status | Where |
| :--- | :--- | :--- |
| Skill sources pinned by commit, license gate (MIT, Apache-2.0, BSD, ISC, CC0, 0BSD + one exception), sha256 per file re-checked at load; 14 sources, 13 pinned | working | src/skills/catalog.ts:12-24, 198-270, 594-598; `.ade/config.json` |
| SkillGuard scan, quarantine, human review bound to sha256; no script ever loaded | working | src/skills/skillguard.ts:27-44, reviews.ts:51-56, catalog.ts:584 |
| Per-story skill choice: LLM proposes, BM25 fallback, frozen at plan approval; fail-closed if a skill changes | working (LLM hook has no caller, so BM25 decides in practice [inf]) | src/skills/select.ts:10-148, intent/compiler.ts:526-632, mission/approve-mission.ts:113-122 |
| Role-scoped skills (review to checker, tests to proof writer), bodies sanitised | working | src/context/story-skills.ts:17-81 |
| Per-role tool denial via CLI flags (Claude isolation, Codex sandbox per role, agy checker in plan mode) | working | src/adapters/claude/isolation.ts, argv.ts:58-101; codex/argv.ts:38-62; agy/argv.ts:41 |
| Design MCP for UI stories, maker only | partial | src/software/maker-call.ts:174-196 |
| Stack detection | partial: JS/TS only | src/context/discovery.ts:67-70 |
| Missing tool handling | missing: errors and points to `ade doctor`; no search, install or ask | src/cli/doctor.ts:248 |
| Per-role MCP column, ui-skills registry, anti-slop lint plugin | planned, not built | docs/roadmap.md:577-594 |

Catalog cache lives outside the repo (`~/.ade/catalog`). Spec drift: `--safe-mode` was dropped because it hid project
skills; local skills sync although the spec says they do not.

## 5. User experience

Evidence [calc]: 58 real missions; 50 concluded, 6 parked, 5 rejected. Median active time 2.8 h, wall clock 4.2 h
(p90 10 h). Median 3 operator touches per mission. 29 of 58 missions had a part stop at "awaiting operator" (scope
violations, diverged base, unresolved findings, call budget, red proofs); mostly transient.

Journey:
1. Install as a developer: Node 24, `npm ci`, `npm run build:web`, `ade doctor` (real probe, cached 24 h), `ade serve`.
   Claude or Codex must already be logged in. The folder must be a git repo. `ade init` writes a double-click `.bat`.
2. Browser panel on 127.0.0.1. Request box; short fixes skip the interview.
3. Interview up to 5 questions, recommended option first, "follow the recommendations" in one click.
4. Approve the briefing, then the plan (rejecting needs a typed reason). Terminal path: `ade plan` prints a digest to
   paste into `ade approve`; answers go in a JSON file.
5. Run: detached `ade run`; score view, activity log, pause and stop.
6. A stop shows a "your turn" card with the reason and "try again" (2 extra tries per part).
7. Delivery fast-forwards the project branch when the base has not moved; unattended runs leave the merge to a
   morning report. "Close mission"; "next steps" only fills the request box.

Friction, ranked: recovery often needs hand work (about 40 incident lines say relaunch, delete, copy or edit files);
no notifications at all (a stop is visible only with the panel open); `base_diverged` and `base_local_changes` show as
"technical reason"; developer-only install; delivery lands in the user's branch with no review step in the panel; money
shown as "API US$" to subscription users; engineering terms and a music metaphor (score, bar, baton). No undo of a
delivery (git by hand). `ade decide` is documented but absent.

What felt good: recommended option first; one click accepts all defaults; a reason required to reject; durable
journal and orphaned missions relaunched; next steps never send by themselves; common stop reasons in plain Portuguese.

## 6. Lessons and failures (from incident notes, ADRs and dogfood)

1. Quota stops cost the most time: a fixed 50% cap; a renewal during a part voided its receipt; a third vendor out of
   quota left a part waiting days; the demo blocked Claude for an hour at 0–3% quota; Flash used 284M tokens in 148
   calls with 91% repeated prompts and 58% of reviews asking for changes.
2. Runs die silently or leave debris: a child signal killed the whole run; a power loss zeroed a worktree index;
   dogfood D1 needed six retries from a Windows fingerprint bug and a manual delete of a stale worktree.
3. Codex on Windows writes blind: its sandbox blocks child processes, so it cannot run the tests (open decision).
4. Long paid probes: 30-min maker timeout, orphan probes overwriting results, a "real round" delivered as not run.
5. Triage timeout of 8 s lost cold calls (6–10 s); the fix moved success from 19/45 to 43/45.
6. Judge drift: a 0–10 scale anchored near 7.5 scored an uglier thumbnail higher (6.28 vs 7.16).
7. A commissioned review (27/09) called the software cycle the wrong template for Studio, and criticised 10 MB base64
   attachments, 300-word handoffs, the two-vote triage and the missing project kit.

Reversed decisions: the US$300 cap became informative (quota decides); `--safe-mode` dropped; no-build JS became
TypeScript plus a panel build; sectors became capabilities and recipes; two-vote triage became one call; concurrency
went from excluded to 6 calls; one detector was rewritten five times and reverted.

## 7. Planned beyond software

Code exists for Studio (src/studio, 12 modules), Research (src/pesquisa), Teaching (src/ensino) and thumbnail-3d
(src/capabilities/thumbnail-3d.ts); no benchmark result recorded. Plan: docs/specs/pipelines-por-capacidade.md
(milestones M0–M12+), versions v2–v5 in docs/plans/versao-definitiva.md (v1 closed at about US$360). TL-ADE-dados holds
YouTube niche data and about 5,300 thumbnails; TL-ADE-negocio holds business research (services first, license later).

## 8. Ideas worth keeping (Faber must match these)

- Gold answers before code and red-before-green proof.
- The writer never reviews; the reviewer comes from another family; park rather than self-review.
- Hash-chained, write-ahead journal with a crash matrix; two-phase effects; no effect repeats.
- Official quota receipts only; never infer a percentage from tokens or dollars; a quota hit burns no round.
- Certificate bound to tree, base, contract and proofs; recertify when the base moves.
- Pinned, licensed, hashed, scanned skills, frozen at approval and scoped per role; per-role tool denial by CLI flags.
- Worker environment stripped of vendor tokens.
- Four control levels learned per area; lint messages that teach the fix; short outputs as a contract.
- The incident ("cicatrizes") file: symptom, cause, fix, and "if it returns, the fix did not hold".
- Judging against real niche references; timeline export before render; corrections by text; Teaching never reveals
  the answer before two real attempts.

## 9. GPT-6 Luna cross-check

Two independent GPT-6 Luna reads (high effort) on a snapshot copy: a feature inventory, and user experience plus quota.
They confirm sections 1 to 5 and add the following.

**Additions**
- Containment also catches weakened tests and uses canaries to detect a workspace escape (src/contain/contain.ts:237).
- Interview questions are generated per request and priced (`intent_priced_questions`); triage routes requests to
  software, research, teaching or other flows (src/intent/triage.ts:48). An early preview shows while the request is
  read, with the current action, elapsed seconds and a duration hint (packages/web/src/intake/Thinking.tsx).
- Attachments: up to 10 files (PNG, JPEG, GIF, WebP, PDF, TXT, MD), 10 MB each (RequestBox.tsx:71-80).
- Small or quick-route requests are approved and launched automatically when the quick approval passes
  (src/panel/quick-intake.ts:44-63). A follow-up request is blocked while a mission runs (Intake.tsx:207).
- Autonomy policy: safe, controlled and restricted work; deployment, secrets, destructive local changes and the first
  use of a skill need the operator (docs/operations/autonomy-and-permissions.md:55-92).
- Closing report: ready, failed, partial and unstarted parts, cost, calls and duration (MissionClosing.tsx).
  `ade status` exits with code 3 when a stop needs attention.
- Models page: subscription tiers, effort per role, blocked models, ranked queues with the reason for each rank; manual
  Google quota entry (packages/web/src/Models.tsx). Codex is penalised as a writer, as a preference, not a cap
  (src/models/catalog.ts:162-168).
- Skills page: search, inspect, enable or disable sources per project; at most 4 skills per part
  (src/skills/select.ts; ADR 0039). User-local Claude skills are excluded from missions; the agy worker runs in an
  isolated home with no user rules, skills or MCP (src/adapters/agy/home.ts). No per-task MCP loading [inf].
- Discard moves work into retained refs instead of deleting it (autonomy-and-permissions.md:271).
- Two delivery paths coexist: the panel's interview, briefing and plan flow, and an older CLI runbook where the operator
  writes plan.json by hand and merges manually (docs/operations/usar-em-outro-projeto.md:3-18).

**More real runs** [journals]: a three-part mission in 83.8 min, about US$10.19, no intervention; another in 404 min,
about US$26.05; one interrupted after 598 min, 50 calls and about US$34.90, with two interventions. Long runs and wide
cost variance.

**Friction added:** a failed request submission reloads the intake and loses the preview (Intake.tsx:85-99).

**Conflicts between readers** (resolve before relying on them)
- USD cap: Luna reads `src/engine/budget.ts:157` as enforcing call and USD caps; the Haiku read found `checkUsdCap` with
  no caller and ADR 0032 making dollars informative. Call budgets are enforced either way.
- `ade decide` (retry, skip, takeover, discard, pick): documented, but the Haiku read did not find it in the CLI
  dispatcher (src/cli/index.ts:65). The panel offers only "try again".
