# OpenAI Codex desktop package: resources inventory and behavior spec

Scope: the resources folder of the installed Codex desktop package (Windows x64), excluding the packed application
archive. Method: static reading of text files only. No binary was run, no app was launched, no window was opened,
and no credential or key material was recorded. Evidence tags:
- (observed: static) = read in a text file of the package
- (inferred from strings) = deduced from names or labels, not read in full context
- (not examined) = listed but not read

## 1. Inventory

| Item | What it is | Evidence |
|---|---|---|
| skills/ (2 curated skills) | "Hatch Pet" (create and package animated pets) and "Onboard New User" (setup flow that makes a first artifact) | (observed: static) |
| plugins/openai-bundled (9 plugins) | Marketplace bundle: codex-app-tools, browser, unified-computer-use, chrome, computer-use, latex, code-review, deep-research, visualize | (observed: static) |
| plugins/.../visualize | Inline visual output skill: HTML fragments, Mermaid, tables, widget state, variant carousel, calendar component | (observed: static) |
| plugins/.../deep-research | Multi-step research workflow ending in a cited DOCX report | (observed: static) |
| plugins/.../computer-use | Windows UI automation through a bundled Node runtime, with confirmation tiers | (observed: static) |
| plugins/.../browser and chrome | In-app browser control; control of the user's own Chrome with its sessions | (observed: static) |
| plugins/.../latex | Compile TeX projects with a bundled engine, detect TeX Live or MacTeX, optional managed TeX Live install | (observed: static) |
| artifact-template-picker | Helper that offers templates through a form-style picker and saves a chosen style | (observed: static) |
| owl-app.ini, owl-electron-app.json, ../owl-shell-runtime.json | Shell metadata: user data folder name "Codex", app version 26.1007.21434, Electron-based shell, Windows x64 runtime with no MSIX dependencies | (observed: static) |
| masque-proxy-origins.txt | One allowed proxy origin for a portal host, used for routing | (observed: static) |
| plugin-signatures/openai-bundled | Receipt of per-plugin archive sizes and hashes plus signature metadata. Keys not read. | (observed: static) |
| accessibility/reading_mode_gdocs_helper_manifest.json | Manifest of a reading-mode helper for Google Docs. Its body embeds key-like material. Contents not recorded. | (observed: static, partial) |
| cua_node/ | Bundled Node runtime with a browser-desktop package whose environment docs cover cloud, Codex app and Orbit modes | (observed: static) |
| tectonic/ | Bundled Tectonic TeX engine executable (about 55 MB) | (inferred from strings; not executed) |
| codex/ | A single large executable (about 294 MB) for the CLI engine | (not examined) |

Caveat on the accessibility manifest: while reading it, a fragment of its key-like field was printed in a tool output. It was
not copied into this spec. The owner should treat that file as sensitive.

## 2. Feature specs

### 2.1 Visual output guidance (visualize plugin): the core for Faber

Behavior the model is told (observed: static):
- Make a visual only when the person needs to see or explore it in the conversation and it clearly improves the explanation.
  Do not add one just because the subject has numbers.
- Tables: when the person asks for a table, answer with a normal Markdown table and nothing else.
- Static structures with labelled nodes and edges: answer with a fenced Mermaid block, no file.
- Dynamics, motion, spatial layout, adjustable inputs: write an HTML fragment.
- Silence during work: no progress commentary while writing the visual. The final answer says only what the visual helps the
  person see or decide. Never announce the skill, the file or the machinery.
- Reference line: after creating or updating a visual, the final reply carries one reference line naming the fragment and a
  width mode. The reference is never called an artifact, attachment, download or link, and gets no Markdown link.
- Fragments are response content, not library files. Downloads only when the person asks.

Fragment contract (observed: static):
- Literal markup, one root with a unique id; no script that depends on where it sits in the page.
- External scripts only from an allow-listed CDN; no network calls from the fragment (no fetch, XHR, WebSocket).
- Theme-aware colours only, through shared tokens: a series palette of six, plus neutrals for grid, foreground and border.
  Hard-coded light or dark palettes are forbidden. Mockups must not reuse the visual tokens; they define their own product colours.
- Type: one base size token; small text only for annotations (14 px and 12 px at default); weights 400 and 500 only.
- Charts: transparent host background, no card around plots, SVG redrawn from its container's measured width on resize,
  at least 64 px reserved on the y axis, domains padded from the data extent, thin neutral grid, one colour per persistent
  category, with labels or shapes paired to colour.
- Progress bars and meters: no outline on filled quantitative marks; neutral or translucent track.
- Composition: few metrics, only those that explain the behaviour; maxima shown as ceilings, not targets; no invented scores;
  no filler cards; no oversized empty space; one compact dominant visual for explainers and step-throughs.
- Mockups: several design alternatives are encouraged when the person explores a screen, shown as a variant carousel with
  descriptive names (never numbered), bound to Tweak-style controls.

States (observed: static):
- Created, updated, blocked. Blocked means the target folder is not writable; the model must use a writable thread folder.
- Interaction state: after a meaningful interaction the fragment sends a short state to the model (choices useful for follow-up
  questions) and keeps UI details privately for restore on reload. Transient values are not stored.

Why it helps Faber: one small interactive picture replaces paragraphs of numbers, and the rules stop decoration and invented
metrics. This maps to Faber's goal of less text and what matters first.

### 2.1.1 Variant carousel (observed: static)
- The model proposes several design alternatives for one component or screen. Each is a named tab; controls bind to options so
  the person can flip variants without a new reply.

