# OpenAI desktop package (v26.1007.2314.0): how assistant output is rendered and made interactive

Date: 2026-10-18. Target: the OpenAI desktop package for Windows (lawful copy owned by the user). It ships a Codex shell whose packaged web frontend also contains the ChatGPT frontend, so both products are covered by one static reading. Method: static reading of the packaged frontend and main-process bundles, plus its English UI strings (about 50,000). Nothing was launched, nothing was enabled, no credentials or personal data were read. REA was used once (JavaScript application analysis, Evidence `ev_77155115e065a0ea478eddab627468b7b70c19c4336adcd83deb225aa673e10d`); it recorded the evidence but its answer exceeded the transport budget, so the rest is static reading of the extracted files.

Tags: (observed: static) = read directly in the shipped files. (inferred from strings) = deduced from UI text or field names, behavior not seen running. No point here is (observed: runtime).

Clean-room note: this file contains behavior in words, product-visible names (tool names the model sees, directive names, UI labels, wire action names) and short on-screen strings only. Model instructions are paraphrased.

---

## 1. Inventory (one line each)

1. Generative widget system, version 1: the server sends a JSON widget tree with expressions, slots, state and actions; the client renders it (observed: static).
2. Generative widget system, version 2 ("model-written program"): the model writes a small JSX-like program that runs in a sandboxed frame and worker and draws with a fixed set of primitives and named server-resolved components (observed: static).
3. A catalog of roughly 236 named, prebuilt widget components (sports, finance, health calculators, local business, flights, learning, follow-ups, onboarding, automations, writing) (observed: static).
4. Round-trip channel: widgets can send a follow-up user message, call a server action without a model turn, or run a small set of client actions (observed: static).
5. Markdown extension directives in assistant text: content references, citations, writing blocks, code comments, follow-up chips, created-thread cards, inline visualizations, live visualizations (observed: static).
6. Inline visualizations (Codex): the model writes an HTML file, the app shows it in an isolated web view with a host-style kit and an allowlist of script sources (observed: static).
7. Apps/MCP widget host: tool results from connected apps render as embedded app frames with host theming, a source viewer and a follow-up confirmation dialog (observed: static).
8. Agent workflow widget: the live step list of an agent task with search, reading and writing groups, sources, wait-state text, permission and tool-approval cards (observed: static).
9. Reasoning display: "Thinking" shimmer, then "Thought for N seconds"; a skip-reasoning control; per-turn "Worked for N" line (observed: static).
10. Citations and sources: inline citation pills with hover carousel, sources side panel, file citations with line/page/slide/sheet locators, memory citations (observed: static).
11. Media in markdown: images with preview, audio and video players with save/speed/PiP, expandable tables, GitHub PR preview chips (observed: static).
12. Artifact preview with annotations (documents, PDFs, slides, spreadsheets): select content, ask for changes, send as a batch, accept or undo (observed: static).
13. Codex task surface: plan card and side panel, subagents panel, multi-agent action rows, approval requests, auto-review denials, usage-limit notices, end-of-turn "Worked for" line (observed: static).
14. Codex review and PR surface: commit form with generated messages, draft PR creation, PR side panel with reviewers, checks, "Fix all", automatic fix, links that open a diff at a file and line (observed: static).
15. Model-facing prompt fragments for the desktop app: media display, file referencing, PR diff links, automations, thread coordination, worktrees, sidebar, non-technical mode, inline code comments, follow-up chips, heartbeats, writing blocks (observed: static).
16. Feature gating names (names only, see 2.14).

---

## 2. Feature specs

### 2.1 Widget language v1: server-sent widget trees (observed: static)

- What it is: a reply message can carry a "widget" (or a named prebuilt widget plus data). The payload has a version field, an optional URL for the program source, and a flag saying the widget contains personal data (so it is withheld from shared pages).
- Structure in words: a tree of nodes. Each node has a component name, props, and children. Any prop can be an expression string (a small safe formula language, similar to CEL: field access, comparisons, ternary, string and list helpers) that is evaluated against a state store and app data. Some props are "slots" that take a sub-tree. A node can repeat over a list.
- State: a state store with named values; nodes read and write it. Widgets can also keep per-widget state across reloads (see 2.4 limits).
- Actions: a node can carry an action with a handler of either "client" or "server" (see 2.5). Actions can chain.
- Error handling: each node has its own error boundary; one bad node shows nothing or a small fallback while the rest keeps working ("soft recovery"). A compile or evaluation failure of the whole program clears the rendered tree instead of crashing the message.
- Why it helps the user: a reply can be a form, a chart, a card or a list instead of a paragraph; the user taps instead of typing.

