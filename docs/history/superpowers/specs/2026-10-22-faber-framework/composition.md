# Faber Flow: adaptive composition with a fixed trust boundary

Status: Revision 3 proposal. No workflow interpreter or product capability is implemented here.

## The central design

Keep one small work loop. Adapt its support to the result, model, available tools and consequences.
The agent can propose the route; the engine owns permissions, state transitions, budgets and evidence validity.
This is neither a universal prompt nor an unrestricted agent-generated execution program.

```text
Understand outcome -> establish current inputs -> choose support -> make a candidate
        ^                                                       |
        |                 bounded correction <--- inspect/check/review
        |                                                       |
        +--- changed scope                            usable delivery + receipt
```

These are responsibilities, not six mandatory model calls. A small answer can satisfy them in one turn.
The default is the least complicated qualified path, preserving safety requirements even when rarely exercised.

## Six reusable mechanisms

| Mechanism | Contract | Skip or constrain it when |
| :--- | :--- | :--- |
| Establish | Exact outcome, negative requirements, current inputs and acceptance | Already present and still applicable; do not make the user restate it |
| Ground | Retrieve a source, example, calculation, observation or missing capability | It adds no needed evidence, or unavailable access would require invention |
| Sequence | Pass checked intermediate output across actual dependencies | No intermediate boundary improves the task; preserve global requirements |
| Branch | Independent collection or reversible candidate alternatives, each bounded | Work is sequential, shares mutable state, costs more than its value, or performs external effects |
| Challenge | Appropriate domain checks and independent judgment, with rejection allowed | The same valid coverage already applies; never skip a required independent/security review |
| Reconcile | Recover output/state/effects and deliver a truthful current receipt | Nothing changed; do not rerun settled work merely because context was compacted |

Compose these through existing native plans and tool orchestration; do not introduce a general programming language,
persist arbitrary executable workflows or make a graph editor necessary. First implementations support only qualified
compositions. More flexible generation is admitted only after its contracts, authority and cost controls are proven.

## Choosing support without making the person manage models

1. Classify missing capability and checkability from the actual task. A knowledge gap may need a source, arithmetic
   a calculator, a visual artifact inspection, and difficult judgment a strong model. Unknown is a valid outcome.
2. Select the qualified direct, grounded, guided, alternative or strong route described in [intelligence.md](intelligence.md).
   Match semantic independence, tool access, risk and whole-task cost rather than a fixed token-price ranking.
3. Keep the conversation's model, effort and tool context stable. New role/model support uses an admitted helper or
   separate conversation; first implement the required helper configuration instead of assuming it already exists.
4. Evaluate support density by model/version. Light workers may need complete examples and smaller bounded tasks.
   Strong models can need fewer prescribed steps while retaining original constraints and the same evidence bar.

Do not require private chain-of-thought, confidence theater or a ritual of self-critique. Observable decisions,
tool inputs/outputs, check receipts and concise reasons are enough. More reasoning text is not a quality measure.

## Evidence, authority and effect ownership

The proposed native evidence link binds an outcome, candidate identity, relevant input versions, checker/reviewer
coverage and result. Reusing a pass requires applicability, not just matching its title. New evidence invalidation
and identifiers need implementation; today's agent-written plan checkboxes do not provide this authority.

For a side effect, distinguish prepared, authorized, attempted, confirmed and ambiguous states. Existing tools retain
their established permission boundaries. Prefer a reviewable draft or reversible local candidate before requesting
an actual reserved action. If a crash occurs after a send but before its receipt, reconcile before retrying.
A local checkpoint cannot unsend a message, recover spent money or roll back an arbitrary website action.

Execution ownership continues through the actual usable result and required checks, not just a worker's exit.
Canceled authority, exhausted quota and unavailable tools preserve honest unfinished work. They cannot be converted
into successful delivery or bypassed by another agent. A planning task ends with a checked plan, not implementation.

## Review that earns its cost

- The writer runs applicable checks first; a fresh reviewer checks intent, relevant integration and failure modes.
- A finding identifies evidence, consequence and affected requirement. Separate introduced, pre-existing and taste issues.
- Calibrate repeatable review guidance against a violation, a safe contrast and an unrelated change. Track false alarms,
  missed seeded faults and human correction burden; reviewer agreement alone does not certify a finding.
- Reuse valid review coverage and inspect the changed portion plus affected dependencies. Record why findings are fixed
  or dismissed; an agent cannot silently dismiss its own required independent review.
- Keep existing security independence and the verifier's repair limit. There is one shared correction budget per task;
  creating helpers or restarting the conversation cannot reset it into an infinite repair/review loop.

## Learning without silently changing policy

An incident can produce a proposed regression fixture, narrow skill change or lesson. Existing keep/owner-adoption
rules decide what becomes lasting policy. Successful preference feedback is not automatically factual correctness.
Retain fewer better rules; remove redundant instruction after an executable check covers it, with normal review.

A workflow improvement experiment uses held-out paired tasks and a frozen baseline in isolated candidates. It may
change proposed routing/procedures, not its own active permission limits, owner decisions or qualification answers.
Promote only with the declared [qualification](qualification.md); retain a fallback and source/version lineage.
No always-on experiment service, hidden API bill, unrequested telemetry or quota harvesting is added by this proposal.

## Same mechanisms, different work

| Request | Useful composition | Acceptance |
| :--- | :--- | :--- |
| Answer a simple stable question | Establish -> direct answer | Meets the actual question; no needless planning ceremony |
| Compare suppliers | Establish -> independent sources -> resolve conflicts -> cited recommendation | Comparable current terms, declared assumptions, no invented sources; no purchase authority |
| Produce a monthly report | Establish inputs -> calculate -> check samples -> render -> deliver | Reconciled figures, usable file, sources and limits; scheduling is separate |
| Create a visual identity | Establish taste -> bounded alternatives -> actual previews -> blind preference | Usable artifacts and explicit human taste; preference does not certify factual/legal claims |
| Improve an interface | Current product map -> existing craft skill -> implementation -> proof skill -> fresh review | Required states inspected, supported accessibility checks, actual artifact not just polished prose |
| Organize local files | Inventory -> reversible preview -> bounded operations -> reconcile inventory | Correct destination, preserved required data, conflicts handled and actual operations accounted for |

Domain-specific detail stays in [domains.md](domains.md). External tools come through the approved catalog from their
original sources; missing modalities remain visible. A general method does not make every domain tool exist today.
