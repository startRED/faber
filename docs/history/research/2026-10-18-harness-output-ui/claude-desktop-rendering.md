# Claude Desktop: how assistant output is rendered and made interactive (behavior spec)

Target: Claude Desktop for Windows, v2.37337.1.0, a lawful copy owned by the user. The app bundle (main process) and the bundled claude.ai web frontend ("Claude Web") were read from the lead's read-only extraction. Date of study: 2026-10-18.

## 0. Method, evidence tags, limits

- Clean room. This file describes behavior in plain words. It reproduces no code, no pseudocode, no internal function/class/variable/module names, no file structure, no source comments. Product-visible tool names and short on-screen strings (under 15 words each) are quoted. Model-facing instructions are paraphrased, never copied.
- Method: text search and reading of the extracted JavaScript bundles and the English string table, plus the bundled resource folders. No app was launched, no runtime was observed, no network was used, no credential or personal data was read.
- Evidence tags: `observed: static` (read directly in the shipped bundle or string table), `inferred from strings` (deduced from UI strings or tool lists, behavior not read end to end). No `observed: runtime` item exists in this document.
- REA Evidence IDs: none. The REA analysis tools were not invoked; plain text search on the extracted corpus was enough. If the lead wants IDs, rerun the same questions through REA.
- Overlap: the sibling spec `claude-web-rich-output.md` is based on public documentation. This one is based on the shipped bundles, so it adds the exact model contract (what the model is told), host mechanics, and strings.

## 1. Inventory (one line each)

1. Visualize inline widgets: built-in server with two tools (`show_widget`, `read_me`) that draw SVG/HTML visuals inline in the chat. (static)
2. `read_me` guidance modules: diagram, mockup, interactive, chart, art, elicitation, plus a design-system core. (static)
3. Widget host shell: streaming DOM morph, deferred scripts, theme variables, sizing, link interception, copy/download menu, `sendPrompt` bridge. (static)
4. Elicitation forms: model-authored HTML forms whose answers come back as one chat line. (static)
5. MCP Apps host protocol: sandboxed iframe, handshake, theme/CSP/permissions, display modes. (static)
6. Inline display tools (Claude Web): about 25 structured cards rendered while the tool input is still streaming (chart, options, steps, comparison, featured, product carousel, link preview, places, itinerary, recipe, weather, quiz, translation, message compose, writing draft, offer next step, and others). (static)
7. Question cards: `ask_user_input` (chat) and `AskUserQuestion` (Code/Cowork), docked above the composer. (static)
8. Code-tab app tools for the model: `update_plan`, `wrap_up`, `mark_chapter`, `show_pane`, `close_pane`, `get_layout`, `set_transcript_view`, `start_session`, `read_widget_context`, plus an "aside" reply mechanism. (static)
9. Transcript item model and three transcript views (normal, thinking, verbose). (static)
10. Tool-call rendering: rows and groups with live and past verbs, counts, step collapsing, subagent and parallel-agent cards, file and web steps. (static + strings)
11. Permission cards (Allow once / for session / always) and the plan card with Accept variants and inline comments. (static)
12. Thinking display ("Thinking…" row, summaries, a thinking-only transcript view). (inferred from strings + static)
13. Markdown renderer: incremental block parser, tables, task lists, callouts, KaTeX math, mermaid, a probability "hypothesis tree" fence, image zoom, GitHub reference chips. (static)
14. Code blocks: syntax highlighting in a worker, copy, and a Run button on shell blocks with output below and a send-output-to-Claude variant. (static)
15. Artifacts (Web): sandboxed iframe protocol with typed content (code, mermaid, react, html, markdown, svg, pdf), versions, share, capability bridge (storage, AI completions, run code, proxy fetch, comments). (static)
16. Cowork live artifacts: create/update/list/verify/screenshot tools and a page-side bridge to call connectors, ask Claude, and run scheduled tasks. (static)
17. Cowork system prompt: formatting, tone, ask-first, task list, sources, file-link and artifact rules. (static)
18. Status and notice strings: usage-limit pauses, max-length continue, tool-use limit, blocked content, "Retry/Discard", queued/not delivered, nudges, sources. (strings)
19. File and download affordances: present_files links, "Show in Explorer", download-and-open, file chips by type. (static + strings)
20. Bundled office/PDF skills and bundled chart/diagram libraries used by artifacts and file creation. (static)
21. In-app changelog of the Code tab (what-new list) as a UI pattern. (static)

---

## 2. Feature specs

### 2.1 Visualize inline widgets (the model's drawing tool)

Behavior:
- A built-in server exposes `read_me` and `show_widget`. The model must call `read_me` once before its first `show_widget` and picks one or more modules: diagram, mockup, interactive, chart, art, elicitation. The tool description tells it not to narrate the setup call to the user. (static)
- `show_widget` takes a snake_case title (also the download filename), the code (SVG when it starts with an svg tag, otherwise an HTML fragment), 1 to 4 short loading messages shown while rendering, and a mobile/desktop sizing hint comes from the `read_me` platform parameter. (static)
- The result renders inline in the message flow as a card that fills the column width. Widget width is about 680 px on desktop and about 380 px on mobile; the guidance changes with it (two columns maximum on mobile, flowcharts still authored at the wide size and scaled down). (static)
- Loading messages: the tool schema asks for playful messages normally, but boring literal ones for serious topics (illness, grief, war, disaster, medical decisions). This is a deliberate tone rule. (static)

