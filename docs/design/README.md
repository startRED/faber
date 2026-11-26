# Faber design

Decided on 2026-10-11. This is the direction that Phase 1 builds. It replaces the Mono design of 2026-10-10. The
approved mock (six screens) is the visual reference, and each Phase 1 piece updates the page of its own part.

The hidden `ux_audit` walkthrough can select fake Codex with `--codex`, using synthetic sandbox sign-in and the existing
recordings; fake Claude remains the default. This test-only choice changes no screen, layout, motion or product words.

## Design and identity

The visual direction lives in this folder, one page per part ([docs/README.md](../README.md) says which). In short:

- One screen structure. Three themes (Dark Room, the default; Workshop; Workbench) and three presets (Morning,
  High contrast, Cobalt) are only sets of token overrides on a shared base.
- People can customize almost everything, and a custom look saves to a small file that can be shared. Three meaning
  colours are locked by the owner's decision: amber for "needs you", coral for Claude and violet for Codex.
- The symbol is "Curioso", a lime creature that peeks. The app icon is one dark icon in every Windows mode.

## First frame and update progress (R157)

- On Windows, an invalid or already linked data folder stops setup before settings and the engine are opened. The existing startup-error path reports the failure; no work screen or new control is added.

- `ui/index.html` paints a static first frame before any script runs: the frame colour of Dark Room, or of the light
  theme when the system asks for light, with the mark centred at half strength. No text (so no language is needed) and
  no animation, reduced motion or not. Its rules sit at the top of `ui/styles.css`, never in an inline `<style>`: the
  shell adds a hash to the page's CSP for an inline style, and a CSP with a hash ignores `'unsafe-inline'`, which
  blocks every style the page adds later (the screen sentinels' planted defects among them). `main.tsx` removes it once the screen mounts, and the saved look takes over.
- The window is built hidden. `screen_ready` shows it after the screen's first frame; a 3 s failsafe shows it anyway.
  A second launch before that moment waits for the same reveal. Because the page paints its own frame, a reveal that
  comes early or by the failsafe still shows the mark on the theme colour, never a blank window.
- While Faber looks for, downloads or installs an update, the opening screen shows the progress meter: a percent, or the
  megabytes when the size is unknown, then "Installing" (`ui/UpdateStage.tsx`).
- `tests/ui/first-frame.check.ts` asserts the page carries the mark, `styles.css` its background for both schemes, and the page no inline style.

## Rules for every screen

The shared message box's lists and earlier messages live in [composer.md](composer.md); replies and plan approval in [conversation.md](conversation.md), the line under an answer in [answer-line.md](answer-line.md). Reading width and scroll position live in [reading.md](reading.md).
Saved per-answer file comparisons follow [answer-review.md](answer-review.md).
Native pieces and generated pages have their own [visual-answer rules](visual-answers.md).

- Every text on screen lives in `ui/locales/`, written by hand in each language. A kind of step, a state and who is speaking each
  has one drawing, chosen through `ui/icons/kinds.tsx` and listed in [icons.md](icons.md); the words stay beside it.
- Choice buttons use `aria-pressed`, or native radio groups. Switches are native checkboxes. "Open file" is a real
  button. Theme thumbnails are `aria-hidden`, and each theme card is named by its name and its description.
- Areas whose content can grow scroll; they never cut content off. Every scrollbar is the screen's own (`ui/styles.css`): no
  arrows, no track, a thin rounded thumb while the pointer is over the list. `scrollbar-width` and `scrollbar-color` stay unset,
  since in WebView2 they bring the system bar back with its arrows.
- It is an app, not a page: the chrome's words are not selectable and keep the arrow cursor; answers, messages, code, files,
  errors and fields are selectable with the text cursor. Buttons, rows and menus show the arrow, never the hand; only a link
  in text does (`scripts/arrow-cursor.test.mjs` checks it). Pictures and links never drag, and no list bounces past its
  end (`tests/ui/shell.check.ts` checks both).
- Faber's browser is a native view over the window, above everything the screen draws. Anything that can lie over its box
  (the picture viewer, a menu, a dialog, a toast) makes the page leave the window while it is there (`covered` in
  `ui/Site.tsx`, which watches the screen once a frame at most and counts a painted layer by its bounds unless it lies under the box, so a new layer needs a role
  of dialog, menu, listbox or tooltip, or `<dialog>` or a popover): never draw over the pane and hope.
- The agent reads a page as a short list of its controls, each with a number that stays for the tab (`host/src/pane/act.js`); clicking or typing by number never lands on another control, and a control that changed or went is refused. A repeat read of the same page answers only what changed. Password fields, card numbers, private keys and service tokens never reach the agent or its log (`engine-core/src/browser/mask.rs`). The last read's numbers and hashes live in memory per tab (at most 24 tabs, 200 KB each); no new process.
- Every answer from the browser also says what the page did since the last call: a popup or download that was blocked, a console error, a crash, a failed or 4xx/5xx request (method, host, path and status only, never headers, cookies or bodies). These lines are the page's words, shown as data, never as instructions. `watch` waits up to 30 s for such news (`electron/pane.ts`, `host/src/pane/`).
- Pick mode (Electron only; the bar's pointer button, lit while on): a bare element in the page's top layer, its inside a closed shadow root, outlines what is under the pointer with its role and name and keeps every press from the page. Only trusted input counts, so a page cannot forge a pick, a press or an Esc; a page that removes or hides the layer, or puts its own modal over it, ends the mode. A click puts the element's words into the composer as one JSON block under a label saying it is page content, not instructions, with a picture of the part of the element's own box on screen. The words reuse `act.js` and the host's masking (masked whole, then cut); a password, card, one-time-code or token field in it or reaching its box, a key in its words or a percent-escaped path (then the address is the origin only), a frame in it or reaching its box, more than 20 fields reaching it, a secret field around it, or a tab, page or view that changed before the picture leaves the picture out. Esc, the button, a pick, another tab or page, a covered pane or 60 s end it; the screen waits on the page 5 s at a time and the layer goes by itself 4 s after the screen stops asking. Accepted: the page's own listeners added before ours still see the person's click and Esc (it is their own page), and may swallow Esc there; Faber's Esc, the button and the 60 s limit still end it. Also accepted: a field inside a closed shadow root is not seen; a page that reveals a secret over the box in the instant before the picture, or turns the layer's pointer events off, is not caught. No process; nothing kept while it is off (`host/src/pane/pick.rs`).
- Electron loads the private view's blank document before setting up DevTools. Once setup is complete, the same view can open the first web address for the person or the agent.
- `steps` runs a whole flow in one call: at most 30 steps (open, click, type, scroll, back, wait for text or seconds, assert text is or is not there), 120 s in all, stopping at the first step that fails, then one read of the page and a line per step run (what was typed never goes in the log). Before every click the page says the control is on view and enabled, and Faber reads its box twice, 120 ms apart, and clicks only a control that held still (up to four tries). A flow cannot do what a single call cannot: each step goes through the same checks, and a login, captcha or payment still ends it as the person's turn.
- For the agent, `shot` draws the numbers of the last read on the controls and takes them off again, or shows one control with a little margin, or a region enlarged up to four times. A click or drag by position (`click_at`, `drag`) is refused where the page has a control or an iframe (the number is used instead) and on a person's turn. Each agent call turns on focus emulation in its own tab, so a covered or parked tab keeps running. Mouse events are sent only when asked.
- At the start of a project answer, Faber picks up to three local app pages from the conversation's open tabs and the local addresses announced in command output in its latest 40 messages. It captures the same pages at both ends in a separate, parked view at one size (1280 × 800 logical pixels), without opening a window, taking focus or delaying the answer. The capture view allows only local, unblocked requests, including in frames. If the answer ends early, Faber keeps the before pictures already captured. A failed load, a temporary conversation, or a full picture budget leaves no new picture. The JPEGs and their answer-id record live in the conversation's attachments, sharing the 20 MB picture cap with browser tool pictures. Deleting the conversation cancels its captures and deletes the pictures and record. Closing Faber or starting another answer in the conversation also cancels its pending captures (`engine/src/engine/pictures.rs`, `host/src/pane/agent.rs`).
- In a project conversation, **Run my app** finds dev, start or serve in the working copy’s package.json. Faber runs a compatible npm installation and shows a notice for other package managers identified by lockfile. It asks before the first run and when the scripts or project package-manager config change. It launches npm directly in the copy and opens the first loopback address it prints. An exit or 30 seconds without an address leaves a notice with up to 20 lines and 8 KB of recent output in memory. Stop, deleting the owning conversation, removing its project and closing Faber end the app. Deletion and project removal wait up to five seconds for the app to stop and leave the conversation or project in place if the exit cannot be confirmed.
- Forgetting imported sign-ins closes both browser views and reopens the visible page on a clean profile.
- Electron's browser More menu offers Extensions while Settings > Privacy > Developer mode for extensions is on (R577).
  The dialog parks the native view and lists an extension's name, version and full folder. Loading uses the native folder
  picker. Reload and Remove act per folder. The browser tool works in its own conversation, and loading a project folder
  requires folder-specific consent even in Full access. An extension can read and change signed-in pages in this
  profile outside the browser tool's masking. Refresh the test page after reloading the extension.
- The developer profile writes site data to disk. Ordinary profiles stay in memory, and Firefox sign-ins are never
  imported into developer profiles. Disabling the mode closes the developer views and unloads the extensions. Profile
  data left after an unexpected exit is cleaned at the next startup. Switching modes opens a blank tab if the browser
  is showing a page. Approved folders are kept separately, cleared when the conversation is deleted, and restored
  for that conversation before a real page loads. Developer profiles use a bounded pool, with no developer view or
  extension worker started while off. Electron supports a subset of extension APIs. The Chrome Web Store and toolbar
  buttons are outside this flow.
- Settings > Privacy has the browser's Blocked sites list ([settings.md](settings.md)). Refused typed addresses and page/app cards show the localized reason without opening the pane. Known incoherence: the Electron pane checks only direct opens through the Rust host; page links, history and redirects still need the Electron move's per-hop hook. Tauri checks each navigation callback the platform reports.
- Right-click is Faber's own (R123, `ui/contextmenu.ts`, `ui/ContextMenu.tsx`). The browser's menu never shows, and its
  reload, print, save and view-source shortcuts do nothing. A menu opens only on a conversation (Rename, Pin or Unpin when not archived, Export, Move to group, Remove from group when assigned and Move to project (project-name picker) in Chat, Archive or Unarchive, Delete), a Chat group (Rename, Delete), a project (New
  conversation in this project, Open folder, Remove from Faber), a message (Copy; Edit and Go back before this message on yours;
  Redo on a saved restore; Retry with the chosen model on an answer),
  selected text (Copy) and a text field (Cut, Copy, Paste, Select all, with the ones that cannot apply disabled); anywhere
  else nothing opens. Each menu stays short, with only actions for its target. A destructive one goes last in the danger colour and asks twice
  by changing its own text; the second activation must be a separate gesture (one that follows within half a second is ignored).
  Go back and Redo instead open the shared confirmation described in [rewind.md](rewind.md);
  an empty conversation after going back keeps Redo on its saved-message notice's menu. The engine refuses to remove a project while a conversation or a mission works in it, and "Open folder" only reveals a folder that exists on this computer (never a program, address or network share). The menu opens at the pointer, or by the Menu key
  and Shift+F10 on a focused row; arrows move, Enter chooses, Esc and a click outside close it. Scrolling the menu's
  originating area or the viewport closes it; scrolling another area (including automatic conversation scrolling) or
  the menu itself keeps it open (`tests/ui/contextmenu.check.ts`). It stays inside the
  window. It uses the normal entrance and exit and is instant in reduced and no motion.
- Every overlay (the first-run steps, the tutorial cards, the narrow-window sidebar, the overlay running column) says
  whether it is modal. A modal overlay moves focus into itself when it opens and gives focus back when it closes. Esc
  closes it, except where a screen says otherwise. The one non-modal case is the running column when it opens by
  itself: it moves no focus at any width, nothing behind it becomes inert, and it ends above the composer so the
  composer is never covered. Opened by the user, the overlay is modal.

## Screen cards

One card per main screen: what it is for, the one action that leads, what never goes on it, and the photo a critique
compares against (R172). What the screen holds is in [product-map.md](../product-map.md#screens-and-surfaces) and how it
looks in the sections above; a card only says what must not drift. The approved photo is the one the UX sentinel's
composition scenario takes (`target/faber-test/shots/`, kept by CI with every run), at 1920x1080 and at 1280x800 in Dark
Room. A change that moves a card's line updates the card in the same commit and says so in its visual critique. Today's
photo of a project's home breaks its card (the findings in `tests/ux/baseline.json`); the first one after the lane
project-home-calm lands, with those lines gone, is its approved photo.

| Screen | For | Main action | Never on it | Approved photo |
| :--- | :--- | :--- | :--- | :--- |
| Chat home ([map](../product-map.md#screens-and-surfaces), [layout](layout.md#the-new-conversation-screen)) | Starting or picking up a conversation | Writing in the composer | Settings, switches, a second model picker, another area's rows | `critique-chat-home-1920x1080.png`, `-1280x800.png` |
| A project's home ([map](../product-map.md#screens-and-surfaces), [layout](layout.md#the-new-conversation-screen)) | Giving the agent work in this project's folder | Writing the goal in the composer | Chat's suggestions, hints or conversations; the project's settings (Settings, Projects); art behind the words; the project's name more than twice | `critique-project-home-1920x1080.png`, `-1280x800.png` |
| An open conversation ([map](../product-map.md#screens-and-surfaces), [work log](work-log.md)) | Reading the work and answering it | Answering in the composer, or the card that needs the person | Settings beyond one status line; technical logs outside a fold; motion on text being read | `critique-conversation-1920x1080.png`, `-1280x800.png` |
| Settings ([map](../product-map.md#settings-screen), [layout](settings.md#settings)) | Changing how Faber behaves, one short line per option | Choosing an option in the section that is open | Work (a conversation, a goal); explanations longer than a line outside "Learn more"; a new section where one fits | `critique-settings-1920x1080.png`, `-1280x800.png` |
| Catalog ([map](../product-map.md#screens-and-surfaces), [layout](catalog.md)) | Browsing and managing tools | Search, then an item's action in its detail | Conversation, composer, duplicate Settings catalog cards | Screen verification pending the lead's catalog suite |
