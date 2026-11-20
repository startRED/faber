# Projects

The published contract of the Projects area: screens, separate copies, stored records and how checks run.

After edits, checks or a plan, both drivers read the conclusion from a hidden JSON block at the end of the agent’s closing message. The engine validates and records the same closing box as a legacy `wrap_up` call, without showing the block while it streams. Successful checks are reused while their inputs and environment are unchanged; done still requires evidence. If the card lists no checks, the agent checks the result directly instead of calling verify. The bounded missing-conclusion request remains as recovery (R509). Under a done that followed edits, the box adds quiet lines from Faber itself (R571): how many things asked a receipt confirms, what is not confirmed, a failed last check, and what it noticed in the edits (a test turned off or weakened, a test file deleted, code marked unwritten, files outside the plan). A done let through after two refusals opens with "Done, but not confirmed" in the amber ink; the lines cost no process or timer (`ui/Plan.tsx`, `ProofLines`). Token savings need a live comparison (`engine-drivers/src/progress.rs`, `new_tools_omit_wrap_up_but_legacy_calls_are_recognized`).

## Projects

The engine types live in `engine-projects/src/project.rs` and its modules (`checks.rs`, `copy.rs`, `git.rs`, `remote.rs`, `lease.rs`).

### Screens

A mission has no screen: a project is worked in by talking to its agent (R160), and the start card, the checks sheet, the
mission board and the local build card left the bundle with their 30 s poll (R171). The old mission machinery was removed from the engine (R215). The Projects area shows one of two things:

- With no project open, the small "Open a project" screen (`ui/Projects.tsx`): the empty state with its one button, the
  recent folders and saved environments, and at most three project conversations to pick up.
  Its folder-purpose sentence and illustration appear here only: the empty sidebar keeps its quiet status and Add button,
  with the duplicate sentence and illustration hidden from layout and accessibility (`tests/ui/shell.check.ts`).