Streaming and states:
- The code streams token by token. The host parses the partial markup incrementally and merges changes into the live DOM by diffing, so earlier parts do not flicker. Scripts are held back while streaming and run only after the stream completes; a shimmer shows while external libraries load. SVG shapes fade in as they arrive. (static)
- Because of this, the model is told: style first and short, content next, script last; prefer inline styles; no gradients, shadows or blur (they flash during diffs); no hidden/tabbed content during streaming (hidden content streams invisibly); everything stacked vertically; auto-fit height with no inner scrolling; never fixed positioning (the frame measures in-flow content, fixed elements collapse it to a minimum height of about 100 px, so use a tall normal-flow faux viewport for modal mockups). (static)
- Fullscreen: explicitly unsupported. The model is told never to call fullscreen or display-mode APIs and never to fake it; if the user wants "expand", build an expanded layout in place. (static)

Model contract (paraphrased from the read_me text):
- Division of labor: all explanation and prose goes in the normal reply text, outside the tool call; the tool output holds only the visual. No headings or paragraphs inside the widget. Tables go in markdown in the reply, not in the widget. (static)
- Be quiet and flat: transparent outer container (the host supplies background), thin 0.5 px borders, 8 px control radius, 12 px card radius, two font weights only (regular and medium), sentence case everywhere, no emoji (icons come from an outline icon webfont already loaded), minimum 11 px text, no bold mid-sentence, bold only for labels and headings. (static)
- Theming: the model must use host-provided CSS variables for surfaces (three levels), text (primary/secondary/muted plus accent/danger/success/warning), borders (default/strong/stronger plus role borders), fonts (sans, serif "voice", mono), radius, padding and gap scales. Dark mode is mandatory: SVG uses nine prebuilt color classes (purple, teal, coral, pink, gray, blue, green, amber, red; seven stops each) that flip automatically; HTML must use variables, never hard-coded text colors. (static)
- Color semantics: color encodes category, not sequence; two to three colors per diagram; gray for neutral; reserve blue/green/amber/red for info/success/warning/error meaning; text on a colored fill uses the darkest stops of the same ramp, and title and subtitle must use two different stops. (static)
- Complexity budget: box subtitles at most 5 words, at most 2 color ramps per diagram (add a one-line legend if color carries meaning), at most 4 full-width boxes per row; detail moves to click-through via `sendPrompt` or to prose. (static)
- Accessibility: HTML widgets start with a visually hidden one-sentence summary heading; SVG uses an image role with title and description; charts get an image role, a label, and fallback text; never rely on color alone (add dash, marker or pattern cues). (static)
- Interaction: `sendPrompt(text)` posts a message as if the user typed it, to be used only when the next step benefits from the model thinking; filtering, sorting, toggling and math stay in widget JavaScript. Buttons that call it carry a trailing arrow. Links are plain anchors; clicks are intercepted and open the host's link-confirmation dialog. Inputs must validate before acting (inline error text, no advancing on empty). Every displayed number must be rounded. (static)
- Libraries: only from an allowlist of six CDN hosts (cdnjs, esm.sh, jsdelivr, unpkg, Google Fonts); everything else is blocked silently. A library script must precede the script that uses it; plain scripts, no modules. (static)
- Thinking budget hint in the tool text: keep reasoning short (about 500 tokens); for complex visuals consider an artifact instead. (static)
- Use-case patterns in the guidance: interactive explainer (controls plus live readout, no card wrapper), compare options (card grid, highlighted recommended option with a thicker accent border and badge, `sendPrompt` buttons), data record (single raised card with avatar and key/value rows), diagrams (flowchart, structural, illustrative, with a font-size calibration table, viewBox safety checklist, box-overlap checks), charts (Chart.js with custom HTML legends, wrapper-height sizing, number formatting, one y-axis only, color follows entity not rank, stat tile / meter / table chosen over a chart when the data is one value or one ratio), geographic maps (choropleth), art (full canvas, bold colors, free custom styles allowed). (static)
- Copy rules for any UI text the model writes in widgets: buttons verb-first, 1 to 3 words, no punctuation; errors say what happened then what to do, no "Error:" prefix; empty states invite; placeholders show a real example; avoid exclamation and first person. (static)

Host actions on a rendered widget (static + strings): copy as image, download as HTML, SVG or PNG, a "copied" toast, and on mobile a view-larger/zoom control. A rejected `sendPrompt` (no recent user gesture) shows a "Not sent" notice.

Why it helps: it turns long descriptions of structure, numbers or options into a compact visual the user can poke, with the model's own chat text staying short. Faber can copy the division of labor (visual in the tool, prose in the reply) and the streaming-friendly authoring rules.

### 2.2 Host protocol for sandboxed UI (MCP Apps)

