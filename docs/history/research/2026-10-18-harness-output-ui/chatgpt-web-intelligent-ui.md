# ChatGPT Web "Intelligent UI" and Apps SDK widgets: behavior spec

Date of research: 2026-10-18. Method: public web only (news coverage, OpenAI developer docs). No login, no owner browser, no prompts sent, no REA run (the optional logged-out shell capture was skipped; it would add only UI strings). No code or internal names reproduced.

Source tags: (observed: static) = read on a public docs page; (press, secondary) = reported by news/third-party sites, not OpenAI's own page; (inferred) = my reading.

## 1. Inventory

- Intelligent UI in the ChatGPT Chat tab, launched 2026-10-07 with GPT-6 (press). Replies mix text, visuals and interactive elements.
- Component library the model picks from: text blocks, graphics, buttons, input forms, charts (press; OpenAI wording via press).
- Layout patterns: side-by-side comparison, manipulable diagram, transit-style map, on-demand small tools (meal-cost splitter, savings tracker, small game) (press).
- Apps SDK inline cards and inline carousels (observed: static).
- Apps SDK fullscreen and picture-in-picture (PiP) display modes (observed: static).
- Widget bridge: reads tool output and input, persists widget state, sends a follow-up message, calls a tool, uploads or selects files; modal and checkout are named (observed: static).
- Review/quality claims: OpenAI says it trained evaluation on clarity, usability and completeness of generated screens, and on when to add interaction versus answer in text (press, paraphrase).

## 2. Feature specs

### 2.1 Intelligent UI in the Chat tab (ChatGPT Web)

- Behavior: the model writes a reply as a composition of text and small screens. Simple questions stay plain text. Explanations can arrive as an interactive diagram; comparisons as side-by-side panels; travel plans can include a map (press: designcompass.org, pymnts.com).
- Trigger: the model chooses the format from the question. No special prompt is required. A user may ask verbally for a specific tool inside the chat (press: designcompass.org; ghacks).
- States:
  - Streaming: screens appear in sequence while the answer generates, so display starts before the reply finishes (press: designcompass.org; ghacks).
  - Done: the reply is complete with its screens in place (inferred).
  - Text fallback: when an explanation fits better as text, the reply stays text (press).
- Interaction: users can ask follow-up questions after reading an explanation, change on-screen input values to see results recomputed, and request tools in conversation (press: designcompass.org).
- User control: users who prefer text can reduce the number of visuals; reported as a setting but exact menu path and wording are not confirmed (press: propakistani.pk; vktr.com). Setting location unverified.
- Rollout: Plus, Pro, Business, Enterprise from 2026-10-07; Free and Go from 2026-10-08 (press). Enterprise access depends on workplace administrator settings (press).
- Model names: one report says paid tiers use one GPT-6 variant and Free/Go another; the reports disagree or are unverified. Do not rely on this.
- Why it helps: the reply leads with the visual answer and moves text to a supporting role. The user acts on the screen, not on a wall of prose.

### 2.2 Apps SDK inline card (display mode default)

- Behavior: every app starts inline, shown in the conversation before the model's reply text (observed: static, developers.openai.com/apps-sdk/concepts/ui-guidelines).
- Use cases: one action or decision (booking confirmation), small structured data (map, order summary, status), self-contained widget (audio player, score card) (observed: static).
- Layout rules (observed: static):
  - Title when the card is document-based or lists items under a parent.
  - At most two actions: one primary, one optional secondary, at the bottom. Each action triggers a conversation turn or a tool call.
  - Height grows with content up to the mobile display height. Cards must fit their content.
  - No tabs, no multiple views, no deep navigation (split into separate cards or tool actions).
  - No nested scrolling.
  - "Show more" to reveal extra items in long lists.
  - Inline edit controls are allowed; edits persist.
  - Expand control to open fullscreen when media or interactivity is rich.
  - No duplicate inputs that repeat chat features (no second chat box inside a card).