- With a project open, the new-conversation screen of Chat (`welcome-grid project`) scoped to the folder, in one column:
  Curioso, then the folder in short form as one quiet line in the interface size (the last two folder names,
  the full path on hover; the sidebar and the crumb already name the project, so the page says it only to screen readers), the greeting "What should we tackle?", the shared
  composer, and under it at most three of this project's own conversations to pick up, each saying only when. No
  suggestions (the composer's hints show what to ask) and no second column. No beam of light: a project screen is a place
  to work; in Dark Room the black hole keeps to the room on the right, so no words sit on its bright part. The sidebar
  lists the projects, and every setting of a project (the
  separate-copies switch and the checks) is in Settings, Projects (C011), one closed entry per project with one row per option with one short line and
  the rest behind the "?" of the row.
- With an AI connected, a quiet button with the terminal icon, "Sessions from the terminal", sits under the pick-up list
  (a short line says what it lists). Opening it asks: Codex starts for a moment, and Claude Code's history of the
  folder is read; the button stays available to close or reopen the list; it says "Looking…", and shows up to five sessions started in a terminal in the folder (the pick-up row: the
  session's title or first message, "Codex · when" or "Claude · when"), or one line when there is none. A row opens the session as a
  project conversation in the folder itself, never a separate copy, whose next message resumes it with its own CLI, even
  with Auto in the picker; opening it again opens the same conversation, archived or not. Its earlier turns stay in the CLI: the conversation shows what is said from then on.
  Claude Code has no list to ask, so its sessions come from its history files of that folder only (parity F5).

When native Git is missing, one quiet card above the composer explains that each conversation works in the project
folder (`reason.git_missing`). Its single ghost button, Download Git, opens the official download page in the system
browser through the existing open-link path. It uses the home notice surface, wraps at narrow widths and adds no
motion or polling. The engine caches the native executable lookup and checks again when the window regains focus.
A new conversation still starts in place, without a branch crumb or Changes; conversations already started in place
stay there after Git becomes available. Chat and missing or disconnected project folders do not show this card.

### Standing goals

A short line beneath each title explains standing goals, memory and terminal sessions whether the fold is open or closed. Above Memory, one more quiet fold (`ui/ProjectGoals.tsx`) built the same way: a text button, the shared Collapse, one
column, read on open and on Refresh, nothing polling. Each goal is a row of the person's words, then one small line: the
state word (Holding, Checking, Broke and being fixed, Needs you, Paused), when it was last checked and its check as code.
Needs you is the only state word set heavier (the ink colour at weight 580, no amber) and carries its reason in plain
words under the row. A goal that waits for the person or was paused offers Watch again; Remove asks once more with Cancel
beside it (500 ms and the key released, as in Memory). The form is one field for the words, one list of the confirmed
checks and Add; with no confirmed check a line says to confirm them first. In Faber's own project one more quiet button,
Watch for crashes, sits beside Add and takes the same words; its row shows the name of the record it watches where the
other rows show a check as code. No other project is offered it, it has no list to pick from, and a project keeps one: a second is refused with a calm line above the list. A read that fails shows its reason above the
list, also before anything was read; with no goal only the empty line shows, and the line that says the switch is off
appears once there are goals. It works with no AI connected. The switch that lets Faber start a fix, and its US$2
daily limit, are in Settings, Projects, never here. If that switch cannot be read, its row says why instead of the
limit.

### Memory

The project page extends its existing single column with one quiet Memory fold below the pick-up list
(`ui/ProjectMemory.tsx`); it remains available with no AI connected. It is the project’s work, not a setting.
Opening it reads the files; Refresh reads again, and closing it drops the list and its editors. Nothing polls or
watches. The shared Collapse follows the look’s normal/reduced/no-motion rules; entries and text never animate.

Each row shows the source, relative file and original line, then literal Markdown text. Paths and paragraphs wrap at
720 px; only 50 rows mount first, with Show more. A file over 256 KiB shows its path and a short reason without entries.
A Faber note over its target shows one quiet line with its size and target, once per file. The generated rules file is read-only. An empty list and a failed read are distinct, and Refresh retries. A partial scan says it is partial.
At a conversation's closing box, Faber checks at most 200 backticked relative file references in its notes, through the
existing no-follow handles. Missing or refused references add ` (stale?)` before the author mark through a normal
revision-guarded edit. A matching person-authored decision stays byte-for-byte unchanged; its status lives with its
approval outside the project. Memory shows one quiet localized line, "File may no longer exist; left out of the start
card.", instead of the raw suffix. The editor keeps the exact source text. A later close clears the status when all
references exist again. Commands, URLs, setting names and prose are not evaluated. No timer, watcher or new control.
Generated rules and follow-up task files are excluded; tasks may name files that future work will create.
Memory refuses Gemini's default private folder for reads and edits, alongside the other CLI private folders.
For Codex, each folder shows only the first nonempty instruction file it reads: `AGENTS.override.md`, then `AGENTS.md`.

Root `.faber/taste.md` and `.faber/lessons.md` entries without a matching external approval show the quiet “Not reviewed” flag in every language, with no decision Keep action. Their existing Edit and Delete actions do not approve new words. Editing or deleting an entry revokes approval of the removed words before saving, so restoring those words still needs fresh approval; a failed revocation preserves the text. Replacements enforce the same 1/2 KiB file targets as append, unless the caller supplies separate explicit consent; deletion can reduce an oversized file. The engine-only append function requires the person’s approval; proposal controls remain unbuilt. Only approved taste enters the next start card, within its existing share. Damaged approval records leave the notes readable and unreviewed.

Keeping another decision preserves every older approval of a live project. If approval records or the current project
list cannot be read safely, Keep reports the existing save error and leaves the approval records untouched.

In `.faber/decisions.md` an entry that ends with its author mark shows it as a quiet word after the file and line ("AI, not reviewed", "AI, kept", "You") and the text without the mark. A `[by person]` mark that Faber has no record of (an agent can write any mark) reads "AI, not reviewed" until the person keeps it, and then "You". Only a decision not yet reviewed has Keep, first among the row's actions; it is a plain ghost button like Edit and needs no confirmation, since it writes nothing to the project and Edit can still change the words. Keep records, in Faber's data folder, that the person kept those words, so the flag reads "not reviewed" again if the words change. Pressing it leaves the list as long as it was, moves focus to that row's Edit and says "Decision kept." to screen readers in a polite status line. Editing the words of an AI decision leaves it the AI's and not reviewed: ending the text with `[by person]` makes it the person's, recorded like a Keep. Only "AI, not reviewed" takes the darker ink, because it is the one that asks for something. An entry with no mark shows neither.

Edit opens a labelled textarea in place, focused on opening, with Save and Cancel. Escape from anywhere in the form
cancels except during text composition or a save. Delete asks on the row first; confirmation requires a separate gesture
at least 500 ms later and releasing the key that armed it, as in the shared context menu. Cancel leaves the file alone. During a
write the actions wait. A changed-file refusal keeps the draft and asks the person to reopen the file. Read-only files get
a plain refusal, and a replaced project folder or ancestor also refuses a stale save. One quiet line
says that changes apply on the AI’s next launch in this folder; another says that separate copies and private home memory are outside this project-folder list. The list
shows sources, not a claim that every nested instruction is active in every session. Faber has no memory store of its own: the one file a project's agent may create is `.faber/decisions.md`, and only for a lasting choice. Memory reads the project folder, so a decision written in a conversation's separate copy shows here once that copy's work is merged into the project; the copies are not scanned, which keeps opening Memory as light as it is.

Projects under linked parent folders work normally. Links from the project root down to a memory file are refused;
the root identity recorded on reading must still match before a save opens any memory file.
An absolute project path containing dot or parent components, or a Windows short name, lists and edits normally;
these project address forms do not relax the checks on relative memory-file paths or private folders.
Private CLI configuration folders and their linked targets stay excluded, using metadata only without listing or reading their contents.

An open memory editor joins the shared draft protection, even when its text is empty: an unattended update cannot
restart Faber while it is open or saving, including when minimized (C004). Cancel or a completed save releases it;
a failed save keeps the editor protected. A pending write stays protected even if the fold closes or the project changes.

A save checks the revision and then replaces the file in one atomic operation: an interruption leaves the old or new
file under its instruction name. Windows refuses in-place writers through replacement. Another atomic save after the
final check, or a previously opened writer on Unix, remains the accepted normal editor window (project-memory R333).

### Separate copies

A person's short first request in a project conversation waits for one cheap read before it reaches the agent (R574,
`engine/src/engine/clarify.rs`): at most 12 s, beside Auto's routing, while the reply shows its usual typing state; a model
chosen by hand starts at once, unjudged, so the read never adds a wait of its own. A choice
that would waste the work if guessed wrong becomes one question on a card in the agent's first reply; a request about looks
that names no look gets two or three one-line directions there instead. Nothing else on screen changes, and a Stop during the
read starts nothing. Cost: one small model call per new project conversation, no process kept.

Copies and registered projects share the [portable Git workflow](../product/git-workflow.md): idle, rate-limited
housekeeping and safe retirement at startup and turn boundaries, with no extra screen, timer or permanent process.

A project conversation works in a separate copy of the repository (R201, `engine-projects/src/project/copy.rs`): a git
worktree in `<data>/copies/<conversation id>` on the branch `faber/<a few words>-<4 characters of the id>`, made with the
git runner of `project/git.rs` (fixed argv, no shell, hooks and filters off) from the head the person has checked out, before
the first message reaches the CLI. The agent starts in the same place of the copy as the project folder is in the
repository. The copy is made outside the engine's lock and in a blocking task of the shell, so a slow checkout holds
neither the screen nor the other conversations. Nothing polls: a copy is removed when its conversation is deleted, and
once at start (`Engine::sweep_copies`) when it is clean and every file its branch changed is the same in the person's
branch or its upstream, which a merge, a fast-forward and a squash all satisfy. Uncommitted changes, or commits no other
branch holds, refuse the delete with `error.copy.unsaved` until the person discards them: the row's × then asks once more,
"Discard changes?", with the reason as its label and tooltip, and only that click discards. The topbar crumb adds
"› faber/…", the branch, in the extra-small data font and the crumb's quiet ink, cut before the title is; the copy's
folder is its tooltip. While the switch is on (the default), every new conversation of the project gets one (parity P3).
A copy costs the disk of the project's checked-out files, since git's history is shared with the project folder and not
copied (11.6 MB for Faber's own 688 files on 2026-10-20), plus whatever the agent builds or installs in it, such as a
dependency or build folder, which can reach gigabytes; all of it goes with the conversation, or once its work is merged.

### What is stored

The old mission machinery (journals, plans, contracts, worker envelopes, the coordinator, the local build) was removed
in R215 (task 060). What stays:

- `projects.json` keeps the fields it does not know and writes them back unchanged, so the `repo` and `grant` an older
  version saved stay in the file and nothing reads them.
- Writes replace the file atomically; a failure leaves the old file and returns `error.store.full` or
  `error.store.write`. A damaged project list or quota ledger is `error.store.corrupt`, never empty data. A damaged
  `budget.json` is copied to `budget.json.quarantine`, and Projects stays closed while that copy exists.
- Folders and files an older version left (`.faber/missions/`, `missions/`, `workspaces/`, `proof-target/`, `qualification/`, mission
  keys of `budget.json`) are never read, repaired or deleted; they never block removing a project or opening Chat.

### How checks run

A project's checks (the commands confirmed in Settings > Projects, R137) run through Faber's `verify` tool as the
person's own process (`proc::spawn_check`): no window, its own kill-on-close job, below-normal priority on Windows, in
the project's folder or the conversation's copy. They can do what the person can do there. The isolated proof runner of
R138 (Low integrity, a sandboxed clone) left with the missions it served (R215).
