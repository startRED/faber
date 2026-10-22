# Panes that size themselves, a browser with tabs and memory, a side chat that feels like a chat

Status: proposal, queued as tasks 010 to 060 (`docs/tasks/`). Each task ticks its box here when it lands.

- [ ] 010 Each pane opens at its own best width
- [x] 020 Faber's browser remembers its pages
- [ ] 030 Faber's browser gets tabs and a real browser bar
- [ ] 040 The quick question becomes a side chat
- [ ] 050 What Faber forgets and should keep
- [ ] 060 Words said twice, and the keys a desktop app has

## The owner's words (2026-10-22)

> Coloque no plano melhorias pro Faber tipo o navegador poder ter várias abas abertas igual no Claude e as abas ao
> abrirem pela primeira vez devem gerenciar seu tamanho sozinho [...] deixar mais parecido com um navegador mesmo, por
> exemplo botão de digitar URL parecendo do Google, nova aba e múltiplas abas, voltar página e avançar [...] tirar isso de
> exemplo porque é desnecessário e ocupa espaço. Melhorar todo o gerenciamento de espaço de abas do Faber [...] veja onde
> mais no Faber podemos fazer mais melhorias pro user experience e coisas de senso comum. Além disso adicionar memória no
> navegador porque quando fecha e abre a aba aberta antes some.

> Cada aba ao ser aberta ou fechada escolher o melhor tamanho para ocupar da tela, tipo essa de decisão e rodando agora.
> Chat lateral tá muito seco, parecendo HTML puro [...] não parece um chat com IA real. Quero um UX melhor que o do
> ChatGPT e do Claude.

"Aba" here means two things: a tab inside the browser, and each view of the right column (Running now, Quick question,
Changes, the browser, a visual answer, a file). Both are in this plan.

## Where Faber is today

Read from `phase-1-base` on 2026-10-22; file references are to that tree.

- **One column, one width.** Running now, Quick question, Changes, the browser, visual answers, a file and Technical
  details all replace each other in the running column (`ui/Running.tsx`), and all share one stored width
  (`faber.layout.run`). Until the person drags, it follows `autoRun`: 360 to 560 px. So the browser opens as narrow as
  the running list, and a drag made for the browser also widens the running list.
- **The browser is one page.** One global native webview (`src-tauri/src/pane/mod.rs`, `State.webview`), tied to one
  conversation at a time (`State.owner`); a page that passes to another conversation opens fresh (R308).
- **The browser forgets.** The webview closes after 5 idle minutes (`IDLE`), switching conversation closes the pane, and
  nothing is stored: no tab list, no last address. Reopening shows the last page the *agent* opened in that
  conversation, never one the person went to, or a blank pane.
- **The bar.** A head row ("Faber's browser", expand, close) above a bar (Back, Forward, Reload only once there is a page;
  the address field with the example "Type an address, like example.com"; Open in your own browser; screen size;
  Sign in with your Firefox). Two rows before the page, and the field is faint.
- **The quick question is a form.** A label ("Your question"), a two-line textarea, a Recap button, a native
  `<select>` for the model and an Ask button (`ui/QuickQuestion.tsx`). Answers vanish once the conversation's next
  turn ends (`engine/src/engine/aside.rs`, `drop_asides`).

## What Claude desktop does (clean-room behaviour study)

From the owner's installed copy of Claude desktop 2.37337 (the private RE corpus). Only behaviour is recorded here;
no code was read into Faber.

