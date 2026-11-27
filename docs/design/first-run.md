# First run and tutorial

How the first-run steps and the tutorial look and behave. The Settings screen is in [settings.md](settings.md).

- When the build has a sharing endpoint and no answer is saved, a modal first asks whether to share anonymous usage and
  crash information. Both buttons have the same appearance and neither is selected. A failed save shows the usual error
  and opens the next step; the question returns on the next start. This also applies to existing installations. Without an
  endpoint the question is absent (R583).

- The first run has three steps in a window with no sidebar: connected accounts, appearance, first conversation. A stepper
  at the top shows the place. Each step is a modal dialog: its heading takes focus when it opens, and Esc and the
  shortcuts of the shell (Ctrl N, Ctrl comma, Ctrl period) do nothing until a button is used.
- Step 1 is the account cards. A static accounts drawing appears above the heading at 900 px wide and 800 px high or more; otherwise Curioso stays there. The compact introduction names the plans and says choosing only checks installation/account; installing or signing in needs its own action. At heights up to 600 px, the introduction drops its extra top margin so the AI choices stay above the sticky footer. When a picked AI's card shows its buttons (Install, Sign in), the step scrolls just enough to keep them above that footer. With Continue and Skip, both go on to step 2 (or straight to the Chat home when a look is
  already saved). Opened later, from the no-AI state or from
  Settings, the same cards have no stepper and only a Close button. They save `setup` and nothing else, so reconnecting an
  account never restarts the first run or the tutorial.
- Antigravity is a third choice, after Codex, always. Its CLI (`agy`) is found on PATH or in its installer's folder
  (`%LOCALAPPDATA%\agy\bin` on Windows). Without it, its card is the other two's: Not installed with Install, the same
  steps while installing, and Try again beside the official page (antigravity.google/docs/cli/install) after a failure;
  outside Windows only the official page, as for the other two. Install does what Antigravity's own installer does, without
  PowerShell: the manifest of the platform, the program it names checked against its SHA-512, `agy.exe` placed in that
  folder, then `agy.exe install`, hidden and ended after 60 s ([the source](../dev/antigravity-install.md)). Its card says the plan under its name ("Google AI Pro or Ultra. The free plan works too, with a smaller weekly
  limit.", from antigravity.google/docs/plans, 2026-11-22) and offers Sign in, which runs `agy acp --login` in a terminal the
  person sees. Whether it is signed in comes from Antigravity's ACP server: `agy acp`, a handshake, then one new session in
  `<data>/agy-check`, read for a session (signed in) or "Authentication required" (signed out); no prompt is sent, and the
  process ends with the answer or after 20 s. The cards ask once when it is picked (a session only when `agy` is found) and again only on
  Check again, never on window focus as for the other two. Each ask leaves one empty conversation in the CLI's own list,
  under that folder; Faber keeps only the verdict. Connected Antigravity cannot answer yet: it is not one of the AIs Faber
  lists or starts (R531).
- A failed install explains the problem and next step in the card, with Try again and the official page together. The same card appears in Settings. A rejected install request also leaves the card ready to retry when no progress event arrived.
- Step 2 shows the three themes side by side, each a small live render drawn with its own tokens. A choice changes the whole
  screen at once and saves nothing. Under the cards: text size (the same 13 to 18 px slider as Appearance), density, motion, "Do you already use
  AI?" (Never, A lot) and "Your name" (optional), in the words Settings uses. Under them, across the whole width, the key: a
  switch "Let the AI see other windows", off, with one line saying that with it on what is on screen goes to the AI of the
  person's subscription and that text on screen written to trick it can try to steer it (R586, the one first-run choice a
  default covers, by the owner's answer 4 of 2026-11-25). Continue saves the appearance, the answer, the name, the key when
  it was switched on (or off, when it was saved on), and, unless the tutorial was already done, `tutorialDone: false`, in one call. Back keeps the
  choices. An install saved by an older version, whose appearance has no `base`, sees step 2 once.
- The tutorial starts at the new conversation, with an AI connected, while `tutorialDone` is false. A beginner gets three
  cards (Chat, Projects, Follow along); someone who answered "A lot" gets two (Auto, the running list); no answer counts as
  a beginner. A card has Curioso, "n of total", a title, one or two sentences, Next (Got it on the last) and, except on
  the last card, Skip.
  Its entrance starts after the anchor has been measured, beside the element it explains; hidden initial coordinates
  never make the first card fly across the window (`tests/ui/motion.check.ts`).
  A ring and a pointer mark the real element the card is about (`data-tour`). When that element is missing or hidden, such
  as the Projects button below 900 px, the card is centred. The cards for the running button are pinned: that button is in
  the corner and is what they are about, so when the greeting is in the way they still sit under it with their pointer, at
  every width from 720 px, instead of floating in the middle (`placeStep` in `ui/tour.ts`).
- The tutorial is modal: the shell is inert, focus starts on Next and stays on the card, and Esc skips. Finishing or
  skipping saves `tutorialDone: true` and gives focus back to where it was, or to the composer. It never reopens. If the
  save fails it still ends for that window and may show once more at the next launch.
