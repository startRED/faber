# A reply says plainly that the agent finished, and how long ago it last moved

Asked by the owner on 2026-10-18, word for word: "E vc deveria ter dito que terminou antes vc ficou calado com a caixa de
digitação piscando pensei que ainda tava mexendo, ia ser bom corrigir isso dps e tbm colocar pra mostrar a quanto tempo
essa ultima atualização na resposta/output foi feita igual no claude fala."

So a person who comes back to an open conversation cannot tell a finished agent from one still at work: the reply just
stops and the composer blinks.

1. When a reply ends (and nothing else runs for the conversation: no Keep going, no Faber's checks, no queued message),
   its foot says so in one quiet line, for instance "Done · 12 min of work". While it is still at work, the same place
   says it is working. Look in the product map for what already shows a reply's state (the work log, the running row,
   the Keep going lines of R223/R224) and extend it; a second status is a bug.
2. The newest reply shows how long ago it last changed ("updated 3 min ago"), ticking at most once a minute and not at
   all while the window is hidden, as Claude's desktop app does. No timer runs for older replies.
3. Words in all five locales by hand; reduced motion and a 720 px window checked; the product map and the work-log page
   of docs/design updated.

Done when: a UI check fails today and passes after for both lines (finished vs working, and the age that moves with a
mocked clock), the idle cost stays as low as before (no new running animation, one timer at most), and the visual
critique of the open conversation does not drop.
