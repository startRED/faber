# claude.ai rich output: behavior spec (as of 2026-10-18)

Scope: behavior of rich output in claude.ai (web), as documented publicly. Covers inline visuals, interactive connectors (MCP Apps), Artifacts, publish and share, Research, memory, citations, and clarifying-question cards.

Method and limits:
- Public sources only (help center, product blog, developer docs, the public MCP Apps spec and its overview). Tool guidance for inline visuals was read as text from the visualization tool description available in this session.
- No login, no owner Chrome, no prompts sent, no app launched, no code extracted. No REA evidence IDs were produced because nothing was reverse-engineered for this part.
- Observation tags: `observed: public doc` (stated in an official page), `observed: tool guidance` (stated in the visualization tool description), `inferred` (my reading, not stated), `unverified` (secondary or third-party source only).
- Behavior is described in plain words. No code, markup, or internal names are reproduced.

---

## 1. Inventory

1. Custom visuals in chat: the model draws inline diagrams, charts, and small interactive widgets inside a reply (public doc, tool guidance).
2. Interactive connectors: connected tools (MCP servers) can show a UI inside the conversation, not only text (public doc, MCP Apps announcement).
3. Artifacts: standalone documents, code, pages, and apps shown in a side panel, with types (public doc).
4. Artifact types named in public material: Docs, Slides, Design, Dashboards (beta), Motion (beta), Other (public doc, unverified for the newest types).
5. Publish and share for artifacts, with access levels (public doc).
6. AI-powered artifacts and data connections for artifacts (public doc).
7. Research mode: multi-step web research that returns a report with sources (public doc).
8. Memory: remembers facts across chats and lets the user review or change them (public doc).
9. Citations: answers linked to source passages (public doc for the developer API; UI behavior in claude.ai unverified).
10. Clarifying questions with choices: a question card with selectable answers, in Claude Code (public doc); a web equivalent was not found.
11. Tool approval for connected tools: a prompt asks the user to allow a call, once or always (public doc).
12. Sandboxed, host-controlled rendering for UI from tools: an iframe, a CSP, and message passing (public MCP Apps spec).

---

## 2. Feature specs

### 2.1 Custom visuals in chat (inline)

Behavior:
- The model decides when a visual helps and draws it inside the reply. Examples: flowcharts, structural diagrams, illustrative diagrams, charts, comparison cards, forms, calculators, small games (public doc; tool guidance).
- The visual sits between paragraphs of the reply. Explanatory text stays in the reply, not in the visual (tool guidance: "text goes in your response, visuals go in the tool").
- The visual is not saved as a file by default. The public article says it is ephemeral; clicking a button inside it sends a follow-up message in the same chat (public doc; tool guidance).
- Export: the user can copy the visual, or obtain it as SVG or HTML (public doc, unverified for the export formats).

States:
- Streaming: the visual draws while the reply streams. Structure is ordered so useful content appears first (tool guidance: style first, content next, script last).
- Loading: the visual renders after streaming; scripts run after streaming completes (tool guidance).
- Error: a blocked external resource fails silently (tool guidance: "the request silently fails"). No visible error state is documented.
- Done: the visual stays in the transcript.

Model contract (paraphrased from tool guidance, observed: tool guidance):
- Two kinds: SVG for diagrams and illustrations, HTML for interactive widgets. SVG uses a fixed 680-unit wide canvas so that units match pixels. Limit: one SVG per call.
- Text in the visual is limited: short labels (about 5 words in boxes), two font sizes, sentence case, no emoji, no titles or prose inside the visual.
- Theming: colors come only from named variables that switch between light and dark. Hardcoded colors are discouraged. Dark mode is mandatory.
- Forbidden: fixed-position overlays, real fullscreen, gradients, drop shadows, blur, and nested scrolling. Height fits the content.
- Interaction: a function sends a follow-up message as if the user typed it. Links open a confirmation dialog first.
- Charts: a chart library loads from an approved CDN; legends are built in HTML, not in the canvas; each canvas needs an accessible label and fallback text.
- Maps: map data must come from a real topology file fetched from an approved source, never invented coordinates.
- Validation: any widget that collects input must show an inline error and stop until the input is valid.
- Allowed external origins for scripts and resources: a short list of CDNs and font hosts (tool guidance). Exact list is in section 2.2 caveats.
- Choice of format: tables go in the reply as text, not inside the visual. The visual is for the card grid or chart only.

