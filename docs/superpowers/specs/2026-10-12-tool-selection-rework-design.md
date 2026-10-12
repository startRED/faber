# Kits: how Faber picks tools for every agent and every step

Date: 2026-10-12. Status: design proposal. Nothing here is implemented, and this document authorizes no build.
It reworks how tools reach agents, and the plan parts that this touches. It does not edit `docs/roadmap.md`; section 13
proposes the exact text changes. Companion spec: [generated pipelines](2026-10-12-generated-pipelines-design.md)
compiles each request into steps, checks and proofs; this spec gives each of those steps its loadout.

**Authorship.** Every choice in this document is an AI proposal (not reviewed), tagged `TS-n`, unless it is marked as
Erick's. Erick decides; the AI proposes. Erick's binding directions for this design, given on 2026-10-12 through the
coordinating session:

| Tag | Erick's direction |
| :-- | :--- |
| E1 | No hand-written kits, not even for the self-development MVP. Every kit is assembled on the fly per task and step |
| E2 | Selection is fast (seconds) and cheap. Latency and quota cost are explicit targets with numbers, and measured |
| E3 | The long tail is the main case. A never-seen task, or a tool that is not installed yet, must still get a good kit |
| E4 | Missing tool: ask while the user is at the screen; later, never block; in autonomous mode, normally install |
| E5 | At the end, list what was installed and decided without the user, each with one-click keep/remove or keep/change |
| E6 | The automatic install path must be made safe, not refused |
| E7 | Answer to Q1: tools Faber installs live in Faber's own store, apart from the CLI configuration, with a per-tool "also use outside Faber" switch |
| E8 | Answer to Q2: background tool tests are approved. No pile-up; light tests on scaled-down prototypes (a sample, a mini app, one screen, a short clip), never a full rebuild; they may run at any time, spread out; keep the shared-account pause and "never the daily grant" unless there is a strong reason |
| E17 | Tools follow the purpose of the request, not only its technical type: "build an app" and "build an app to sell" get different kits. Purpose, audience, deliverable and stage are first-class facets. An AI should choose the tools; the most accurate method is to be proven, kept per task family. Cover every kind of work with a task-family taxonomy for cold start (priors and facets, never a hand-written kit; E3 binds) |
| E18 | Overlapping tools need not be either/or: they may run together or in separate passes (a base pass, then polish passes with their own focus), each polish pass kept only if it beats the version before it. Erick names ponytail, caveman, Taste-Skill and Impeccable as good tools for cold-start priors |

Erick's answers E10 to E14 in the companion spec bind this spec too: the self-development MVP carries the complete
software bar (section 17.1); result quality is measured against the plain CLIs on probes, and against the real TL-ADE
Software line only in a small head-to-head, all inside one US$25 envelope (companion spec, sections 14.4 and 14.8); the
Probe lane stays under its own per-user caps (section 15.3). Under E12 and E13, Kits is Faber's answer to fixed tools,
code written from memory in an unfamiliar stack, a missing tool-gap loop and JavaScript-only stack detection, and it
matches the TL-ADE's pinned, hashed, role-scoped skills (section 8).

Feature names follow AGENTS.md: no vendor names. The names used here are **Kit**, **Loadout**, **Tool card**,
**Field brief**, **Kit receipt**, **Tool store**, **Probe lane**, **Picker** and **Pass**.

## 1. The failure this fixes

The roadmap promises that "each step loads only the tools it needs", but no part of the plan decides which tools those
are. Today a session gets whatever the CLI finds in the user's setup. That has four costs:

1. **Context.** Every installed skill puts its description in every session, and every MCP server its tool names.
2. **Conflicts.** Two style skills, or a process skill that fights Faber's own orchestration, load together.
3. **Blind spots.** A task nobody foresaw runs with no tool for it, or with the wrong one.
4. **No learning.** Faber keeps a scoreboard for models, none for tools.

E1 rules out the easy patch, a hand-written kit per project. A hand-written kit makes Faber good only at tasks someone
foresaw. The mechanism below treats every task as possibly new.

## 2. What we know (verified facts and open ones)

Facts gathered on 2026-10-12 by research agents from official docs, local `--help` output (Claude Code 2.1.296,
Codex 0.162.1) and the repo's research notes. "UNVERIFIED" means a probe must confirm it before code relies on it.

### 2.1 Per-session levers in Claude Code 2.1.296

| Lever | Status | Source |
| :--- | :--- | :--- |
| `--mcp-config <file>` adds servers; `--strict-mcp-config` loads only those | VERIFIED | local `--help`; code.claude.com/docs/en/cli-reference |
| `--allowedTools` / `--disallowedTools`; a bare tool name removes it from context; `Skill(name)` matches one skill | VERIFIED | cli-reference; code.claude.com/docs/en/permissions |
| `mcp__<server>` deny form on the command line (verified only in subagent `disallowedTools`) | UNVERIFIED | code.claude.com/docs/en/sub-agents |
| `--settings <file or JSON>` overrides the same keys for the session; holds `permissions`, `env`, `skillOverrides` | VERIFIED | code.claude.com/docs/en/settings |
| `skillOverrides` per skill: `on`, `name-only`, `user-invocable-only`, `off` | VERIFIED | code.claude.com/docs/en/skills |
| `enabledPlugins` inside `--settings` (key exists; value format) | UNVERIFIED | settings reference |
| `--plugin-dir <path>` loads a plugin for this session only | VERIFIED | cli-reference |
| `--add-dir`: skills still load from it (exact folder layout) | VERIFIED / layout UNVERIFIED | cli-reference |
| `--append-system-prompt-file` appends text to the system prompt | VERIFIED | cli-reference |
| `--safe-mode` turns off CLAUDE.md, skills, plugins, hooks and MCP, keeps sign-in; interplay with `--mcp-config` | VERIFIED / interplay UNVERIFIED | cli-reference |
| Skill listing is capped at 1% of the context window; each description at 1,536 chars; over budget, the least-used skills lose their descriptions | VERIFIED | code.claude.com/docs/en/skills |
| MCP tool schemas are deferred by default (tool search); only names and server instructions load | VERIFIED | code.claude.com/docs/en/mcp |
| Resume does not restore `--mcp-config`, `--settings`, `--plugin-dir` or `--add-dir` | VERIFIED (repo research) | docs/research/2026-10-11/r4-protocols-sessions.md |
| Changing tools invalidates the whole prompt cache; invoking a skill keeps it | VERIFIED (repo research) | r3-cache-switching.md |

### 2.2 Per-session levers in Codex 0.162.1

| Lever | Status | Source |
| :--- | :--- | :--- |
| `-c key=value` dotted TOML overrides (syntax) | VERIFIED | local `codex --help` |
| `mcp_servers.<name>.enabled`, `enabled_tools`, `disabled_tools` keys | VERIFIED (schema) | codex-rs/core/config.schema.json |
| A profile cannot change `mcp_servers` | VERIFIED (schema) | config.schema.json |
| Skills found in `.agents/skills` from the working folder up to the repo root, `~/.agents/skills`, `/etc/codex/skills`, bundled | VERIFIED | learn.chatgpt.com/docs/build-skills |
| Skill listing budget: 2% of the context window, or 8,000 chars | VERIFIED | same |
| Disable a skill with `[[skills.config]] path=… enabled=false` (needs a restart) | VERIFIED; per session UNVERIFIED | same; schema `SkillConfig` |
| app-server `thread/start` takes `developerInstructions` and an open `config` object | VERIFIED (generated schema) | local `generate-json-schema` |
| MCP or skill overrides passed through that `config` object take effect | UNVERIFIED | — |
| `turn/start` and `thread/settings/update` take `disabledPluginIds` | VERIFIED (schema) | app-server README |
| `codex exec --ignore-user-config` (sign-in still read from `CODEX_HOME`); same for app-server | VERIFIED for exec / UNVERIFIED for app-server | local `codex exec --help` |
| MCP tools use tool search by default since 0.143.0 | VERIFIED (repo research) | r4-changelogs-issues.md |

### 2.3 What the research says about choosing tools

Numbers come from abstracts and search snippets; nobody re-ran them. Treat them as direction, not as targets.

- **Curated skills help, but not always.** SkillsBench (arXiv 2602.12670): curated skills raised pass rate by 16.2
  points on average (+4.5 in software engineering, +51.9 in healthcare); 16 of 84 tasks got worse; skills a model wrote
  for itself from its own knowledge did not help on average.
- **Models pick tools by wording, not by function.** "Tool Preferences in Agentic LLMs are Unreliable" (arXiv
  2505.18135): edited descriptions drew more than 10x the use. A model choosing among many descriptions can be gamed.
- **Fewer tools in context work better.** RAG-MCP (2505.03275) raised selection accuracy from 13.6% to 43.1% and cut
  prompt tokens by more than half by retrieving tools first. "Less is More" (2411.15399) found the same on small models.
- **Describe tools by example requests.** Tool2Vec (2409.02141) indexed tools by example queries instead of their own
  descriptions: up to +27 Recall@K. COLT (2405.16089) picks sets that complement each other, not near-duplicates.
- **Learned routing works with feedback.** RouteLLM (2406.18665) and bandit routers learn from outcomes; Faber's own
  measurement agrees: a small model classifying blind plus a fixed table got 81 of 88 picks right; letting a model
  choose among anonymous profiles got 76 of 88, spent more and was slower (roadmap, "Auto: a blind router").
- **Reusable workflows from past runs help.** Agent Workflow Memory (2409.07429): +51% relative on WebArena.
- **Retrieval alone is weak; a judging step on a shortlist helps.** ToolRet (2503.01763, 43k tools): average nDCG@10
  of BM25 22.3, the best dense embedder 33.8, the best cross-encoder reranker 35.5; with task instructions in the query
  36.5, 42.7 and 47.5. Rich, structured queries help every retriever (BM25 +14). RAG-MCP's 43.1% against 13.6% is a
  shortlist then a model pick.
- **Shortlist size matters.** "How Many Tools Should an LLM Agent See?" (2605.24660, registries of 20 to 3,251
  tools): adaptive shortlists covered 90.3% of needed tools against 90.8% for a fixed 50; with Claude Sonnet 4.6,
  adaptive lists reached 93.1% against 87.1% for a fixed 5.
