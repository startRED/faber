# Generated pipelines: a plan compiled from each request

Date: 2026-10-12. Status: design proposal. Nothing here is implemented, and this document authorizes no build.
Companion spec: [Kits](2026-10-12-tool-selection-rework-design.md) gives each step its tools, model and effort; this
spec decides the steps, the checks and the proofs. One compiler uses both. Input:
[TL-ADE pipeline map, full translation](../../research/2026-10-12-tl-ade-pipelines.md).

**Authorship.** Every choice here is an AI proposal (not reviewed), tagged `GP-n`, unless marked as Erick's. Erick
decides; the AI proposes. Erick's direction for this design (2026-10-12, through the coordinating session):

| Tag | Erick's direction |
| :-- | :--- |
| E9 | Faber's first real version does everything the TL-ADE was meant to do, at higher quality than the TL-ADE could reach, and universally: no fixed pipeline for specific things; a pipeline is generated for every request, from the request itself |
| E8 | (from the Kits spec) Background tests run on small prototype versions of the task; the same probe idea serves cheap drafts here |
| E10 | Answer to Q1: "first real version" means the first public version, and other domains come before it. The self-development MVP must already be the best at software: its software bar is complete, not a slice (everything the Software and Client website lines check, plus what a demanding expert adds for Rust, Tauri, SolidJS and software in general) |
| E11 | Answer to Q2: no run against the TL-ADE (it was never finished and was slow); no "line run as a fixed pipeline"; no one-off US$100–150 benchmark. Keep the nearly free plan-level comparison with the map's stations and checks; measure result quality cheaply, e.g. generated plan vs plain CLI on small prototype probes, spread over time under the pace rules |
| E12 | Faber must be clearly better than (a) the real TL-ADE in software, already at the self-development MVP, and (b) the hypothetical finished TL-ADE with all 26 lines working, by the first public version. Fact: the Software line is the only one the TL-ADE ran (Erick calls it complete; a code inventory found some stations partial, section 14.6); the other lines were in development. E11 still binds. Where a line is already excellent, say plainly that Faber matches it |
| E13 | Faber beats the TL-ADE on every feature in the TL-ADE dossier, by a wide margin. Where the TL-ADE is strong, Faber matches it explicitly and then exceeds it; where it is weak, Faber states its concrete answer and how it is measured |
| E14 | Answer to the section 18 question, broader than asked (updates E11 and E12): real comparisons against the TL-ADE Software line are allowed, and so are any tests or probes, while their combined cost stays under US$25 API-equivalent; above that, ask. They run under the pace rules, never on the daily grant, and pause while someone else uses the shared account |

Feature names (no vendor names, AGENTS.md): **Blueprint** (the plan compiled for one request), **Deliverable sheet**,
**Bar** (the expert checks for a deliverable), **Bar library**, **Moves** (the building blocks), **Fact sheet**.

## 1. What the TL-ADE taught, and the target

The TL-ADE map has fixed lines: a list of stations per kind of request. The map shows that their value is not the
order of stations. It is 17 recurring moves and, above all, **domain-expert checks** a generalist would not think of:
six-view height alignment for a 3D model, reading a listing to its last line, money paths as tests, the title checked
against the finished video. Fixed lines fail in five ways: they cover only foreseen requests, the same move is written
26 times, they run stations a request does not need and miss ones it does, expert checks exist only where someone
wrote them, and tools are fixed per station.

**Target (E9).** For any request, Faber compiles a Blueprint that:

1. contains every expert check the matching TL-ADE line has, or a stronger one, or a stated reason it does not apply;
2. adds the expert checks no line had, for any domain, grounded so they are not invented;
3. runs only the steps those checks need, cheapest first, with back edges to the owner of each defect;
4. gets its tools, models and proofs from the same compile;
5. is cheap and fast enough to run on every request (section 13).

Software comes first and complete: the self-development MVP already carries the full software bar (E10, section
15.1). Every other domain reaches the same standard before the first public version.

## 2. What the research says

Numbers come from abstracts and search results gathered on 2026-10-12; nobody re-ran them. Direction, not targets.

- **Composing from small blocks works.** Self-Discover (arXiv 2402.03620): models that select and compose atomic
  reasoning modules per task beat chain-of-thought by up to 32%, using 10 to 40 times less compute than
  self-consistency.
- **Per-request plans beat fixed ones.** MaAS (2502.04180) builds a system per query at 6 to 45% of baseline inference
  cost; FlowReasoner (2504.15257) designs a system per query. AFlow (2410.10762) searches workflows as code. No study
  found compares per-request and fixed pipelines at equal budget. Faber's benchmark (section 14) compares plans with the
  map's fixed lines and results with the plain CLI; it never runs a fixed line (E11).
- **Reuse what worked.** Agent Workflow Memory (2409.07429) and Agentic Plan Caching (2506.14852, about 50% cost and
  27% latency cut per a search summary, UNVERIFIED) reuse plans from past runs.
- **Generated checklists help, ungrounded ones hurt.** TICK (2410.03608): instruction-specific checklists raised
  judge-human agreement (46.4% to 52.2%) and self-refinement (+7.8%). Rubrics as Rewards (2507.17746): rubric rewards
  beat Likert judges (up to +31% relative on HealthBench). But naive generated rubrics lowered judge accuracy below no
  rubric (55.6% to 42.9%, "Rethinking Rubric Generation", 2602.05125, UNVERIFIED), and models struggle to write valid
  criteria alone (RubricBench, 2603.01562, UNVERIFIED). Criteria drift as people see outputs (EvalGen, 2404.12272).
- **Judges are biased.** Position, length and self-preference biases are documented (2306.05685, 2404.13076,
  2404.04475). Mitigations: both orders, length control, a judge from another vendor.

Design consequences: compose from a small block set; generate checks per request but **ground, filter and track** them;
cache bars and plans; judge across vendors in both orders.

## 3. The compiler in one page

```text
request (+ attachments, project fingerprint, taste, memory)
  │
  ├─1 Deliverable sheet   (lead, one turn)  pieces, where each will live, audience, facts, risk answers, questions
  ├─2 Bar                 (code; bar finder from the other vendor only on a library miss)
  │                        expert checks per piece, each grounded, typed program / judge / gate, with severity
  ├─3 Proofs              (code)  each check → a proof: a program with a threshold, or a blind judge rubric, or a gate
  ├─4 Steps               (lead writes craft steps; code inserts moves, back-chains from proofs, prunes, orders)
  ├─5 Loadouts            (Kits assembler per step: vendor, model, effort, role, tools, budget)
  ├─6 Validate + estimate (code)  every check has evidence and an owner; every step feeds a check or a deliverable
  ▼
start card (plan, time, quota, questions with defaults, assumptions, tool gaps)  →  run  →  back edges · partial reruns · gates
  ▼
compile receipt + check outcomes → Bar library and recipes learn
```

Four rules (GP-1 to GP-4):

1. **The bar comes before the plan.** Steps exist to pass checks. A step that feeds no check and no deliverable is cut.
2. **The bar comes from someone who will not do the work.** Library bars are chosen by code; new bars are drafted by
   the vendor that will not produce the piece when both are connected. The lead cannot delete a check; it may mark one
   "not applicable" with a reason that the final judge sees.
3. **Models judge, code weighs.** The lead writes the sheet and the domain craft steps; code inserts moves, owners,
   order, gates and loadouts. This is the Auto router's measured lesson (81 of 88 picks with a model labelling and a
   table choosing, against 76 of 88 with a model choosing).
4. **Every check ends in pass, fail or not run.** "Not run" is reported with its reason and never counts as a pass.
5. **Every answer Faber found by itself is shown.** Few questions must not mean silent guesses. The start card lists
   each assumption that switches a check group or shapes a deliverable, with where it came from (the request, a project
   file, a saved answer, taste): "Assumed: card payments (saved answer), web only (no mobile project found)". Each has
   one click to change it, which recompiles only what depends on it. In autonomous mode the list goes into the end
   report under "decided without you". (Erick, 2026-10-12, E15.)

## 4. Building blocks: the 17 moves and the 15 powers, reconciled

The 17 TL-ADE moves and the roadmap's 15 powers overlap. They become one set with three kinds of block, and the 15
powers stay the user-facing list (GP-5). No new power is needed.