Limits:
- Canvas 680 units wide; no window resizing control; no fullscreen (tool guidance).
- Visual is in-flow only; no position-fixed elements (tool guidance).

Why it helps the user:
- Makes structure visible (flow, comparison, numbers) with fewer words in the reply.
- Each button turns a visual element into a next question without retyping.

Cited URLs:
- https://support.claude.com/en/articles/13979539-custom-visuals-in-chat-and-cowork (public doc)
- https://claude.com/blog/claude-builds-visuals (public doc)
- Tool guidance text: observed: tool guidance (in the visualization tool description; not a URL)

### 2.2 Interactive connectors (MCP Apps)

Behavior:
- A connected tool can return a UI that appears inside the conversation (public doc). Examples given: a formatted preview of a chat message, a project timeline (public blog).
- The user sees the interactive piece next to the reply and can act on it, for example edit a draft in the preview and then send it through the tool (public blog, inferred from the example).
- Connectors are turned on per conversation from the "+" menu, under Connectors (unverified third-party guide; consistent with the remote MCP guide).

States:
- Loading: the connector UI is rendered from a resource the server provides (public spec: the host fetches a UI resource and renders it).
- Approval: a tool call can require approval. The approval prompt offers allowing once or "allow always" (public doc). The guide advises allowing always only for trusted servers and disabling unneeded tools in a tools menu.
- Error: not documented in public material.
- Fallback: a host that does not support the extension should show plain text (public spec overview, inferred).

Model and host contract (public MCP Apps spec, observed: public doc):
- Extension identifier in the MCP extension list. Each UI is a resource under a special URI scheme, with a dedicated HTML content type and a profile parameter.
- A tool declares in its metadata which UI resource it uses, and who may see the tool: the model, the app itself, or both.
- The UI runs in a sandboxed iframe. The host applies a content security policy declared by the server. The host and the UI talk over JSON-RPC messages through postMessage, with methods under a `ui/` prefix.
- Display modes named in the spec: inline, fullscreen, picture-in-picture (public spec).
- The UI can call tools and receive results. Calls that change data go through the normal approval path (inferred).

Limits:
- No size or time limit found in the sources read.
- A third-party host list of supporting products exists; claude.ai is listed with full remote MCP support (public doc, unverified detail).

Why it helps the user:
- Replaces long tool output with a form or preview the user can check before a side effect happens.

Cited URLs:
- https://claude.com/blog/interactive-tools-in-claude (public blog)
- https://claude.com/docs/connectors/custom/remote-mcp (public doc: approvals and "allow always" advice)
- https://claude.com/docs/connectors (public doc: support by product)
- https://github.com/modelcontextprotocol/ext-apps (public repo, spec source)
- https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/main/specification/2026-01-26/apps.mdx (public spec, 2026-01-26 version)
- https://modelcontextprotocol.io/extensions/apps/overview.md (public spec overview)
- https://modelcontextprotocol.io/extensions/overview.md (public extension overview)

Caveat: the approval wording and the claude.ai host support details come from search snippets, not full page reads. Treat as unverified until read in full.

### 2.3 Artifacts

Behavior:
- An artifact opens in a side panel when the model creates a document, code file, page, or small app (public doc).
- Types: Docs, Slides, Design, Dashboards (beta), Motion (beta), Other (public doc). Presence of the newest types was not confirmed in full.
- Editing: the user and the model can change an artifact across turns; versions are kept (public doc, unverified detail).
- Storage: a per-artifact storage limit of about 20 MB is listed in the public material (unverified figure).
- AI-powered artifacts: an artifact can call the model at runtime (public doc, unverified).
- Data connections: an artifact can read connected data when the owner allows it (public doc, unverified).

States:
- Creating: the panel shows the artifact as it is generated.
- Ready: the artifact can be viewed, copied, downloaded, or published.
- Error: not documented in detail.

Model contract:
- The model creates an artifact by type. The type tells the panel how to show it.

