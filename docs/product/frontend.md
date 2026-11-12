# Frontend craftsmanship

How Faber approaches websites and application interfaces it builds, without imposing its own desktop look on them.
The person's 2026-10-22 blueprint is evidence to assess, not 243 mandatory features.

## Original skills, existing delivery

Faber supplies two original instruction skills beside its own guide in the existing `guide-plugin` folder:

| Skill | When it applies | Source |
| :--- | :--- | :--- |
| frontend-craft | Before writing anything a person sees in a browser: a site, an app screen, a form or an explainer, even one HTML file | [Instructions](../../engine-projects/src/frontend-skills/frontend-craft/SKILL.md) |
| frontend-proof | Before saying a page or screen is done, or an explicitly requested audit | [Instructions](../../engine-projects/src/frontend-skills/frontend-proof/SKILL.md) |

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
3. Look at the result at a desktop and a phone size before saying it is done, with the session's browser tool, the
   project's harness or an installed Chromium browser run headless (no window, pictures kept in the temporary
   folder), fix what shows and look again. Faber starts nothing for this: the agent runs it under the permission mode
   in force, for under a second per picture, and nothing stays running.
4. Retain proof of the affected task and visual result, independent review where required, actual measurements and
   anything unverified. Never present a visual score as proof of reliability or an automated scan as AA conformance.

A new screen in a folder with no design system gets a direction taken from its subject (a named brand, a layout
that uses a wide screen, one type contrast, a small palette, a drawn mark) instead of a card centered on a gradient.
A page that teaches lists the facts an expert expects, draws every step it names with text readable on a phone, and
is reread once as a checker before delivery; the persona asks the same reread of any explanation in Chat.

For forms collecting information about the form user, the skills cover supported, defined input purposes through
matching programmatic metadata (such as HTML `autocomplete`). Labels and input types alone may not establish that
purpose. This does not require metadata for every field or other people's records, and browser autofill is not the
pass condition. A synthetic markup check can establish this narrow contract, not general WCAG conformance or a
measured gain in generated design quality.

Frontend quality is judged on task clarity, visual hierarchy, type, spacing, useful content, coherent identity,
interaction states and accessibility. A particular font, gradient, animation library or fixed card layout is not a
universal quality standard. No new dependency is required by these skills.

## Evidence and limits

Illustrated product demos use the existing optional Stageshow catalog entry, pinned to its upstream source and
verified by the same install path as other skills. The original [Faber demo](../../demos/faber/README.md) runs in a
separate local workspace; its React kit and dependencies never enter Faber's application bundle. The scene labels
its synthetic conversation and uses the app's existing visual tokens. It is presentation evidence, not a real CLI run.
Agents copy the needed runtime/example files into the task workspace before editing or installing dependencies;
the verified Catalog snapshot stays a reference, so a rehearsal does not invalidate an installed skill.

The first-party craft/proof instructions distinguish this task from an operational interface, ask for a separate
narrow composition and reduced-motion still, and require a before/during/after rehearsal with trustworthy coverage.
An empty, gapped or unfinished capture cannot establish a pass, including when a third-party tool prints one.

The delivery checks live inside `engine-projects/src/guide.rs` and `engine/src/engine/kits.rs`: file creation/repair, lazy
discovery, both vendors and areas, isolated requests, guarded modes, unchanged/resumed sessions and directory safety.
The existing guide checks still cover its words and screen language. These checks establish delivery, not a measured
improvement in generated design quality. That needs paired task runs and independent visual judgment.
The first [exploratory pair](../history/research/2026-10-22-frontend-evaluation.md) tied at 17/20 in a blind review, with
different strengths. It does not establish a causal gain: only one sample per condition was built, the exact model
variant was not exposed, and consultation of the existing art-direction skill differed.
The [refinement record](../history/research/2026-10-22-frontend-skill-refinement.md) keeps the follow-up controls and evidence
separate. Drafted guidance is not proof of improved generated output.

The [source study](../history/research/2026-10-22-frontend-sources.md) distinguishes standards from practitioner advice and
proposed measurements. The [243-item audit](../history/research/2026-10-22-frontend-blueprint-audit.md) records static evidence
and uncertainty per ID. Existing tests cited there were not executed by the audit.

The [implementation record](../history/superpowers/plans/2026-10-22-frontend-excellence.md) separates implemented reliability
changes, measured observations and remaining contracts. Draft persistence, reading anchors and engine admission
need their own implementation and runtime proofs; instruction delivery alone establishes none of them. In particular,
a saved draft or admission guard is not a crash-safe outgoing journal or reconciliation after a lost acknowledgement.