| TL-ADE move | Power it belongs to | Becomes |
| :--- | :--- | :--- |
| 1 Ask only what changes the result; defaults never block; at most 3, never what the request already says | 1 Ask only what changes the result | Move `ask` |
| 2 Learn from real outliers | 2 Learn from what worked | Move `outliers` |
| 3 Diverge cheaply before converging | 3 Cheap draft before expensive work | Move `diverge` (isolated drafts or probes) |
| 4 Test where it will live | 4 See it where it will live | Move `in-place` |
| 5 Programs do the deterministic part | 8 Facts with sources (numbers by a program) | Rule: programs measure, models judge; move `measure` |
| 6 Protected invariants checked by a program | 6 Better than the original | Move `guard` |
| 7 Cross-vendor blind judges; original wins ties; writer never approves; pairwise comparisons swap the order | 5 Another model checks; 6 | Move `judge`; rule: the writer never approves |
| 8 Back edges to the owner; reruns owner and dependents | 9 Redo only the part that changed | Rule: owners and partial reruns (section 9) |
| 9 Lines compose lines | "Powers, not tracks" | Rule: composition (section 8) |
| 10 Human gates with defaults; while a user action is pending, everything independent is delivered | 1, guardrails 3 and 6 | Move `gate`; rule: partial delivery (section 10) |
| 11 Honesty limits | 8 Facts with sources; guardrail 4 | Move `source`; rule 4 above |
| 12 Taste memory | 12 Remember taste | Rule: taste feeds bar, drafts and defaults; move `taste-capture` |
| 13 Domain-expert checks | 7 Proof before work (its content) | The Bar (section 6) |
| 14 Risk-conditional checks | 15 Safe to sell; 1 | Bar risk groups (section 6.4) |
| 15 Fact-level reruns after delivery | 9 Redo only the part that changed | Fact sheet (section 5.4); rule: after delivery (section 9) |
| 16 Fast path for a small tweak | 13 Right-sized | Rule: fast path (section 9) |
| 17 Secrets stay out; the person publishes and buys | guardrails 1 and 3 | Rule: never the real key (section 10) |
| Page frame gain "continues where it stopped" | 10 Pick up where it stopped | Rule: every step durable (MVP journal) |
| Legend "AI inside an app" (Blender, Photoshop, Premiere) | 11 Drive the user's apps | A tool capability chosen by Kits |
| (none) | 13 Right-sized | Loadouts (Kits spec, section 6.1) |
| (none) | 14 Don't reinvent | Move `reuse` |

**The block set.**

- **Moves (step blocks the compiler inserts):** `ask`, `outliers`, `reuse`, `diverge`, `in-place`, `measure`, `guard`,
  `source`, `judge`, `gate`, `taste-capture`. Plus `craft`: the domain production steps the lead writes ("block out
  over the six views", "assemble the edit").
- **Rules (always on, never steps):** bar before plan; bar from a non-producer; models judge, code weighs; programs
  measure, models judge; the writer never approves; owners and partial reruns; after delivery; fast path; partial
  delivery; never the real key; composition; honesty; durability; right-sizing; taste.
- **Check groups:** Safe to sell, risk groups, protected invariants, honesty checks.

Proposed wording changes to five powers are in section 16. They sharpen the powers; they do not add any.

## 5. Data shapes

### 5.1 Deliverable sheet (lead, first turn)

```json
{
  "pieces": [
    {"id": "video", "type": "youtube-video", "lives_in": "YouTube, desktop and phone, long-form",
     "made_from": ["recording"], "protected": []},
    {"id": "title", "type": "video-title", "lives_in": "YouTube home feed at phone width", "pairs_with": ["thumb"]},
    {"id": "thumb", "type": "thumbnail", "lives_in": "YouTube home feed, about 168 px wide on phones"}
  ],
  "audience": "people with an old PC who want it faster for little money",
  "facts": ["channel name", "the video's promise", "parts and prices shown"],
  "risk": {"public": true, "others_data": false, "money": false, "minors": false, "regulated": false,
           "commercial": true, "irreversible": ["publish"]},
  "user_actions": ["record the footage"],
  "questions": [{"text": "Must the edit keep the benchmark section?", "options": ["keep", "may cut"],
                 "default": "keep", "changes": ["bar:video.must-keep"]}],
  "taste": ["likes bold high-contrast thumbnails", "dislikes red arrows"]
}
```

- `type` is normalized by an alias table (`yt thumbnail`, `miniatura` → `thumbnail`) shipped as data, like Kits labels.
- `risk` generalizes the Software line's questions (three in the map, four fields in its code: who uses it, does it
  hold other people's data, where it runs) to every domain: public, other people's data, money, minors, regulated field (health, law, finance),
  commercial use, irreversible effects. Answers are inferred from the request and the fingerprint; a question is asked
  only when an answer is unclear and switches a check group (at most 3 questions, each with a default).

### 5.2 Bar check

```json
{
  "id": "thumb.read-small", "piece": "thumb", "attribute": "thumb.story.legibility",
  "check": "At 168x94 px among 11 real thumbnails from the niche, a viewer tells who, against what and where at a glance",
  "kind": "program+judge",
  "evidence": "feed mock render (move in-place)",
  "pass": "both judges, both orders",
  "severity": "blocking",
  "ground": {"type": "evidence", "ref": "outliers collected by step S2"},
  "also": [{"type": "standard", "ref": "https://support.google.com/youtube/answer/72431", "checked": "2026-10-12"}],
  "from": "library:thumbnail@4"
}
```

- `kind`: `program` (a measurement with a threshold), `program+judge` (a program prepares the evidence, a model
  judges), `judge` (a blind rubric item for a model from another vendor), `gate` (a person decides).
- `severity`: `blocking`, `major` (one repair attempt, then reported), `minor` (reported).
- `attribute` links the check to the step that sets that attribute (its owner, section 9).

### 5.3 Blueprint node

```json
{
  "id": "S7", "block": "craft", "piece": "thumb", "does": "final thumbnail render",
  "inputs": ["S5.scene", "title.final"], "sets": ["thumb.composition", "thumb.text", "thumb.color"],
  "labels": {"verb": "produce", "needs": ["app:blender", "media:image"], "risk": "low", "fresh": "stable"},
  "must_pass": ["thumb.read-small", "thumb.taste"], "cost_class": "expensive", "gate": null
}
```

`labels` are exactly the Kits labels, so the Kits assembler turns each node into a loadout without another model call.

### 5.4 Fact sheet

One file per mission, `.faber/missions/<id>/facts.json`: every fact that more than one piece uses (price, size, names,
dates, numbers, claims), each with its source and the pieces that read it. Pieces read facts by key; they never copy
them. A changed fact reruns only the pieces that read it.

## 6. The Bar: expert checks for any domain

### 6.1 How a bar is made

1. **Library lookup (code, milliseconds).** By piece type, where it lives, and risk answers. A hit gives the checks,
   their grounds and their track record.
2. **Bar finder, only on a miss (GP-6).** Two drafters work in isolation, on mid-size models, with read-only web access
   and no repository access. When both vendors are connected, they are one per vendor, and the vendor that will
   produce the piece never drafts alone. Each gets the sheet and this instruction:

   > You are the most demanding expert reviewer of a {type} that will live in {lives_in}, for {audience}. List up to
   > 12 things you would check before accepting it. Each check names the attribute, says how to check it (a program
   > or measurement, a comparison, or your judgement), states what passes, and cites its ground: a standard or
   > official specification (URL), the tool's official documentation, real examples you can point to, or the user's
   > own requirement. An expert habit with no source is allowed; mark it "judgement" and say why in one line.

3. **Merge (code).** Normalize attributes, drop duplicates, mark checks both drafters found as "agreed".
4. **Ground validation (GP-7).** Each URL is fetched; one cheap call answers "does this page support this check?" with
   a quote. Unsupported grounds become "judgement". A number (a threshold) without a supporting source is turned into
   a relative check ("loudness even within 1 LU across segments") or dropped.
5. **Critic (another vendor than the lead, one call).** "What would this expert still check that is missing here? Which
   checks do not apply to this request?" At most 3 additions, which go through validation too.
6. **Filters.** At most 12 judged checks per piece and 6 blocking ones; program checks have no cap, since they cost no
   model tokens. Judgement-only checks at most 30% of the bar, and only if
   agreed or confirmed by the critic. A check that contradicts the user's stated requirement is dropped.
7. **Owner checks.** The user's own requirements from the request, project decisions and taste are added with
   `ground: owner`.
8. **Risk groups** switch on by the sheet's risk answers (section 6.4).

### 6.2 Grounds, strongest first

| Ground | Example | Rule |
| :--- | :--- | :--- |
| `standard` | WCAG 2.2 SC 1.4.3 contrast 4.5:1 and 3:1; Apple Podcasts about -16 LKFS, true peak ≤ -1 dBFS; OWASP Top 10:2025 A01 broken access control; Core Web Vitals LCP ≤ 2.5 s, INP ≤ 200 ms, CLS ≤ 0.1; glTF 2.0 and its validator | Fetched live and dated; re-checked every 90 days and at use when older |
| `official` | the tool's own docs (tModLoader, Blender "Apply Rotation & Scale", a printer's file guide for bleed) | Same |
| `evidence` | outliers collected in this run; a recorded failure (Erick's map: a "cheap" GPU listing with 6 defects at the end of its description) | Linked to the run or incident |
| `owner` | the user's words, project decisions, taste notes, Erick's TL-ADE map | Never pruned automatically |
| `library` | a check inherited from the library with its own ground and track record | Keeps the original ground |
| `judgement` | an expert habit with a one-line reason | Capped; pruned when it catches nothing |

Grounds age. Example: YouTube's live thumbnail help now recommends 3840×2160 with a 640 px minimum width, while older
versions said 1280×720 (read 2026-10-12, support.google.com/youtube/answer/72431). A dated, re-fetched ground catches
that; a hand-written line would not.

Some fields have no standard. Logo minimum size, for example, varies by brand guide. Then the check is relative ("the
mark reads at 16 px and in one colour") with `judgement` or `evidence` ground, never an invented number.

### 6.3 Caching and learning

- **Bar library (GP-8).** Entries keyed by piece type, context and risk answers. Scope: project
  (`.faber/bars/<type>.json`), then global after the same entry wins in 3 or more projects (the existing rule R51),
  then the manifest when the maintainer's benchmark confirms it.
- **Seeds.** The TL-ADE lines' checks become library entries with `ground: owner` (Erick's map). The map's 26 lines
  give about 120 checks: six-view heights and silhouette match for 3D, last-line reading and total cost for listings,
  money paths and cross-user access for public sites, title against finished video, "who, against what, where" on
  thumbnail sketches at feed size, a title that completes the thumbnail and never repeats it, two copies of one story
  counted as one source, text set as real text and never generated into an image, an AI voice only when asked and
  already approved, loudness measured before mixing, protected areas unchanged for image edits, numbers recomputed for
  data, and so on.
- **Track record per check.** Applied, caught (failed and the fix was accepted), false alarm (failed but overridden by
  the user or by both judges), miss (a defect in this attribute found later by the user or a later step).
- **Learning rules.** A miss adds or strengthens a check, grounded in that incident. A `judgement` check with no catch in
  30 applications, or more than half false alarms over 10, is pruned. `standard`, `owner` and risk-group checks are
  never pruned automatically: they are rare and essential.
- **Plans are cached too.** A finished Blueprint becomes a recipe keyed by its sheet signature (piece types, contexts,
  risk answers). On a similar request the lead adapts the recipe instead of writing from scratch, which is cheaper and
  faster. It stays a hint, never a track.

### 6.4 Risk groups (examples)

| Risk answer | Checks switched on (ground) |
| :--- | :--- |
| public web page or app | cross-user data access test (OWASP A01); WCAG 2.2 AA contrast and 24 px targets; Core Web Vitals at the 75th percentile on a phone; broken links; search basics (official search-engine guidance); the cookie notice where the law requires it; work ends on a test address and Faber never holds the real site's key (section 10) |
| money moves (checkout, booking, invoices) | every money path as an executable test written before the build (Erick's map); for subscriptions, every billing state as a test (trial, upgrade, downgrade, cancel, failed payment, refund) and payment webhooks safe to replay (the provider's official docs); invoices follow the user's country rules (official) |
| other people's data | data stored only where stated; access by owner only (tenant isolation tested); export and delete paths work; a restore from backup actually run once; outgoing mail authenticated (SPF, DKIM, DMARC: RFC 7208, 6376, 7489) |
| installed on people's devices (desktop, mobile, CLI) | builds and a smoke test on every target OS; signed installer and update signature checked; no visible console windows; idle memory and battery within a stated budget; survives sleep, offline and restart; least permissions; for mobile, the store's official review guidelines |
| commercial use | Safe to sell: secrets, known vulnerabilities (OSV), licenses of everything used (SPDX) |
| public text or claims | no invented prices, testimonials or facts; every claim tied to a source (IFCN principles of source transparency) |
| editing or transforming an existing artifact (edit, translation, re-cut) | protected parts (faces, logos, text, names, numbers, links, frozen script parts) unchanged by program diff; original wins ties |
| a real person's voice or likeness | only a voice or likeness the person approved; a synthetic voice only when asked (owner, Erick's map) |

