# Screen faults seen in the installed app

Why: on 2026-10-29 a session photographed the installed Faber after two hours of real work, next to T3 Code and the
Codex desktop app (the live look of the `visual-critique` skill), and three judges of other vendors read the photos.
Each fault below was then found in the code. They are bugs or wrong sentences, not redesign: fix them in the current
screen (the Electron move keeps the same components). Task 120 owns the single live state; take these first or fold them
into it, never twice.

- The running command shows twice: inside the opened step (`ui/Thread.tsx` grouped tools, `StepRow`/`ToolChip`) and
  again as the latest line under it (`latest()`/`after()` in `ui/Thread.tsx`), whether or not the step is open. Show it
  once; in the opened step let a long command wrap instead of cutting it at the edge (`.tool-chip` uses `nowrap`).
- The quota sentence is wrong when the vendor did not change: `reason.moved.*` says "{from} is out of quota, the
  conversation went on in {to}" with vendor names only (`ui/Thread.tsx`, `engine/src/engine/recovery.rs` stores
  `moved_from`), next to the new model's name, so a person reads "Sonnet is out of quota, went on in Claude". Name the
  one that ran out and the one that took over in one sentence, and say nothing about a move inside one vendor.
- "Plano 1 de 4" counts finished steps while the second step is the current one (`ui/Plan.tsx`, `ui/Run.tsx`): say
  "1 de 4 feitas" in both places. The plan rows draw empty checkboxes that look clickable; draw a state mark instead.
- The same word counts different things on one screen: the header pill counts running conversations
  (`ui/App.tsx`), the right panel heading adds finished cards, its Commands tab counts commands of every conversation
  (`ui/Running.tsx`), and the line under the answer counts this conversation's background work (`ui/Details.tsx`,
  `ui/background.ts`). The owner saw "1 rodando", "Rodando agora 1", "Comandos 2" and "2 coisas ainda rodando" at once.
  Give each count its own noun, or drop the ones task 120 makes redundant.
- "Enquanto você estava fora (2 h)" measures the time since Continue-alone was turned on (`engine/src/engine/keep_going.rs`),
  not time away, and sits next to the folded run's own "1 h · 7 turnos" (`ui/Run.tsx`). Say what it measures.
- Between two turns the screen said "Pronto · atualizado há 8 min" over "Procurando alterações pra verificar" while
  Continue-alone was on, and nothing visible changed in 18 s (`ui/foot.ts`, `ui/Details.tsx`,
  `engine/src/engine/verifying.rs`). Say that Faber is checking before it goes on, and show when the next step starts
  when it is known.
- A stop square (■) marks "running" in the header pill and the sidebar; it reads as a Stop button. Use the running mark
  of the design system.
- Helper and background titles reach the screen in English, verbatim from the agent's tool input
  (`ui/background.ts`, `ui/worklog.ts`), and the "talk about this helper" draft copies them into the composer
  (`agent.tellAbout`). Task 130 makes agents title in the person's language; this task only makes sure every place that
  shows a title takes the same one.
- Faber's Codex threads show in the person's own Codex app history titled "Faber started your context fresh: nothing
  said ear…" (the handoff of `engine/src/sibling.rs` becomes the thread's first user message after a clear). The
  app-server has `thread/name/set {threadId, name}` (checked with `codex app-server generate-json-schema`, codex-cli on
  2026-10-29): name each thread Faber starts with the conversation's title, in the person's language.
- Settings showed Claude as "Não ligada" while Claude was answering and its quota had been read; minutes later the
  sidebar dropped Claude's quota bar (the bar list is the engine's detected CLIs, `engine/src/engine.rs`, replaced by
  each setup check, `src-tauri/src/setup.rs`). The open pull request for a slow sign-in check may be the cause: check
  it after that lands, and if it still happens, a check that fails must never remove a vendor that is working.

Proofs: a screen check per fault that fails before the fix. Walk it at 720 px and in the longest language. A live look
(`visual-critique`, section 4) before and after. What's-new entry. Extends: the product-map entries of the work log,
the running column, keep-going, accounts and the sidebar.
