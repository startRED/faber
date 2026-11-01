# Faber design

Decided on 2026-10-11. This is the direction that Phase 1 builds. It replaces the Mono design of 2026-10-10. The
approved mock (six screens) is the visual reference, and each Phase 1 piece updates the page of its own part.

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

The shared message box's lists and earlier messages live in [composer.md](composer.md); replies and plan approval in [conversation.md](conversation.md). Reading width and scroll position live in [reading.md](reading.md).
Native pieces and generated pages have their own [visual-answer rules](visual-answers.md).

- Every text on screen lives in `ui/locales/`, written by hand in each language.
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
- Forgetting imported sign-ins closes both browser views and reopens the visible page on a clean profile.
- Right-click is Faber's own (R123, `ui/contextmenu.ts`, `ui/ContextMenu.tsx`). The browser's menu never shows, and its
  reload, print, save and view-source shortcuts do nothing. A menu opens only on a conversation (Rename, Pin or Unpin when not archived, Export, Move to group, Remove from group when assigned and Move to project (project-name picker) in Chat, Archive or Unarchive, Delete), a Chat group (Rename, Delete), a project (New
  conversation in this project, Open folder, Remove from Faber), a message (Copy; Edit and Go back before this message on yours;
  Redo on a saved restore; Retry with the chosen model on an answer),
  selected text (Copy) and a text field (Cut, Copy, Paste, Select all, with the ones that cannot apply disabled); anywhere
  else nothing opens. Each menu stays short, with only actions for its target. A destructive one goes last in the danger colour and asks twice
  by changing its own text; the second activation must be a separate gesture (one that follows within half a second is ignored).
  Go back and Redo instead open the shared confirmation described in [rewind.md](rewind.md);
  an empty conversation after going back keeps Redo on its saved-message notice's menu. The engine refuses to remove a project while a conversation or a mission works in it, and "Open folder" only reveals a folder that exists on this computer (never a program, address or network share). The menu opens at the pointer, or by the Menu key
  and Shift+F10 on a focused row; arrows move, Enter chooses, Esc and a click outside close it, and it stays inside the
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