## 7. From the bar to the steps

The lead writes the `craft` steps per piece. Code then inserts moves by rule, back-chains from the proofs, prunes and
orders (GP-9).

| Trigger in the sheet or bar | Moves inserted |
| :--- | :--- |
| A piece competes for attention (feed, marketplace, search, listing, ad) | `outliers` first: top and bottom examples, baseline, why they worked |
| Anything made from scratch | `reuse`: the user's past work and licensed ready work, searched first |
| A piece has taste or fit checks and an expensive producing step | `diverge`: 3 to 10 cheap drafts or probes, isolated, from both vendors when connected, each under a different frame (borrow from another field, invert the problem, the customer's place, a tight rule); the user's own ideas written first when they have some, so they are not anchored; near-duplicates merged by a program; `in-place` + `judge` on drafts; for idea-type pieces, per finalist the closest existing thing with a link and a pre-mortem; the other vendor ranks drafts in a pairwise tournament, each pair in both orders, into a short list of 3 to 5, not one winner; `gate` pick (default: the top of the list) — all before the expensive step. Back edges: too few distinct drafts → a new round unlike the first; everything already exists → a round from distant fields |
| A check needs the real context | `in-place`: phone-size feed ("who, against what, where" at a glance), page as image, print proof, read in place, small and one colour, used as a hurried, novice or distracted customer |
| A check can be measured | `measure` by a program; numbers recomputed independently |
| The request edits or transforms something that exists (edit, translation, re-cut) | `guard` (program diff of protected parts) and `judge` against the original, which wins ties |
| A piece states facts | `source`: open the real pages, store quotes, record per fact who confirms, who disagrees and from when; copies of one story count as one source; a program checks the quotes exist and recomputes any arithmetic |
| The work acts outside (moves files, fills forms, works on a site) | effect journal: look at the current state first, log every action, re-read the state before any retry so nothing is done twice, and report what was done and what was not |
| A piece may be changed later | deliver its editable source with it (layers, scenes, stems, project file), and keep the losing drafts |
| An irreversible or outside effect (publish, buy, send, release) | `gate` with a safe default: stays a draft or on staging |
| The user must act (record, sign in) | `gate` of type user action; only dependent steps wait |
| Every piece | a final cross-vendor `judge`; `taste-capture` after delivery |

**Back-chaining.** Each check needs evidence; each evidence needs a producing step; each step needs inputs. The
compiler walks back from the checks, so every step exists because a check or a deliverable needs it. Steps the lead
wrote that feed nothing are cut, with a line in the compile receipt.

**Kill early (GP-10).** A check whose attribute is decided before an expensive step also runs on the cheapest artifact
that already carries that attribute: silhouette readability on sketches before Blender, money paths as tests before
code, the title against the thumbnail before the edit, the sketches in a real feed before any 3D. An expensive step
starts only after every check that could fail earlier has passed earlier.

**Draft probes (E8).** A draft is a probe in the Kits sense: the smallest version of the piece that can still fail
the fit checks (one screen at phone width, a 6-second animatic, a low-poly blockout, the opening paragraph, one slide).
Sizes XS and S as in the Kits spec. When a full piece later fails a check its draft passed, the miss is logged against
that draft size and piece type; at more than 2 misses in 10, that type moves to size S drafts.

**Order.** Fact sheet first; longest dependency path first (MVP rule); cheap before expensive; programs before judges;
the other vendor's judge last for each piece.

## 8. Composition: requests that span several domains

- **Piece graph.** A request becomes pieces with relations: the title pairs with the thumbnail, the video is made from
  the recording, the page uses the brand, the slides use the numbers.
- **Fact sheet** (section 5.4) is the single source of truth across pieces.
- **Cross-piece checks (GP-11).** For every pair of pieces that share a fact or a relation, the compiler adds an
  "agree" check: the title's promise matches the finished video; the page uses the brand's tokens; each slide claim is
  supported by the computed numbers; the product size is the same in the 3D model and the photos; the ad price equals
  the fact sheet. A program when the facts are structured, a judge otherwise.
- **Sub-blueprints.** A piece whose type has its own bar compiles as a sub-blueprint (the hub lines of the TL-ADE).
  Depth at most 2; one budget and one fact sheet; the parent owns the cross-piece checks.

## 9. Back edges, owners and partial reruns

- **Owners.** Every attribute has exactly one step that sets it (`sets`). A failed check goes to that step, with the
  check and its evidence. Nothing else reruns unless its inputs changed. In the self-development MVP the owner is the
  holder of the lease on the failing path.
- **Repair budget.** One focused repair, then one diagnostic escalation, sharing the same caps (MVP spec, section 7).
- **Escalate upstream.** If the owner fails the same check twice, the defect moves to the step that set the owner's
  brief (a bad thumbnail idea goes back to the idea, before any more Blender time), when that is cheaper than another
  attempt. The move is logged as a decision.
- **Unclear owner.** A judge finding without an attribute gets one cheap diagnosis call that maps it onto the owner
  map; if still unclear, the most downstream owner tries first.
- **Change requests.** "Cut less of the coffee part" or "the price is now R$ 89" map to attributes or facts: by code
  when the change names a piece or fact, otherwise with one cheap call. Owners and the dependents whose inputs changed
  rerun; everything else keeps its receipts.
- **Content addressing.** Each step's output is keyed by its brief, inputs, loadout (tool versions, model) and base
  revision. Identical keys reuse the earlier result.
- **After delivery (GP-17; the map's default and its rule 15).** The mission stays open in the same conversation, and
  the user never repeats themselves. A follow-up first answers or re-filters from what the mission already collected
  (sources, listings, outliers) and searches only for what is missing ("must have a warranty" re-filters the listings
  already read). A changed fact reruns only the pieces that read it: new shop hours redo only the texts that show the
  hours; a spelling fix in the title redoes only the description. A changed look never reruns the math. "Swap to the
  second draft" restarts from that kept draft.
- **Fast path (GP-16; the map's rule 16).** A request or follow-up that changes one attribute or fact of an existing
  piece, is reversible and switches on no risk group skips the questions, the bar finder, the critic and `diverge`. The
  owner and its dependents rerun with the checks that read the changed attribute, the protected-parts guard and the
  domain's mandatory proofs (for software, the AGENTS.md proof set). A failed check leaves the fast path and the
  request compiles in full. Targets in section 13.

## 10. Human gates and taste

| Gate type | Default | Normal mode | Autonomous mode |
| :--- | :--- | :--- | :--- |
| Pick (directions, drafts, titles) | the judges' pick, or the option taste predicts | waits; other work continues | default at once, logged |
| User action (record, sign in) | none possible | dependents wait; every independent piece is finished, checked and delivered now | same, then parked with a note |
| Preview (before an expensive final render or export) | proceed | shows the preview; continues unless the user asks for a change | proceeds |
| Effect (publish, buy, send, release) | safe: stays a draft or on staging; Faber never holds the real key | asks (guardrail 3) | parked (guardrail 3) |
| Direction, taste in identity, money, legal | the safest option | asks | parked |

- **Partial delivery (map rule 10).** While a user action is pending, every piece that does not depend on it is
  finished, checked and delivered, with a note of what waits (the YouTube case: idea, title, script and shot list
  before the recording). When the input arrives, the mission continues from that step.
- **Never the real key (GP-18; map rule 17; guardrails 1 and 3).** No session, tool or MCP server receives the
  production deploy key, the payment account, a store-publishing account or the admin login of a real site or app.
  Work ends on staging or a test address, with a handover package (for a client site: steps to put the domain and
  hosting in the client's name, the code, a manual and the report of what was measured). The person publishes, buys
  and releases.
- **Fewer questions with use.** While taste predicts fewer than 8 of the user's last 10 picks for a piece type, a pick
  gate shows 3 options. Above that, it shows the recommended one with "see the others".
- **Taste feeds four places (GP-12):** owner-grounded checks in the bar ("no red arrows"); the directions seeded for
  `diverge`; the judges' rubric (judges see taste notes, never who made what); and gate defaults.
- **`taste-capture`** after delivery: one tap or one sentence, stored per piece type in the project, global only with
  the user's approval (R51).

## 11. One compiler: steps, tools, proofs and models together

- Every node carries Kits labels; the Kits assembler gives each node its loadout (vendor, model, effort, role, tools,
  context budget). Drafts and probes run on cheaper models; final pieces on stronger ones (right-sized).
- **Checks create tool needs (GP-13).** A program check names the measurement it needs (`op:loudness-measure`,
  `verify:screen`, `op:image-diff`, `op:silhouette-compare`, `verify:web-e2e`). These become `after` check cards in the
  kit, and a missing one enters the Kits gap pipeline (catalog, search beyond, install within bounds). If no program
  can be had, the check falls back to a judge and is marked weaker in the report (rule 4).
- **Judges.** The judge's vendor differs from the producer's; creative pieces get two judges from different vendors
  (guardrail 5), both orders, a tie when they disagree, length-controlled presentation. Every pairwise comparison
  (drafts, tournaments, edit against original) runs in both orders.
- **The estimate** sums the loadouts' expected cost and time per piece and goes on the start card.
- **Toolbox.** `mission.start` takes a Blueprint; `proof.define` is filled from the bar; the lead may add proofs, never
  remove a mandatory one.

## 12. Worked examples

**Full YouTube video ("make my next video about upgrading an old PC").** Pieces: idea, title, thumbnail, script,
recording (user action), edit, delivery. Library bars exist for all. Moves: `outliers` for idea, title and thumbnail
(one shared collection); `diverge` for ideas (isolated, both vendors), titles (10, written with the thumbnail) and
thumbnail sketches (6 to 10 ugly ones); `in-place` feed tests on phone width; a gate for recording; cross-piece checks
title↔thumbnail, title↔finished video (the line's check), script↔edit must-keep parts. Back edge: the title check after
the edit returns to the title step only; the edit does not rerun. Matches the line's 11 stations and its one back edge,
and adds: the shared outlier set (one collection instead of three), the fact sheet for the parts and prices shown, and
a loudness check on the final mix. Before the recording arrives, idea, title, script and shot list are already
delivered; a spelling fix to the title later redoes only the description.

**3D model ("a stylized fox for my indie game, rigged, 2k triangles").** Bar from the library (Erick's map) plus the
validated grounds: six-view turnaround with eye, shoulder and foot heights aligned (program+judge); silhouette match of
the model against the six drawings (program); triangle budget 2,000 (program, owner ground); applied rotation and scale
before export (Blender manual); the exported glTF passes the Khronos validator (standard); the rig deforms cleanly in
three test poses (judge). Kill early: silhouette readability on the 6 to 8 sketches before any blockout.

**Purchase ("a used RTX 4070 under R$ 2,500 in São Paulo").** Bar: every finalist's listing read to its last line
(evidence: the map's GPU with 6 defects at the end); compatibility from spec tables, not memory (official); total cost
with shipping and tax (program); seller history read; photos match the listing (judge). Moves: price probe first (warn
if the budget cannot buy new), listing sweep (program), `source` for each claim, three options plus what was discarded
and why, a buy gate that never defaults to buying.

**Novel: a wedding invitation, print and phone.** No line exists. Bar finder: bleed and safe margin from the chosen
printer's file guide (official, since no standard sets them), PDF/X output declared (standard), names, date, time and
address frozen from the fact sheet and checked by program in both versions, the RSVP link opens, text legible at phone
width (in-place), the two versions agree (cross-piece). `diverge`: three directions as one-page drafts; pick gate with
the couple's taste as default.

**Novel: a Terraria mod that adds a fishing rod.** Bar finder plus the Kits field brief: the mod builds against the
current tModLoader (official docs); the item loads without errors in a test world (program, if the game is installed;
otherwise reported as not run); the sprite matches vanilla item size and style (judge with vanilla references from
`outliers`); stats balanced against comparable vanilla rods (evidence). The game itself is a user app, never installed
automatically (Kits section 12).

## 13. Speed and cost targets for compiling a plan

| Case | Time to the start card | Cost (API-equivalent) |
| :--- | :--- | :--- |
| Fast path (section 9): one attribute or fact of an existing piece, reversible, no risk group | ≤ 1 s added; no extra model call | ≤ US$0.001 |
| Library hit | p50 ≤ 30 s, p90 ≤ 60 s | ≤ US$0.15 |
| Library miss (bar finder) | p90 ≤ 180 s; questions appear within 30 s so the user answers while the bar is built | ≤ US$0.40 |
| Composition (3 or more pieces) | p90 ≤ 180 s | ≤ US$0.60 |
| Compiler code (merge, owners, order, prune, loadouts) | ≤ 200 ms for 200 nodes | 0 |
| Replan after a change | ≤ 5 s when the change names a piece or fact; ≤ 15 s otherwise | ≤ US$0.01 |
| Planning share of a mission's own cost | ≤ 5%; ≤ 10% for a new domain | — |

The lead answers within the roadmap's 2 s first-token goal with a one-line acknowledgement; the plan follows.
Measured by a compile receipt with a timestamp per phase (sheet, bar, critic, steps, code) and own-cost ledger tags
(`plan.sheet`, `plan.bar`, `plan.critic`, `plan.compile`), reported in every benchmark run.

## 14. Proving it beats the TL-ADE

The roadmap's benchmark of about 40 real requests becomes this set (GP-14): one request per TL-ADE line, plus 14 that
no line covers. The roadmap line "Measured against the TL-ADE" becomes the protocol below. No map line is emulated as
a fixed pipeline (E11). Plans are compared with the map's stations and checks (nearly free); results are compared with
the plain CLI on small prototype probes; and the real TL-ADE Software line runs only in the head-to-head of section
14.8, inside the US$25 envelope (E14).

### 14.1 The requests

Set A, one per line (26 lines; section 14.5):

| Line | Request | The line's strength that must be matched |
| :--- | :--- | :--- |
| Quick question | "Can a 650 W power supply run an RTX 5070 with a Ryzen 7?" | every fact has its source |
| Translation | "Translate this restaurant menu PDF into English for tourists" | numbers, names and links unchanged; read in place |
| PC tasks | "Organize my Downloads folder by type and year; delete nothing" | every action logged, never repeated blindly |
| Learning | "Teach me SQL joins this week; I know Excel well" | exercises with grading |
| Title | "Titles for my video fixing a dead laptop" | written with the thumbnail, tested in a real feed |
| Edit image | "Remove the stranger behind me; don't touch my face" | protected areas checked; the original wins ties |
| New image | "A cover image for my post on home coffee roasting" | three cheap drafts; text never warped |
| Audio and voice | "Clean this interview; there is fan noise" | measured before mixing; says when it could not listen |
| Animated video | "A 45-second explainer of how our app syncs" | scenes independent; only a bad scene reruns |
| Research | "Is intermittent fasting better than calorie counting? Evidence only" | one story counts once; no overclaiming |
| Ideas | "Side-business ideas I can run with one 3D printer" | isolated divergence; lookalike and risk per idea |
| Purchase and price | "A used RTX 4070 under R$ 2,500 in São Paulo" | reads each listing to the end; total cost |
| Text and script | "The script for my 8-minute video on budget gaming PCs, in my voice" | a second model removes the AI feel |
| Data and spreadsheets | "Why does our March sales sheet not match the bank?" | a program computes and recomputes |
| Documents and slides | "This project report as a 10-slide board deck" | every page seen as an image |
| Video idea | "What should my next video be? My channel repairs retro consoles" | ideas from real outliers in the niche |
| Thumbnail from scratch | "A thumbnail for my video restoring a yellowed Game Boy" | idea proven on a sketch before Blender |
| Video editing | "Edit this 40-minute recording into 12 minutes with captions" | a change redoes only its segment |
| 3D model | "A stylized fox for my indie game, rigged, 2k triangles" | a program measures model against drawing |
| Brand and logo | "Logo and brand kit for my bakery, Pão da Vila" | tested small and in one colour |
| Software | "Add CSV export to the reports page of my app" | test before code; the writer never approves |
| Full YouTube video | "My next video about upgrading an old PC, end to end" | a changed title does not redo the edit |
| Marketing campaign | "A two-week Instagram campaign for the bakery's sourdough" | no invented facts; nothing published by Faber |
| Product launch | "Launch my mechanical keyboard kit: page, renders, video, ads, deck" | a changed price redoes only the copy |
| Data to presentation | "This survey CSV into findings and a 12-slide deck" | a corrected number updates everything |
| Client website | "A website for a dental clinic with online booking" | money paths as tests; stays on staging |

Set B, no line covers them: a Terraria mod with a new fishing rod; a podcast episode edited with chapters and show
notes; a wedding invitation for print and phone; a spreadsheet macro that merges monthly sales files; an 8-page
children's picture book with a consistent character; a board game rulebook with print-and-play cards; a 3D-printable
replacement part from a photo and measurements; an Arduino plant-watering sketch with a wiring diagram; a converter for
an undocumented file format; a Discord bot for a gaming community; an app-store listing (screenshots, text,
keywords); a conference talk package (slides, speaker notes, a 60-second teaser); a 7-day trip itinerary with bookings
left to the user; a résumé and cover letter tailored to one job post.

Each request has two hidden variants written by the other vendor and rotated, so Faber is not tuned to one wording.

### 14.2 Level 1: plan parity (cheap, every version)

- **Map lines as fixtures.** The 26 lines are encoded once, as plans only, in the test fixtures: their stations, checks
  and back edges, from Erick's map. They are never run, and never ship as a path Faber runs.
- **Coverage.** For each Set A request, two judges from different vendors map every station purpose, check, back edge,
  "why better" claim and after-delivery behaviour of the line onto the generated Blueprint: covered, stronger, justified as not applicable, or missing. Target: no missing
  blocking item on any line, and at least 95% of all items covered or stronger.
- **Preference.** The same two judges compare the generated Blueprint and the line, rendered in the same format and
  length, blind, in both orders: "Which plan will produce the better deliverable with less waste?" A tie when the orders
  disagree. Target: wins or ties on at least 24 of 26 lines, and no line lost by both judges.
- **Set B.** Comparators: the nearest TL-ADE line forced onto the request (what a fixed-line system would run) and the
  plan plain Claude Code or plain Codex writes. Target: wins or ties on at least 13 of 14 against the nearest line and
  12 of 14 against the plain plan.

### 14.3 Level 2: result quality on probes (cheap, spread over time)

- **Two arms only:** Faber's generated Blueprint and plain Claude Code or plain Codex, on the same machine, models and
  installed tools. No fixed-line emulation (E11); the real TL-ADE runs only in the head-to-head (section 14.8).
- **Probes, not full deliverables.** Each benchmark request gets XS and S probe versions, as in the Kits spec's Probe
  lane: one screen of the dental-clinic site with its booking path, a 60-second piece of the interview, one slide of
  the board deck, the fox's head in six views, one listing read end to end, one item of the Terraria mod. Probes are
  frozen in the probe bank so later versions are compared on the same inputs.
- **Scoring:** blind cross-vendor judges on both results, in both orders; executable proofs; the matching map line's
  checks and the generated bar applied to both results (the map's expert checks score Faber without running the
  TL-ADE); tokens at API rates; wall time.
- Human steps use frozen fixtures: recordings and photos are prepared files, picks take their defaults, purchases stop
  at the buy gate. Lines that named paid apps (Photoshop, Premiere) use the tools the machine has, chosen by Kits, for
  both arms.
- **Seeded-defect probes.** A probe repository carries planted defects of known classes (a replayed payment
  webhook, a cross-tenant read, a silent behaviour change in a refactor, a deprecated API in an unfamiliar stack, a
  console window on Windows, a near-typo package name, a keyboard trap). Both arms work on it; a program counts the planted defects each one caught or shipped.
  For each class, the judges also record whether a check of the matching map line would have caught it, on paper.
- **Follow-up probes.** For each request whose line has an after-delivery example, one probe applies that change after
  delivery, in both arms. A program counts which steps reran; target: only the owner and its dependents, and the
  delivered pieces otherwise byte-identical.
- **Pace:** the Probe lane's rules (Kits spec, section 15.3): one pair at a time, at least 20 minutes apart, only room
  left under the pace line after real work, never the daily grant, paused while someone else uses the shared account.
- **Targets:** per domain group, wins or ties at least 9 of 10 decisive probe pairs against the plain CLI, at no more
  than 70% of its cost (the roadmap's goal).
- **Carry-over to full size** follows the Kits spec's transfer rules: rates instead of totals, and a per-domain
  calibration against real use (results the user kept or redid, first-try proofs, review findings). A domain whose
  probes do not predict real use is marked probe-weak and reported as such.

### 14.4 Cost of the benchmark

- **Envelope (E14).** All comparisons, benchmark runs and probes together stay under US$25 API-equivalent; above
  that, Faber asks Erick first. Spend is tagged `bench.*` in the own-cost ledger and runs under the pace rules, never on
  the daily grant, paused while someone else uses the shared account. The Kits Probe lane is a product feature under
  its own per-user quota caps (E8) and is not counted here (AI reading of E14).
- **First allocation:** the head-to-head at most US$20 (section 14.8); a plan-parity sample of 8 requests about US$3;
  result and seeded-defect probes about US$2 (at most US$0.20 per pair).
- **Later spend:** plan parity on all 40 requests (about US$20) and the first public version gate (plan parity on all
  40 plus at least 3 decisive probe pairs per request, up to about US$24) exceed what remains. Erick (2026-10-12, E16):
  US$25 is the hard ceiling only until the Claude weekly reset on Tuesday 2026-10-13; these larger tests run after
  that reset, under the pace rules.
- Judges follow the roadmap: a cheap model screens, a strong one breaks close calls.
- Level 0, free: compiler unit tests in `cargo t` on fixture sheets (owners, pruning, order, gates, composition), with
  no model and no process.

### 14.5 How many lines

Erick's map has 26 lines (9 simple, 12 complete, 5 combinations), confirmed by the full translation. An earlier
digest said 27; the map never does. Set A therefore has 26 requests and Set B 14, for 40 in all.

### 14.6 Against a working TL-ADE (Erick's direction E12)

The bar to beat in software is the real Software line, as a read-only code inventory of the TL-ADE repository found it
(main, last commit 2026-10-10). For the other 25 lines, the bar is the map, as if each line were finished. The real
Software line also runs in the head-to-head of section 14.8 (E14).

| Station | What the code does |
| :--- | :--- |
| Questions | 4 fields, saved in the project; they switch the launch checks |
| Affected parts | Partial |
| Test first | A red proof: the test must fail before the change |
| Code, library check | The new-library check (at least 30 days old, at least 100 downloads) runs after the code is written, not before install |
| Quick tests, secret scan | Present |
| Review | Another vendor; the full suite runs in parallel with the review |
| Screen check | Design checks, automated accessibility rules, light and dark themes; no keyboard check and no speed check |
| Launch check | Search basics, cookies, cross-user access, user journeys; no speed check and no broken-link check; none at all for solo projects; site checks need an undocumented address setting |
| Receipt and back edges | A receipt per change; failures loop back to the writer |
| Parallel work | One lane by default |
| Track record | 69 missions; 305 stories done (223 delivered, 81 waiting for the operator); 604 approvals against 386 change requests; dogfood task D1 took 27 s and US$0.28 |

No Client website line exists in the code.

**Where Faber only matches.** The red-proof test first, the receipt, other-vendor review, the suite running in
parallel with the review, the secret scan, saved risk answers and back edges to the writer are already excellent.
Faber seeds them as owner checks and claims a tie there, not a win. The same holds for the finished lines' own expert
checks (six views, listing to its last line, thumbnail sketches in a real feed): Faber matches them.

**Recorded baseline, no run.** Faber runs the TL-ADE's own dogfood task D1 as a probe and compares its time, cost and
proofs with the recorded 27 s and US$0.28. The journals' rates (81 of 305 stories waiting for the operator; 386 change
requests for 604 approvals) are reference points for Faber's own waiting and first-review pass rates, not targets,
because the work differs.

**Where a working fixed line still loses, and what Faber does there (GP-19).**

| Fixed-line limit | Faber's answer | Measured by |
| :--- | :--- | :--- |
| A new library checked only after the code is written | The registry check runs before install: existence, age, downloads, near-typo names, advisories | Seeded probe: a near-typo package is never installed |
| No keyboard or speed check on screens | Keyboard reachability, focus, and speed measured on every changed screen | Seeded probe: a keyboard trap and a slow screen |
| No launch checks for solo projects; no speed or broken-link check; an undocumented address setting | Checks by where the result lives, for any audience, including solo: speed, broken links, devices, stores, CLIs, libraries; the address comes from the sheet | Program checks per target |
| The same 4 questions for every request | 7 risk answers inferred, questions only when unclear, domain questions from the bar | Plan fixtures; seeded-defect probes |
| The same checks for every stack and kind of change | Bar by stack (fingerprint) and change kind (feature, bug fix, refactor, upgrade, distribution); bar finder for the rest | Seeded-defect probes |
| No research step: code from model memory | `fresh` labels, field brief from the official docs of the installed versions | Build errors and invented or deprecated API calls per probe |
| One lane by default, fixed order | Back-chained graph, parallel workers on leased paths by default within the pace rules, longest path first, kill early | Wall time per probe at equal or lower cost |
| Screens checked only after the code | Draft probes of new screens judged before code | Screens redone after code |
| No learning | Bar track record, misses add checks, recipes, taste | Repeat-defect rate per defect class over time |
| One piece per request | Fact sheet across code, docs, locales, screenshots, marketing text | Follow-up probes: stale mentions left |
| Fixed tools (Blender, Photoshop, Premiere) | Kits picks from what the user has and what works | Kit receipts; probe pairs with and without |
| 26 foreseen requests, 5 fixed combinations | A plan compiled per request, composition at any depth up to 2 | Set B plan parity against the nearest line |

**Worked comparisons.**

| Request | TL-ADE (working line) | Faber | Measure |
| :--- | :--- | :--- | :--- |
| "A subscription invoicing SaaS for freelancers, ready to sell" | Software line: 4 questions, launch check without speed or broken links; no money paths (no Client website line in code) | Money, data and commercial groups: every billing state tested before code, webhook replay, tenant isolation, export and delete, restore drill, mail authentication, licenses, speed | Seeded-defect probe: replayed webhook, cross-tenant read, cancelled user still billed |
| "Sessions show as stuck after resume" in a large repo | Strong: red proof, other-vendor review, suite in parallel; affected parts only partial | Matches those; adds reproduction from the user's log, every caller of the changed function checked, and the miss becomes a project check | Reopened fixes; regressions in sibling callers; repeat-class rate |
| "Add push notifications to my Elixir Phoenix app" (unfamiliar stack) | Same stations; code from memory; generic checks | Field brief from the installed versions' docs; `mix format`, Credo, Dialyzer, reversible migrations | Build errors and invented API calls per probe pair |
| "Split `route.rs` into modules, no behaviour change" | "A test that passes only when done" does not fit a refactor; falls back to existing tests | Golden outputs captured on the base first; public API and saved-schema diff empty by program; benchmark within noise | Seeded silent behaviour change: caught or shipped |
| "A tray app for Windows and macOS that backs up a folder" | Solo project: no launch checks; screens without keyboard or speed checks | Device group: builds per OS, signed installer and updater, no console window, idle memory, sleep and offline | Program checks per OS on the S probe |
| "Edit this interview as a podcast with chapters" (nearest: Audio and voice) | Cleans and mixes; no chapters, show notes or platform loudness | Bar finder: -16 LKFS and -1 dBFS true peak for Apple, chapters at topic changes, show-note timestamps match the audio | Loudness by program; timestamps against the transcript |
| "A used car under R$ 60,000 near Campinas" (Purchase and price) | Fixed questions (used? warranty? postcode?) and part-compatibility tables | Matches the listing-to-last-line habit; questions and checks from the bar: reference price table, recalls, fines and auction history, an inspection before paying | Plan coverage; a frozen listing with planted issues |

Targets: (a) at the MVP, the software rows above win on their measures against the plain CLI, and the paper mapping
shows that each seeded class Faber catches is either outside the Software line's checks or matched by them; (b) at the
first public version, the same for the non-software rows and all of Set B.

### 14.7 Feature by feature against the TL-ADE dossier (Erick's direction E13)

Source: [TL-ADE feature dossier](../../research/2026-10-12-tl-ade-features.md). "Match" means Faber has the same
property; "then" names what Faber adds. Rows marked MVP need the additions listed after the table (GP-21).

| TL-ADE feature | TL-ADE | Faber | Measured by |
| :--- | :--- | :--- | :--- |
| Journal and crash matrix | Hash-chained, fsynced, torn-tail repair, two-phase effects, resume with lease | Match (MVP); then the kill test runs at every effect boundary on every build (gate G5) and every stop reason has an automatic recovery | G5: no duplicate effect, lost work or manual step |
| Certificate and recertify | Bound to tree, base, contract, proofs and reviewer family; a moved base invalidates it | Match, plus toolchain, dependencies and environment; then a moved base reruns only the proofs whose inputs changed, and the receipt lists every bar check as pass, fail or not run | Planted base move: no stale receipt accepted; recertify time against a full rerun |
| Skills pinned, hashed, frozen at approval, role-scoped | 13 of 14 sources pinned; hash at load; scanned; a lexical pick frozen at approval; at most 4 per part | Match (Kits sections 8 and 12); then per-step picks from measured lift, attestation of what each CLI really loaded, quarantine with a canary, and a gap loop that finds what is missing | 100% sessions attested; a changed hash never loads |
| Per-role tool denial by CLI flags | Per-role flags and sandboxes | Match; then attested per session, and writers cannot commit, push or leave their leased area | Attestation mismatches; escape tests |
| Official quota receipts | Read from each CLI; a refresh costs a model call; stale after 24 h | Match: official readings only; then free readings from real work, a learned weekly shape, the pace line, spare quota spent on quality, the shared-account pause, economy mode (TL-ADE has none of these) | Quota stops per mission; days each weekly quota lasts |
| A quota hit burns no round | Pause, keep the session, retry after renewal | Match (MVP); then the step moves to the other subscription when its loadout allows | Rounds lost to quota events: 0; wait after a hit |
| Stripped worker environment | Removes listed token prefixes | Match and exceed (MVP): workers start from an allow-list, so an unknown token name never passes | A planted token variable never reaches a worker |
| Red proof, other-family review, suite beside review | Working | Match (software bar, section 15.1.1); proofs and review run in parallel per candidate | — |
| Interview | Up to 5 questions, median 4; recommended first; one click accepts all | Match the defaults and one click; then at most 3, only when an answer is unclear and switches checks, shown within 30 s while the bar builds; most requests ask none | Questions per request |
| Closing report; discard into retained refs | Working | Match; then keep or remove and keep or change per decision, and one-click undo of a whole delivery (MVP) | Undo test per delivery type |
| Incident file | Written by hand | A miss adds a grounded check by itself; false alarms are pruned | Repeat-defect rate per class |
| Notifications | None; a stop is seen only with the panel open | Needs-you notification and sound, never taking focus (R92); tray number | Time from a stop to the user seeing it |
| Recovery | About 40 incident lines of hand work | Automatic recovery per stop reason; manual edits quarantined (MVP) | Manual steps per 100 missions: target 0 |
| Install | Developer only: Node, npm, terminal | Signed installer, first run, CLI download to disk | Steps in a terminal from install to first request: 0 |
| Lanes | 1 by default | Parallel workers on leased paths by default, within the pace rules | Wall time per mission at equal or lower quota |
| Missing tools | Error and a pointer to a doctor command | Kits gap loop: catalog, search beyond, field brief, safe install | Gap tier latency and cost |
| Stack detection | JavaScript and TypeScript only | About 40 fingerprint rules for any stack, plus the bar finder | Need recall at least 85% |
| Learning | Stored, never applied | Evidence cells, bar track record, recipes and the Auto loop feed the next pick | Learned picks against priors on the gold set |
| Updater | None; old engines are refused | Signed updater for the installed channel; local self-update with canary and rollback (G6) | G6 |
| Run length and cost | Median 25 min and US$2.91 per story; missions up to 598 min | Right-sized steps, parallel workers, kill early, planning at most 5% | Head-to-head: time and quota at most half of the TL-ADE's (target) |
| Stop reasons | Some shown as "technical reason" | Every reason is a `reason.*` key with a hand-written sentence in each language and its next action | Locale check: no raw reason on screen |
| Undo of a delivery | None; git by hand | Deliveries land on the mission branch; one click undoes a delivered change (MVP) | Undo test per delivery type |
| Codex writing on Windows | Its sandbox blocks child processes, so it cannot run tests | Faber already forces `windows.sandbox="unelevated"` (`engine/src/codex.rs:26-35`); Codex workers run tests, so the TL-ADE's lesson 3 does not apply | Screen suite runs a Codex writer's proofs on Windows |
| Follow-ups during a run | Blocked | Accepted and mapped to owners (section 9) | Follow-up probes |
| Money on screen | API US$ shown to subscription users | Quota share and pace, never dollars, for subscription users | — |
| Judge scale | 0 to 10, drifted (an uglier thumbnail scored higher) | Pass or fail checks and pairwise comparisons in both orders; no absolute scores | Swap consistency |
| Paid probes | 30-minute timeouts; orphan probes overwrote results | Probe lane: at most 5 minutes, one at a time, frozen bank, preempted pairs discarded | — |
| One template for every kind of work | The software cycle applied to creative work (review of 27/09) | A plan compiled per request | Set B plan parity |

**Proposed additions to the self-development MVP spec (not applied here):** the mission journal is hash-chained,
fsynced, repairs a torn tail and records every outside effect in two phases (intent, then result); every stop reason
maps to an automatic recovery, and manual edits quarantine the attempt; a moved base reruns only the affected proofs; a
quota event never counts as an attempt; workers start from an allow-listed environment; proofs and review run in
parallel; one click undoes a delivered change; follow-ups are accepted during a run; every stop reason is a locale key
with its next action.

### 14.8 Head-to-head with the real TL-ADE Software line (E14)

A small real comparison on software tasks, inside the envelope of section 14.4 (GP-22).

- **Tasks:** 10 at size S, two per kind: feature, bug fix, refactor, unfamiliar stack, user interface. Eight are in
  small TypeScript web repositories, the TL-ADE's home stack, so it is not handicapped; the two unfamiliar-stack tasks
  are in Go and Elixir Phoenix. Repositories are frozen copies neither system has seen.
- **Planted defects:** each task carries one or two traps from the seeded classes: a sibling caller with the same bug,
  a near-typo package that the obvious solution would add, an undocumented edge case a refactor must keep, a keyboard
  trap and a slow list in the UI task, a deprecated API in the unfamiliar stack. Held-out acceptance tests, unseen by
  both systems, check every task.
- **Same conditions:** the same request text; one frozen answer sheet for questions (the recommended option unless the
  sheet says otherwise); the same machine and subscriptions; each system on its own defaults (the TL-ADE with 1 lane
  and its own routing), unmodified, at its last commit. The order alternates per task; one run at a time; only while
  the PC is idle; never a visible window (a TL-ADE run that cannot stay hidden waits for Erick's away time).
- **Judges:** a strong Claude model and a strong Codex model, blind. Diffs are normalized (formatting, commit text and
  tool files stripped) and shown in both orders; a tie when the orders or the judges disagree. Rubric: correct, in
  scope, maintainable, well tested.
- **Metrics per task:** defects that escape (held-out test failures, planted traps shipped, blocking judge findings a
  program confirms); rounds (writer and review loops, from each system's journal); wall time from request to delivery;
  quota (official readings before and after, plus API-equivalent tokens); operator touches (every stop that needs a
  person beyond the answer sheet, extra approvals, manual recovery); and the judges' preference.
- **Budget:** each run capped at US$2.00, and a capped run counts as not delivered. Expected about US$1.00 for a TL-ADE
  run and US$0.60 for a Faber run, so about US$16 plus about US$1.50 of judging; the head-to-head stops at US$20 and
  reports what ran.
- **Targets (wide margin, E13):** escaped defects at most half of the TL-ADE's total, and no task where Faber ships a
  defect the TL-ADE caught; operator touches at most half; median wall time at most half; quota at most 70%; the judges
  prefer Faber or tie on at least 8 of 10. A tie is reported as a tie (likely on a plain bug fix in TypeScript).
- **Output:** raw journals and receipts of both systems, kept with the benchmark results. Faber never reads the TL-ADE's
  credentials or copies its code.

## 15. Phasing

### 15.1 Self-development MVP (Phase 1a, target 2026-10-20 to 22)

The MVP's lead already writes a plan, contracts and a graph (MVP spec, section 6). The compiler extends that. Software
is the only domain in the MVP, and it is complete there, not a slice (Erick's decision E10; GP-15):

| Part | MVP version |
| :--- | :--- |
| Deliverable sheet | Pieces are changes, screens, docs; risk answers: the Software line's questions plus money, minors, regulated, commercial and irreversible (section 5.1), saved in the project (for Faber: public users, no other people's data, desktop on Windows, macOS and Linux) |
| Bar | The complete software bar of section 15.1.1; the lead may add checks, each citing a repo document heading or an official URL; code checks that the cited path and heading exist, and fetches cited URLs. For the benchmark's software probes in other stacks (section 14.6), the bar finder runs, software only |
| Moves | `ask` (at most 3, with defaults), `reuse` (product map and repo searched before a new screen, button or setting), `diverge` (2 or 3 draft probes of a new or reworked screen, judged at the real widths before it is built), `measure` (all program checks), `in-place` (the real app at the shell widths, and ghost users), `guard` (frozen contracts, public shapes, saved-data schemas), `judge` (other-vendor review), `gate` (promotion and self-update trust) |
| Compiler code | Extends the MVP graph validator: every check has evidence and an owner; every node feeds a check or deliverable; kill-early order; owner = lease holder of the failing path |
| Partial reruns | The MVP's contract-revision invalidation plus content-addressed receipts |
| Receipts | Compile receipt and per-check outcomes from day one |
| Benchmark | Plan parity for the Software and Client website lines against the map; result probes for software requests against the plain CLI |
| Gate | New MVP gate **G10 Software bar**: every check below has a runnable form and ran on the real goal of G8. If the target date and the complete bar conflict, the bar wins (E10) |

#### 15.1.1 The complete software bar

Program checks (P) cost no model tokens, so they have no cap; judged checks (J, a model from the other vendor) follow
the caps of section 6.1. PJ means a program prepares the evidence and a model judges it. Grounds: `map` = Erick's
TL-ADE map (owner), `AGENTS` and `roadmap` = owner, `MVP` = the self-development MVP spec, others are official sources.

| Group | Checks | Kind | Ground |
| :--- | :--- | :--- | :--- |
| Software line | Risk questions answered and saved, switching check groups; affected callers listed before code; a test that fails on the base and passes on the change, written first; a new dependency really exists on its registry, with publisher, age and downloads, and its name is not a near-typo of a popular package; quick tests and secret scan before review; other-vendor review of the exact composed tree; screen check when UI changed (real app, screenshots, keyboard, colours follow the system theme); full tests (`cargo t --all` when UI or shell changed); save with receipt; small tweaks take the fast path | P, J, PJ | map, MVP |
| Client website line, applied to a desktop app | Money paths as tests: every spend path (live caps, daily grant, admission stop) has an executable test written before the code; new screens drafted and judged at the real widths before code; ghost users from the other vendor use the changed flow as a hurried person, a novice and a distracted person (interrupted mid-flow, at the narrowest width; the map's "distracted on a phone"), plus a keyboard-only person, through the hidden test build; a stuck ghost user sends the screen back to its drafts, a failed spend path back to building only that part; staging: work lands on the mission branch and the next slot, and only the person releases (R93–R95); no agent session ever holds the updater signing key or a publishing account; security, speed and law checks below | P, PJ, G | map, MVP |
| Rust | `cargo fmt --check`; both clippy runs with `-D warnings`, including `await_holding_lock`; no `unwrap`, `expect` or `panic!` on paths reachable from IPC or user input outside tests without a stated reason; no blocking calls inside async tasks; no new `unsafe` without a SAFETY note and review; errors return `error.*` and `reason.*` keys, never sentences | P, J | AGENTS; clippy and tokio docs |
| Processes and data | Child processes never through a shell, npm shims resolved; no visible window (window audit); stop kills the whole process tree; saved data written atomically, versioned, unknown fields kept, readable by the last good build; crash and restart at every effect boundary lose nothing; idle memory test ≤ 250 MB; no new long-lived process without a measurement; credential folders never touched (audit) | P | AGENTS, roadmap, MVP |
| Tauri | The fixed command list and capability files only change with a flagged review; the content security policy is present and not loosened (no `unsafe-eval`, no remote scripts); no remote content and no IPC from remote origins; every command validates and scopes its inputs in Rust; updater signature checks kept and the development channel separate; heavy screens checked on every webview CI runs | P, J, PJ | Tauri 2 security docs; roadmap |
| SolidJS, TypeScript and screen | `npm run lint`, strict `typecheck`, `build`; props never destructured and effects clean up timers and listeners; long lists stay windowed (only the last 40 messages drawn); every visible string in `ui/locales/` with all five languages present, hand-written, and a new language reviewed by a native speaker; only design-system tokens and components; amber only for "needs you" and the locked meaning colours untouched; no duplicate screen, button or setting (product map); coherence pass of every screen side by side at 720, 900 and 1100 px and in each theme | P, PJ, G | AGENTS; Solid docs; `docs/design.md`; R77 |
| Accessibility | WCAG 2.2 AA: contrast 4.5:1 and 3:1, targets at least 24 px, everything reachable by keyboard, visible focus and focus restoration, reduced motion respected, text from 13 to 18 px | PJ | W3C WCAG 2.2; MVP |
| Supply chain and safety | Known vulnerabilities in `Cargo.lock` and `package-lock.json` (OSV); licenses of every dependency allowed (SPDX from `cargo metadata` and package manifests); secret scan on every outgoing change; no telemetry and no new network destination without a logged reason | P | OSV, SPDX; AGENTS |
| Change kinds | Feature: test first. Bug fix: reproduce from the report, then a failing test; every caller of the changed function checked. Refactor: golden outputs captured on the base before any edit; public API and saved-schema diff empty by program; benchmarks within noise. Dependency upgrade: changelog read, breaking changes listed and tested | P, PJ | map; Rust API guidelines; AGENTS |
| Any change | Every bug fix has a test that fails without the fix and names its root cause; edge cases tried on purpose (empty, huge, Unicode paths, offline, timeouts, two windows); public contracts and saved schemas unchanged or versioned; time to first token stays ≤ 2 s; docs and locales updated when behaviour changes; English commit message, one coherent change | P, J | roadmap, AGENTS |

The program checks are Faber's own code (AGENTS.md: original code), so the MVP needs no third-party install for them:
the OSV check queries the public OSV service with package names and versions only, licenses are read from local
metadata, and the registry check reads public registry pages. Ghost users and draft probes reuse the hidden test build
and its screen driver; where a platform lacks that driver, the check runs on screenshots and is marked weaker until it
has one.

### 15.2 Phase 3 (Projects with the lead agent)

All domains: the full sheet; the bar finder with ground validation; all moves including `outliers`, `diverge` with draft
probes, `source` and `taste-capture`; composition with the fact sheet and sub-blueprints; taste in bars, drafts and
gates; bar library learning; recipe reuse; plan parity on all 40 requests each major version; result probes against
the plain CLI, spread over time.

### 15.3 Phase 5, the first public version, and later

The first public version is Erick's "first real version" (E10). It does everything the map's lines were meant to do:
its gate is plan parity on all 40 requests and at least 3 decisive probe pairs per request against the plain CLI
(section 14.4). So the minimum check programs and catalog tools of Phases 6 (image), 8 (video), 9 (PC apps) and 10
(mods) move into Phases 3 to 5, without their screens; free tools stand in where a line named a paid app. Until a
program exists, its check runs as a judge, marked weaker, and that domain cannot pass the gate on weaker checks alone.
The other domains follow the map's build order: after Software and Client website, Thumbnail from scratch comes first;
the rest follow one at a time, each tested before release.

## 16. Proposed roadmap text changes (do not apply in this task)

```diff
@@ Universal by design
 There is no fixed pipeline per kind of request and no list of supported tasks. In Projects, the lead agent is a live
+session of one CLI. For each request, Faber compiles its own plan: it lists the pieces to deliver and where each will
+live, writes down what a demanding expert would check in each (grounded in standards, official docs, real examples or
+the user's own requirements), turns those checks into proofs, and only then picks the steps, tools and models that pass
+them ([spec](superpowers/specs/2026-10-12-generated-pipelines-design.md)).
@@ powers table
-| Cheap draft before expensive work | rough sketches or three quick versions, picked before hours go into the final one |
+| Cheap draft before expensive work | rough sketches, small prototypes or three quick versions, made in isolation by different models, picked where they will live before hours go into the final one |
-| Better than the original or it does not stay | blind side-by-side of the edit against the original; the original wins ties |
+| Better than the original or it does not stay | protected parts (faces, logos, numbers, names, links) checked by a program; blind side-by-side of the edit against the original; the original wins ties |
-| Proof before work | the agent states how success is checked, then runs it; "done" needs that evidence |
+| Proof before work | before planning, the agent writes down what a demanding expert would check, each check grounded; the checks become proofs, the plan is built to pass them, and "done" needs that evidence |
-| Facts with sources | every claim tied to the page that proves it; numbers computed by a program and recomputed |
+| Facts with sources | every claim tied to the page that proves it; programs measure, compute and recompute, models judge; what could not be checked is said |
-| Redo only the part that changed | deliverables are pieces with owners and dependencies; a change reruns the owner and what depends on it |
+| Redo only the part that changed | deliverables are pieces with owners and dependencies; a defect or a change goes back only to the step that owns it, and what depends on it reruns |
@@ Building software
-- **Measured against the TL-ADE.** The same requests run on both: design scores from blind judges, bugs found after
-  delivery, coherence findings, time, tokens and how often the user had to step in.
+- **Measured against the TL-ADE.** The benchmark has one request per line of the map and 14 that no line covers.
+  Generated plans must cover every station and check of the matching line or say why it does not apply, and win or
+  tie blind cross-vendor judges on at least 24 of 26 lines. Results are compared with plain Claude Code and Codex on
+  small prototype probes: wins or ties on 9 of 10, at no more than 70% of the cost. A head-to-head of 10 software tasks
+  against the real TL-ADE Software line counts escaped defects, rounds, time, quota and operator touches. All of this
+  stays under US$25 unless Erick approves more.
@@ Phases
-| 5 | First public version | |
+| 5 | First public version | everything the TL-ADE map's lines were meant to do, in every domain: plan parity on all 40 benchmark requests and probe results against the plain CLIs; the minimum check programs and tools of Phases 6, 8, 9 and 10 come forward, their screens do not |
```

The self-development MVP row proposed in the MVP spec also gains: "with the complete software bar (generated
pipelines spec, section 15.1.1)".

## 17. Risks and what to measure before trusting it

| Risk | Mitigation | Measure before default-on |
| :--- | :--- | :--- |
| Invented or irrelevant checks | Grounds with validation, two isolated drafters, a critic, caps, pruning by track record | Share of checks that ever catch a defect; false alarms per bar; judge-human agreement on 30 sampled bars |
| Over-planning: too many moves for a small request | Fast path; back-chaining cuts steps with no consumer; planning cost targets | Planning share of mission cost; user interventions on simple requests |
| Plans that read well but deliver worse | Result probes against the plain CLI, not only plan parity; real-use signals | Probe win rate per domain; agreement of probes with real use (kept or redone) |
| Probe results that do not carry over to full size | Kits transfer rules: rates, discount, per-domain calibration; probe-weak domains reported | Calibration per domain |
| Judge bias | Two vendors, both orders, ties, same format and length | Swap consistency of judges per run |
| Map lines encoded unfairly as plan fixtures | Encoded from Erick's map; reviewed by the other vendor against the map | Disagreements logged in the fixture |
| The complete software bar slows the MVP | Program checks cost no tokens; judged checks capped; the bar wins over the date (E10) | Time and cost per landed change, before and after G10 |
| Tuning to the benchmark wording | Hidden variants from the other vendor; Set B refreshed each major version | Win rate on variants vs originals |
| Owner loops on unclear defects | Repair caps; one diagnosis; logged escalation | Repairs per mission |
| Checks that need programs Faber lacks | Judge fallback marked weaker; Kits gap pipeline | Share of checks run as weaker judges, per domain |
| Planning delay | Questions shown within 30 s; library hits; recipe reuse | Time to start card, p50 and p90 |

## 18. Questions for Erick

All questions are answered (E10, E11, E14). No open question remains. Any comparison or probe spend beyond the US$25
envelope of section 14.4 goes to Erick first.

## 19. AI decisions in this document

All are AI proposals, not reviewed, except where a row names Erick's decision.

| Tag | Decision |
| :-- | :--- |
| GP-1 | The bar comes before the plan; steps exist to pass checks |
| GP-2 | The bar comes from someone who will not do the work; the lead cannot delete a check |
| GP-3 | Models judge, code weighs: the lead writes the sheet and craft steps; code inserts moves, owners, order, gates |
| GP-4 | Every check ends pass, fail or not run; "not run" is reported, never counted as a pass |
| GP-5 | The 17 moves and 15 powers become one block set: 11 moves plus craft, always-on rules, check groups; no new power |
| GP-6 | Bar finder on a library miss: two isolated drafters (one per vendor), merge, critic |
| GP-7 | Ground validation by fetching the source; unsupported numbers become relative checks or are dropped |
| GP-8 | Bar library scoped project → global (R51) → manifest; track record per check; prune only judgement checks |
| GP-9 | Move insertion by trigger table; back-chaining from proofs |
| GP-10 | Kill early: checks run on the cheapest artifact that carries their attribute |
| GP-11 | Fact sheet and cross-piece "agree" checks for composition; sub-blueprints at depth ≤ 2 |
| GP-12 | Taste feeds bars, drafts, judges and gate defaults; gates shrink to one option when taste predicts 8 of 10 |
| GP-13 | Program checks create tool needs for the Kits assembler; judge fallback marked weaker |
| GP-14 | Within E11 and E14: the 40-request benchmark (26 lines plus 14 novel requests), plan parity against the map, result probes against the plain CLI under the pace rules, hidden variants; no fixed-line emulation; one US$25 envelope |
| GP-15 | Within E10: MVP scope of section 15.1, the complete software bar of 15.1.1 and gate G10 |
| GP-16 | Fast path: one attribute or fact, reversible, no risk group; skips questions, bar finder, critic and drafts, keeps the checks on the changed attribute, the guard and mandatory proofs |
| GP-17 | After delivery: the mission stays open; follow-ups reuse what was collected; changed facts rerun only their readers; partial delivery while a user action is pending; follow-up probes in the benchmark |
| GP-18 | Never the real key: no session or tool holds production deploy, payment, publishing or admin credentials; work ends on staging with a handover package |
| GP-19 | Within E12: the fixed-line limits of section 14.6 with their answers and measures; Faber claims only a tie where a line is already excellent |
| GP-20 | Within E12: change kinds in the software bar; subscription-money and device risk groups; seeded-defect probes |
| GP-21 | Within E13: the feature scorecard of section 14.7 and the proposed MVP additions |
| GP-22 | Within E14: the head-to-head of section 14.8 (10 tasks, planted defects, blind judges, five metrics, at most US$20) |

## 20. Sources

- Repo: `docs/research/2026-10-12-tl-ade-pipelines.md`; `docs/roadmap.md`; `AGENTS.md`;
  `docs/superpowers/specs/2026-10-12-self-development-mvp-design.md`;
  `docs/superpowers/specs/2026-10-12-tool-selection-rework-design.md`; `engine/src/route.rs`.
- Plans and workflows: arXiv 2402.03620 (Self-Discover), 2410.10762 (AFlow), 2408.08435 (ADAS), 2502.04180 (MaAS),
  2504.15257 (FlowReasoner), 2502.04306 (ScoreFlow), 2409.07429 (Agent Workflow Memory), 2506.14852 (Agentic Plan
  Caching), 2503.09572 (Plan-and-Act).
- Checklists and judges: 2410.03608 (TICK), 2507.17746 (Rubrics as Rewards), 2503.05142 (RocketEval), 2505.08775
  (HealthBench), 2404.12272 (EvalGen), 2602.05125 (Rethinking Rubric Generation), 2603.01562 (RubricBench), 2306.05685
  (MT-Bench judges), 2404.13076 (self-preference), 2404.04475 (length-controlled AlpacaEval).
- Grounds: W3C WCAG 2.2 (w3.org/TR/WCAG22); Apple Podcasts audio requirements (podcasters.apple.com/support/893);
  Spotify loudness normalization; EBU R 128 and ITU-R BS.1770-5; YouTube thumbnail help (support.google.com/youtube/
  answer/72431); OWASP Top 10:2025 and ASVS 5.0.0; web.dev Core Web Vitals; MQM (themqm.org) and ISO 17100; PDF/X
  (ISO 15930); Khronos glTF 2.0 and glTF-Validator; Blender manual (Apply); IFCN Code of Principles; OSV and SPDX.
  Read 2026-10-12 by research agents, several from official-page excerpts only.