- UI served by a tool is HTML with a special MIME profile, loaded in an iframe on a separate origin with scripts, same-origin, and forms allowed, but no top navigation or popups by default. The host waits up to about 30 seconds for the frame to say it is ready before failing. (static)
- Messages exchanged: initialize (host sends theme, locale, available display modes, container size), open-link (host asks the user to confirm), download-file, send-message (into chat), size-changed (frame reports content height), tool-input and partial tool-input (stream the model's arguments into the UI), tool-result, tool-cancelled, host-context-changed (theme/size changes), request-teardown, request-display-mode (inline, fullscreen, picture-in-picture). (static)
- The host sends theme as a named set of CSS variables and may inject font faces. A declared domain list per UI drives the content security policy (connect, resource, frame, base-URI domains); a declared permissions list (camera, microphone, geolocation, clipboard write) drives the iframe permission policy. (static)
- Why it helps Faber: a documented, small contract lets any tool or the model ship UI without Faber trusting it; the same shape (iframe, CSP, theme variables, a few messages) is enough.

### 2.3 Elicitation forms (collecting arguments with a form)

Behavior:
- Used when a skill or slash command needs information the model cannot infer. The model must first try to infer from the conversation and attachments, skip the form if everything is known, and ask only the missing items ("a one-question form beats five"). (static)
- Questions are phrased as conversational questions, not field labels ("Which side are you on?"). The form has a fixed header whose title is "[subject] details", a fixed icon, a body of question groups, and a footer with Skip and Continue. The shell wires toggles, the "Other" reveal, file upload and submit itself: the model writes no scripts and no click handlers. (static)
- Input kinds: pills for short labels (up to 4 words), cards with icon and subtitle for richer choices, preview tiles with a tiny drawn illustration for output-format pickers, sliders with end labels for scales, date fields, free text areas, and a file dropzone always paired with a text fallback. At least one non-pill format when three or more choice groups exist. An escape-hatch "Other" option reveals a text field and must be localized. (static)
- Color: selection is always blue; accent colors (warning, danger, success) only for the unselected state of options with a real semantic reason; never inline background/border on options (it would hide the selected state). (static)
- Files picked in the form are attached to the conversation exactly like the chat attach button.

Return path:
- Submitting posts one chat line shaped like "Contract details — Side: Customer · Diet: Vegan, Gluten-free · Deadline: 2027-01-05". Labels are the field names humanized; multi-selects comma-joined; short free text flattened; longer free text wrapped in quotes; very long text is summarized in the line with its length and repeated in full after it; uploaded files appear as "name (attached)". After submit the form fades into a read-only state. (static)

Why it helps: a form with structured answers returned as a single readable message works with any model and needs no new protocol on the wire.

### 2.4 Inline display tools (Claude Web) and cards

- A registry maps tool names to renderers, and many of them render while the tool input is still arriving (card appears and fills in). Catalog seen in the lists: chart, options card, step card, comparison card, featured card, product carousel, link preview, places (with a map), itinerary (with a map), recipe, weather, quiz, translation, message compose, writing draft, "offer next step", plus search/read result cards. (static, names; per-field schemas not recovered)
- offer-next-step: shows suggested follow-up prompts as buttons; selecting one sends it as the user's next message. (static + inferred)
- chart display: renders a native chart card (types, series, axis labels) from structured arguments, a lighter alternative to writing a Visualize widget. (static)
- Places and itinerary: map-based cards with ordered stops. (static)
- Unknown: field-level argument schemas of most cards (see section 4).

Why it helps: when the shape of data is known (steps, options, comparison, recipe), a typed card removes the need for the model to author markup and renders instantly while streaming.

### 2.5 Question cards

Two surfaces with one idea: replace a free-text clarifying question with a docked card above the composer.

- Chat: `ask_user_input` offers question types single choice, multiple choice, rank, free form and number. Answers come back as plain text with "Q:" and "A:" pairs per question. The card sits above the input and is dismissible. (static)
- Code and Cowork: `AskUserQuestion` takes several questions, each with a short header, options with label and description, an optional preview per option, and a multi-select flag. The docked question sheet shows one question at a time with a step indicator, an automatic "Other" free-text entry, and a way to skip. Previews let the user compare concrete artifacts (mockup or code) before choosing. (static + strings)
- Cowork system prompt rule: always ask clarifying questions through this tool before starting any non-trivial task, never as prose. A skill invocation text routes argument collection to the elicitation form instead of this card. (static)
- Answer text is delivered to the model as the next user turn; plan-mode exit is a sibling flow (see 2.8).

Why it helps: structured choices cut typing, give the model clean answers, and turn "wall of questions" into a card.

### 2.6 Task list, wrap-up box, chapters and the Code-tab app tools

These are app-level tools the Code tab gives the model so progress is shown as UI instead of prose (all static, from the session prompt and tool descriptions):
- `update_plan` (task list): shown under the latest message. The model must send the whole plan with the very first tool call of any turn that edits files or runs more than one command, in the same message as another tool call (never alone). Items are short imperative phrases, one is in progress at a time; "advance" completes the current and starts the next at no extra round trip; out-of-order and whole-list updates exist. A scope field says whether the request is a first task, a new unrelated task, or a continuation; new unrelated work triggers a start-session offer instead. The model is told not to repeat the plan as a numbered list in its text, and that this list replaces the CLI's own task tools.
- `wrap_up` (next-step box): called once at the end of any turn that kept a list, changed files, or ran commands. Carries a 2-to-3 sentence recap (what changed and what is not done or unverified; the first sentence becomes the sidebar status), and optionally one question with at most two buttons. The question must be one decision (not two joined by "and"), under 100 characters, with no recommendation inside it; the first button is the recommendation. Buttons are either app actions (commit, create PR, merge, fix CI, address comments, resolve conflicts, verify, archive, new session) or short replies of 1 to 3 words. If the box shows a question, the model must write no text after it. A "blocked" flag marks the session as waiting.
- `mark_chapter`: a divider in the transcript plus a floating table of contents; used sparingly (3 to 8 per session) at phase changes; title is a short noun phrase.
- `show_pane`: opens the diff, a file at a line, terminal (optionally a specific tab), PR, tasks, plan or artifact pane. Rule: "prefer showing over describing": after edits, show the diff; when pointing at code, open the file at the line. `close_pane`, `get_layout`, and `set_transcript_view` complete the set; popped-out panes are never raised or reused and the result tells the model to ask the user to look there.
- `start_session`: spawn a parallel session; modes include a user-asked start, a card the user must click (own initiative), and an offer card that waits for the answer.
- Aside: when the user types while the model works, the app answers instantly with a short reply in the model's voice, then tells the model what was said so it does not repeat itself; the model can follow up through a reply tool. Progress updates and the final message stay plain text.
- Formatting rules given to the model: file mentions are markdown links with relative path and optional line; pull requests are full links, never a bare `#123`; shell commands go in their own fenced block tagged `bash`, one command per block, no prompt, no output (the app adds a Run button); link a terminal tab with a special hash link using the tab id; only open files inside the session folders.

Why it helps: it replaces progress narration and "what next?" paragraphs with a list, a recap box and buttons, and replaces "go look at the file" with an action.

### 2.7 Transcript model and views

- Each assistant turn is a sequence of items of four kinds: text, thinking, tool, card. Tool items group consecutive calls; cards are the inline display or app-tool results. (static)
- Three views: normal (tools collapsed to a summary), thinking (adds reasoning), verbose (shows everything including tool input and output). The model can switch the view for the user. (static)
- Run summary and "Claude was nudged" rows mark system-originated turns; "Ran scheduled task" marks scheduled runs. Messages can be queued ("Queued"), marked not delivered with a retry control, and can have a hidden original prompt viewable via a "View sent prompt" debug action. (strings)
- Long-output collapsing: tool groups collapse to counts ("N steps", "Hide steps"), search results show "+N more" and "Show more/less". An exact generic long-text collapse rule was not found. (strings; unknown rule)

### 2.8 Tool-call rendering

- Row pattern: a verb in present form while running and past form when done, plus the object: "Reading {file}" then "Read {file}", "Editing/Edited", "Creating/Created", "Running a command" then "Ran a command" with an Output section, "Searching" then "Searched" with a result count, "Reading {hostname}" then "Read a page" with "Couldn't read" on failure, "Searching the web" then "Searched the web" with a Results list, and "Stopped …" variants for cancelled steps. (strings)
- Groups: a header with a step count ("N steps"), Hide/Show steps, failed-count badge, "(a tool failed)" appended in the accessibility summary. (strings)
- Subagents: a card "Running subagent" then Done / Failed / Stopped, with step counts; multiple agents show as "Parallel agents" with "Running N agents" and "Ran N agents", each with "in progress"/"Done" per item and "No steps yet" before the first. (strings)
- Connector calls: "Claude wants to use {tool} from {integration}" with Request and Response sections and "Tool result", "Connector data hidden when shared" in shared views. (strings)
- Screenshots: thumbnail with "Click to view full screenshot"; "N image attachment" for accessibility. (strings)
- Compaction: a progress line "Compacting our conversation so we can keep chatting..." with a percentage. (strings)
- File outputs: a chip with a type label (Code, Table, Spreadsheet, Document, Image, Diagram, Presentation, Skill, Plugin), "Download", "Download all", "Open in {app}", "Show in Explorer", with errors like "This file is too large to download." (strings)
- Diffs and terminal output live in panes (diff pane, terminal panel) opened by the model's `show_pane` and by user click, not inline in the message.

### 2.9 Permission cards and the plan card

- Permission card: asks about a tool or action, with Allow once, allow for this session, and Always choices, a short human-readable description, and the exact input behind a disclosure. Auto/accept-edits/plan permission modes shown as a mode switcher; "Couldn't switch to auto mode" error exists. (static + strings)
- Plan card: shown when the model leaves plan mode. Accept variants (accept and continue with different permission modes), a reject path, and inline comments on plan text that go back to the model as feedback. (static + strings)
- Tool-use limit and max-length notices appear as inline system notices with a Continue button. (strings)

### 2.10 Thinking display

- A "Thinking…" row appears while reasoning runs; completed reasoning collapses to a summary line; in the thinking and verbose views the reasoning text is shown in full. Reasoning is never mixed into the answer text. (inferred from strings + static item kinds)

### 2.11 Markdown renderer

- Streaming: markdown is parsed in blocks. Finished blocks are committed and never re-render; only the last (frontier) block re-parses as tokens arrive. An unclosed inline span (bold, code, link) is marked incomplete so it styles smoothly and does not flash raw markers. Fenced code is held until the language tag has settled, then highlights. Each new block fades in. (static)
- Supported: tables (with an expand control for wide ones), task lists, callouts (GitHub-style note/tip/warning blocks), KaTeX math inline and block, strikethrough, autolinks, images with zoom, GitHub issue/PR reference chips, footnote-like source links. (static + strings)
- Special fences: `mermaid` renders a diagram in a sandboxed frame with a toggle between diagram and source and an error fallback; `math`; and `search_tree`, a "Hypothesis tree" with probabilities for competing hypotheses. (static)
- Citations: web answers show a "Sources" affordance and source chips; Cowork's prompt demands a Sources section of markdown links after any answer that used web search. Exact chip rendering was not read. (static + strings; partial)

### 2.12 Code blocks

- Highlighting is done in a worker so streaming stays responsive; header shows the language; copy button with a "Copied" state. (static + strings)
- Shell-tagged blocks get a Play button with tooltips "Run in terminal", "Run in terminal and send output to Claude", "Running — click to cancel". The command runs in the terminal panel; a result area under the block shows the output, or "(no output)", with an action to open the full output in the terminal; the "send output to Claude" variant feeds the output back as the next message. (static)

### 2.13 Artifacts (Claude Web)

- Content types: code, mermaid, react, html, markdown, svg, pdf. Each renders in a sandboxed iframe on a separate origin; the host and frame exchange typed messages (set content, theme, errors, size, navigation requests, capability calls). Errors in the frame are surfaced to the panel and can be sent to the model to fix. (static)
- Panel: side panel with the artifact, a version selector ("Previous version/Next version" with an "N / M" counter), copy, download, Share/Hide/Open labels, and "Untitled artifact" until titled. Chat shows a compact chip "Created artifact"/"Updated artifact". (static + strings)
- Capabilities a page may use: persistent storage (per artifact), model completions from inside the page, running code, proxied fetches to allowed hosts, and comments. Publishing makes a shareable link with access levels. (static)
- Bundled libraries available to artifacts and file creation: chart library, a grid/table library, a diagram library. (static)

### 2.14 Cowork live artifacts

- Tools: create, update, list, verify and screenshot an artifact. "Verify" renders it and reports errors; "screenshot" lets the model look at its own result. A page-side bridge lets the artifact call connector tools, ask the model, and trigger scheduled tasks. The model is told to reuse live artifacts for dashboards and trackers, and never use browser storage for state that must persist. (static)

### 2.15 Cowork system prompt formatting rules (paraphrased)

- Default to minimal formatting; prose for conversational and simple answers; lists and headers only when asked or clearly needed; avoid bold overuse; match the length to the question.
- Ask first: for any non-trivial request, call the question tool before working, with the minimum questions.
- Task list widget for multi-step work; update it as steps finish.
- After web search, end with a Sources section of markdown links.
- Files are shared with a link to the file path (present_files / computer link); deliverables go to the outputs folder; give a short summary after a file, not a long recap.
- Artifacts: choose file types by purpose; never rely on browser storage; keep state in the artifact capabilities.
- Tone: warm, direct, no flattery, no emoji unless used by the user.

### 2.16 Status, error and notice patterns (strings)

- Usage-limit pause: "Paused: Your usage limit resets at {time}. Continue then." with variants for free usage and day names; a Continue button when allowed. (strings)
- "Claude reached its max length for this message." with Continue; "Claude reached its tool-use limit for this turn."; "Max effort uses your limits faster than High." (strings)
- Blocked states: policy block, content-filter block, "Claude is busy right now. Try again in a moment.", with Retry. (strings)
- Support cards: sensitive-topic and election-info cards with a short line, a primary action and "More ways to get help". (strings)
- Delivery states for the user's message: Queued, Not delivered with Retry, edit-message error messages. (strings)

---

## 3. Top 10 ideas for Faber, ranked by value divided by effort

1. Plan list plus wrap-up box as two tiny tools (value high, effort low). Give the CLI session a `update_plan`-like list and a `wrap_up`-like box (recap of 2 to 3 sentences, one decision question, up to 2 buttons that map to Faber actions). It removes the closing paragraph and the "next steps" prose. Adopt the rules: whole list on the first call, same message as another tool call, no text after a question box.
2. "Show, don't describe" pane tool (high, low). A `show_pane`-like command the agent calls to open the diff, a file at a line, the terminal or the plan pane. Faber already has panes; adding one allowed call and one prompt line replaces "see file X line Y" paragraphs.
3. Docked question card with options, descriptions, previews, multi-select, auto "Other" (high, low-medium). Models ask clarifying questions in prose; a card above the composer returns clean answers and ends walls of questions. Use the plain "Q:/A:" return format so it works for every CLI.
4. Tool-call rows with live and past verbs and collapsed groups (high, low). Present tense while running, past when done, count of steps, expand for input/output; default view collapses everything but the final answer. This alone removes most raw text in agent transcripts.
5. Three transcript views (normal, thinking, verbose) (medium-high, low). A single switch decides how much the user sees; thinking is its own item kind, never inline.
6. Incremental markdown renderer with committed vs frontier blocks (medium-high, medium). Freeze finished blocks, reparse only the last one, hold fences until the language settles, fade new blocks in. Cheaper on memory (Faber's R125 limit) and removes flicker.
7. Run button on shell fences, with output under the block and a "send output to the AI" variant (medium-high, low-medium). Prompt rule: one command per fenced block, no prompt character, no output inside.
8. Inline visuals via a sandboxed widget host (high, high). Adopt the shape, not the code: iframe in a separate origin, theme variables, a small message set, sendPrompt-like bridge limited to user gestures, streaming morph with scripts deferred. Start with SVG-only (no scripts) to keep memory low and security simple, then add HTML.
9. Elicitation form as a typed card (medium, medium). Instead of letting the model author HTML, offer a typed form schema (pills, cards, slider, date, file, free text, "Other") that Faber renders natively and returns as one readable line. Same idea as Visualize forms without a sandbox.
10. Typed display cards (options, steps, comparison, recipe-like, link preview, offer-next-step) rendered from structured tool input as it streams (medium, medium). Start with offer-next-step buttons and a comparison card; they turn recommendation paragraphs into scannable UI.

Also worth copying as prompt text (free): minimal-formatting default, sentence case, verb-first button labels, "visual in the tool, explanation in the reply", and the formatting rules for file links and PR links.

---

## 4. Unknowns and bounds of the search

- No runtime observation: nothing was launched, so rendering timing, animation feel, the exact look of cards, and real error behavior are unverified.
- Display-tool field schemas (options, step, comparison, featured, quiz, product, link preview) were not recovered; only names and the streaming flag.
- Long-output collapse for plain text answers (a "show more" on very long messages) was not found; tool groups and search results collapse, but a general rule is unknown.
- Citation chips in Claude Web (hover cards, numbering, grouping) were not read beyond strings and the Cowork Sources rule.
- Artifact panel details (tabs, publish flow, access levels, versions storage) were seen through strings and the sandbox message list, not end to end.
- Thinking-summary generation (who writes the summary line, when it is shown vs hidden) is inferred.
- The permission card's exact option sets per permission mode were read from strings, not traced.
- The Code-tab model contract is for one version (2.37337.1.0). Prompts change often; re-read before depending on wording.
- Excluded on request: session archiving and child sessions (covered by the earlier study).
- Not searched: the native helper binaries (covered by another worker), mobile-only code paths, voice mode rendering, and the Chrome extension.
- Legal: behavior only; nothing here is code or a verbatim prompt. Do not paste model instructions from this spec into Faber; rewrite them in Faber's own words.

---

## 5. Gaps filled (second pass, static reading only; nothing was launched)

All points are (observed: static) unless marked otherwise. Counts are limits the app enforces silently: extra items are dropped, never an error. Section 4's unknowns on card field schemas, citation chips, artifact panel, thinking summary and long output are answered here; what is still open is listed in 5.9.

### 5.1 Rules common to every inline display card

- The card's data arrives as the tool's input. It may be an object or a JSON string; a string is parsed leniently. If nothing usable remains after cleaning (no title, no items), the card draws nothing and the plain tool row (or a skeleton while still streaming) stays.
- Most cards draw only once the tool result is complete. A subset (the ones with long lists or visuals) also draw while the arguments still stream, filling in as the partial data becomes parseable; until then they show a skeleton.
- Every field is optional unless named as required below; wrong-typed fields are ignored rather than rejected. Text is capped per field (see each card); over-long text is cut.
- In a shared or read-only view, cards render without actions that send a message to the model.
- Some cards are withdrawn after the fact when the server refuses the tool call, so no orphan card stays; some are refused when the conversation's author surface is untrusted. (inferred from strings and flags)
- The set seen in this build also includes a stock-data card, a "recommend apps" card and a message-compose card whose field shape was not recovered.

### 5.2 Field-level shape of each card (in words)

- **Options card.** Optional title and summary; 1 to 8 options. Each option needs a title and may add a description and up to 8 short bullet lines (empty ones dropped). Shown as a horizontally paged set with previous/next and "go to option N" dots; the summary sits below. Options without a title are dropped; zero valid options means no card.
- **Step card.** 1 to 8 steps, each with a required title and optional description, plus an optional summary. Default view is one step at a time with Back, Next and "Start over"; "View all steps" switches to a full list and "View step by step" returns.
- **Comparison card.** Up to 3 products side by side. Each has a required name, optional price, optional link and up to 8 attributes; an attribute is a label plus a value that is text or a yes/no flag (a yes shows as "Yes"; empty values drop their row). Optional summary.
- **Featured card and product carousel.** Up to 6 products, each with a required name and optional price, one-line blurb and link. The carousel pages with "Previous products" / "Next products" and page dots. A single product card shows a three-image mosaic (one large, two small) only when at least three usable images exist, each image with a small source label. The footer shows vendor and price joined by a dot, linking out in a new tab, plus an info icon "About these sources" explaining the listing comes from the web and is not sponsored.
- **Places card.** Up to 8 places, each with a required name, optional description and up to 3 tips. Has "View all" and "View one by one" modes and next/previous controls named after the place; a map goes with it.
- **Itinerary card.** Up to 7 days, each with an optional day label (otherwise "Day N") and up to 12 stops; a stop has a name, optional time and optional blurb. Optional title and summary.
- **Translation card.** Requires source language, target language, source text and translation (language names up to 48 characters, texts up to 2000, spaces collapsed). Optional pronunciation and a short summary (up to 120). Optional language codes in a basic locale shape drive reading aloud. Actions: Copy (briefly "Copied"), Read aloud with Pause/Resume, and Edit/Done to change the source and re-translate; during that the card says "Translating", then "Translation updated", and on failure "Couldn't update the translation. Try again." A read-aloud usage limit has its own message.
- **Quiz card.** A title plus questions; each question has an id, prompt, at least two options (id and text; duplicate ids dropped), the id of the correct option, an explanation, and optionally a hint and short custom right/wrong feedback (80 characters). Invalid questions are dropped. It opens as Quiz or Flashcards according to a starting-mode field and the person can switch with a "Format" control. Option order is shuffled deterministically per question, so a reload shows the same order. Quiz mode: "Question 3 of 8", "Show hint", "Check answer", then "That's right." or "Not quite." with the correct answer and explanation, Back/Next, finally "Results" with "N of M correct", a per-question list marked Correct / Incorrect / Not answered, and "Start over". Flashcards mode: "Card N of M", "View answer" / "View question" flip, Back/Next, Start over.
- **Recipe card.** Requires a title and at least one valid ingredient or step. Ingredient: name plus positive amount, optional unit. Step: text, optional title, optional timer in seconds (up to 30 days). Optional description, image(s) with credit, notes, base servings (default 4). The person can raise or lower servings (amounts rescale), change units between "As written", "Metric" and US customary, print, copy, tick ingredients and steps complete, and enter "Start cooking": one step per screen with "Step N of M", previous/next, per-step timers (Start/Stop, "Timer complete") and "Exit cooking mode". Pieces and pinches never convert.
- **Chart card.** Style line, bar or scatter (default line); 1 to 12 series; optional title; per-axis options (title, min, max, linear or log scale, category labels up to 2000). A series gives either a list of values (gaps allowed) or x/y points, plus an optional name; numeric strings are accepted. Unusable data shows "Unable to display data"; unnamed series read "Series N".
- **Link preview card.** Up to 6 links, each needing a safe web address and a title (200 characters), optional snippet (400) and domain (253); optional summary (200). Unsafe links are dropped.
- **Prompt tiles ("insight") card.** Optional heading (80) and 2 to 4 tiles, each with label (100), optional one-line text (120) and the prompt it will place (2000). Fewer than two valid tiles replaces the card with its plain fallback sentence when one was supplied. Clicking a tile puts the prompt in the message box and focuses it; it does not send.
- **Prompts card (larger tile set).** Up to 12 tiles from up to 24 submitted, each with a title (120), optional description (240), prompt (4000), optional art and short art label (30); optional heading (400), summary (300) and an "illustrated" flag. (partial)
- **Writing draft card.** Presents short-form writing (email, post, message) as a document-like preview with Copy ("Copied to clipboard"), an editable body ("Write your draft…") and, after edits, "Send edits" to feed changes back (with an option to also update the person's writing style). A feedback row asks "How is this draft?" with thumbs up/down; thumbs down asks "What could be improved?" with reasons (wrong tone or style, too long, missed key points, factual issue, other) and "Submit feedback", then "Thanks for your feedback".
- **Weather card.** Current conditions, location (with country when given) and a daily strip with highs and lows; a dash when a value is missing; the sky condition drives the look; a footer credits the data provider.
- **Source card.** A reference-style card with title, credit text and safely rendered body; dropped when title or credit is missing. (partial)
- **Offer next step / offer schedule.** See 5.3. The schedule offer is a caption plus one chip proposing to turn a repeating request into a scheduled task, worded by the detected frequency, and can be dismissed.

### 5.3 "Offer next step" in detail

- A slim row under the last message: caption ("Suggested next step: …"), one chip, a dismiss control. The model picks a kind: document, spreadsheet, slide deck, one-page brief, email draft or task list. Each kind has a fixed caption (for example "Get this as a slide deck you can present.") and a fixed chip label (for example "Create a slide deck").
- Clicking sends a built-in request on the person's behalf, in two flavours: turn the last answer into the thing, or make a companion to what was just built, naming that work. The request says to build it right in the chat, ask at most one question, and use sensible defaults for what is missing.
- It appears only when all hold: it is the last message and finished streaming; no prose follows the call; the reply did not end incomplete; the person is not near or over their usage limit; file creation is enabled for kinds that make files; a server pacing verdict allows it (it can say stopped, so the row is not shown every turn); no other offer owns the slot; and the person has not already used or dismissed it. A short delay precedes showing. Use and dismissal are remembered per conversation and call. The exact pacing numbers come from the server and were not recovered.
- Why it helps: one suggested action, rarely, at the end, instead of a closing paragraph of options.

### 5.4 Long-output collapsing and "show more" rules

- No general collapse of long assistant prose was found: answer text renders in full; length is controlled by the model's instructions.
- Tool timelines: a run of steps with three or more items collapses automatically so only the last two stay visible, under a header "N steps" (N = hidden count) with a chevron; opening changes it to "Hide steps". Fewer than three are never collapsed.
- The model's short narration inside a timeline is clamped to about 200 px once it finishes streaming, with a bottom fade and a "Show more" / "Show less" toggle. The toggle is hidden until hover or focus on pointer devices and always visible on touch; the height animates; text still streaming is never clamped.
- Other places: search results show a few rows then "+N more"; the memory-saved card lists the first 4 lines then "Show more"; diffs fold unchanged regions behind "Show more lines"; wide tables use an expand control; long tool values offer "Show more of the value". (strings)
- Offscreen blocks that are small enough are rendered lazily until near the viewport, and the app postpones that work while the person is typing or pressing. (performance behavior)

### 5.5 Citations and source lists

- A citation is a small pill (about 18 px tall, label capped near 180 px) inline at the end of the claim. The label is the site name; fallback "Untitled"; drive documents read "Google Doc".
- Grouping: one or two citations on a claim show as separate adjacent pills; three or more collapse into one pill reading "{first source} + N". A single citation with several underlying sources opens a list of them.
- Hover or focus opens a preview card (about 310 to 337 px wide) listing each source: favicon (or a letter fallback), source name, then title. Sources with a link come first, then a divider, then sources without one. Linked pills tint on hover and show an arrow; clicking opens a new tab. Pills without a link are focusable but inert.
- Listings in product cards use a quieter treatment (vendor line plus the "About these sources" tooltip, 5.2).
- Copying a markdown artifact offers "Copy with citations": inline marks become numbered markers and a "Sources" list of numbered links is appended; markdown downloads include the same list. Other citation origins (web search, drive, other tools) differ only in analytics.
- The search itself is a transcript row: "Searching the web" then "Searched the web" with a results list; failure reads "Failed to search the web".

### 5.6 Artifact panel: tabs, versions and states

- Header controls: a Preview / Code switch ("Artifact view"); a version picker ("Switch artifact version", showing "Latest" or "Version N"); a Copy menu (Copy, Copy as image, and for markdown Copy with citations; "Couldn't copy image" on failure); Download (as the artifact's file type, or "Download as PDF" with an error line on failure); Publish/Published, Share/Shared and "Unpublished"; a comment mode ("Done commenting" leaves it); Refresh; "Copy to project" with a "Copied to project" state and an error line; "More options"; Close.
- Several artifacts in one conversation: a "Switch between artifacts" control lists them with version counts.
- States: "Loading artifact" while fetching; "Untitled artifact" until titled. Sharing first checks that a saved copy of that version exists: "Checking for a saved copy of this version…", then success or one of "This version is still saving. Try again in a moment.", "This version wasn't saved, so it can't be shared. Try another version.", or a reload hint when the check itself failed. An unpublished artifact cannot be republished ("Create a new artifact to publish again"). Artifacts using connectors show "Manage connectors"; artifacts with storage offer "Clear storage".
- Each model edit creates a new version; older ones stay viewable, each with its own saved copy.

### 5.7 How thinking is shown

- While reasoning runs the row says "Thinking"; afterwards "Thought about it", or better a duration: "Thought for 8s", "Thought for 2m 5s", "Thought for 1h 3m" (from start and stop times; nothing for zero).
- Label priority: (1) a short title the system supplied for that block; (2) for a single block with no title, its first meaningful line with markdown marks stripped, cut at 200 characters plus an ellipsis; (3) the duration wording; (4) "Thought process". With several blocks in a row a lone title wins, else the summed duration shows.
- Redacted reasoning blocks are counted but show no text. Full reasoning text appears only in the thinking and verbose views (2.7). In the Code tab the summary line can be produced on demand instead of always (a gated setting).

### 5.8 Empty, loading and error states of widgets

- Visualize widget: a shimmer while its scripts load; it draws progressively while the model is still writing it; per-widget actions copy, download, download as image and get a link. Each action ends in a distinct outcome (copied, copied by the page itself, failed on load, failed on render, refused by the page, no answer within about 4 seconds), shown as a brief confirmation or failure line, never a blank frame.
- App-style widgets (MCP Apps): when the sandboxed frame cannot start, the host silently falls back to the plain tool row. Reasons seen: feature off, unknown or unverified server, missing connection, missing sandbox address, conversation not yet identified, frame start error, frame bundle failed to load, tool still awaiting approval, or the call produced no result. The person sees only the fallback row (and the approval prompt in the pending case).
- Display cards: skeleton before data is complete; nothing when data is invalid; "Unable to display data" for a bad chart; a dedicated line for translation edit failure; quiz questions with fewer than two options skipped.
- Offer rows: hidden whenever any condition fails (5.3), never an error. Artifacts: 5.6.
- A row "Reading widget context" appears when the model asks the app for the state of an interactive widget. (strings)

### 5.9 Still open after this pass

- Field shape of the message-compose, stock-data and recommend-apps cards; exact timings and animation curves; the server pacing rule for offers; the Docs and Slides artifact types (no renderer found in the bundles; they look server-defined and opened in the panel); a full walk of settings pages. No runtime observation was done.
- Ideas for Faber from this pass: (1) typed cards with hard caps and "drop what is invalid, show nothing if empty" validation, so a half-streamed card never shows junk; (2) timeline auto-collapse to the last two steps plus "N steps"; (3) one rare, dismissible "next step" chip with pacing; (4) quiz/flashcard and step-by-step cards as reusable patterns; (5) citation pills that group at three and show a hover list; (6) a thinking row labelled by supplied title, first line, or duration, in that order.