### 2.2 Widget language v2: model-written program (observed: static)

- The model emits a short program, not a JSON tree. The program calls a render function once with a root element. It can declare state and effects like a UI program (state, memo, callback, ref, effect, a stable id, a "now" clock that ticks).
- Data hooks the program can read: app data (the facts the server attached to this message), constants, theme (light or dark), breakpoints and an is-mobile flag. These update the view when they change.
- Two kinds of building blocks:
  - Lowercase primitives, always available in the client: layout and text and form pieces (see 2.3).
  - PascalCase components that are resolved on the server: the program names them, the host asks the server for their data and shows them when resolved. The reply carries a list of required components; the host waits for them (status pending, resolved or failed per component) before drawing the whole thing. Link previews (title, favicon, image) are also pre-fetched per URL and passed in.
- Sandbox: the program runs inside an isolated frame with a worker; host and frame must agree on a protocol version (14 in this build) or the widget fails with a "protocol mismatch" error. Strict content-security rules apply (a per-load nonce is used). A frame that violates policy reports "policy denied". The host can request a diagnostic snapshot from the frame, and sends recovery diagnostics back so the model or server can repair a broken program.
- Failure states: payload too large (the whole widget is dropped and the plain text stays); compile error; runtime error in a callback (the callback error is caught and reported, the view remains); component failed (that part is empty, others draw).
- Re-resolution: when a message scrolls back into view after a long time, server components can be asked again ("refresh on visibility"), so stale data (prices, scores) updates.
- Placeholder: while a turn is still preparing a widget, an empty assistant message with a widget placeholder is shown so the layout does not jump; margins tighten while a widget is in flight.
- Why it helps: the model can build a custom small tool (a split-the-bill calculator, a comparison that reacts to a slider) in one reply, and it stays inside a sandbox.

### 2.3 What the model can use (component catalog)

Primitives (observed: static): Box, Row, Col, Flow, Grid, Spacer, Divider, Overflow row, Scope, Each (repeat), Show (conditional), Text, Title, Caption, Label, Bold, Italic, Underline, Highlight, Code, Math, Markdown, Icon, Image, Search image, Svg, Favicon, Badge, Button, Pressable, Form, Input, Textarea, Select, Checkbox, Radio group, Slider, Segmented control, Date picker, Menu, Popover, Table, List, Chart, Pie chart, Map, Carousel (base carousel with horizontal swipe), Animate, Animate group, Transition, Shimmer text, Loading block, Loading dot, Loading indicator, Pulse indicator, Run-interval, Audio player, Speech synthesizer (play/stop a spoken line), YouTube embed, and a hook to the agent-workflow widget.

Prebuilt named components (observed: static; grouped, about 236):
- Health calculators (about 50 single-purpose clinical score widgets, for example stroke risk, pneumonia severity, glasgow coma scale, bleeding risk).
- Sports: schedules and standings for NBA, NFL, NHL, MLB, EPL, IPL, F1, NCAA men and women, WNBA; scores; box scores; player and tennis summaries; cricket match; soccer, football and march madness widgets.
- Finance: stock chart, stock comparison, heatmap; a "ledger" family of personal-finance cards (accounts, net worth, credit score and detail, spend by category with recent transactions, recurring and upcoming activity, income tracker, portfolio distribution, watchlist, fee and interest, category correction, repair bank connection, onboarding, status banner); currency and unit converters.
- Local and commerce: local business, restaurant (menu, availability, reviews, reservation time pills), hotel/point-of-interest style entity cards, product carousel and rationale, flight results, flight search form, flight tracker, package tracker, jobs, care providers, events and sidebar widgets.
- Information: weather (three versions, with source), news article and carousel, Reddit threads, web links carousel, entity card, entity overview and carousel, entity explorer, person, people-also-ask, fact table, tabs and tab groups, bento layout, image grid, image generation widget, async video carousel.
- Productivity: checklist and prompt checklist, clock, stopwatch, timer, calendar events and list, draft email and email preview, poll, rating, copy-words, word card, shared activity planner, personality quiz.
- Learning: flashcards (open, update, play audio, flip), quiz, vocab card, fill-in-the-blank, sentence builder, speak card, image/audio label cards, image choice card, a learning-visual host, homework helper, multiple-choice block.
- Ask-the-user inputs: ask for location (two versions), details, files; the model uses these instead of a free-text question.
- Follow-ups: pill group, card group, choice card, link list, quiz, conversation follow-up actions, plugin follow-up.
- Automations: suggest automation, list automations, display automation, offer scheduled prompt, offer site schedule, automation plan summary, task expansion and autopause cards.
- Onboarding and promos: onboarding text, feature card, selection card, starter tasks, plugin suggestions, what's-new cards, voice conversation offer.
- Writing: writing block (see 2.7).
- Not a card but related: widgets can be marked "hidden" or "invalid" in the reference-type switch, and unknown types fall back to plain text.

