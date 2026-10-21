# Finish visual answers beside the chat (parity M1)

Step 1 covers closed HTML and SVG blocks in replies: a small card opens the offline visual in the running column's
resizable pane, with earlier versions kept in the conversation's messages (R311 through R315). Ten built-in pieces are
also implemented inline in replies, with saved controls and plain-code fallback (R348); their checks are in the
product map. Extend the existing reply, pane and card in Chat and Projects; do not introduce a second viewer. Row M1
stays begun until the remaining parts below are done.

Land each remaining step on its own, in this order:

1. **Network for one answer.** Ask just in time when the person allows one visual to use the network. Keep offline as
   the default and keep the frame isolated from Faber. Define the grant's lifetime and what addresses it may reach;
   show the owner a short card for those choices before implementation. Done when engine and screen checks prove
   offline remains blocked, only the allowed answer gains access, and closing or switching versions follows the agreed
   grant rules. Measure memory and processor cost; update the map, design and five-language what's-new note.
2. **Project files and Mermaid.** Detect pages and small apps the agent writes as files in a project, and diagrams
   written as Mermaid. Reuse the reply card, pane and version controls. Define which files count and how file versions
   are retained without reading unrelated project files. Done when fixtures for both CLIs prove detection and version
   retention, and screen checks cover opening each form, isolation, earlier versions, a 720 px window and reduced
   motion. Measure memory and processor cost; update the map, design and five-language what's-new note.
