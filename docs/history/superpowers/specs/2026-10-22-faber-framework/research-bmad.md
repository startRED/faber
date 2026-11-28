# BMAD: complaint audit and counterevidence

Status: Research for Flow revision 2, consulted on 2026-10-22. Public reports were not reproduced locally.

## Method and limits

Targeted searches covered official docs/changelog, GitHub issues/discussions and first-person community reports.
Topics: ceremony, context, cost, resume, parallel ownership, verification, models, runtime compatibility and domains.
This is a thematic audit, not a census of every complaint or a prevalence estimate. Reports are self-selected.
Closed/stale is not proof of a fix; an open report is not proof that every installation is affected.
The public GitHub API was unreachable from this PC; web page bodies and documentation supplied the evidence.
Current page content takes precedence over stale search snippets. Implementation claims still need reproduction.

Each row links the report, its qualification, the proposed response and a concrete experiment. B identifiers are
research references only, not new product decisions. Sources describe different versions; do not merge them into
one fictional current BMAD release. No code, prompts or third-party assets were copied.

## Twelve problem classes

| ID | Observation and evidence status | Proposed Flow response | Experiment |
| :--- | :--- | :--- | :--- |
| B01 | [#1818](https://github.com/bmad-code-org/BMAD-METHOD/issues/1818), March 2026: specification/context overhead; later closed as stale after architecture changes, not independently proven fixed | Minimal outcome contract; expand preparation only for a concrete uncertainty or boundary | Small edit completes without a full project-spec pipeline |
| B02 | [Token-cost report](https://www.reddit.com/r/BMAD_Method/comments/1ruregn/burning_too_many_tokens_with_bmad_full_flow/) and [broader criticism](https://www.reddit.com/r/BMAD_Method/comments/1r6aruo/bmad_method_sucks/): users describe repeated context and overengineering; anecdotal, not measured baselines | Count briefing/review/retry overhead; retrieve sources on demand; stop duplicate work | Compare total tokens and accepted quality on small, medium and resumed tasks |
| B03 | [Discussion #2144](https://github.com/bmad-code-org/BMAD-METHOD/discussions/2144), March 2026: a user missed deeper investigation checkpoints in a simplified path; version-specific preference | Escalate rigor inside one adaptive loop without restarting a complete ceremony | A simple-looking task with a hidden dependency triggers investigation before writing |
| B04 | [#2849](https://github.com/bmad-code-org/BMAD-METHOD/issues/2849), September 8, 2026: open at consultation; detailed report of costly resume/reimplementation, not reproduced here | Stable outcome/attempt identity; resume from pinned inputs and missing evidence before new writing | Completed candidate plus unrelated newer files resumes into checking, not rebuilding |
| B05 | [#2849](https://github.com/bmad-code-org/BMAD-METHOD/issues/2849) also reports missing run ownership; same open report, not another independent observation | Scope and changed-output attribution; current copies plus conflict detection | Two disjoint tasks never inspect, stage or claim each other's changes |
| B06 | [#2760](https://github.com/bmad-code-org/BMAD-METHOD/issues/2760), August 19, 2026, v6.11: open at consultation; duplicate review/lifecycle report | Reuse valid review coverage by exact input, rubric and independence, not layer name | Same candidate requests acceptance audit without repeating valid unrelated review dimensions |
| B07 | [#446](https://github.com/bmad-code-org/BMAD-METHOD/issues/446), August 2025, v4: manual coordination and unclear next step; maintainers disputed use of the wrong workflow | Existing native plan/running view selects and explains the next authorized ready outcome | Newcomer can identify now, next and blocker without knowing agent commands |
| B08 | [Review docs](https://github.com/bmad-code-org/BMAD-METHOD/blob/main/docs/build/review-a-change.md): capabilities can make review sequential or in-session; documented limitation, not a universal bug | Probe real capabilities; independent review requires an actual independent session | Background nesting unavailable cannot silently turn review into self-review |
| B09 | [Discussion #2778](https://github.com/bmad-code-org/BMAD-METHOD/discussions/2778), August 2026: mixed but positive project experience identifies fragmented requirements and interpretation | Requirement IDs and source-to-outcome coverage; material changes create a revision | Negative requirements and later decisions survive planning and handoff |
| B10 | [#1958](https://github.com/bmad-code-org/BMAD-METHOD/issues/1958), March 2026: language-specific vocabulary reported to mislead other stacks; closed with vocabulary-expansion explanation | Domain-neutral core, environment-checked examples and task-specific acceptance | C++/document/research task does not inherit irrelevant React assumptions |
| B11 | [#1149](https://github.com/bmad-code-org/BMAD-METHOD/issues/1149), December 2025, and [changelog](https://github.com/bmad-code-org/BMAD-METHOD/blob/main/CHANGELOG.md): Windows install/encoding incidents, some documented fixes | Current native process/filesystem rules, capability checks and actionable recovery | Spaces, accents, BOM, locked files and interrupted setup do not produce silent success |
| B12 | [Method](https://www.bmadcode.com/method) centers software; [Builder](https://bmad-builder-docs.bmad-method.org/) supports workflows beyond it. This is an evaluation boundary, not evidence that BMAD cannot do non-code work | One outcome loop with concrete domain checks and artifacts | Same orchestration serves research, writing, spreadsheet, visual and code tasks |

## Current strengths, not differentiators

[Build a Change](https://docs.bmad-method.org/build/build-a-change/) already investigates intent, supports a light
path and independent review, and treats small work differently from large initiatives. Do not compare Flow with
an invented requirement that BMAD always run an entire simulated agile team.

[Project-context theory](https://docs.bmad-method.org/existing-codebases/theory-of-project-context/) already argues
for selective context rather than generated documentation that restates the code. Short prompts are not a new idea.

[Multi-agent discussions](https://docs.bmad-method.org/customize/run-multi-agent-discussions/) distinguish modes
and actual independent voices. Multiple named personas in one context do not establish independent verification.

The [changelog](https://github.com/bmad-code-org/BMAD-METHOD/blob/main/CHANGELOG.md) records review convergence,
preserved work on intent-gap halts and reviewed-diff handling. Historical failures are regression cases; they are
not automatically current competitive defects. Inspect pinned versions before benchmarking each claim.

## Resulting design constraints

Review-coverage reuse also appears in SoLoHiC's September 10, 2026 comment on
[#2760](https://github.com/bmad-code-org/BMAD-METHOD/issues/2760). That is an external design suggestion, not a Flow
invention or proof of a fix. Flow adapts the idea to Faber's existing exact-input evidence and independent-review
requirements. No comment text or implementation is copied into the product.

1. The cheapest path must still preserve requirements and mandatory checks.
2. Resuming should first identify what is still missing, not repeat planning or implementation by habit.
3. Review coverage and lifecycle transition are different facts. Reuse only evidence that is still valid.
4. Runtime capabilities, scopes and actual output determine whether delegation happened as promised.
5. Domain fit and lower total cost must be measured; adding more documents cannot establish either.
