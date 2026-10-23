---
name: frontend-craft
description: Build or refine website and app UIs with project-aware composition, interaction states and accessibility. Excludes video, images and backend-only work.
---

# Frontend craft

The deliverable is an interface a person can use. More components, fashionable effects or a higher checklist count
are not evidence of a better result. Preserve the request, the project's identity and its existing design system.

## Start with the task and the existing product

- Identify who uses the screen, what they came to do, the primary action and how they know it worked. For a small
  change, infer this from the surrounding flow instead of conducting an interview.
- Inspect the routes, shared controls, tokens, content and installed libraries before adding anything. Extend the
  closest existing surface; a second model picker, settings route or navigation pattern is usually a regression.
- Distinguish a marketing site from a work application. The former may use expressive editorial composition; the
  latter prioritizes reading, input, predictable controls and preserving work. A game HUD has different constraints.
- For a new visual identity, use relevant real references and describe what to learn from their hierarchy, density,
  type or imagery. Browse original sources when available; a reference is evidence to interpret, never code to copy.
  Choose one coherent direction within the person's brief. Respect any project requirement to show alternatives.
  A local repair does not need three concepts or a redesign.
- When an applicable design or art-direction skill is already available, follow its relevant workflow for a new
  composition or identity. These interaction rules complement that work; reading a reference or running a helper
  script alone does not apply the design workflow. A local behavior repair should retain the existing direction.

## Compose before decorating

- Give the primary action visual priority. Group related controls by proximity and alignment before drawing more
  containers, borders and cards. Let important information have space; do not equate whitespace with empty padding.
- Inspect the first useful viewport at narrow and wide sizes. Make the task and essential context discoverable
  there; an expressive introduction should not bury products, navigation or critical demonstration limitations.
  Judge this against the actual brief, rather than imposing a hero or banning one for every project.
- Choose typography for the content and available assets. Use a restrained scale, readable body line length and
  consistent leading. Test real headings, numerals, punctuation, code and links. Do not impose a fashionable font.
- Reuse the project's spacing, type, color, radius and depth tokens. Use semantic colors consistently, with words or
  shapes as additional status cues. Validate contrast in each affected theme, including muted and disabled states.
- Use assets that explain or establish identity. Do not fill unused space with decorative dashboards, arbitrary
  gradients, invented testimonials or unsupported business claims. Mark sample data as sample data.
- Match native expectations for selection, cursors, scrolling and shortcuts. An unusual visual composition must not
  make a familiar task behave unpredictably.
- Make frequent actions recognizable through clear labels and relevant context, not memorized commands alone.
  Shortcuts complement a usable interface; they do not compensate for unclear navigation.

## Specify the interaction, not just its ideal screenshot

For each affected flow, implement the states it actually needs: initial/empty, loading, populated, selected, disabled,
submitting, success and recoverable error. Avoid adding state machinery where a native control already handles it.

Choose a loading cue for the actual wait: inline feedback for an action, a skeleton only for known content structure,
and determinate progress only when progress is measured. Keep usable controls available rather than skeletonizing them.

- React immediately to an action, but distinguish local intent, durable save, remote acceptance and completion.
  Optimistic changes need a rollback/retry policy. Repeated submission, stale results and cancellation must not apply
  an older result over newer work. Use the existing queue and persistence path rather than creating a parallel one.
- Preserve entered text, references and attachments through recoverable failures. State precisely what survives a
  reload; never label an in-memory cache as durable. Disabling Submit is feedback, not server-side idempotency.
- Keep reading position and selection stable while content arrives. Follow new output only while the person is
  already following it; otherwise offer a quiet way back. Revisit focus after navigation, removal and closing overlays.
- When lists page, reorder or filter, check that focus, selection and temporary markers still belong to the intended
  record. Positionally reused DOM can show different content. Preserve any newer focus choice; do not change the
  whole rendering strategy without checking update and rendering cost.