- **Pane widths by kind.** The right side is tiles next to the chat. Browser, changes, files, artifacts and pull
  requests open at about 60% (3 to the chat's 2); terminal, plan, tasks and side chat at about a third. The width a
  person drags is remembered per pane kind, for every session. Opening or closing a pane resizes the transcript. The
  chat keeps 320 px, a pane 280 px.
- **Browser tabs.** A strip with favicon and page title (the host, the port for localhost), close on hover or on the
  selected tab, middle click closes, a New tab button, an overflow menu, at most 9 tabs. Ctrl+T, Ctrl+W,
  Ctrl+Shift+T (reopen closed), Ctrl+1..8 and Ctrl+9, Ctrl+Tab, Ctrl+L, Ctrl+R, Ctrl+[ and ]. A right click offers
  open in the default browser, close, close others. The agent reuses the active tab only when it is blank, otherwise
  it opens a new one. Closing the last tab closes the pane.
- **The bar.** Back, Forward, Reload, then the address field, then a "..." menu. The field's placeholder is "Type a
  URL"; focus selects all; words that are not an address search Google; Escape restores the address, a second Escape
  leaves; unfocused, the host is bright and the path muted; a spinner replaces Reload while loading. Narrow panes drop
  Forward first, then the path. The menu holds open in the default browser, copy address, screenshot, screen size,
  allowed sites and cookies (until quit, shared, per session) and a red Clear browsing data.
- **Memory.** Tabs live per session and come back when the person returns to the session; closed tabs are kept for
  Ctrl+Shift+T. They do **not** survive a restart.
- **Side chat.** A floating or docked panel, Ctrl+;. An empty state that says what it is for, a composer at the
  bottom ("Ask a quick question…", then "Follow up…"), the person's turns and streamed markdown answers, Esc stops,
  a Clear button. Several side chats can be open as tabs. It sees the session's context and adds nothing to it.

## The design, and where it goes past them

### 1. Every view chooses its width (task 010)

Each view of the column has a kind with its own usual width, and a drag is remembered **per kind**
(`faber.layout.panes`, replacing `faber.layout.run`; the old value migrates to every kind once):

| Kind | Usual width | Why |
| :--- | :--- | :--- |
| Browser, Changes, visual answer, a file | 60% of the work area (window less the sidebar) | pages and diffs need room; Claude's choice |
| Quick question (side chat) | 420 px | a chat reads well at 360 to 480 px |
| Running now, Commands, Technical details, a helper | today's `autoRun` (360 to 560 px) | lists, not reading |

Switching views changes the width in the column's existing motion, and closing the column gives the conversation its
room back. The stage keeps 440 px (`STAGE_MIN`); a usual width that would leave less steps down to it. Past Claude:
the column docks by room, not by a fixed 1,100 px — it docks whenever the stage keeps 440 px beside it — and the
sidebar folds by itself (with motion, remembered only while that view is open) when a wide view opens in a window where
the stage would otherwise drop under 560 px, coming back when the view closes. A double click on the grip still
returns a kind to its usual width.

### 2. The browser remembers (task 020)

A conversation keeps its browser tabs with it: address, title, which one is in front, which one the agent drives. They
live in the conversation's file in the engine (like interactive choices, R350), never in browser storage, are left out
of exports and leave with the conversation. Reopening the pane, coming back to the conversation or restarting Faber
brings the same tabs back, the front one loaded. Cookies and sign-ins stay private and end at quit as today (R308,
R370); no picture of a page is kept (R236). The 5-minute idle close stays (memory), but now it closes only the
webview, not what it showed. Past Claude: tabs survive a restart.

### 3. Tabs and a real browser bar (task 030)

- **One row before the page.** The tab strip takes the head row (the title "Faber's browser" goes: the tabs say it),
  with New tab, expand and close at its end. The bar under it: Back, Forward, Reload (always there, dimmed when they
  cannot act), the address field filling the rest, and one "..." menu.
- **The address field** is a filled pill, taller and with the screen's normal text contrast, placeholder "Search or
  type an address" (no example). Focus selects all; Enter opens; words that are not an address search the web
  (Google, as Claude); Escape restores, a second Escape leaves; unfocused, the host is bright and the path quiet; a
  thin progress line under the bar while loading. Narrow panes drop Forward first.
- **The "..." menu** gathers what now sits as icons: Open in your own browser, Copy address, Screen size, Sign in with
  your Firefox, Clear this conversation's browsing data (asks first).
- **Tabs** show favicon and title (host, or the port for localhost), close on hover and on the front tab, middle click
  closes, a right click offers Open in your own browser, Close, Close others. At most 16 tabs, an overflow menu past
  the strip's room. Keys work while the pane has focus: Ctrl+T, Ctrl+W, Ctrl+Shift+T, Ctrl+Tab, Ctrl+1..9, Ctrl+L,
  Ctrl+R, Alt+Left and Alt+Right. Closing the last tab closes the pane.
- **The agent's tab.** The agent opens a new tab unless the front one is blank, and its tab shows a small busy mark
  while it acts. Read, click and type act on the agent's tab, never on one the person is using; the person may hand a
  tab over by bringing it to the front while the agent waits (today's hand-over, R235). The engine's `Browser` request
  gains an optional tab.
- **Memory (R125).** Only the front tab has a live webview; the others are records (address, title, favicon, scroll)
  that load when brought to the front, the way a browser's memory saver does. While the agent works on a tab that is
  not in front, that tab keeps a second live webview until its turn ends: at most two, and the task measures both
  (one +26–82 MB per the 2026-10-17 spike). A tab brought back from a record starts with fresh Back history.

### 4. The side chat (task 040)

The quick question keeps its engine (R250, R252, R265: a read-only fork, its own process, nothing written back) and
becomes a chat:

- The person's questions on the right in a soft bubble, the answers on the left as the thread's own markdown, streamed,
  with the model's mark as in the main thread, and "Send to the agent" under an answer that calls for action.
- A composer pinned at the bottom in the main composer's look: one box, placeholder "Ask about this conversation",
  then "Follow up"; Enter sends, Shift+Enter breaks the line, Esc stops; the model is a chip inside it (no native
  select).
- An empty state with one line on what it is for and three suggestions as chips: Recap (R265), "What is left to do?",
  "Explain the last step".
- The thread stays: it is kept with the conversation (newest 50 turns), across closing the view and restarting, with a
  Clear button in the head. Each question still forks fresh; the side thread's last turns go with it, so a follow-up
  works. This extends R250 (answers no longer drop when the next turn ends).
- Past Claude: suggestions in the empty state, the model chip, and Send to the agent.

### 5. What Faber forgets (task 050)

From a sweep of the screen on 2026-10-22: the scroll place of each conversation (it jumps to the end on every return),
the sidebar's open projects and its search, the Catalog's filter, search, page and the Catalog as the restored place,
drafts in the quick question, standing goals and helper replies (`keep`/`kept` of `ui/unsent.ts` exists, they don't
use it), the open column view per conversation (the browser through task 020), and Settings' last section.

### 6. Words and keys (task 060)

Placeholders that repeat their label or hold an example: the browser's (task 030), the standing goal's field ("For
example: every check passes" under a label and a section line saying the same), the Ctrl+K box ("Search" twice), the
quick question's example questions (task 040). Keys: Ctrl+1, 2, 3 for Chat, Projects and Catalog (outside the browser
pane, whose Ctrl+digits pick tabs), Alt+Up and Alt+Down for the previous and next conversation, in the one `SHORTCUTS`
table so tooltips and the keys sheet follow.

## Decisions this needs

The tasks write each as an `R-next` row when they land. None changes an owner decision: R250 grows (the side thread
stays), and R303's single column stays single, now with widths by kind. Google as the search for words follows the
owner's "parecendo do Google"; a Settings row to choose another engine is left for when someone asks.