### 2.4 Streaming and partial rendering (observed: static)

- Directive sequences stream: a message can contain several widget directives that appear one after another; a flag tracks that a directive sequence is still active so the message does not finish early.
- Text streaming has a safety buffer: unfinished directive or fence lines are held back until they can be classified, so a half-written directive never flashes as raw text (observed: static, writing-block and visualization parsers match whole lines only).
- Placeholders: widget skeleton (loading blocks, shimmer) while pending; reveal animation when resolved.
- Widget state persistence limits: a serialized state is capped at 8,192 bytes, 8 levels deep and 128 entries (512 entries for the combined widget store); anything over is silently dropped, never an error. State is kept per conversation in local storage so a reload restores it.

### 2.5 Interaction round-trips (what a click sends back) (observed: static)

- Client actions the host supports: "issue a new turn" (sends a follow-up user message with a query string and extra fields, as if the user typed it), "open entity detail" (a category plus a query; categories include local business, restaurant, hotel, hospital, point of interest, urgent care), "open image lightbox" (a list of images plus the selected index), "open URL", "copy to clipboard", "log event" (telemetry only), and a plugin-tool run (one tool name plus a JSON arguments object).
- Server actions: the widget posts the action to a "widget server action" endpoint and updates its state from the response; no model turn happens. A public variant of the endpoint is used on shared pages.
- Negotiation: the host tells the server which client actions it supports; the server only sends widgets whose actions the host can run. An action may name a fallback action; the chain is followed at most three deep.
- Gating: a widget can ask that its actions be disabled while a turn is streaming or while earlier actions are pending, so a user cannot double-fire. In shared or read-only views, actions that would send a message to the model are removed.
- Specific widget handlers exist for flashcards (view started, card viewed, flipped, audio requested, error shown) and the homework helper; each logs a named event.
- What a click does to the conversation: a new-turn action appears in the thread as a normal user message and starts a normal assistant turn; a server action only changes the widget.
- Why it helps: questions become buttons; the model asks less and the user commits with one tap.

### 2.6 Markdown directives in assistant text (observed: static)

- Content references: the reference-type switch includes attribution (with footer), businesses map, calculator, checkout confirmation, client-defined widget, container, entity and entity metadata, explore more, file and file navigation list, follow-up (several), forecast, grouped webpages (with a model-predicted fallback), image group and inline image (plus optimistic variants), library folder, link title, location search, map, MCP source, nav list, product and product group/reviews/rationale, python, sources footnote, sports (schedule, standings), status, stock, time, title citation, tldr, url, video, webpage (and extended), writing, and the widget type.
- Citation pill: a small chip in the text; hover opens a card with a source carousel (previous and next, "2/5"), date and snippet; additional sources are announced to screen readers; "View sources" at message actions opens a Sources panel. Memory citations show "N memory citations" and a "Memories cited" tooltip; there is an introduction banner explaining memory sources, with dismiss.
- File citations (Codex): chips with the file name and a type label (code, document, image, presentation, spreadsheet, file) and a locator: line, line range, page, slide plus object, or sheet plus object.
- Code comment directive: one line per comment with a short title, a one-paragraph body, a file, optional start and end lines and a priority from 0 to 3; renders as a review comment attached to the line.
- Follow-up chip: a list item with a visible phrase and a full prompt; clicking sends the prompt as the next user message.
- Created-thread directive: after the model creates a thread, a line carrying the thread id renders as a card linking to it.
- Inline visualization directive: one line naming an HTML file (and optionally a thread); an alternate "live" variant exists; an optional "wide" layout mode and a title.
- Rendering errors: a failed markdown render shows "Markdown couldn't render" with "Try again".

