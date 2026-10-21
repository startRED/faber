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
  Curioso, the project and its folder in short form as one quiet line in the interface size (the last two folder names,
  the full path on hover; the sidebar and the crumb already name it), the greeting "What should we tackle?", the shared
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
folder is its tooltip.

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
