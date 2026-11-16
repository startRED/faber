# The screen tells a conversation that is waiting from one that is stuck

Area: project
Files: `ui/` (sidebar rows, running panel, conversation footer), `ui/locales/` (hand-written, all languages), `docs/product-map.md`, the design page of conversations
Depends: 400-every-wait-has-a-wake
Size: one pull request

Why: On 2026-11-15 the person watched the Projects sidebar and saw "encerrou sem conclusão" in orange on every conversation of a project, with a spinner on the project, and could not tell which conversations were alive, which waited for something that would arrive, and which were stuck. The label is the same for a lead waiting on its children, a child waiting for a pull request's checks, a conversation whose turn ended before it concluded, and a conversation that will never wake. The "Rodando agora" panel shows only the first words of the wait note, often cut. The person sent screenshots of it twice, annoyed. At the second look all of them were healthy; at the first look three were stuck. A screen that cries wolf teaches the person to stop trusting it.

Build: Every row says one of: working; waiting for X until T (X in plain words: "the checks of pull request 135", "the conversation Registro das leituras", "a helper"); stuck (a wait whose subject is already finished, whose helper is dead, or that is past its deadline without a wake) with the reason and a way to wake it; ended without a conclusion (the AI stopped and did not conclude). Orange is for stuck and for ended without a conclusion only; a healthy wait looks calm. The running panel shows the whole subject and the deadline. All new text goes through `ui/locales/` in every language, written by hand, English first.

Leave out: new states in the engine beyond what 400 and 410 give; colours or layout of unrelated screens.

Done: Screen checks fail without the change: a fake conversation in each of the four states shows its own wording and colour in the sidebar and the panel, and a wait past its deadline shows as stuck. Visual critique of the touched screens before and after, at desktop and narrow sizes.
