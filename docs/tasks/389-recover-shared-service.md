# Recover the shared service after correlated stalls

Area: shared CLI service and conversation recovery
Files: engine-drivers/src/codex.rs (CodexServer lifecycle, server loop, WriterExit and stopUnconfirmed), engine/src/engine.rs (codex_server), engine/src/engine/silence.rs, engine/src/engine/resume_cut.rs, engine/src/engine/closed.rs, engine/tests/codex_session.rs, docs/product-map.md and relevant design page
Depends: none
Size: one pull request

Why: Not fixed yet. On November 16 at 00:36, the shared Codex app-server had 2.7 GB of memory and 537 threads; CPU time was nearly flat, from 384 seconds at 00:20 to 386 seconds at 00:36. All its conversations and helpers were silent while the Claude leader worked normally. The owner's monitor killed the server and its children at 00:37; Faber restarted it and work resumed. The internal cause is not proven.

Build: Detect correlated lack of progress in the shared service, use a bounded health check and safely restart when warranted. Preserve or recover affected conversations without duplicate sends, and show a truthful reason. Measure resident memory, process-tree threads and CPU; distinguish the no-CLI R125 test threshold under 400 MB from a CLI-server resource budget. Do not add an always-running extra process or blindly kill based on memory.

Leave out: tests using user credentials or real user processes. A cheap Terra low run is optional.

Done: A synthetic fake or isolated native failure reproduces the stall and verifies detection, bounded recovery, conversation preservation without duplicate sends and the visible reason. Preserve the existing codex_session proof that one failed thread does not kill an otherwise healthy shared server. Show the tests fail before the fix and pass afterward. Record resource measurements and the CLI-server budget separately from R125. Update the product map and design page in the same future commit. Obtain an independent risk review.
