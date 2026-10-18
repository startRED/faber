# Clean-room study: how other AI apps turn answers into UI, 2026-10-18

Asked by the owner on 2026-10-18: study ChatGPT web, Claude web, the ChatGPT/Codex desktop app and Claude desktop with
REA, so Faber can later reimplement anything from them by behavior, starting with less raw text and more visual,
interactive answers. Every page here is behavior in plain words: no code, no pseudocode, no internal names. Short
on-screen strings and the tool names a model sees are quoted. Model instructions are paraphrased, never copied.

A private reference corpus (the extracted and reformatted bundles, file and line indexes, native-helper notes and the
matching public sources) lives on the owner's disk, outside every repository, and is never shared. A study session
reads it and writes a page like these. The session that builds Faber works from the pages only.

## Pages

| Page | Target | Source |
|---|---|---|
| [claude-desktop-rendering.md](claude-desktop-rendering.md) | Claude desktop 2.37337.1.0 and the claude.ai frontend it bundles | shipped bundles, static |
| [claude-strings-inventory.md](claude-strings-inventory.md) | Claude desktop resources: UI strings, frames, helpers | shipped files, static |
| [claude-web-rich-output.md](claude-web-rich-output.md) | claude.ai rich output, MCP Apps | public docs |
| [openai-desktop-rendering.md](openai-desktop-rendering.md) | ChatGPT + Codex desktop 26.1007.2314.0 and the chatgpt.com frontend it bundles | shipped bundles, static |
| [openai-resources-inventory.md](openai-resources-inventory.md) | the same package: bundled skills and plugins (visualize, template picker, research, computer use) | shipped files, static |
| [chatgpt-web-intelligent-ui.md](chatgpt-web-intelligent-ui.md) | GPT-6 Intelligent UI, Apps SDK | public docs and press |
| [native-helpers.md](native-helpers.md) | the eight native helper programs of both apps | strings and imports only (no decompiler on this PC) |

Nothing was seen running: no app was launched, no account was used, no prompt was sent. Timings, animation feel and
server-side prompts are unknown. The widget-authoring prompt of Intelligent UI lives on OpenAI's servers and is not in
the package.

## What both apps do that Faber does not

Both apps use the same three layers. Each layer works with any model, and Faber already has a place for all three.

1. **App tools the model calls instead of writing prose.** Claude desktop gives the CLI session tools for a task list
   under the message, a closing box (a recap of two or three sentences, one question, at most two buttons), chapter
   dividers, "open this pane" (diff, file at a line, terminal), question cards and next-step offers. Codex desktop has a
   plan card, a short "what I am doing" label (3 to 6 words), a "Worked for N" line and a subagents panel. Faber
   already answers an MCP server over stdio for its helper tool (R161), so the same server can offer these tools to
   Claude Code and Codex.
2. **Directives inside the answer text.** ChatGPT and Codex hide one-line markers in markdown, and the client draws a
   chip or card for each: follow-up chips (a short phrase plus the full prompt it sends), review comments tied to a
   file and lines with a priority 0 to 3, file citations with a line, page, slide or sheet locator, writing blocks for a
   finished email or post with two or three tone versions, and "thread created" cards. Unfinished marker lines are held
   back until they are whole, so nothing raw flashes. This needs no new protocol, only a prompt line and a parser.
3. **Sandboxed visuals.** Claude draws SVG or HTML widgets inline. Each widget streams in piece by piece, its scripts
   run only when it is complete, and it is themed by host variables. A click sends a prompt only after a real user
   gesture. ChatGPT goes further: the model writes a small UI program that runs in a sandboxed frame. It draws from about
   60 building blocks and about 236 ready components whose data comes from the server. Codex desktop draws an HTML file
   the model wrote, in an isolated view with an allowlist of script sources and no network access.

Both apps add typed cards with hard limits on top: options, steps, comparison, quiz and flashcards, recipe, chart,
link preview, itinerary. Invalid items are dropped, and a card with nothing valid draws nothing, so half-streamed data
never shows junk.

## The rules they give the model (the cheapest win)

These rules are paraphrased from the shipped prompts and tool descriptions. Faber can state them in its own words in
the prompt it adds to each CLI:
- Make a visual only when seeing or poking something beats reading about it. Use a table for tabular data, a diagram
  for a static structure, and an interactive card for motion, adjustable inputs or choices.
- Prose goes in the reply and the visual goes in the tool. Never repeat in text what the card shows: one line after the
  card says only what it helps the person decide.
- Ask through a card, never as paragraphs of questions. Infer what you can and ask only what is missing.
- No progress narration and no closing "next steps" paragraph: the task list and the closing box carry them.
- Use minimal formatting. Match length to the question. Button labels start with a verb and have 1 to 3 words.
- Point at things with an action: open the diff or the file at the line instead of writing "see file X".
- Follow-ups are rare and single: one suggested next step, paced, dismissible.

## Ranked for Faber (value over effort)

| # | Idea | Effort | Pages |
|---|---|---|---|
| 1 | Prompt rules above, in Faber's words, added for both CLIs | tiny | all |
| 2 | Task list and closing box as tools on Faber's MCP server (recap, one question, up to two buttons that map to Faber actions) | low | claude-desktop 2.6, openai-desktop 2.11-2.12 |
| 3 | Work log rows with live and past verbs; three or more steps collapse to the last two plus "N steps" | low | claude-desktop 2.8, 5.4 |
| 4 | Follow-up chips and file-citation chips as text directives | low | openai-desktop 2.6 |
| 5 | Question card for Codex too (today Claude Code only), plus previews per option | low-medium | claude-desktop 2.5, openai-desktop 2.3 |
| 6 | "Open the pane" tool: diff, file at a line, terminal | low | claude-desktop 2.6 |
| 7 | Review-comment directive drawn on the diff, priority 0-3 | medium | openai-desktop 2.6, 2.13 |
| 8 | Typed cards (options, steps, comparison, chart) with hard caps; drop invalid items | medium | claude-desktop 5.1-5.2 |
| 9 | Sandboxed inline visuals: SVG first (no scripts, cheapest on memory), then HTML with an allowlist and no network | high | claude-desktop 2.1-2.2, openai-desktop 2.8 |
| 10 | Host for MCP Apps UI from the user's own MCP servers | high | claude-desktop 2.2, claude-web |

Before building any of these: read the product map (Faber already has a question card, block-by-block markdown and
one work-log style), check the taste cards, and measure memory. A sandboxed frame keeps a process and a DOM alive, so
R125 applies.

## Log

| Date | Tools | Target | SHA-256 of app.asar |
|---|---|---|---|
| 2026-10-18 | REA (JavaScript application analysis, native strings and imports), @electron/asar, prettier; lead Claude Opus 5.5, workers Claude Sonnet 5.5 and Haiku 5.5 | Claude desktop 2.37337.1.0 (Windows, MSIX) | 6290a54e1472e26e3cd6324a2ba23fccb7a5bb1cc9329eeb841a405d377634cc |
| 2026-10-18 | same | OpenAI desktop (ChatGPT + Codex) 26.1007.2314.0 (Windows, MSIX) | c5fdef328cfa2ad9095786f9c3a7aaa447e68834d31299d6ef2592da02029038 |

Legal: lawful copies owned by the owner, studied for behavior (17 U.S.C. 102(b); EU 2009/24/EC arts. 5(3) and 6;
Lei 9.609/98 art. 6). A clean room avoids copyright copying only, so check patents and license terms before shipping
a close copy of a distinctive feature. Before committing reimplemented code, compare it with the corpus for literal
matches and rewrite any.