- Put recoverable field errors beside their field and connect them programmatically. Distinguish invalid input from
  a service outage. Say what can be done next; never clear valid input to show an error.
- Use a modal only for a task that must suspend the background; trap and restore focus only then. Prefer existing
  accessible components. A non-modal activity panel must not steal focus from the composer.

## Build for the available space and input

- Derive breakpoints from where content fails, not from a device name. Use flexible tracks, wrapping and intrinsic
  sizing. Keep reading width usable when panels dock; overlay or stack them when it does not fit.
- Test long unbroken names, expanded translations, empty lists, large counts, missing media and large text. Give a
  table or code block its own horizontal scrolling when needed, without making the whole page overflow.
- Use native semantic controls, real labels and deliberate heading order. Icons need accessible names when they are
  controls. Keyboard order follows the task; do not repair a broken order with positive tabindex values.
- By default, grid/flex visual reordering leaves keyboard order tied to DOM order. Keep the source order meaningful
  and check both after changing grid areas, order, or a responsive arrangement.
- For custom tabs, menus, dialogs and splitters follow the relevant WAI-ARIA APG pattern, including keyboard behavior.
  An ARIA role alone does not implement interaction. Prefer native HTML when it supplies the required behavior.
- Provide visible, unobscured focus and alternatives to dragging. WCAG 2.2 AA target sizing is 24 CSS px or one of
  its documented exceptions; this is a compliance floor, not the ideal touch size. Follow the platform's larger
  touch target guidance when appropriate. Check contrast and forced colors rather than relying on color alone.

## Use motion and resources deliberately

- Name motion's purpose: feedback, location, continuity or rare delight. Frequent text/input/navigation should feel
  immediate. Do not animate readable data for style. Use the project's motion tokens and existing motion skill.
- Prefer CSS for simple state changes, transform and opacity for movement, reversible transitions for rapid changes.
  Reduced motion and fine-pointer hover gating belong in the same implementation. No perpetual hidden animation.
- Profile before introducing workers, virtualization, prefetch or will-change. Each can add overhead or break search,
  focus and scroll anchoring. Keep heavy logs and previews lazy; release resources when they are no longer used.
- Separate measured latency from proposed budgets. Good web INP is <=200 ms at p75 of visits, not p95 of clicks.
  A desktop WebView still needs direct interaction/frame measurements; an INP label alone proves little.

## Inspect and deliver

Use the sibling `frontend-proof` skill for the affected flows before claiming they work. Fix the most consequential
observed defect, inspect again, and stop when the requested behavior and its relevant quality checks hold. Name
anything unverified. A beautiful screenshot never proves persistence, accessibility or a successful operation.

## Original sources, read only when relevant

- [Microsoft Fluent layout](https://fluent2.microsoft.design/layout): grouping, spacing and responsive composition.
- [Apple HIG layout](https://developer.apple.com/design/human-interface-guidelines/layout): platform-aware layout.
- [WAI-ARIA APG](https://www.w3.org/WAI/ARIA/apg/): semantics and complete widget interaction patterns.
- [WCAG 2.2](https://www.w3.org/TR/WCAG22/): normative criteria; distinguish them from advisory techniques.
- [GOV.UK errors](https://design-system.service.gov.uk/components/error-message/): input errors and recovery.
- [web.dev INP](https://web.dev/articles/optimize-inp): responsiveness measurement and main-thread work.
- [Emil Kowalski](https://emilkowal.ski/ui/7-practical-animation-tips): motion judgment, not universal visual rules.
- [IBM Carbon loading](https://www.carbondesignsystem.com/building-blocks/core/patterns/loading): choosing loading cues.
- [Josh W. Comeau on Grid](https://www.joshwcomeau.com/css/interactive-guide-to-grid/): layout and keyboard order.
- [NN/g on recognition](https://www.nngroup.com/articles/recognition-and-recall/): discoverability and context.