### 2.1.2 Mermaid route (observed: static)
- Used only for static structure. No file; the fenced block is the whole visual.

### 2.1.3 Calendar component (observed: static)
- A day-view schedule: each event is a block with start, end, title and an optional colour token, placed on a time axis.
  Overlap handling by side-by-side lanes is inferred from labels.

### 2.2 Artifact template picker (observed: static)
- Two tools are exposed to the model: one lists templates by family, one chooses one.
- Families: document, presentation, spreadsheet, and three Google-hosted kinds (docs, slides, sheets).
- At most ten templates are offered per request.
- The person sees a form-style picker. Its prompt asks what style the family should use. Options include the offered templates
  and an option to pick a reference from a file or an image.
- After a reference is chosen, a save prompt asks whether to keep it as a reusable template, with a name of at most 64 characters.
- The model does not render the artifact in this step. It only selects a style; a document or slides tool builds it afterwards.

### 2.3 Deep research (observed: static)
- Method: plan questions, gather sources, read, cross-check, then write.
- Output contract: a DOCX report is mandatory. Citations are footnotes. A render check of the document is part of QA.
- Progress is shown as steps; the final answer delivers the document.
- Faber idea: a report card with footnoted sources and a visible verification step, instead of a long chat answer.

### 2.4 Computer use, Windows (observed: static)
- The model works through a Node-based desktop API: list apps and windows, capture a window state (screenshot by default,
  accessibility text on request), act, then refresh state before the next action. Element indexes are valid only for the state
  that produced them.
- Confirmation tiers (summarised): read-only steps need no question; sending, submitting, uploading, changing sharing, or
  entering sensitive data needs the person's explicit go-ahead; authentication dialogs, password managers, security settings
  and terminal applications are off-limits.
- Recovery: retry a lightweight call once after a 2-second wait; on window activation failure, drop old coordinates; a locked
  desktop stops everything and asks the person to unlock.
- Faber idea: an action preview card before each sensitive step, using the same tiers.

### 2.5 Browser and Chrome control (observed: static, names and summaries)
- In-app browser: open, navigate, inspect, click, type, screenshot, local web testing. Prefer semantic connectors for linked
  resources when present.
- Chrome: works on the person's own signed-in tabs and extensions. Purpose-built connectors are preferred when they exist.

### 2.6 LaTeX (observed: static, names and summaries)
- Compile: try the bundled engine for simple projects, fall back to an installed TeX distribution.
- Doctor: report which tools are present, run a small smoke compile.
- Runtime installer: detect an existing distribution first; a managed install only when none is found (consent step inferred from labels).

### 2.7 Onboarding (observed: static)
- Setup is a sequence of named steps: role, first task, follow-up questions, first artifact, completion.
- Interactive surfaces are preferred in this order: built-in role and task panels, a generic option picker, a structured input
  request, connector sign-in modals, then a plain chat question.
- When an option picker or structured input is shown, the turn stops and the answer arrives as the next message.
- Dismiss means stop; skip means continue with defaults.
- The first artifact is chosen by lightest fit: chat text for short copy, a document for briefs, a spreadsheet for trackers,
  a deck for readouts, a site only for an explicit web prototype. With no concrete input, the artifact is labelled a starter template.
- Closing line pattern (quoted, under 15 words): "Done. I drafted {artifact_name} here in the thread." followed by an offer to revise.

### 2.8 Hatch Pet (observed: static, headings only)
- Staged workflow for animated companions: brand discovery, a look-direction stage with blind review, a visual job graph,
  transparency rules, packaging. Relevant to Faber as an example of staged visual review with accept or reject rules.
  Not examined in depth.

### 2.9 Plugin catalogue (observed: static)
- Nine plugins are listed. Each has a manifest with a display name and default starter prompts. Plugins are installed from
  signed archives; the receipt records sizes and hashes.

## 3. Top 10 ideas for Faber (ranked by value / effort)

1. Visual-or-not gate: a short rule that picks table, diagram, or interactive card before writing, with one reference line per visual. Low effort, high value against walls of text.
2. Fragment contract for inline HTML cards: theme tokens, no network, resize-aware charts. Medium effort.
3. Variant carousel with named options for UI mockups in a Projects chat. Medium effort.
4. Option picker and structured input panels that stop the turn and resume on answer, replacing numbered questions. Low effort.
5. Action preview card with confirmation tiers before any send, upload, submit or sharing step. Medium effort.
6. Report card for research: footnoted sources plus a visible verification step. Medium effort.
7. Template chooser: at most ten templates, ask for a style, optional save as reusable with a 64-character name. Low effort.
8. Interaction state for widgets: a short model-facing state and a private UI state restored on reload. Medium effort.
9. Calendar day-view component with time blocks and lanes for overlap. Medium effort.
10. Setup flow with named steps and lightest-artifact choice for first use. Low to medium effort.

## 4. Unknowns and bounds of the search

- The 294 MB CLI executable was not examined. Its wire formats and model prompts are unknown.
- The packed application archive was excluded by scope. Renderer code and the exact widget bridge names are not covered.
- Plugin skill files were read at heading and summary level; latex, browser, chrome and unified-computer-use were not read line by line.
- Tectonic was identified by name and size only; not executed.
- Signature keys were not read. Only the existence of signed receipts is recorded.
- The accessibility manifest was partly printed by accident (see section 1). Its purpose is inferred from the file name only.
- No runtime evidence (observed: runtime) was gathered; every claim above is static or inferred.