- **Names, wording and order sway models.** MPMA (2505.11154): a manipulated tool name or description won up to 100%
  of selections; a subtler variant 22% to 92% by model. BiasBusters (2510.00307): models fixate on one provider or on
  earlier-listed tools; filtering to a relevant subset and then sampling uniformly reduces this. No study measured
  selection with brand names hidden (UNVERIFIED either way).
- **No head-to-head exists.** No paper found compares, on the same tasks, a model picking from the full list, a model
  labelling needs for code to pick, and a shortlist picked by a model. Faber's offline test (section 6.3) decides.

The design follows from these: **a model describes the need and may choose only among a short, shuffled list of tool
cards that Faber wrote, with brand names hidden; code builds the list and validates the choice; outcomes, not
descriptions, rank tools over time.** That is the Auto router's lesson applied to tools, with a judging step where
combinations matter.

## 3. The mechanism in one page

```text
request or plan step
  │  labels (verb, needs, risk, freshness)   ← small model at request time, the lead at plan time; never tool names
  │  facets (purpose, audience, deliverable, stage, region)  ← same call; saved per project
  │  fingerprint (stack facts from files)    ← deterministic rules, no model
  ▼
Picker (section 6.2): code shortlists cards by facets and examples → a small model picks the combination and the
passes from blind cards → code validates (jobs, conflicts, budget, risk, pins); fallback: the code pick
  ▼
Loadout assembler (engine code, milliseconds)
  ├─ inventory: user's own tools (as the CLIs report them), Tool store, project cards, engine checks, field briefs
  ├─ tool cards: what each tool serves, its job slot, stage, token cost, conflicts, examples, evidence
  ├─ route table (route.rs, unchanged): vendor, model, effort
  └─ picks: one Loadout = vendor + model + effort + role + kit + proofs + review lenses + budget
  ▼
gaps? (a need no card covers well)
  ├─ Tool store / catalog search (local, <100 ms) → install within bounds (section 12)
  ├─ search beyond the catalog (small research worker, parallel to planning)
  ├─ field brief from primary docs (cited, project-local)
  └─ floor: the unmodified CLI with its own web search
  ▼
session starts with exactly that kit (enforced by flags, checked against what the CLI reports, inline fallback)
  ▼
Kit receipt: what was picked and why → outcome (proof first try, review findings, user kept or redid)
  ▼
evidence per (verb, need, stack, vendor, model) cell → ranks the next pick; Probe lane tests tools on small prototypes
```

Three rules hold everywhere:

1. **Needs, not names (TS-1, revised for E17).** No model names a tool from memory. Models label needs; code builds a
   shortlist; a model may then choose only among those cards, shown under opaque ids, with Faber's own capability
   text, no brand or author names, in shuffled order; code validates. This keeps vendor and position bias down and
   makes description stuffing useless, because the model never sees a tool author's words.
2. **Tools change only at session boundaries (TS-2).** A session keeps its kit from start to end (roadmap rule 3).
   A new need starts a new session with a written hand-off, or waits until the cache is lost anyway (model move-up,
   resume after the cache lifetime, compaction). Re-kitting is free then.
3. **Deterministic outside, judgement inside (TS-3).** Anything a program can check (lint, type check, token use,
   loudness, license, known vulnerabilities, design-token use) runs in the engine as a proof, not as a skill in a
   model's context. Context is for knowledge a model must apply while it works.

## 4. Data shapes

### 4.1 Labels

Produced at request time by the Auto classifier (one call that already runs) and at plan time by the lead, as part of
each plan node. Labels never contain tool names.

```json
{
  "verb": "build",
  "kind": "code",
  "difficulty": "medium",
  "needs": ["framework:tauri", "framework:solidjs", "ui:component!", "verify:screen"],
  "risk": "low",
  "fresh": "stable"
}
```

- `verb` is closed, 10 values (TS-4): `research`, `plan`, `design`, `build`, `fix`, `verify`, `review`, `write`,
  `produce` (images, audio, video, 3D, other assets), `operate` (drive apps or the PC).
- `needs` are `family:value`. Families are closed, 18 values: `lang`, `framework`, `platform`, `service`, `data`,
  `artifact`, `media`, `domain`, `op`, `verify`, `style`, `app`, `ui`, plus `growth` (positioning, landing, SEO,
  analytics, email, launch), `commerce` (pricing, payments, subscriptions, tax and invoices, store listings), `legal`
  (terms, privacy, consumer, licenses, compliance), `security` (auth, hardening, secrets, supply chain) and `infra`
  (deploy, CI, cloud, containers, monitoring). Values are open text, normalized by an alias
  table that ships as data (`tmodloader`, `tModLoader`, `tml` → `tmodloader`). A trailing `!` marks a need the step cannot do
  without. At most 8 needs per step.
- `risk` (`low`, `medium`, `high`) limits which tools may load (section 12).
- `fresh` (`stable`, `recent`, `unknown`) says whether the model's own knowledge is likely stale: a library version newer
  than the model, a niche platform. It decides whether a field brief is worth writing (section 10).
- `kind` and `difficulty` are exactly today's route.rs fields. The router stays blind: it still sees only the request.
- Facets (section 4.5) travel with the labels: `purpose`, `audience`, `deliverable`, `stage`, `region`.

### 4.2 Fingerprint

Stack facts come from files, not from a model (TS-5). Rules are data in the manifest: a file glob, an optional content
pattern, and the needs it implies. Examples: `Cargo.toml` → `lang:rust`; `src-tauri/tauri.conf.json` →
`framework:tauri`; `package.json` containing `"solid-js"` → `framework:solidjs`; a `.csproj` referencing tModLoader →
`platform:tmodloader`; `mix.exs` → `lang:elixir`; `wrangler.toml` → `service:cloudflare`; attached `.wav` or `.mp3` →
`media:audio`. About 40 rules at first. A reviewer's fingerprint comes from the paths in the diff.

### 4.3 Tool card

One card per tool version. Catalog cards ship in the signed manifest; other cards are written locally (section 9).

```json
{
  "id": "impeccable@<version>",
  "form": "skill",
  "source": {"origin": "user|store|project|engine|brief", "url": "https://github.com/pbakaus/impeccable",
             "version": "<pinned>", "sha256": "<hash>", "license": "Apache-2.0"},
  "vetting": "vetted|experimental|user-own|not-vetted|faber-written",
  "runs_code": false,
  "sends_data": "none|public-identifiers|project-content",
  "clis": {"claude": "native|inline", "codex": "native|inline"},
  "verbs": ["design", "build", "review"],
  "serves": ["ui:visual-design", "ui:critique", "style:*"],
  "job": "ui.craft",
  "families": ["design", "frontend"],
  "purposes": ["*"],
  "pass_focus": ["base", "ux", "ui", "anti-slop"],
  "blind_text": "Critiques and polishes interface craft; finds generic generated-look patterns; 59 detector rules",
  "exclusive": true,
  "conflicts": ["frontend-design"],
  "stage": "during",
  "cost": {"listing_tokens": 110, "core_tokens": 2400, "mcp_tools": 0, "measured": true},
  "examples": ["polish spacing on the settings screen", "audit this page for generic AI look", "..."],
  "requires": {"os": ["windows", "macos", "linux"], "apps": [], "gpu_gb": 0},
  "prior_lift_pp": 4,
  "card": {"writer": "scout|auto|maintainer", "content_hash": "<hash>", "written": "2026-10-12"}
}
```

- `form`: `skill`, `mcp`, `plugin`, `check` (a program the engine runs), `cli-feature` (web search, image generation,
  computer use…), `brief` (a field brief), `reference` (a project document: design system, product map, AGENTS.md).