- Follow-up text: a short model-written message placed after the card, suggesting edits, next steps or related actions; it must not repeat what the card shows (observed: static).
- Branding: the host appends the app's logo and name; the card must not add its own logo in the reply (observed: static).

### 2.3 Apps SDK inline carousel

- Behavior: a horizontal set of similar options (restaurants, playlists, events) where each item needs an image and more detail than a row can hold (observed: static).
- Limits: 3 to 8 items. Each item has an image or visual and usually a title. Metadata limited to the most relevant details; the page states both "two lines" and "three lines max", so use the stricter. Optional badge. One CTA per item where possible (observed: static).

### 2.4 Apps SDK fullscreen

- Behavior: for rich tasks (interactive maps, editing canvases, diagrams, detailed browsing such as listings or menus). The chat composer stays overlaid so the user can keep talking to the app (observed: static).
- States: while the reply streams, the composer input shimmers; when done, a truncated snippet of the reply appears above the composer; tapping it opens the chat sheet (observed: static).
- Constraint: design for the composer, since prompts can arrive at any time and may trigger tool calls. Existing editors may be embedded if they meet an iframe policy (observed: static).

### 2.5 Apps SDK picture-in-picture

- Behavior: a floating window for ongoing or live sessions (games, quizzes, live collaboration, learning). It stays visible while the chat continues, stays at the top on scroll, stays until dismissed or the session ends, then returns to inline position and scrolls away (observed: static).
- Should update in response to prompts. Avoid heavy controls or static content in PiP; put those inline or in fullscreen (observed: static).
- Mobile: PiP may be coerced to fullscreen (observed: third-party inspector docs, not official; treat as unverified).

### 2.6 Widget bridge (how the widget talks back)

Observed: static (developers.openai.com/apps-sdk/build/chatgpt-ui). The page names these operations:
- Read the tool's output and input. The host sends initialize, then input, then result; the widget renders from structured data. Recommended: separate data-fetch tools from render tools so the widget renders once with final data.
- Read and write widget state for one rendered instance. Explicitly not durable storage and not a source of truth. Write after each meaningful UI change.
- Send a follow-up message into the conversation (a conversation turn).
- Call a server tool from the widget (for example re-roll without remounting).
- Send selections or staged edits to the model as model context.
- Model-visible vs UI-only: some content is visible to the model, some is UI-only, and file identifiers can be passed to the model on later turns.
- Named but not detailed on the page: upload and select files, get a file download link, open a host modal, open a payment sheet (checkout, private beta).
- Not covered on the page: how to switch display mode, close, or open links (the page mentions display modes but names no method). Do not assume.
- Whether the model sees the structured data, or only a text summary, is not stated. The example also returns a text summary, which suggests text is the model-facing channel (inferred).
- Security/limits: nested frames blocked by default; narrow allowlists for connected domains, resource domains and frame domains; changing the page code requires a new resource address; durable storage needs rate limits (observed: static). No numeric size limits given.

### 2.7 Accessibility and theming (Apps SDK, observed: static)

- Text and background at least WCAG AA contrast. Alt text on all images. Text resizing must not break layouts.
- Colors: system colors for text, icons, dividers. Brand accent only on accents, badges, and primary buttons. No overriding backgrounds or text colors, no custom gradients or patterns.
- Type: host system font stack; no custom fonts even in fullscreen; limit size variation (body and body-small preferred); bold/italic only inside content.
- Spacing: system grid, consistent padding, system corner radii; avoid edge-to-edge text; order is headline, supporting text, CTA.
- Icons: system or monochrome outline icons. Images follow enforced aspect ratios.
- Tooling: an optional open UI kit with styling tokens and accessible components, and a Figma library (observed: static).
- Light/dark: the page only says to use system colors; no explicit light/dark rule.
- Accessibility of Intelligent UI in ChatGPT itself: not mentioned in any source I could read (designcompass.org says nothing). Unknown.

## 3. Top 10 ideas for Faber, ranked by value/effort