Why it helps the user:
- Long output moves out of the chat stream, so the reply stays short and the document stays editable.

Cited URLs:
- https://support.claude.com/en/articles/17153992-what-are-artifacts (public doc)
- https://support.claude.com/en/articles/14729249-use-artifacts-in-claude-cowork (public doc, the Cowork variant)

### 2.4 Publish and share artifacts

Behavior:
- An artifact can be published to a link. Access levels are part of the publish flow (public doc). Named levels in the public article were not all confirmed.
- Publishing is a user action; it is not automatic (inferred from the article's flow).
- Shared viewers can read; editing access is a separate grant (unverified).

States:
- Private, published, shared. Unpublish returns to private (inferred).

Why it helps the user:
- Turns a generated page or document into a link for others without copying.

Cited URL:
- https://support.claude.com/en/articles/9547008-publish-and-share-artifacts (public doc)

### 2.5 Research mode

Behavior:
- Research mode runs multi-step web research on a question and returns a written report with sources (public doc).
- The report is shown in the chat with its sources. The step-by-step progress view was not confirmed.

States:
- Running: research steps shown as progress (unverified format).
- Done: report with sources.
- Error or limit reached: not documented in the read sources.

Model contract:
- The user turns research on for a message; the model plans searches, reads sources, and writes a structured report (public doc).

Why it helps the user:
- Separates "find and read" work from the answer, and keeps the sources visible.

Cited URL:
- https://support.claude.com/en/articles/11088861-using-research-on-claude (public doc)

### 2.6 Memory

Behavior:
- Memory keeps facts from past chats so that later chats can use them (public doc).
- The user can review, edit, or remove remembered items (public doc, unverified exact controls).
- The memory mechanism described in the help article: summaries of past chats are used as context (public doc, unverified detail).

States:
- Off, on, reviewing. Exact labels not confirmed.

Why it helps the user:
- The user does not repeat preferences and project facts.

Cited URL:
- https://support.claude.com/en/articles/11817273-how-does-claude-s-memory-work (public doc)

### 2.7 Citations

Behavior:
- In the developer API, documents passed to the model can be cited; the answer carries pointers to the exact passages used (public doc, API, not the claude.ai UI).
- claude.ai shows sources for research and for search answers (public doc for research; UI detail for normal chats unverified).

Model contract (API):
- A document is passed with citations turned on. The response then includes citation objects that point to character, page, or block ranges (public doc, API).

Why it helps the user:
- The reader can check a claim against the exact source passage.

Cited URL:
- https://platform.claude.com/docs/build-with-claude/citations (public doc, API)

### 2.8 Clarifying questions as choices

Behavior (Claude Code, public doc):
- The model can pause and ask a question with selectable options, single or multiple choice, plus a free "Other" answer. Options may carry a "recommended" mark (unverified third-party guides; the Agent SDK docs describe the same tool).
- It is most common in plan mode.
- A recent change, reported by a user issue and not confirmed by Anthropic, reserves the multiple-choice prompt for decisions the model cannot make itself (unverified).

Behavior (visualization tool guidance, observed: tool guidance):
- A form pattern for collecting arguments: a short question per field, choices shown as pills, cards (with icon and subtitle), or preview tiles; sliders for ranges; date fields; file drop zone with a text fallback.
- Choice is a single-select or multi-select group. An "Other" option reveals a text field.
- Submitting sends one line back as the next user message: "Label: value" pairs joined by a separator. Long text is sent in a fold after the line. A skipped form sends a fixed sentence asking the model to proceed with defaults.
- Selection state uses one accent color; an accent color for warning, danger, or success only when the meaning is clear.
- Header titles follow "[subject] details". Questions are phrased as questions, not labels.
- The model should infer what it can from context and ask only what is needed.

States:
- Pending: the form is shown and waits.
- Answered: the answers are sent as a line; the form stays in the transcript.
- Skipped: the skip sentence is sent.

Model contract: the form is a visual, drawn by the model in the same sandbox as section 2.1 (observed: tool guidance). The web claude.ai clarifying-question card was not found in sources (see section 4).

Why it helps the user:
- Replaces a guess with a choice; answers return as a clean, short message.

Cited URLs:
- https://code.claude.com/docs/en/agent-sdk/approvals (public doc, Agent SDK)
- https://claudelog.com/faqs/what-is-ask-user-question-tool-in-claude-code/ (third-party, unverified)
- Form pattern: observed: tool guidance (visualization tool description; not a URL)

### 2.9 Tool approval for connected tools

Behavior:
- Before a connected tool acts, the user sees a request and can allow it once or always (public doc).
- The guide advises: review each request, allow always only for trusted servers, and turn off unneeded tools in the tools menu (public doc).
- Remote servers can read, change, and act for the user; the guide says to monitor what Claude does (public doc).

States:
- Pending approval, allowed once, allowed always, denied. Denied outcome is not described in the read sources (inferred).

Why it helps the user:
- Makes side effects explicit before they run.

Cited URL: https://claude.com/docs/connectors/custom/remote-mcp (public doc)

### 2.10 Docs, Slides, Design artifact types

Behavior: the public material names Docs, Slides, and Design types. Detailed behavior was not confirmed in full, and launch dates were found only in third-party pages (unverified). Not specified here beyond the type names.

---

## 3. Top 10 ideas for Faber, ranked by value and effort

Faber's rules apply: original code only; no vendor names in product or feature names; third-party tools come from the catalog, never bundled. Effort is a rough guess for the Tauri shell plus the SolidJS UI, with the engine unchanged unless noted.

1. Question cards (value high, effort low to medium). When a CLI asks a question, show choices as buttons with an "Other" text field, and send the answer back as one short line. Source: 2.8.
2. Answer-first layout (value high, effort low). Put a one-line "what matters" summary above the raw output, and fold the long raw text. This is a prompt rule plus a display rule, no new feature. Source: 2.1 (text-in-response rule).
3. Plan and progress card (value high, effort low to medium). Steps with states (pending, running, done, failed), shown as a card that updates while the agent works. Source: 2.8 states, 2.5.
4. Approval card for tool calls (value high, effort medium). Show what the agent wants to run, with allow once, allow always, and deny; keep "allow always" per tool. Source: 2.9.
5. Inline sandboxed visuals (value high, effort medium). Let agent replies embed a diagram or chart in a sandboxed frame with a strict content policy, theme variables, and buttons that send a follow-up message. Source: 2.1.
6. Artifacts side panel (value medium-high, effort medium). Long documents, code, and previews open in a panel with copy, download, and version history; the chat keeps the summary. Source: 2.3.
7. Source chips (value medium, effort low to medium). Where an answer uses sources, show linked chips that open the passage or URL. Source: 2.7.
8. Memory review panel (value medium, effort medium). A settings page listing remembered facts, each editable and removable. Must be a settings screen, not a work screen (Faber rule). Source: 2.6.
9. Research report mode (value medium-high, effort high). A run that shows steps, collects sources, and ends with a sectioned report. Source: 2.5.
10. Host for interactive tool UI (value high, effort high). A sandboxed host for tool-provided UIs that follows the public extension shape (declared resource, message channel, approval path, text fallback). Only catalog tools, never bundled. Source: 2.2.

Quick-win order if effort is the constraint: 2, 1, 3, 4, 7.

---

## 4. Unknowns and bounds

- The web clarifying-question card UI in claude.ai was not found. Only Claude Code and the Agent SDK were documented in the read sources.
- The exact allowed-origin list for the inline sandbox was read from tool guidance, not from a public doc. Treat as a snapshot.
- Artifact storage limit (about 20 MB) and the publish access-level names were taken from search snippets, not full reads.
- Docs, Slides, Design, Dashboards, and Motion: launch dates and behavior are unverified (third-party pages only).
- Research report progress UI: not documented in the read sources.
- Memory controls: the exact screen and labels were not confirmed.
- Whether a claude.ai host supports MCP Apps approval flows for UI-initiated tool calls: not documented in read sources.
- Full-text reads of the MCP Apps spec and the connectors pages were not completed. Protocol details in 2.2 come from prior reads and search snippets.
- No runtime observation. Nothing was run in a logged-in account, and no prompts were sent.
- Search snippets from third-party sites were not used as the basis for any claim marked as settled.