### 2.7 Writing blocks (observed: static)

- A fenced block with a variant (email, chat message, social post, document, standard, or one defined by an active skill) and a unique id. Used only when the reply delivers a finished artifact the user can copy and use elsewhere; not for explanations, plans or progress.
- Email variant takes subject, recipient, cc, bcc as attributes; recipient fields fill the compose fields, so the model must not put addresses in the body and must not invent them.
- Tone alternatives: two or three alternatives inside one block, each starting with a marker line; a single tone section is forbidden. At most three blocks per reply; the same id means "revision of this block".
- UI: the block shows as an editable card with copy/use actions (the block has its own directive renderer).

### 2.8 Inline visualizations in Codex (observed: static)

- The model writes an HTML file and points a directive at it. The app opens it in an isolated web view, not in the chat DOM.
- Protocol: the page talks to the host over the MCP-Apps style app protocol. The host passes style variables so the page matches the app theme; the page reports ready, height, click, hover, download and error events back; the host can capture a screenshot of the view for the model to look at.
- Allowed script and style sources are an allowlist: the vendor chart CDN, cdnjs, jsDelivr, esm.sh, unpkg and Google/Bunny font hosts, plus blob and data URLs. Connections are limited to blob and data (no network calls from the page). Base and frame domains are empty. A bundled mini kit of chart styles is available.
- Reading rules: the host only serves files that are inside the thread's own boundary (the workspace or the thread output directory); other paths are refused.
- States: loading skeleton, ready (height set by the page), error (an error card, plain file link remains), download.
- Why it helps: the model can draw a real chart or diagram in one HTML file, inline, without shipping a UI kit.

### 2.9 Apps/MCP widget host (observed: static)

- A tool result from a connected app can include an embedded frame. The host styles it with the app theme, gives it fullscreen and picture-in-picture, and brokers messages.
- When the embedded app asks to send a follow-up message, a confirmation dialog shows the message first (follow-up confirmation); a context preview shows what data the frame is about to send to the model.
- Tool approval phrasing is templated per connector: "Allow <app> to <action>?" style questions for GitHub, Figma, Gmail and others, with Approve and Cancel.

### 2.10 Agent workflow widget (observed: static)

- A live list for an agent task: steps with working and worked forms ("Searching the web", "Searched the web", "Searching: <query>"; "Scanning document"; "Analyzing image"; "Thinking" and "Thought for a while"; "Preparing response"; "Worked for <duration>"). Reading and writing groups are labeled by connector ("Reading <app>", "Writing <app>"). A Sources button and a "+N more" list. Code steps have "See all N lines" and "Hide lines".
- Task states: working, canceling, "Task canceled", canceled item label.
- Waiting text is rotated from banks of short phrases (such as "Booting up agent", "Mapping things out", "Gathering details", "This is taking longer than usual", "Feel free to check back in a moment") so a long wait feels alive; separate banks for the first load, mid-task and long outliers.
- Cards inside the flow: connector authorization ("<agent> needs access to continue"), tool approval ("<agent> needs your permission to continue") with Approve and Cancel, connect/reconnect/connected/not now, generic tool card with Read/Write/Request/Details and "View fullscreen", calendar event card (create and save variants).

### 2.11 Reasoning and progress (observed: static)

