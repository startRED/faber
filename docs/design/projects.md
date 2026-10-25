# Projects

The published contract of the Projects area: screens, separate copies, stored records and how checks run.

## Projects

The engine types live in `engine/src/project.rs` and its modules (`checks.rs`, `copy.rs`, `git.rs`, `remote.rs`, `lease.rs`).

### Screens

A mission has no screen: a project is worked in by talking to its agent (R160), and the start card, the checks sheet, the
mission board and the local build card left the bundle with their 30 s poll (R171). The old mission machinery was removed from the engine (R215). The Projects area shows one of two things:

- With no project open, the small "Open a project" screen (`ui/Projects.tsx`): the empty state with its one button, the
  recent folders and saved environments, and at most three project conversations to pick up.
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
  (its tooltip says what it lists). Only a click asks: Codex starts for a moment, and Claude Code's history of the
  folder is read; the section then takes the
  heading, says "Looking…", and shows up to five sessions started in a terminal in the folder (the pick-up row: the
  session's title or first message, "Codex · when" or "Claude · when"), or one line when there is none. A row opens the session as a
  project conversation in the folder itself, never a separate copy, whose next message resumes it with its own CLI, even
  with Auto in the picker; opening it again opens the same conversation, archived or not. Its earlier turns stay in the CLI: the conversation shows what is said from then on.
  Claude Code has no list to ask, so its sessions come from its history files of that folder only (parity F5).

### Standing goals

Above Memory, one more quiet fold (`ui/ProjectGoals.tsx`) built the same way: a text button, the shared Collapse, one
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
An empty list and a failed read are distinct, and Refresh retries. A partial scan says it is partial.
For Codex, each folder shows only the first nonempty instruction file it reads: `AGENTS.override.md`, then `AGENTS.md`.

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

A project conversation works in a separate copy of the repository (R201, `engine/src/project/copy.rs`): a git
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
