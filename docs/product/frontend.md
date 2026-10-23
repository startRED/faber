# Frontend craftsmanship

How Faber approaches websites and application interfaces it builds, without imposing its own desktop look on them.
The person's 2026-10-22 blueprint is evidence to assess, not 243 mandatory features.

## Original skills, existing delivery

Faber supplies two original instruction skills beside its own guide in the existing `guide-plugin` folder:

| Skill | When it applies | Source |
| :--- | :--- | :--- |
| frontend-craft | Building or improving a website or application UI | [Instructions](../../engine/src/frontend-skills/frontend-craft/SKILL.md) |
| frontend-proof | Verifying an implemented UI or an explicitly requested audit | [Instructions](../../engine/src/frontend-skills/frontend-proof/SKILL.md) |

These are first-party instructions, not copies or bundled versions of catalog skills. Existing design, motion and
framework skills remain useful; these supply the project-aware interaction and proof workflow between them. They
create no new setting, catalog card, tool, model, timer, process or permission to act on external services.
For a new composition or identity, frontend-craft explicitly complements an applicable installed art-direction
workflow. A local behavior repair keeps the existing identity. The proof workflow checks the actual completion of
a slow path; an independent instant-success mode cannot establish that the waiting path really finishes.

- Claude Code in Projects discovers them as native skills of the existing private plugin, with only descriptions
  resident until needed. Both files are explicitly allowed for instruction loading, alongside the guide.
- Codex in both areas and Claude Code in Chat receive a bounded index pointing to the same two files. An older
  Claude Code without plugin support uses that index too. File reads stay within the existing permission policy.
- If the guide folder cannot be safely written, the guide's existing text fallback includes a short frontend
  checklist. It is a fallback, not the full skills or proof that they ran.
- Internal isolated requests receive neither the guide nor these skills. Their folder keeps its existing write
  protection and refuses unexpected entries and linked directories. Original instruction files are repaired when
  they differ from this build, just like the guide.

The file bodies are embedded as static text. Session discovery grows by two descriptions or two file references;
the complete skill text is read only when the agent uses it. There is no resident frontend service. This describes
the allocation model, not a measured whole-app RAM/CPU result or proof that every model will follow instructions.

## The workflow

1. Identify the task and extend existing routes, controls and tokens. Match the deliverable: marketing composition,
   work application, desktop shell and game HUD have different constraints. Preserve the person's visual direction.
2. Implement the affected states and recovery path with semantic controls, keyboard/focus behavior, responsive
   content, reduced motion and bounded rendering. Inspect the running result, not just the source.
3. Retain proof of the affected task and visual result, independent review where required, actual measurements and
   anything unverified. Never present a visual score as proof of reliability or an automated scan as AA conformance.

Frontend quality is judged on task clarity, visual hierarchy, type, spacing, useful content, coherent identity,
interaction states and accessibility. A particular font, gradient, animation library or fixed card layout is not a
universal quality standard. No new dependency is required by these skills.

## Evidence and limits

The delivery checks live inside `engine/src/guide.rs` and `engine/src/engine/kits.rs`: file creation/repair, lazy
discovery, both vendors and areas, isolated requests, guarded modes, unchanged/resumed sessions and directory safety.
The existing guide checks still cover its words and screen language. These checks establish delivery, not a measured
improvement in generated design quality. That needs paired task runs and independent visual judgment.
The first [exploratory pair](../research/2026-10-22-frontend-evaluation.md) tied at 17/20 in a blind review, with
different strengths. It does not establish a causal gain: only one sample per condition was built, the exact model
variant was not exposed, and consultation of the existing art-direction skill differed.
The [refinement record](../research/2026-10-22-frontend-skill-refinement.md) keeps the follow-up controls and evidence
separate. Drafted guidance is not proof of improved generated output.

The [source study](../research/2026-10-22-frontend-sources.md) distinguishes standards from practitioner advice and
proposed measurements. The [243-item audit](../research/2026-10-22-frontend-blueprint-audit.md) records static evidence
and uncertainty per ID. Existing tests cited there were not executed by the audit.

The [implementation record](../superpowers/plans/2026-10-22-frontend-excellence.md) keeps unfinished reliability and
measurement work explicit. No instruction skill makes durable attachments, an outgoing journal or exact reading
anchors exist by itself.
