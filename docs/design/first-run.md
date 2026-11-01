# First run and tutorial

How the first-run steps and the tutorial look and behave. The Settings screen is in [settings.md](settings.md).

- The first run has three steps in a window with no sidebar: connected accounts, appearance, first conversation. A stepper
  at the top shows the place. Each step is a modal dialog: its heading takes focus when it opens, and Esc and the
  shortcuts of the shell (Ctrl N, Ctrl comma, Ctrl period) do nothing until a button is used.
- Step 1 is the account cards. The compact introduction names the plans and says choosing only checks installation/account; installing or signing in needs its own action. At heights up to 600 px, the introduction drops its extra top margin so the AI choices stay above the sticky footer. When a picked AI's card shows its buttons (Install, Sign in), the step scrolls just enough to keep them above that footer. With Continue and Skip, both go on to step 2 (or straight to the Chat home when a look is
  already saved). Opened later, from the no-AI state or from
  Settings, the same cards have no stepper and only a Close button. They save `setup` and nothing else, so reconnecting an
  account never restarts the first run or the tutorial.
- A failed install explains the problem and next step in the card, with Try again and the official page together. The same card appears in Settings.
- Step 2 shows the three themes side by side, each a small live render drawn with its own tokens. A choice changes the whole
  screen at once and saves nothing. Under the cards: text size (the same 13 to 18 px slider as Appearance), density, motion, "Do you already use
  AI?" (Never, A lot) and "Your name" (optional), in the words Settings uses. Continue saves the appearance, the answer, the name and, unless the tutorial
  was already done, `tutorialDone: false`, in one call. Back keeps the
  choices. An install saved by an older version, whose appearance has no `base`, sees step 2 once.
- The tutorial starts at the new conversation, with an AI connected, while `tutorialDone` is false. A beginner gets three
  cards (Chat, Projects, Follow along); someone who answered "A lot" gets two (Auto, the running list); no answer counts as
  a beginner. A card has Curioso, "n of total", a title, one or two sentences, Next (Got it on the last) and, except on
  the last card, Skip.
  A ring and a pointer mark the real element the card is about (`data-tour`). When that element is missing or hidden, such
  as the Projects button below 900 px, the card is centred. The cards for the running button are pinned: that button is in
  the corner and is what they are about, so when the greeting is in the way they still sit under it with their pointer, at
  every width from 720 px, instead of floating in the middle (`placeStep` in `ui/tour.ts`).
- The tutorial is modal: the shell is inert, focus starts on Next and stays on the card, and Esc skips. Finishing or
  skipping saves `tutorialDone: true` and gives focus back to where it was, or to the composer. It never reopens. If the
  save fails it still ends for that window and may show once more at the next launch.