- A thinking shimmer is shown while reasoning; afterward a collapsed row reads "Thought" or "Thought for <elapsed>". There is a control to skip reasoning (with an error message if it fails).
- Progress is shown as: tool rows, a "Worked for <time>" row at the end of a turn, an "update running summary" tool (3 to 6 words, at most 50 characters, in the user's language, such as "Refining the layout") that fills the pill showing what the agent is doing, and the plan card (below).
- The running summary tool is described to the model as: call it once when starting substantial work, only again when the objective or phase really changes, never on a timer or for routine tool calls, no tool names or file names, skip for brief answers.

### 2.12 Codex task surface (observed: static)

- Plan card: titled "Plan" (and "Writing plan" while streaming), collapsible, expandable to a side panel, downloadable. Plan items group into tasks with titles; empty-title items are ignored.
- Subagents: a panel titled "Subagents" with counts "Active - N" and "Done - N", "No active subagents", per-agent model and reasoning effort (none, minimal, light, medium, high, extra high, max, ultra), working/waiting status and "N ago" for finished ones; clicking opens the subagent's own thread, with "Back to subagents". In the main thread, rows summarize actions: created, messaged, resumed, closed, interrupted, listed, each with in-progress, completed and failed forms, and agent states (running, completed, errored, interrupted, shutdown, pending init, not found).
- Approvals: an "Awaiting approval" row; when the automatic reviewer denies an action, a card explains "Why it was denied" and "What approval allows" (records approval and allows one retry; it does not run it). After repeated denials a warning reads "Turn ended by Auto-review", with a hint to add context or switch permission mode.
- Summary panel: a toggleable, pinnable end-of-turn summary panel with its own error fallback.
- Usage limits: a notice whose wording depends on plan and role (add credits, upgrade, wait until a reset time, ask the workspace owner).
- Terminal: an integrated terminal with tabs, a reload on error, and a warning when the terminal is outside the chat's workspaces.
- Non-technical mode: when the user asks for a non-technical UI, the app hides shell output and the model is told to describe actions in plain words and avoid citing intermediate code.

### 2.13 Review, diffs and PR flows (observed: static)

- Commit form: message box with "leave blank to generate", branch target (existing or new), include-unstaged option, buttons Commit, Push, Commit and push; generation can fail with a clear message; creating a PR or draft PR shows a loading title.
- PR actions menu: open in browser or the provider, copy link, pin, close, reopen, convert to draft, mark ready, and "Review with Codex".
- PR side panel: request reviewers (search users), checks list with "Fix all" and "Remove all", automatic fix (start, pause, retry on schedule, "View progress").
- Links: the model can write a link in a custom scheme carrying the PR URL, file path, line and side (left or right); clicking opens the diff at that spot.
- Inline code review: comment directives (2.6) attach to diff lines; comments carry priority.
- GitHub PR chips in text show repository, number and status (draft, open, closed, merged).

### 2.14 Artifact preview and annotations (observed: static)

- Previews for documents, PDFs, presentations and spreadsheets with a table of contents (dockable), slide context menu (delete, duplicate, hide/show) and element ordering (bring forward, send back).
- Annotations: select text, an object, a cell or a range; a comment box ("Describe a change or ask a question"); annotations collect into a batch ("N annotations") and "Apply these N annotations" sends them as one request; while the model works the batch shows "Working"; afterwards "See latest changes", "Accept all", "Undo", "Dismiss". Save and send failures have their own messages.

### 2.15 Feature gates and flags (names only; not enabled)

Names seen in strings, not values: model-written widget payload flag, widget feedback flag, link-impression telemetry flag ("genui link shown/impressions"), disable-actions-during-active-turn (a per-widget switch), refresh-on-visibility, shared-page restriction for widgets, plan-dependent usage-limit variants, a sidebar-section tools flag, a thread tools flag, a worktree tools flag, a workspace-dependencies flag, a prose-detail-level instruction flag, a heartbeat flag, a hosted-automations flag. Real gate IDs were not recovered (they are opaque).

---

## 2.16 What the model is told (paraphrased)

Desktop-app context (observed: static):
- Media: use normal markdown image syntax to show images, video and audio; for local files give an absolute path, never a relative one; to play audio, use the same image syntax with the audio file path. Prefer media already shown by a tool; use a preview tool for remote video and audio; do not download remote media to dodge display limits.
- Files: always reference code and workspace files by full absolute path; web URLs as markdown links.
- PR links: use the review link scheme with URL-encoded PR URL and file path, a verified one-based line, and a side; enterprise links must use the task's own git host.
- Automations: search for the automation tool and follow its schema instead of writing raw directives. Heartbeat monitors: stay quiet unless something meaningful changed; do not add "status update every run" unless asked.
- Threads: "task, thread, chat, conversation" are synonyms; use thread tools to create, fork, read, wait for, message, hand off, pin, archive or title threads; prefer compact wait snapshots over repeated reads; only create a user-owned thread when the user explicitly asks; for subtasks use multi-agent tools instead; after creating a thread, emit the created-thread line.
- Worktrees: reuse a suitable existing checkout unless asked; create and archive worktrees with tools.
- Sidebar: use the section tools to organize; moving into the pinned section pins.
- Non-technical UI: describe what commands do instead of naming them; do not cite intermediate code; give technical detail only when asked or when it helps debugging.
- Inline code comments: emit one directive per actionable comment and none when there are none; tight line ranges; priority 0 to 3.
- Follow-ups: emit artifact follow-ups as list items with a visible phrase and the full prompt; no closing bracket in the phrase; escape quotes in the prompt.
- Heartbeat turns: the user message may be a system heartbeat; end with exactly one non-empty block carrying a decision (notify or do not notify), a one-line message, and the automation id; never reply empty; delete or pause the automation when it no longer serves its purpose.
- Writing blocks: described in 2.7.
- Git: a branch prefix, commit instructions and PR instructions from user settings are appended.

Widget side (inferred from strings and from the program API; the full authoring prompt is server-side and was not in the package):
- The model is given the primitives and hooks (2.2), a facade with the client actions (2.5), and the rule that a program must call render once.
- Entity detail categories are a closed list (2.5).

---

## 3. Top 10 ideas for Faber, ranked by value over effort

1. Follow-up chips as a directive (a list item with a short visible phrase and a full prompt). Cheap to parse, removes typing, and works with any CLI because it is only text the agent writes. Add the instruction to the system context of each agent.
2. Question-as-buttons: a "choice card" and an "ask the user" widget (options, yes/no, short form). The click sends the answer as the next user message. This is the 80 percent of generative UI with 5 percent of the risk.
3. Code comment directive for reviews (title, body, file, start, end, priority) rendered as line-attached comments in Faber's diff view. Matches how Faber's Projects area already deals with files.
4. Plan card with collapse, side panel and download, plus a "Worked for N" end-of-turn line and a running-summary one-liner (3 to 6 words). Replaces walls of tool text with one stable status row.
5. File citation chips with line, page, slide and sheet locators, and absolute-path rules in the agent instructions, so every file mention is clickable and opens at the right place.
6. Inline visualizations as HTML files shown in an isolated web view with a small allowlist, style variables from the app theme, a height report and a screenshot hook so the agent can look at its own chart. High value for Faber's "rich output" goal; medium effort (a sandboxed view in Tauri, a path-boundary rule).
7. Subagents panel with active and done counts, model and effort per agent, waiting state, and a "back" path from a child thread, aligned with Faber's agent-with-helpers direction.
8. Approval card standard: one row "Awaiting approval", a denial explanation ("why denied" and "what approval allows"), and a stop-after-repeated-denials rule.
9. Writing blocks for finished artifacts (email, message, post, document) with copy and edit, and tone alternatives; stops the agent from burying a draft in prose.
10. Widget actions with a closed set (send message, open URL, copy, open file, server action) and a rule that actions are disabled while a turn streams, plus size and depth limits on any saved widget state (8 KB, depth 8, 128 entries are sensible numbers).

Design cautions worth copying: unknown or oversize widgets fall back to plain text; every widget node can fail on its own; widgets are removed in shared views when they could send messages; the model is told to use a widget only when it delivers something text cannot.

---

## 4. Unknowns and bounds

- Nothing was run, so streaming behavior, timings, animation and real layout are not (observed: runtime).
- The authoring prompt that teaches the model the widget language and the component list, and the server-side component resolvers, are not in the package; component data fields are known only from the shipped component sources and strings for the ones read.
- Exact field-by-field schemas for most of the 236 components were not recorded here; only groups and examples.
- Feature gate values and rollout rules are not recoverable statically.
- The visualization tool the model calls (its description) and the exact style variables were not recorded; only the allowlist and event set.
- Memory and processor cost of the widget sandbox is unmeasured.
- Not covered: native-helper binaries and the non-asar folders (skills, plugins, template picker), which another worker covers; the session archiving study already done.
- REA evidence for this target is partial (one retained evidence item; the full response was too large to read).