Faber's pain is text walls from CLI agents. Ideas are behavior only.

1. Lead with the visual answer for comparisons: a side-by-side panel when the question compares two or more things (high value, low-medium effort).
2. Use a "text when text is enough" rule: stay text for simple answers; show a visual only when it reduces reading (high value, low effort: a heuristic plus a user setting).
3. Give the user a single "fewer visuals" control, placed in Settings only, never on a work screen (high value, low effort).
4. Inline cards with at most two actions (primary plus optional secondary) that each send a message back to the agent (high value, medium effort).
5. Follow-up line after a card that says only what the card does not show: next step or edit (high value, low effort).
6. Stream screens as they are generated, with a placeholder that holds the layout and a brief shimmer while text streams (medium-high value, medium effort).
7. Plan/progress card: steps with state, updated in place, expandable to fullscreen (medium value, medium effort).
8. Options carousel for 3 to 8 alternatives, each with one CTA (medium value, medium effort).
9. Widget state kept per instance and never treated as storage; durable data stays in the engine (medium value, low effort; a safety rule more than a feature).
10. PiP-like floating card for a long-running task (build, test run) that stays visible while the chat continues and closes when it ends (medium value, high effort; define the close rule first).

Also worth a note: a "widget returns data to the model" channel (explicit model-context update) is a good design, so the agent sees a user's selection without the user retyping it.

## 4. Unknowns and bounds of the search

- OpenAI's own Intelligent UI announcement page (openai.com/index/gpt-6-for-everyone/) returned HTTP 403 to fetch; the feature facts come from press and secondary sites (designcompass.org, ghacks, techrepublic, vktr, letsdatascience, pymnts, propakistani). Re-check the primary page with a browser the owner allows, or wait for it to be cached.
- The fewer-visuals setting: location and exact wording not confirmed.
- Model tier names: contradictory across sources; unverified.
- The design-guidelines page was read, but it has no explicit "show vs say" rule; that split is my inference from the card and follow-up rules. The ghacks page returned 403 on fetch and was used only from search snippets.
- No independent test of accuracy or reliability of generated UI found.
- No numeric latency or size limits for Intelligent UI found.
- The logged-out chatgpt.com shell was not captured via REA (not required; the value is low and it risks noise). Not done.
- Help-center release notes not reachable in search results.
- Fullscreen and PiP official rules were partly in third-party mirrors; I used the official page text where it was fetched.

## Sources

- https://developers.openai.com/apps-sdk/concepts/ui-guidelines (observed: static; display modes, layout, styling, accessibility)
- https://developers.openai.com/apps-sdk/concepts/design-guidelines (observed: static; partly, via search summary and fetch)
- https://developers.openai.com/apps-sdk/build/chatgpt-ui (observed: static; widget bridge and lifecycle)
- https://openai.com/index/gpt-6-for-everyone/ (403 on fetch; seen only in search results)
- https://designcompass.org/en/2026/10/08/openai-chatgpt-intelligent-ui-design/ (press)
- https://www.ghacks.net/2026/10/08/openai-brings-gpt-6-to-all-chatgpt-users-adding-intelligent-ui-with-interactive-answers/ (press; search snippet only, fetch returned 403)
- https://www.techrepublic.com/article/news-gpt-6-intelligent-ui-charts-forms/ (press, search summary)
- https://www.vktr.com/ai-platforms/openai-brings-gpt6-to-all-chatgpt-users-with-interactive-intelligent-ui/ (press, search summary)
- https://letsdatascience.com/news/openai-brings-gpt-6-and-intelligent-ui-to-chatgpt-4f0d797c (press, search summary)
- https://www.pymnts.com/?p=4287844 (press, search summary)
- https://propakistani.pk/2026/10/08/openai-is-turning-chatgpt-into-a-more-interactive-interface/ (press, search summary)
- https://techcrunch.com/2026/10/07/chatgpt-is-getting-a-lot-more-visual-with-the-launch-of-a-new-interface/ (in search results only, not fetched)