- `job` is a slot. An `exclusive` job takes one tool (one visual style, one test runner). A non-exclusive job may take
  several (several reference documents). Job `process.*` (planning, delegation, brainstorming with the user, "always use
  skills") belongs to Faber itself (TS-6): process skills never load into workers, because they fight Faber's own plan,
  proof and review flow.
- `stage`: `before` (read first: references, product map, field briefs), `during` (loaded while working), `after`
  (checks and review lenses). Section 7 uses it for order.
- `examples` are 5 to 10 example requests. Matching uses them, not the tool's own description (Tool2Vec, section 2.3).
  Requests that later used the tool well are added locally, so the index learns from use.
- `families` and `purposes` are facets for the shortlist: a growth or commerce card lists `sell`, `client` and
  similar purposes, so a personal-use request never gets it unless a need asks for it. `pass_focus` says which passes
  the tool serves (section 7.1). `blind_text` is the card writer's capability summary with no brand or author name; it
  is the only text the picker model sees.
- `prior_lift_pp`: the cold-start guess of how many percentage points the tool adds to "accepted on the first try".
  Section 15 replaces it with evidence.

### 4.4 Loadout and Kit receipt

```json
{
  "task": "T3", "attempt": 1, "role": "writer",
  "vendor": "codex", "model": "gpt-6.1-sol", "effort": "medium",
  "kit": [
    {"id": "design-system@project", "stage": "before", "delivery": "inline", "why": "owns style:* in this project"},
    {"id": "impeccable@<v>", "stage": "during", "delivery": "inline-core", "why": "ui:component, prior +4 pp, 9 uses"}
  ],
  "proofs": ["lint", "typecheck", "build", "fmt", "clippy", "clippy-test-hooks", "cargo-t-all"],
  "review_lenses": ["ui.coherence", "a11y.basic"],
  "gaps": [],
  "budget": {"kit_tokens": 3120, "cap": 4000},
  "receipt": {
    "labels_from": "lead-plan", "table": "kits-2026-10-20", "inventory": "sha256:…",
    "dropped": [{"id": "taste-skill", "reason": "job ui.style owned by the project design system"}],
    "timing_ms": {"assemble": 7, "catalog_search": 0},
    "cost_usd": {"labels": 0.0, "cards": 0.0, "gaps": 0.0},
    "attested": true,
    "outcome": {"proof_first_try": null, "repairs": null, "review_blocking": null,
                "kit_used": [], "user": "unknown", "tokens": null}
  }
}
```

The receipt is written before dispatch and completed after proof, review and the user's reaction. In the
self-development MVP it lives at `.faber/missions/<id>/kits/<task>-<attempt>.json`, next to the other mission records.

### 4.5 Purpose, audience, deliverable and stage (Erick's direction E17)

Facets come from the same labelling call, are saved per project (like the TL-ADE's saved answers), and are asked only
when unclear and when the answer changes the kit or the pieces (it counts against the 3-question cap). Away, Faber uses
the saved project facts, else "personal or unknown", and the end report lists "what selling would add".

| Facet | Values (open text, normalized by an alias table) |
| :--- | :--- |
| `purpose` | sell, personal use, client work, internal team, open source, learning, research, portfolio, prototype, event or jam |
| `audience` | self, family and friends, a team, a client, the public, businesses, developers, players, students, children, regulated professions |
| `deliverable` | web, mobile or desktop app, SaaS, site, game or mod, library or CLI, API, extension, bot, automation, data or model, document or deck, image, video, audio, 3D, brand, campaign, course, report, hardware build |
| `stage` | idea, validate, prototype, build, launch, grow, maintain, migrate or rescue |
| `region` | where it runs or sells (from the request and the user's locale): drives legal, tax, payments and language |

Purpose adds need families as priors (data in the manifest, TS-22). They are needs, never tools:

| Purpose | Needs it adds | Pieces and checks it adds (generated pipelines spec) |
| :--- | :--- | :--- |
| sell | `growth:positioning`, `growth:landing`, `growth:seo`, `growth:analytics`, `growth:email`, `growth:launch`, `commerce:pricing`, `commerce:payments`, `commerce:tax-invoice`, `legal:terms-privacy`, `legal:consumer`, `infra:monitoring`, `security:hardening` | landing and pricing pages, legal pages, a store listing; money, public, commercial and other-people's-data risk groups |
| personal use | none of the above | no launch, money or growth pieces; core quality only (tests, review, the user's own taste) |
| client work | `artifact:handover`, `artifact:manual`, `infra:handover` | handover package; client-owned accounts; the client's brand |
| internal team | `security:auth`, `security:audit-log`, `infra:deploy` | roles and sign-in; internal docs |
| open source | `legal:oss-license`, `artifact:contributing`, `artifact:changelog`, `infra:ci` | license, contribution guide, release notes |
| learning | `domain:teaching` | explanations and exercises; smaller steps |
| prototype | none | polish passes off; speed over finish |

**Example.** "A habit tracker for me" (like Erick's personal Skin Sniper): stack needs from the fingerprint, minimal
code, tests and review; nothing about pricing, SEO, analytics or legal; launch checks off. "A habit tracker to sell":
the same core, plus landing, pricing, subscriptions with every billing state tested, terms and privacy for the user's
region, SEO, privacy-respecting analytics, onboarding email and a store listing, each picked per step from the cards
that serve those needs, with the money, public and commercial risk groups on.

### 4.6 Task families: the cold-start taxonomy

Families are facets and priors for the picker and the gold set (section 6.3). Each family lists typical needs; no
family is a kit (E1, E3).

| Family | Covers |
| :--- | :--- |
| Software: web | frontend, backend and APIs, full stack, SaaS |
| Software: platforms | mobile, desktop, CLI and libraries, browser extensions, bots, embedded and IoT |
| Games | game code, mods, game design, playtesting |
| Design | product and UX design, UI design, design systems, prototyping |
| Brand | identity, logo, voice |
| Assets for software | sprites and tiles, icons, illustrations, UI imagery, sound effects and music, 3D models, store screenshots |
| Data and ML | analysis, spreadsheets, dashboards, pipelines, training and evaluation, LLM apps and agents |
| Infra and DevOps | CI and CD, cloud, containers, infrastructure as code, monitoring, cost |
| Security | secure review, auth, secrets, supply chain, testing one's own app |
| Quality | tests, accessibility, performance, localization |
| Docs | technical and API docs, tutorials, internal knowledge |
| Media | video, motion, audio and podcast, voice, music, photo, image generation, 3D and rendering |
| Writing | copy, scripts, articles, social posts, translation |
| Marketing and growth | positioning, landing pages, SEO, ads, email, social, analytics, launch, community |
| Commerce | pricing, payments and billing, tax and invoices, store listings, e-commerce, support |
| Legal and compliance | terms and privacy, data protection, licenses, accessibility law, contracts |
| Research | market, technical, academic, competitive, fact checking |
| Learning | tutoring, courses, exercises |
| Office | documents, slides, spreadsheets, PDFs, forms |
| PC and operations | files, apps, browser tasks, upkeep |
| Personal life | purchases, travel, events |
| Making | 3D printing, electronics, CAD |
| Product and project | specs, roadmaps, issue triage, release notes |

Cold-start coverage target: before Phase 3, every family has at least 2 vetted cards or engine checks, or a recorded
reason to rely on field briefs; and at least 8 gold tasks in the picker test.

## 5. Who decides, and when

| Moment | Who | What happens | Extra model calls |
| :--- | :--- | :--- | :--- |
| First message of a conversation | Auto classifier (small model, blind) | Adds `verb` and `needs` to today's `kind`/`difficulty`; the assembler builds the request kit for the lead or Chat session | none (same call, about 60 more output tokens) |
| Plan | The lead | Each plan node carries labels; the assembler builds a draft loadout per node; gaps start resolving now, in parallel | none (labels are part of the plan output, about 40 tokens per node) |
| Plan, after labels | Picker (section 6.2), where the family's winner is a model pick | One small-model call for the whole plan picks every node's combination and passes from blind shortlists; code validates | one call, about 1,500 input tokens per node, batched |
| Start card | The user, if present | The card shows the plan, time, quota and any tool gaps with their choice (section 11) | none |
| Step dispatch | Engine | Re-runs the assembler (milliseconds): the inventory may have grown since planning | none |
| During a step | Worker | Cannot change its kit. If it needs something, it says so in its result packet (`needs: ["media:audio-loudness"]`); the engine re-labels and the next attempt or step gets a new session | none |
| Cache already lost | Engine | On model move-up, resume after the cache lifetime or compaction, the kit is recomputed for free | none |

The lead itself gets a small kit: the Faber toolbox, memory search, catalog search, the product map, plus the request
kit when it does the work itself ("solve it right away"). It does not carry every craft skill the user owns (TS-7).

## 6. The assembler

A pure function in the engine, no I/O inside, unit-testable without processes (TS-8):

`loadout(labels, fingerprint, inventory, cards, evidence, route_inputs, role, presence) -> Loadout`

1. **Needs.** Union of label needs and fingerprint needs. Fingerprint needs count half unless the label repeats them.
2. **Candidates.** Cards whose `serves` matches a need exactly, by alias, or by family wildcard (`lang:*`); whose
   `verbs` contain the step verb; whose `requires` hold on this machine; whose `vetting` and `runs_code` the role and
   risk allow (section 12). Ties in lexical match use BM25 over `examples`. No embeddings: the roadmap rules out a
   local model.
3. **Utility.** For each candidate: `u = Σ crit(need) × lift(card, cell) − tokens_resident / 1000`.
   - `crit` is 1.0 for a `!` need, 0.6 otherwise, halved for a wildcard match.
   - `lift` is in percentage points of first-try acceptance. It comes from the most specific evidence cell with at
     least 5 observations, backing off from (verb, need, stack, vendor, model) to (verb, need, vendor) to (need) to the
     card's prior (section 15).
   - The token term says: a tool must earn about 1 point of lift per 1,000 resident tokens. Example: +5 pp for 2,000
     tokens stays (`u = 3`); +1 pp for 3,000 tokens goes (`u = −2`). The constant is a starting value for experiments.
4. **Jobs and conflicts.** One card per exclusive job; a card in another card's `conflicts` list cannot join it.
   A project card that serves a job outranks third-party cards for it (a project with its own design system owns
   `style:*`; a third-party style skill is not loaded).
5. **Pack.** Greedy by `u` per token until the role budget is reached or `u ≤ 0`. Defaults (TS-9): worker 4,000
   resident tokens (2% of a 200K window), lead 6,000, reviewer 3,000, research worker 1,000. At most 4 `during` skills
   and 2 MCP servers besides the Faber toolbox.
6. **Stage split.** `before` cards become the brief's first reads; `during` cards load into the session; `after` cards
   become proofs (if `form` is `check`) or review lenses (otherwise).
7. **Gaps.** Any need whose best card has `u ≤ 0`, or no card, is a gap (section 10).
8. **User overrides win.** A tool the user pinned "always" for this project loads even over budget (with a note in the
   receipt); one pinned "never" never loads.
9. **Stickiness.** If a live session of the same role, vendor and model has a warm cache and its kit covers this step
   (same `during` items, or at least 80% overlap and no `!` need missing), reuse it instead of starting a new session.

### 6.1 Joint choice with model, effort, vendor and role

One function picks the whole worker loadout (TS-10). It does not replace the Auto table; it wraps it.

- **Role** comes from the verb and risk: `research` and `review` are read-only roles, `build` and `fix` are writer
  roles, and so on. The role sets the permission envelope (code, never a model); the kit fills the envelope.
- **Model and effort** come from route.rs exactly as today, so the measured 81 of 88 stays intact at cold start.
- **Vendor coverage.** Some tools exist for one CLI only (a CLI-native image generator, an MCP server that only one
  CLI runs well). The assembler runs steps 2 to 7 for each vendor among route.rs's candidates that clear the bar. A
  candidate that leaves a `!` need uncovered while another covers it is dropped. Among the rest, route.rs's cheapest-
  that-clears-the-bar rule decides as before.
- **Cross-vendor review stays a hard rule:** a reviewer's vendor differs from the writer's.
- **Kit lift may right-size later.** When paired experiments show that a cheaper model with a kit matches a stronger
  model without it (posterior probability at least 0.9), that kit may lower the difficulty bar one step for that cell.
  Not at cold start; only from evidence (Phase 3).

### 6.2 The picker (E17)

The chosen method (TS-23) combines the code pick above with a judging step where combinations matter:

1. **Query from labels.** The labels and facets, not the raw request, form the query (ToolRet: structured queries raise
   every retriever).
2. **Shortlist by code.** Steps 2 and 3 above, plus facet filters (`families`, `purposes`, `pass_focus`). Adaptive
   size: up to 3 cards per job and need, at most 24 in all (the shortlist study above).
3. **Blind model pick.** One small model (the Auto router's cheap rung) sees the step, the facets, and each shortlisted
   card as an opaque id with its `blind_text`, examples, evidence (lift, observations, kept passes), conflicts and
   token cost, in shuffled order. It returns the combination, the pass plan (section 7.1) and one reason per pick. It
   never sees brand names, authors or the tool's own description.
4. **Code validates.** Steps 4 to 8 above: exclusive jobs, same-session conflicts, budget, requirements, risk and user
   pins. An invalid or late answer (over 3 s) falls back to the code pick; the receipt records both.
5. **Per family.** The manifest keeps the winning method per task family from the test below (`picker: code` or
   `picker: blind-model`). Where the code pick ties, it stays: no call, no latency.

Why: RAG-MCP's shortlist-then-pick tripled accuracy over a full list; retrieval alone stays weak (ToolRet); wording,
names and order sway models (MPMA, BiasBusters), so the model sees only Faber's blind cards in shuffled order; and
Faber's router measured labels plus a table above free model choice (81 of 88 against 76 of 88), so the code pick
remains the fallback and the default where evidence is dense.

### 6.3 Picker accuracy test (offline, decides per family)

- **Gold set.** About 230 tasks: the 23 task families × 10, each in paired facet variants (sell against personal use,
  prototype against launch), including the 60 Faber steps and the 40 benchmark requests.
- **Inventories** of 40, 150, 500 and 1,500 cards built from real cards plus distractors, including stuffed and
  brand-heavy decoys.
- **Gold kits.** Two strong models from different vendors each mark, from the family's relevant cards, the required,
  useful and harmful tools and the pass order; agreement becomes gold, disagreement is reviewed by the maintainer on a
  sample. Later, outcomes from probes and receipts refine the gold (gold v2).
- **Arms.** (a) a model picks from the full list (40, 150 and 500 only); (b) labels and code (section 6); (c) a
  shortlist and a model pick with brand names shown; (d) Faber's picker (section 6.2).
- **Metrics.** Recall of required tools; harmful or conflicting picks; set F1; pass-order accuracy; purpose
  sensitivity (paired variants differ where gold differs); vendor bias (same-vendor share, brand shown against hidden);
  position stability across two shuffles; decoy pick rate; latency p50 and p90; cost per plan.
- **Decision rule per family.** Highest required recall with harmful picks at most 2%, p90 at most 3 s and at most
  US$0.003 per plan. A model arm replaces the code pick only if it beats it by at least 5 points of set F1, with a
  bootstrap 95% interval above zero. Re-run on every catalog release.
- **Cost:** at most US$5, inside the companion spec's US$25 envelope (Erick's E14).

## 7. Order: across a step and across a mission

**Within a step:** `before` reads (field briefs, design system, product map, the relevant AGENTS.md sections) →
`during` work with the loaded kit → `after` deterministic checks run by the engine → `after` review lenses given to the
cross-vendor reviewer. Cheap before expensive, deterministic before judgement, the other vendor last.

**Across a mission:** the lead's plan graph already orders work (MVP spec section 6). Cards add two things:

- A `before` card that is costly to read (a long field brief, a references search) becomes its own short read-only
  node that feeds every node needing it, so it is paid once.
- `after` cards with `form: check` are added to `proof.define` automatically (section 16.1, N2). The lead may add proofs;
  it cannot remove a mandatory one.

### 7.1 Passes: overlapping tools in sequence (Erick's direction E18)

Tools that overlap need not be either/or (TS-24).

- **A pass is a step** with its own kit in a fresh session. Two tools that conflict in one session (for example two
  `ui.style` skills) may run in different passes. A card's `conflicts` means "not in the same session".
- **Base pass first:** direction, design system, structure (for code: the working change). **Polish passes** follow,
  each with one focus: UX, UI, anti-slop, distinctiveness; for code: simplify, then review lenses; for copy: tighten,
  voice; for assets: cleanup, consistency.
- **The picker chooses combination and order:** together, A then B, or B then A (for example Taste-Skill for the base
  direction, then Impeccable for UX and anti-slop; or both in the base pass when they do not share a job). Code
  enumerates the valid plans; the picker or the evidence picks one.
- **Keep only what beats the version before.** Deterministic gates run first (the Impeccable detector where it has a
  callable command, axe, lint, type check, tests); then a blind judge from the other vendor compares the new and the
  previous version in both orders. The earlier version wins ties ("better than the original or it does not stay").
- **Stop** when a pass is not kept, or after 3 polish passes per piece, or when the remaining budget is low.
- **Learning.** Evidence cells add the ordered tool tuple per family and job: kept rate and judge wins. The Probe lane
  tests orders on XS and S probes (one screen, one function, one paragraph).

### 7.2 Cold-start priors Erick named (E18)

| Tool | Job and use | Priors |
| :--- | :--- | :--- |
| ponytail | `code.minimal`: during `build` and `fix`, and the focus of a "simplify" polish pass | lift prior +3 pp for writers |
| caveman | `output.terse`: worker result packets, hand-offs, reviewer finding lists and internal logs, the TL-ADE lesson "short outputs as a contract" | never in text a person reads: chat answers, reports, docs, commit messages, screen text |
| Taste-Skill | `ui.style`: base design direction when the project has no design system of its own | conflicts with other `ui.style` cards in one session; may precede Impeccable |
| Impeccable | `ui.craft` and polish passes (UX, UI, anti-slop); its detector as a gate | after Taste-Skill, or alone |

**Change from R33 for Erick:** catalog-v1 said "caveman out"; this proposes caveman back in, for internal hand-offs
only.

## 8. Enforcing a kit in each CLI

Two delivery modes (TS-11):

- **Native:** the CLI loads exactly the kit through its own per-session levers. Cheapest for large skills (only the
  description sits in context until used) and observable (skill invocations show in the event stream).
- **Inline:** the engine writes the kit's `before` and `during` content into the session's appended system prompt
  (Claude Code `--append-system-prompt-file`) or developer instructions (Codex `developerInstructions`), and switches
  the CLI's own skill discovery off where it can. Inline works the same on both CLIs, needs only verified levers, and
  sits in the cached prefix. A skill body is cut to its core (cap 2,500 tokens); its reference files stay readable
  from the Tool store on request.

| Kit part | Claude Code (native) | Codex (native) | Inline floor |
| :--- | :--- | :--- | :--- |
| Hide user skills not in the kit | `--settings` `skillOverrides: off`, or `--disallowedTools "Skill(name)"` | `skills.config enabled=false` through thread `config` (UNVERIFIED per session) | Claude: `--disallowedTools Skill` hides all; Codex: accepted cost until verified |
| Add Tool store skills | a per-kit plugin folder generated by Faber, passed with `--plugin-dir` | `.agents/skills` inside a Faber-owned worktree (never committed: MVP commits only accepted paths) | appended text |
| MCP servers | Faber-owned: `--mcp-config` + `--strict-mcp-config`; a user server in the kit: no strict mode, others denied with `mcp__<server>` (UNVERIFIED on the command line) | `mcp_servers.<name>.enabled=false` for the others (UNVERIFIED through app-server) | none: MCP has no inline form |
| Plugins | `enabledPlugins` in `--settings` (format UNVERIFIED) | `disabledPluginIds` on the turn (VERIFIED) | — |
| Near-miss tools | `skillOverrides: name-only` for up to 10 close candidates, so the model can still call one without a restart (Claude only) | the worker asks via its result packet | same |

Rules:

- **Attest every session (TS-12).** After start, compare what the CLI reports about itself (Claude Code `system/init`:
  tools, MCP servers, skills, plugins; Codex: thread start response and skill list) with the loadout. A mismatch kills
  the session before its first model request where possible, logs a `reason.kit_mismatch` key, and retries inline.
- **Re-pass every flag on resume.** Claude Code resume does not restore `--settings`, `--mcp-config`, `--plugin-dir` or
  `--add-dir` (section 2.1). The driver stores the loadout with the session and always re-applies it.
- **Levers are capabilities.** Each lever is an entry in the CLI's driver descriptor, probed on install and on every CLI
  update ("Use everything each CLI has"). A lever that disappears flips that CLI to inline delivery by itself.
- **Frozen at approval.** The loadouts on the approved start card are fixed with their versions and file hashes. A
  hash that changes before load is never loaded: the step re-assembles without that tool, or parks if it was required
  (the TL-ADE's fail-closed rule, matched).
- **Clean worker environment.** Workers start from an allow-list of environment variables (path, home, temp, locale, the
  system variables the OS needs, and each CLI's own settings location), never from the parent's full environment, so token variables of any name stay out.
  A test plants a fake token variable and checks that no worker sees it.
- **Faber never opens files in `~/.claude` or `~/.codex`** (AGENTS.md). The inventory of the user's own tools comes
  only from what each CLI reports (names, descriptions, server names and status). Faber never copies a user MCP server's
  configuration, which may hold keys; to use one, it leaves it in place and scopes it by name.

## 9. Where tools come from

| Source | Card written by | Prior lift | Loads by default | Install place |
| :--- | :--- | :--- | :--- | :--- |
| User's own (already in their CLI) | small-model card writer, from what the CLI reports | +4 pp (the user chose it) | when it wins a slot | stays where the user put it |
| Catalog, vetted (Essential / Recommended) | scout + maintainer, in the signed manifest | +6 to +8 pp | when it wins a slot | Tool store |
| Catalog, Experimental | scout | +2 pp | when it wins a slot | Tool store |
| Not vetted (found beyond the catalog) | scout worker, locally | +1 pp | only within the bounds of section 12 | Tool store, quarantine |
| Project cards (design system, product map, AGENTS.md sections, docs) | generated from the repo | +3 pp | for matching verbs | the repo |
| Engine checks (proof set, OSV-Scanner, token linters) | Faber | n/a (proofs) | as `after` stage | Faber or Tool store |
| CLI features (web search, image generation, computer use) | driver descriptor | +3 pp | when it wins a slot | the CLI |
| Field briefs | research worker | +4 pp if `fresh` is not `stable`, else 0 | when it wins a slot | `.faber/briefs/` |

- **Card writer (TS-13).** One small-model call (Haiku or Luna, medium) per tool version, cached by content hash. It
  fills only the closed vocabulary fields (`verbs`, `serves`, `job`, `stage`, `conflicts`, `examples`). The tool's text
  is data (guardrail 8): a description that says "use me for everything" gains nothing, because claims only make a tool
  eligible; outcomes rank it. Token costs are measured, not declared.
- **Tool store (Erick's decision E7).** Tools Faber installs live in Faber's own store
  (`<faber data>/store/<id>/<version>/`), mounted per session, with the previous version kept. A per-tool switch
  "Also use outside Faber" copies it into the CLI's own configuration. Reasons: auto-installs must not silently change
  the user's plain Claude Code or Codex sessions; removal is one folder; Faber can read the full files of its own store
  to write good cards without touching `~/.claude`. The store has a size cap (default 5 GB, shown in Settings) and
  removes unused, unpinned tools after 30 days.

## 10. The long tail: a good kit for a never-seen task

The long tail is the main case (E3). A step whose needs no card covers well goes through the **gap pipeline**. It
starts at plan time, runs in parallel with planning, and never holds up steps that do not depend on it.

| Tier | What | Latency target | Cost target |
| :--- | :--- | :--- | :--- |
| G0 | Search the local manifest copy (vetted and Experimental items not installed yet) | ≤ 100 ms | none |
| G1 | Search beyond the catalog: a read-only research worker on a cheap model checks the official vendor organizations, MCP registries, skill directories and GitHub; returns at most 3 candidates with source, SPDX license, last release, kind, and whether it runs code | p90 ≤ 120 s | ≤ US$0.10 API-equivalent |
| G2 | Field brief: a read-only research worker, with web access and no repo access, reads primary sources (official docs, the reference example repo) and writes a cited brief of at most 3,000 tokens: versions, APIs, build steps, pitfalls, links | p90 ≤ 180 s | ≤ US$0.15 |
| G3 | Floor: the unmodified CLI, its own knowledge and web search | 0 | 0 |

- G1 and G2 run together. G2 runs only when `fresh` is not `stable` or G0/G1 found nothing: SkillsBench found that
  knowledge a model writes from memory does not help on average, so a brief must bring new information from sources.
- A field brief is a project card (`.faber/briefs/<need>.md`, with sources, fetch date and covered versions). It is
  re-checked after 30 days or when the fingerprint shows a new version. One brief serves every worker in the mission.
- Found tools install by the presence rules and bounds of sections 11 and 12, then join the next session that needs them.

### 10.1 Worked examples

**"Make a Terraria mod that adds a fishing rod."** Labels: `verb: build`, needs `lang:csharp`, `platform:tmodloader!`,
`artifact:game-mod`, `media:pixel-sprite`, `verify:mod-build`, `fresh: recent`. No card serves `tmodloader`. G0 finds
nothing; G1 looks for an official tModLoader resource and finds the documentation and the ExampleMod repository (not a
tool); G2 writes a brief from the official wiki and ExampleMod (with the license shown, for "Don't reinvent"). The
build check needs tModLoader on the machine: that is a user app, so it is never auto-installed (section 12); the card
asks or parks it. Sprites: if no image tool is installed, the generic `produce` card falls back to the CLI-native image
feature, which steers that step to the vendor that has it (section 6.1). Kit for the code step: brief (`before`),
C# library docs card (`during`, wildcard), build check (`after`).

**"Edit my podcast: cut the ums and long silences, even out the loudness."** Labels: `verb: produce`, needs
`media:audio!`, `op:silence-cut`, `op:loudness`, `op:transcribe`, `verify:listen`. FFmpeg is a vetted catalog item
(roadmap: composed video uses FFmpeg); if missing, it installs from its original source with a pinned version and
checksum (section 12). A local transcription model is offered only on a capable GPU (catalog `requires`). Proofs,
all deterministic and run by the engine: loudness measured on the result, duration before and after, a transcript diff
showing no content words were cut. The kit in context is small: the FFmpeg recipe card and the transcript.

**An unfamiliar stack: "add a settings page to this Phoenix LiveView app."** The fingerprint finds `mix.exs` and the
Phoenix version in `mix.lock`. No Elixir card exists; the generic library-docs card (`lang:*`, `framework:*`) matches
by wildcard; `fresh` is `recent` because the version is newer than the model's knowledge, so G2 writes a brief from the
official docs for that exact version. The project's own templates and components become `before` reference cards.

**Faber's own repository: "show the quota pace in the Running column."** No hand-written kit (E1). Fingerprint:
`lang:rust`, `framework:tauri`, `framework:solidjs`, `lang:typescript`. The lead labels the UI node `verb: build`,
needs `ui:component!`, `ui:design-system`, `artifact:locale-strings`, `verify:screen`. Assembled kit: `docs/design.md`
and the tokens (`before`, project cards, owning `style:*`), the AGENTS.md rules on locales (`before`), Impeccable if the user
has it (`during`, `ui.craft`); Taste-Skill dropped (its `style` job is owned by the project design system); process
skills such as brainstorming dropped (`process.*` is Faber's); proofs: the AGENTS.md proof set with `cargo t --all`
because the screen changes; review lens `ui.coherence` for the other vendor.

## 11. When the right tool is missing: presence rules

Erick's rule (E4), with the parts left to the AI decided here (TS-15). Presence is read from the work's phase, not
guessed from activity:

| Presence | When | What Faber does |
| :--- | :--- | :--- |
| **Shaping** | Normal mode, before the start card is accepted (or, without a plan, before the first step runs) | The start card lists each gap with two choices: "Continue without it" with the downside in one plain sentence ("the mod will build, but the sprites will be placeholders"), or "Let Faber find and install the best tools". Default: find and install. Asking costs nothing: the user is reading the card anyway |
| **Waiting** | Normal mode, after the start card | Never block. Same rule as Away: Faber searches, installs what the bounds allow, continues without what they do not, and leaves that choice for the user's return. Only a step whose `!` need can be met by nothing automatic (a sign-in, a payment, a license to accept, a user app such as Steam or Blender) raises a needs-you card, and only that step waits |
| **Away** | Autonomous mode | Faber picks the best option, which normally means installing within the bounds. Everything else is parked with a note; the rest goes on |

Why Waiting follows the Away rule: guardrail 6 says a mission never stops to wait for the user; R92 says notify only
when the person is needed; the user who is waiting wants the result; and the bounds below make an automatic install
low-risk and one click to undo. One rule for both "not at the screen" states is also simpler and less surprising.

## 12. Safe automatic installs

Erick wants the automatic path (E6). These bounds make it safe (TS-16). An item that fails a bound is not refused for
good: it falls to the next safer option, and in the end it is parked for the user.

| Item | Automatic install when the user is not shaping the work | Bounds |
| :--- | :--- | :--- |
| Vetted catalog item | Yes | From its original source; pinned version; checksum from the signed manifest; SPDX license allows the use (non-commercial never automatic for work marked commercial); requirements met; smoke test from the manifest passes; into the Tool store |
| Not vetted, text only (a skill with no scripts, no executable files, no hooks, no `allowed-tools` grant) | Yes, into quarantine | Pinned commit and file hashes; text scan (secrets, exfiltration URLs, instruction-override patterns, size cap); license detected or the item is flagged "license unknown" and kept out of deliverables marked commercial; loaded only into roles with no shell and no network (never the lead or a reviewer); a canary token sits in the session's workspace, and any attempt to output or send it removes the tool and discards that session's result |
| Not vetted, runs code (MCP server, plugin, scripts, binaries) | Only inside the isolated runner, and only after the runner passes its escape tests | Same as above plus a dependency vulnerability scan (OSV-Scanner) and the OS malware scan of the download; runs only in the disposable runner that the MVP's proof containment builds (no credential folders, no user profile, no provider secrets, no remote git, no visible window, network limited to the item's declared hosts). Until the runner qualifies: continue without it and park the choice |
| Anything that needs a sign-in, a key, a payment, a license acceptance, admin rights, a driver, a service, a PATH change, or a user app install | Never automatic | Needs-you card (Waiting) or parked (Away) |
| A tool that sends project content to a third party (`sends_data: project-content`) | Never automatic | Asked once per tool. Tools that send only public identifiers (a library name to a docs service) may install automatically; the report says so |

Limits per mission (TS-17): at most 3 automatic not-vetted installs, at most 500 MB per download, and the store cap.

Production keys never enter a loadout (the TL-ADE map's rule "never holds the real site's key"; generated pipelines
spec, section 10). A tool that deploys, charges or publishes gets only a staging target or a test account; the real
release, payment or publication is the person's act.
Every install is a self-change: logged, the previous state kept, and reversible with one click.

## 13. Proposed roadmap text changes (do not apply in this task)

```diff
@@ Guardrails enforced by code, never by the model
-3. Sending, publishing, paying or deleting outside a copy asks for approval every time. Changing Faber itself
-   (installing, updating or removing a tool, changing a setting) asks too, with "allow once" and "always allow this
-   kind" on the card, and a "full trust for Faber's own setup" switch in Settings. Full trust never covers sending,
-   publishing, paying or deleting. Every self-change is logged and keeps the previous version. Nothing else asks.
+3. Sending, publishing, paying or deleting outside a copy asks for approval every time. A tool a task needs follows
+   the tool-gap rule: while the user is shaping the work, the start card asks; after that, Faber installs it on its own
+   only within the install bounds (a vetted item from its pinned source and checksum; a not-vetted item only after the
+   security scan and a canary trial, text into quarantine, code only inside the isolated runner), always into Faber's
+   own tool store, and lists it in the report with one-click removal. No automatic install covers a tool that needs a
+   sign-in, a payment, a license to accept or a system change. Updating or removing a tool and changing a setting ask,
+   with "allow once" and "always allow this kind" on the card, and a "full trust for Faber's own setup" switch in
+   Settings. Full trust never covers sending, publishing, paying or deleting. Every self-change is logged and keeps the
+   previous version. Nothing else asks.
@@ Normal and autonomous modes
-  publishing, paying or deleting outside a copy is never approved on its own: it is parked with a note and the rest
-  goes on. Changes to Faber itself follow the user's trust setting, parked without full trust. The work runs to the end,
-  then leaves a report: what was done, what was decided alone and why, what is parked.
+  publishing, paying or deleting outside a copy is never approved on its own: it is parked with a note and the rest
+  goes on. Tools the work needs install on their own within the install bounds (guardrail 3); other changes to Faber
+  itself follow the user's trust setting, parked without full trust. The work runs to the end, then leaves a report:
+  what was done, what was installed and decided alone and why (each with keep or remove, keep or change), and what is
+  parked.
@@ Building software: one voice, a team behind it
-  each step loads only what it needs.
+  each step loads only what it needs: the engine assembles each step's kit from the needs the lead labels, never from
+  tool names ([spec](superpowers/specs/2026-10-12-tool-selection-rework-design.md)).
@@ Toolbox
+| `kit.request` / `kit.explain` | asks for a need (never a tool name) for the next session; explains why each tool was picked |
@@ Curated catalog and live manifest
-- **Faber recommends only tools it has vetted.** A vetted tool works, is safe, has a license that allows the use, is
-  maintained and improves the result. The agent may find and install any other tool. That tool is marked "not vetted",
-  and the agent asks before installing it.
+- **Faber recommends only tools it has vetted.** A vetted tool works, is safe, has a license that allows the use, is
+  maintained and improves the result. The agent may find any other tool. That tool is marked "not vetted" and installs
+  only within the install bounds of guardrail 3.
@@
-  it serves, the CLIs it works with and its requirements (OS, GPU memory, apps such as Blender). "Recommended" needs
-  evidence: the benchmark shows it helps, or it is the reference tool for its job.
+  it serves, the CLIs it works with, its requirements (OS, GPU memory, apps such as Blender), and a tool card: the needs
+  it serves, its job, stage, conflicts, measured context cost and example requests. "Recommended" needs evidence for
+  a job and stack: paired runs show it helps, or it is the reference tool for its job.
```

Erick approved the tool store (E7), so also change the Architecture bullet "The user's own setup comes along" to:
"Projects can use the
skills, MCP servers and plugins already configured in each CLI; each session loads only the kit its step needs. Tools
Faber installs live in its own tool store and mount per session; a per-tool switch also makes one available in plain
Claude Code or Codex."

The self-development MVP spec needs one sentence in section 5: "The approved MCP set and skills are the step's
attested kit."

## 14. End report: installed and decided without you

E5 fits three structures that already exist (TS-18):

- **The report** (autonomous mode's end report, and the summary when a Waiting mission finishes) gets a section
  "Installed and decided without you". One row per tool installed: name, source, license, vetted or not, why, which
  parts used it, with **Keep** and **Remove**. One row per decision made alone, with **Keep** and **Change**.
- **Remove** unmounts and deletes the tool from the store. If delivered parts used it, the row says which, and offers
  "Redo those parts without it" (the "redo only the part that changed" power).
- **Decisions** are the existing decision list entries (AI, "not reviewed"); Keep marks them reviewed, Change opens the
  decision with its default.
- **Parked items** stay where they are, with their note. A tool that was not installed because of a bound shows there,
  with the one-line downside and "Install now".
- Keep on a not-vetted tool also offers "Use it outside Faber too" (E7).

## 15. Learning and measurement

### 15.1 Signals

| Signal | Source | Strength |
| :--- | :--- | :--- |
| Proof passed on the first run | engine receipts | strong, objective |
| Repairs needed; blocking review findings | queue receipts | strong |
| User kept, redid or rejected within 7 days | Projects and Chat actions (Auto learning uses the same) | strong but slow |
| Tokens and own cost of the step | usage ledger | strong |
| Tool used or not ("returns") | native: skill and MCP calls in the event stream; inline: `kit_used` in the result packet | medium (self-report is weaker) |
| Paired with/without probes | Probe lane | strongest, after calibration |

### 15.2 Evidence cells

Per (verb, need, stack, vendor, model) and per tool: a Beta count of "accepted on the first try" with the tool in the
kit, and the same cell's baseline without any tool in that job. Online lift = difference of the two means, shrunk
toward the prior; it ranks tools in step 3 of the assembler. Counts decay with a 90-day half-life and halve on a tool's
major version or a new model generation. Evidence stays per project; it becomes global after it repeats in 3 or more
projects (the existing rule R51).

### 15.3 Probe lane (Erick's decision E8)

Online data is confounded: harder steps get more tools. Credit comes from paired **probes**: small prototype versions
of real work, run twice with one kit change (TS-19). Erick's conditions (E8): no pile-up of tests; tests are light,
on scaled-down versions of the task, never a full rebuild; because they are cheap they may run at any time, spread out;
the shared-account pause and "never the daily grant" stay.

**What a probe is.** A scaled-down copy of a real step that keeps what the tool acts on and drops the bulk. Two sizes:
XS (one unit) and S (a mini version of 3 to 5 units).

| Real step | XS probe | S probe |
| :--- | :--- | :--- |
| A feature across six screens | one screen with the same components and tokens | a mini app with three screens |
| A Rust change across engine modules | one function and its tests in a mini crate | a mini crate with 3 to 5 files |
| Edit a 40-minute podcast | a 60-second clip with the same voices and noise | a 5-minute clip |
| A 12-page report | one page with one chart | three pages |
| A 3D character | one prop in the same style | the head and one hand |
| A Terraria mod | one item that builds | one item, one tile, one NPC |

- **Where probes come from.** (a) Shrinking a finished real step: the engine keeps the brief, labels and proofs, and a
  cheap model writes the smallest version that still exercises the same needs (one call, at most US$0.005). (b) The
  probe bank: a probe is frozen once made (inputs, brief, proofs) and reused for every later challenger in the same
  job, so results stay comparable over time. (c) Seed probes per job in the manifest, written by the maintainer, for
  cold start. Probes are built only from the step's scoped inputs, never hold secrets, and stay on the machine.
- **How a pair runs.** Same probe, same model and effort, one kit change (tool X removed, or challenger Y in X's job).
  Both arms run hidden in disposable copies. Deterministic proofs run on both; a blind judge from the other vendor
  compares them in both orders; disagreement between the two orders is a tie.
- **Cost and pace.** Target per pair: at most US$0.10 API-equivalent and 5 minutes. One pair at a time, at least 20
  minutes apart, at most 12 pairs a day and 2 per tool a day.
- **Which quota.** Never the daily grant for real work (kept; R88, R91). Probes use only room that the account pace
  line leaves unused after every real-work reservation, expiring quota first, capped at 2% of a weekly window per week.
  Real work always wins: probes are admitted last, and a running probe is interrupted and its pair discarded when real
  work needs the room. The shared-account pause stays: no probes while someone else is using the account, nor for 30
  minutes after. There is no strong reason to change either rule; both protect the people sharing the subscription.
- **Machine load.** Probes run only when the PC is idle enough (no full-screen app or game, CPU under 50% over the last
  5 minutes, on mains power on laptops) and never open a window. Builds share one probe cache in Faber's data folder
  (default cap 2 GB); probes never create build folders in scratch or temporary copies.
- **Order:** by value of information: Experimental tools that win slots often; field briefs against none; Recommended
  tools after a version change or 60 days; tools with a high return rate.
- **Long-tail tournaments:** for a new label signature that recurs, up to 3 kit variants on its probe; the winner
  becomes the signature's recipe.

**When a probe result carries over to full-size work.**

1. **Same cell.** A probe counts only for the evidence cell it shares with the work it stands for: same verb, need,
   stack, vendor and model. It runs on the same model and effort as those steps.
2. **Rates, not totals.** Probe metrics do not depend on size (first-try proof pass, blocking findings per unit, judge
   preference), so a one-screen result is comparable with a six-screen one.
3. **Discounted.** A decisive probe pair counts as `w` full-size pairs; `w` starts at 0.5.
4. **Calibrated per job.** Whenever full-size outcomes exist for the same contrast (real steps that ran with and without
   the tool, from online data), Faber checks whether the probes predicted the winner. At least 85% agreement over 10 or
   more comparisons raises `w` to 0.8. Below 70% lowers `w` to 0.2 and marks the job "probe-weak": there, probes only
   order the queue and cannot change kits. At most once a month per job, one full-size paired check may run on a real
   step that costs under US$0.50, only on expiring quota, to keep the calibration honest.
5. **Scale-sensitive tools.** A card may say `scale_sensitive: true` (repository indexers, architecture skills, long
   document tools). Their probes use size S, and probe evidence alone never promotes them; they need full-size online
   outcomes.
6. **Limits of a probe.** Probe evidence can make a tool the local default in its slot. It cannot alone make a tool
   Recommended: that also needs at least 3 full-size confirmations (online outcomes in the cell where the tool at least
   matches the cell's baseline).

The same probe idea serves cheap drafts before expensive work in generated plans
([generated pipelines](2026-10-12-generated-pipelines-design.md), section 7). The result probes of that spec's
benchmark (generated plan against the plain CLI, Erick's E11, its section 14.3) run in this lane as well: one queue,
the same daily and weekly caps, at most US$0.20 per pair because each also compiles a plan.

### 15.4 Earning and losing "Recommended"

- **Earn**, per job and stack: posterior probability of winning more than half of decisive pairs at least 0.9, with at
  least 12 weighted decisive pairs (probe pairs count `w` each) across at least 3 projects or 2 benchmark requests, at
  least 3 full-size confirmations, and median token increase at most 15% (or fewer repairs that pay for it). The
  maintainer's benchmark sets the global tier in the manifest; local evidence can promote a tool for that user only.
- **Lose**: probability of losing more than half of decisive pairs at least 0.9 with at least 8 weighted pairs; or
  loaded at least 20 times with at least 80% unused; or any security, license or maintenance failure (immediately, by
  manifest).
- **Cold start:** card priors (section 9), user pins, and the user's taste (a disliked style lowers that job's prior).
  No exploration on real work at cold start: real work exploits; the Probe lane explores.

### 15.5 Speed and cost targets

| Measure | Target | How it is measured |
| :--- | :--- | :--- |
| Assembly, labels to loadout | p50 ≤ 20 ms, p99 ≤ 200 ms with 2,000 cards | timer in every receipt; benchmark run with a synthetic 2,000-card set |
| Request labeling overhead | +≤ 0.3 s p50, +≤ 0.5 s p90 over today's route (2.8 s / 3.9 s) | the same 22 requests, run twice, old and new classifier prompt |
| Request labeling cost | ≤ 150 extra output tokens; ≤ US$0.001 API-equivalent per request | usage of the classify call priced with the dated table (Haiku 5.5: $0.10 in, $0.50 out per MTok) |
| Plan labeling | 0 extra calls; ≤ 40 output tokens per node | plan output tokens before and after |
| Card writing | 1 call per tool version; ≤ US$0.003 each; ≤ US$0.30 for a first inventory of 100 tools | own-cost ledger tag `kit.card` |
| Selection share of a mission | ≤ 1% of its own cost without gap work, ≤ 3% with it | own-cost ledger tags `kit.label`, `kit.card`, `kit.gap` |
| Gap tiers | G0 ≤ 100 ms; G1 p90 ≤ 120 s, ≤ US$0.10; G2 p90 ≤ 180 s, ≤ US$0.15 | worker receipts |
| Start delay from kits | 0 s for covered steps; a gap delays only steps that need it, at most its gap budget | dispatch timestamps |
| Resident kit tokens | within the role budget; estimate within ±20% of the CLI's own measure | Claude Code context breakdown (`get_context_usage`); Codex: token delta of a probe turn |
| Attestation | 100% of sessions attested; every mismatch logged with a reason key | receipts |
| Probe pair | ≤ US$0.10 and ≤ 5 min per pair; one at a time; ≤ 12 a day; ≤ 2% of a weekly window per week | own-cost ledger tag `kit.probe`; probe receipts |
| Probe transfer | agreement with full-size outcomes ≥ 85% per job before `w` rises to 0.8 | calibration log per job (section 15.3) |
| Picker | required-tool recall ≥ 90% and harmful picks ≤ 2% on the gold set; p90 ≤ 3 s; ≤ US$0.003 per plan | section 6.3 test; receipts |
| Passes | a kept polish pass wins its blind comparison; ≤ 3 polish passes per piece; pass cost ≤ 30% of the piece's cost | pass receipts; own-cost ledger tag `kit.pass` |

## 16. Neighbouring plan parts

### 16.1 Improve

- **N1 Briefs (TS-20).** The brief gets a kit sheet: each tool, its stage, one line on when to use it. The result
  packet gets `kit_used` and `needs`. Explicit instructions raise tool use, which the research says descriptions alone
  do not do reliably.
- **N2 Proof selection.** `after` check cards propose proofs from the labels (UI → screen check at the shell widths;
  lockfile change → OSV-Scanner; audio → loudness). The lead adds acceptance; it cannot drop a mandatory proof.
  Deterministic checks never load into a model's context.
- **N3 Review lenses.** The reviewer's kit comes from the diff's fingerprint (security basics, accessibility, the
  project's design system, the language's idioms). The cross-vendor rule is unchanged.
- **N4 Recipes become data.** A recipe is a label signature, its loadout and its outcome, plus any field briefs. The
  assembler uses a recipe as a prior for the same signature. The model may still read it as a hint, never as a track.
- **N5 Catalog is card-first.** The scout writes cards; CI smoke-loads each item in both CLIs and measures its context
  cost; "not a weaker duplicate" is judged by paired runs, not by opinion. Jobs and conflicts are required fields.
- **N6 Product map and design system are project cards** that own their jobs, so third-party style tools never fight
  the project's own design.
- **N7 Taste** feeds job priors (a style the user rejected loses its slot).
- **N8 Roles** become permission envelopes derived from verb and risk; the MVP's four roles stay as they are.
- **N9 Chat (Phase 2)** gets a request kit from the same Auto call; its kit changes only when the cache is lost anyway.
- **N10 RAM.** Strict MCP per session means user MCP servers a step does not need do not start, which lowers the
  per-session RAM the roadmap counts. Measure it with the Phase 4 RAM budget.

### 16.2 Keep as is

- Auto: blind classification and the fixed table (extended output only; route.rs logic unchanged).
- Powers, not tracks; recipes are hints for the model.
- Guardrails 1, 2 and 4 to 9; guardrail 3 changes only for task installs (section 13).
- Catalog vetting criteria, tiers, pinned version, checksum, SPDX license, the non-commercial warning, nothing in the
  installer, updates never automatic.
- Rule 3 for driving the CLIs (no tool change mid-conversation), now enforced by the kit design.
- No local model and no hosted classifier; telemetry off.
- The MVP's envelope, leases, serial queue, proof set and cross-vendor review.

## 17. Phasing

### 17.1 Self-development MVP (Phase 1a, target 2026-10-20 to 22)

Automatic from day one (E1), smaller in scope (TS-21):

| Part | MVP version |
| :--- | :--- |
| Labels | Plan nodes carry `verb`, `needs`, `risk`, `fresh`; request labels from the extended Auto prompt |
| Fingerprint | About 40 rules as data; Faber's repo yields Rust, Tauri, SolidJS, TypeScript |
| Cards | Auto-written for: the user's installed skills and MCP servers as the CLIs report them; project cards (AGENTS.md sections, `docs/design.md`, tokens, roadmap rules for driving the CLIs); the AGENTS.md proof set as checks. No catalog yet |
| Assembler | Section 6, steps 1 to 8, with priors only; online Beta counts recorded, used only to break ties |
| Picker | The code pick decides; the blind model pick (section 6.2) runs in shadow and logs disagreements, until the section 6.3 test chooses per family |
| Facets | `purpose`, `audience`, `deliverable`, `stage` labelled and saved; Faber's own project facts recorded once |
| Passes | UI changes: a base pass and up to 2 polish passes with the keep rule; code: an optional "simplify" pass |
| Delivery | Inline on both CLIs. Native on Claude Code (generated kit plugin plus `skillOverrides`) only if the day-1 probe proves: only kit skills visible, no hooks or MCP from it, deny rules beat a skill's `allowed-tools` |
| Attestation | Claude Code `system/init` compared with the loadout; Codex best effort, logged |
| Gaps | G0 is empty (no catalog); G2 field briefs and G3 floor work. G1 runs and reports candidates; installs wait for Phase 3 and appear in the report as "suggested, not installed" with their downside |
| Presence rules | The start card shows gaps and the two choices; Waiting and Away continue with briefs and park installs |
| Receipts | Every selection and outcome, in `.faber/missions/<id>/kits/` |
| Software bar checks | The check programs of the complete software bar (Erick's E10; generated pipelines spec, section 15.1.1) are engine-native cards: advisory lookup by package name, license read from local metadata, secret scan, registry existence check, the AGENTS.md lints and proofs. They need no third-party install, so the MVP's "no installs before Phase 3" rule does not weaken the bar |
| Targets | Section 15.5 measured from the first real goal |

Day-1 probe (with the MVP's G1 feasibility work on 13 to 14 October): run each lever in section 8 marked UNVERIFIED in
a hidden, bounded real-CLI session; record which pass in the driver descriptor. A failed lever only means inline.

New MVP gate **G9 Kits:** every worker session attested; assembly p99 under 200 ms; labeling overhead within target;
no kit-related escape in the G1 bypass tests; receipts complete for the real goal of G8.

### 17.2 Phase 2 (Full Chat)

Chat gets request kits from the Auto call; the Auto learning loop reads kit receipts too.

### 17.3 Phase 3 (Projects with the lead agent, catalog)

Catalog cards in the manifest; the Tool store; native delivery on both CLIs where levers pass; the install pipeline with
the section 12 bounds and quarantine; G1 results that install; the end-report rows of section 14; evidence cells with
back-off; the Probe lane; Recommended and demotion rules; near-miss tools (`name-only`); stickiness;
`kit.request` and `kit.explain`; right-sizing from kit lift.

### 17.4 Phase 4 and later

Automatic installs of not-vetted code once the isolated runner passes its escape tests; kit levers shipped in the live
manifest; one shared MCP server copy per project. Later phases (image, video, PC apps, mods) add cards and checks, not
new mechanisms.

## 18. Risks and what to measure before trusting it

| Risk | Mitigation | Must measure before default-on |
| :--- | :--- | :--- |
| Under-kitting: a kit worse than the user's full setup | Generic wildcard cards; workers can ask for a need; user pins | On the rotating benchmark sample: kit vs full setup vs bare CLI. Kit must tie or beat full setup on blind quality and proofs, and spend less. If not, the default becomes "full setup minus conflicts" until fixed |
| Wrong labels | Closed families; fingerprint for stacks; worker escalation | Need recall ≥ 85% and precision ≥ 70% on a gold set of 60 Faber steps and the 40 benchmark requests |
| Unverified or drifting CLI levers | Inline floor; attestation; probe on every CLI update | Day-1 probe results per lever |
| More sessions, colder caches | Stickiness; batch steps that share a kit; re-kit only when the cache is lost | Cache-read share per mission against the current baseline |
| Field briefs that do not help (SkillsBench) | Only when `fresh` is not `stable`; sources required; paired runs | Paired lift on at least 10 long-tail tasks |
| Supply chain through automatic installs | Section 12 bounds, canary, quarantine, store rollback, end report | Escape tests for the isolated runner; canary trials; audit log shows no access to credential folders |
| Item text steering the card writer | Closed vocabulary; examples; outcomes rank, claims only qualify | Spot audit of 20 auto-cards per release |
| Disk use of the store (Faber's data drive) | 5 GB cap, cleanup, size shown | Store size after a month of real use |
| Too little data per user to learn | Priors; paired probes; maintainer benchmark evidence in the manifest | Number of decisive pairs per month on Erick's machine |
| Probe results that do not carry over to full-size work | Same cell, size-free metrics, discount `w`, per-job calibration, size S and online-only promotion for scale-sensitive tools | Probe-vs-full-size agreement per job; jobs marked probe-weak |
| Probes slowing the PC or filling the disk | One pair at a time, idle-only, no windows, shared 2 GB probe cache | CPU and disk use of the probe lane during a week of real use |
| Third-party services seeing project content | `sends_data` on every card; never automatic for project content | Card audit |

## 19. Candidate tools from the earlier list (verified 2026-10-12)

Facts from GitHub and vendor pages read by research agents; star counts are snapshots. None of these is vetted by this
document; each would enter the catalog through curation and earn its tier by evidence.

| Candidate | Verified facts | Card treatment |
| :--- | :--- | :--- |
| Taste-Skill (github.com/Leonxlnx/taste-skill) | Exists, MIT, about 95k stars; SKILL.md files; variants `design-taste-frontend`, `gpt-taste`, `high-end-visual-design` (soft), `redesign-existing-projects`, `industrial-brutalist-ui` all in this repo | Job `ui.style` (exclusive); the variants conflict with each other. Erick's R33 keeps it in the catalog |
| Impeccable (github.com/pbakaus/impeccable) | Exists, Apache-2.0, about 80k stars; one skill with about 24 commands and 59 deterministic detector rules; started from the frontend-design skill | `ui.craft` (during) and `ui.critique` (after lens); its detectors are a candidate engine check if it has a callable command (UNVERIFIED); conflicts with frontend-design |
| frontend-design (github.com/anthropics/skills) | Repo exists (most skills Apache-2.0); the folder itself UNVERIFIED | `ui.craft`; conflicts with Impeccable |
| Vercel web-design-guidelines / react-best-practices (github.com/vercel-labs/agent-skills) | Exists, MIT, about 32k stars | guidelines: `ui.review-rules` lens; react-best-practices: only with `framework:react` (never for Faber's SolidJS) |
| UI UX Pro Max (github.com/nextlevelbuilder/ui-ux-pro-max-skill) | Exists, MIT, about 135k stars; a skill plus a Python BM25 search over a CSV database | Runs code; `ui.style` and reference data; conflicts with Taste-Skill in `ui.style` |
| frontend-design-pro (github.com/Krishna-Modi12/frontend-design-pro) | Exists but tiny: 11 stars, one owner; "19 skills" is the project's own claim | Not a candidate: unproven, and a skill router duplicates Faber's own selection (`process.*`) |
| obra/superpowers | Exists, MIT, about 298k stars; TDD, debugging, brainstorming, plans, subagent work | Process skills excluded from workers (`process.*`); `systematic-debugging` eligible for `fix`, TDD for `build` and `verify` |
| Supabase (github.com/supabase/agent-skills) | Official, MIT | Only with `service:supabase` in the fingerprint |
| Neon MCP (github.com/neondatabase/mcp-server-neon) | Official; license UNVERIFIED; Neon says it is for local development | `service:neon`; never `risk: high` steps |
| Prisma MCP (mcp.prisma.io) | Official, remote; skills list UNVERIFIED | `data:prisma`; `sends_data` to check |
| Better Auth (github.com/better-auth/skills) | Exists; license UNVERIFIED | `service:better-auth` |
| Clerk (github.com/clerk/skills; mcp.clerk.com) | Official, MIT | `service:clerk` |
| mcp-builder (anthropics/skills) | Folder UNVERIFIED | `artifact:mcp-server` |
| Cloudflare (github.com/cloudflare/skills) | Official, Apache-2.0, 17 skills | Only with `service:cloudflare` |
| GSAP (github.com/greensock/gsap-skills) | Official, MIT, 8 skills | Only with a `gsap` dependency; motion.dev not checked |
| Playwright MCP (github.com/microsoft/playwright-mcp) | Official, Apache-2.0 | `verify:web-e2e`; Faber's own app is tested through its testkit instead |
| Context7 (github.com/upstash/context7) | MIT, Upstash; sends library names and queries to its service | Generic `lang:*` / `framework:*` docs card for the long tail; `sends_data: public-identifiers` |
| Tauri MCP (github.com/hypothesi/mcp-server-tauri) | Community, MIT, not affiliated with Tauri | Experimental at most |
| SolidJS `llms.txt` | UNVERIFIED | A project card if it exists |

## 20. Alternatives considered

Divergent ideation under five frames (logistics, on-call, attacker, removed assumptions, biology) produced 30 ideas.
What survived is folded in above: the pick list from labels (logistics), the hard budget and attestation (on-call),
ranking by paired outcomes and never by self-description (attacker), plan verbs as a closed set (removed assumptions),
repressor-style conflicts and decaying evidence (biology), install during planning, the "returns" signal, and the
field brief instead of trusting an unknown package. Rejected as traps:

- **Embedding retrieval:** needs a local model, which the roadmap rules out; lexical matching over examples is enough
  for a few thousand cards.
- **Slicing skills into paragraphs:** breaks the author's design and makes updates hard; core-plus-references already
  gives most of the saving.
- **A Faber MCP gateway that proxies every tool:** makes Faber the runtime of unvetted code, and both CLIs already defer
  MCP schemas.
- **A failing first attempt to discover needs:** doubles cost on every new task; the worker's `needs` field gets the
  same signal at the end of a normal attempt.
- **Ranking by stars or install counts:** easy to game and blind to fit; a weak prior at most.
- **Letting the lead pick tools:** the measured router result and the tool-preference research both argue against it.

## 21. Questions for Erick

Both earlier questions are answered (E7, E8). One item to confirm: caveman back in the catalog for internal hand-offs
only, a change from R33 (section 7.2). Default if unanswered: yes, as proposed.

## 22. AI decisions in this document

All are AI proposals, not reviewed, except where a row names Erick's decision.

| Tag | Decision |
| :-- | :--- |
| TS-1 | Models label needs; code picks tools; no model names a tool for selection |
| TS-2 | Tools change only at session boundaries, or when the cache is lost anyway |
| TS-3 | Deterministic checks run in the engine, not as skills in context |
| TS-4 | Closed verb set (10) and need families (13), open normalized values |
| TS-5 | Stack facts from a file fingerprint, not a model |
| TS-6 | `process.*` jobs belong to Faber; process skills never load into workers |
| TS-7 | The lead carries a small kit, not the user's whole setup |
| TS-8 | The assembler is a pure engine function |
| TS-9 | Role budgets: worker 4,000, lead 6,000, reviewer 3,000, research 1,000 tokens; at most 4 skills and 2 MCP servers |
| TS-10 | One loadout function wraps route.rs; route.rs picks are unchanged at cold start |
| TS-11 | Native and inline delivery, inline as the floor |
| TS-12 | Every session is attested against its loadout |
| TS-13 | Cards written by a small model in a closed vocabulary, cached by content hash |
| TS-14 | Erick's decision E7: tool store, per-session mounting, per-tool "also outside Faber"; the AI details (5 GB cap, 30-day cleanup) stay proposals |
| TS-15 | Presence from the work's phase; Waiting follows the Away rule |
| TS-16 | Install bounds of section 12 |
| TS-17 | At most 3 automatic not-vetted installs per mission, 500 MB per download, 5 GB store cap |
| TS-18 | End report rows for installs and lone decisions, with keep/remove and keep/change |
| TS-19 | Probe lane within Erick's decision E8: XS/S prototype probes, one pair at a time, ≤ 12 a day, room under the pace line only (≤ 2% of a weekly window per week), real work preempts, shared-account pause, transfer rules of section 15.3 |
| TS-20 | Kit sheet in briefs; `kit_used` and `needs` in result packets |
| TS-21 | MVP scope of section 17.1 and gate G9; the software-bar check programs as engine-native cards follow Erick's E10 |
| TS-22 | Within E17: facets `purpose`, `audience`, `deliverable`, `stage`, `region`; the purpose-to-needs table; 18 need families; the task-family taxonomy of section 4.6 |
| TS-23 | Within E17: the picker of section 6.2 (labels as query, code shortlist, blind shuffled model pick, code validation, code fallback), kept per family by the section 6.3 test |
| TS-24 | Within E18: passes of section 7.1 with the keep-if-better rule, a cap of 3 polish passes, ordered-tuple evidence |
| TS-25 | Within E18: the cold-start priors of section 7.2; caveman only for internal hand-offs (a change from R33 for Erick to confirm) |

## 23. Sources

- Repo: `AGENTS.md`; `docs/roadmap.md`; `docs/superpowers/specs/2026-10-11-faber-redesign-design.md` (A5, A6, A11);
  `docs/superpowers/specs/2026-10-12-self-development-mvp-design.md`; `docs/superpowers/specs/2026-10-11-auto-quota-pacing-design.md`;
  `engine/src/route.rs`; `docs/research/2026-10-11/` (r3-cache-switching, r4-context-cost, r4-protocols-sessions,
  r4-changelogs-issues, r4-limits-billing).
- Claude Code docs: code.claude.com/docs/en/cli-reference, /settings, /skills, /mcp, /sub-agents, /permissions,
  /env-vars; local `claude --help` (2.1.296). Note: the MCP description cap reads 2,048 chars in the docs and 4,096 in
  the 2.1.296 changelog notes; the probe settles it.
- Codex docs: learn.chatgpt.com/docs/build-skills, learn.chatgpt.com/docs/plugins,
  developers.openai.com/codex/config-reference, codex-rs/core/config.schema.json, codex-rs/app-server/README.md; local
  `codex --help`, `codex exec --help`, app-server schema (0.162.1).
- Research: arXiv 2602.12670 (SkillsBench), 2505.18135 (tool preferences), 2505.03275 (RAG-MCP), 2411.15399 (Less is
  More), 2409.02141 (Tool2Vec), 2405.16089 (COLT), 2510.20036 (ToolScope), 2506.01056 (MCP-Zero), 2406.18665
  (RouteLLM), 2409.07429 (Agent Workflow Memory), 2305.16291 (Voyager), 2508.01780 (LiveMCPBench). Read from abstracts
  and search results only.
- Products: cursor.com/docs/context/rules (rule types incl. description-requested and glob-attached),
  docs.openhands.dev/overview/skills (keyword and path triggers), kiro.dev/docs/steering (inclusion modes),
  docs.github.com/en/copilot/reference/custom-agents-configuration (tool lists per agent), Roo Code custom modes (tool
  groups). All use static, glob, keyword or model-description selection; none found learns tool choice from outcomes.
