# Electron keeps the unsent draft and the previous thread after a forced close

Area: Chat
Files: electron/, ui/ (draft storage), testkit/src/screen/, tests/ui/
Depends: none
Size: about 150 changed lines

Why: the 1.0 gate's Electron hidden-build run (reports/reforma/helpers/g390j.result.md, evidence in reports/reforma/helpers/g390j/) found:
1. High: after a forced process close, Electron comes back with an empty composer. Before the close the text, the attachment marker (`faber.unsent-attachments.new-chat-` = 1) and one stored file exist; after it the localStorage draft entries are gone and no attachment shows (the file stays in IndexedDB). Tauri keeps text, marker and attachment in the identical test (`g390j/electron-restart-storage/storage.json` against `g390j/tauri-restart-storage/storage.json`).
2. Medium: the "a hard kill preserves the answer and Continue resumes the native session" scenario lands on the home, not on the previous thread, in Electron; Tauri passes.
3. Medium: at the compact size the sticky return control hides the focused heading of a Catalog detail (see M1 in the result file).

Build: first reproduce each with the evidence's scenario. Find why Electron loses localStorage on a forced close (storage not flushed before the process dies) and make the draft survive, for example by flushing on each change or keeping the draft where the engine already persists it, whichever the code already does for Tauri. Reopen the previous thread after a hard kill. Keep the heading unobscured at 720x480 and return focus to the card. A check per fix fails without it, in the Electron shell.

Leave out: other shells and any new setting.

Done: the restart check shows the text, marker and attachment back in Electron, the hard-kill scenario passes in Electron, and the compact heading is visible; the checks fail without the change.
