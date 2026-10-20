# Settings and first run

How the Settings screen, the first-run steps and the tutorial look and behave.

## Settings

- Settings replaces the shell, as before, with a list of five sections on the left: General, Projects, Appearance,
  Accounts and limits and Privacy, and "Faber <version>" once at the bottom of the list. Ctrl comma opens it on General,
  the quota of a vendor in the sidebar opens it on Accounts and limits, and Esc or Back closes it. Switching sections is
  a short cross-fade (the owner, 2026-10-17, reversing R169): the old section leaves as an inert copy while the new one
  arrives, the new section opens scrolled to its top, and with reduced or no motion it is instant. All sections share one content width, 720 px (`--st-w`).
- One grammar for every section (`ui/controls.tsx`). A group is a card (`--s1` with an inset `--line`, `--r-md`) under a
  small title; inside it every option is one row: the name and one short line (one line, 70 characters at most) at the left
  and the control at the right, rows divided by hairlines. The longer text of an option unfolds under its row from a small
  round "?" next to the name (`aria-expanded`, `aria-controls`, one accessible name, `set.learnMore`); there is no
  "Learn more" link. A fold (`Fold`) is a row whose title is a button; what is inside it is not in the page while it is
  closed. Switches are native checkboxes with `role="switch"`; segmented controls and selects sit in the control slot; the
  mode menu is a select-only combobox whose open list shows each choice with its own line. Text fields share one class,
  `.field`. Under 960 px the control slot moves under the name.
- General has, in groups: *You* (language, name); *How the AI works* (Permissions as a menu of the five modes, Web search,
  CLI hooks, Chat working folder with the recent folders where its control exists, and a closed "Codex" fold with
  reasoning summary and answer length); *Notifications and system* (Notify me, Play a sound, and on Windows keep the
  computer awake while Faber works and start Faber at sign-in); *Updates* (one switch, "Receive previews", off by default,
  with one short line; it acts at the next update check and turning it off never installs an older version). "Don't ask" is the default for new and
  existing settings; "Ask before commands" preserves the previous behavior. Choices save at once and stay at the saved
  value if the save fails. R107 applies the same default to Chat. Chat uses its chosen canonical folder
  (fixed before the first message), or private per-conversation scratch; deletion/draft cleanup never removes a chosen
  folder. General explains the rule and offers the folder choice.
- Projects starts with *Skills for projects*: one card closed like a project's, its head the name, how many are on ("2 of
  21 on") and the chevron. Open, it shows one quiet line, then a switch per skill of the catalog: its name as its
  repository spells it, one line of what it does and its licence, the repository behind the "?". While one installs or
  goes the line says so and every switch waits; a failure turns the switch back and shows the save-error toast; the card
  is absent when the list cannot be read. *Tools for projects* follows, the same card for the catalog's tool servers, its
  quiet line saying each one runs on this computer with Node.js. The projects follow, each in its own closed card.
- R140 (Chat; the person's own project sessions take the same modes, and mission roles never do): the Permissions choice is a
  menu of five modes, each with one plain line and its long text behind the closed "?" of the row, which is not in the page
  while closed. The composer of Chat carries the same choice as a chip. Mode by CLI (the CLI that lacks a mode runs the
  nearest safer one and the reply says so, `mode.fallback.auto`):

  | Mode | Claude Code `--permission-mode` | Codex approvalPolicy / sandbox / approvalsReviewer |
  |---|---|---|
  | Ask | `default` | `untrusted` / `workspace-write` / `user` |
  | Accept edits | `acceptEdits` | `on-request` / `workspace-write` / `user` |
  | Plan | `plan` | `on-request` / `read-only` / `user` |
  | Auto | `auto` (else `acceptEdits`) | `on-request` / `workspace-write` / `auto_review` |
  | Full access (default, stored `dontAsk`) | `bypassPermissions` | `never` / `danger-full-access` |

  Rules that hold in every mode: Plan never takes a remembered "Always allow" and remembers none. Moving to a stricter mode
  stops a running turn and revokes every remembered grant. The revocation is a new random token, `grantsEpoch`, written in the
  same settings save that changes the mode; each grant carries the token that was current when it was made (`allowAlwaysEpochs`),
  and it stands only while the settings hold that same token, on every check and after every restart. No clock is involved. A
  grant with no token (an older install) stands only while the settings have never held one. The conversation files are also
  cleaned, but only as tidiness: a write that failed cannot bring a grant back. Settings that cannot be read start a new token
  that is kept in memory and is the one in force until a save writes it, so a read that works again never brings the old token
  back. A card remembers the token it was made under: an "Always allow" on it is kept only if the token is still the same when the
  answer settles, and otherwise lets that one request through and remembers nothing. Two Faber processes on one data folder are
  not supported (single-instance protection prevents it; `FABER_DATA_DIR` is a dev and test hook), so no cross-process
  serialization of this is built. A stricter web setting stops a running turn but
  does not change command grants.
  No mode promises zero execution: Ask lets plain reads through, Plan lets read-only commands through, and Accept edits lets
  Codex run commands inside the working folder. Leaving Claude's plan mode names `default` for the session. Settings that cannot
  be read start in Ask, not Full access (a missing file still means Full access). Every isolated or internal Codex thread has
  `web_search` disabled. Credential-folder denies hold for Claude's file tools in every mode. For Codex Chat they are not
  applied yet: permission profiles that extend `:workspace` or `:read-only` could carry them on compatible backends, and that is
  a follow-up. Until then Faber only refuses a forwarded request that names a credential folder.

  Web search (Off, Cached, Live; Live by default in the saved Chat settings, Off for any launch nobody chose for): Codex `web_search` in the thread config; Claude Code allows
  WebSearch and WebFetch (Live), only WebSearch and denies WebFetch (Cached, which it lacks), or denies both (Off).
  Cached is offered only while Codex is connected. Codex reasoning summary and verbosity go into the thread config as
  `model_reasoning_summary` and `model_verbosity` and stay unset (the model's own) until chosen.
- R121 (it replaces the next item for the two choices on screen): "Don't ask" is full access. No approval cards, commands
  run without Faber's sandbox and with network, WebFetch and WebSearch work, any folder can be the working folder, and the
  person's own MCP servers, plugins and hooks stay on, as in the CLI itself. Claude Code runs in bypass mode with Read and
  Edit deny rules for the credential folders (`.claude`, `.codex`, `.gemini`, `.ssh` and the other key files, and where a
  documented variable moved them), which apply in bypass mode to its file tools but not to arbitrary commands. Codex runs with
  `danger-full-access` and `never` approvals and has no way to block reads of the credential folders in that mode (see the
  R121 section of [the R97 security review](../security-r97-review.md)). Ask names its modes (Claude `default`, Codex
  `untrusted`) and carries the same credential denies and the same refusal of forwarded credential requests; it asks except for
  what the person pre-allowed in the tool's own settings. The system and credential folders are refused as working folders, judged
  by their resolved path (R124, R124b), and Faber's own requests never run the person's hooks. A CLI and everything it started
  end together (a Windows job object made before the CLI runs, failing closed; R124b, R124c). Known residual: the CLI is
  created suspended and joins its job a moment later (about 100 ms, measured on Windows 11), so if Faber ends in that
  moment the CLI stays suspended, outside any job, until the PC restarts. It never runs a single instruction. Std Rust
  cannot create a process inside a job yet (`raw_attribute` is unstable), so Faber starts as few CLIs as it can: Codex is
  asked for its models only while Faber has none, one asking at a time. Settings shows the
  details; the thread shows nothing about it (R168). The isolated mode below is `Permissions::Isolated`: Faber's
  own model list, router, internal checks and self-development roles use it, and the person cannot pick it.
- No prompt can block a full-access turn. Where the installed Claude Code declares `--permission-prompts` as an option line in a successful `claude --help` (read once
  at start, and again when a launch fails with "unknown option", which also drops the newer flags at once; present on 2.1.296), full access starts it with `--permission-mode bypassPermissions --permission-prompts none` and no host prompt
  tool: a request that would have prompted is denied at once, listed in the turn's `permission_denials`, and kept on the reply as a
  parked request, as safe metadata only (tool, a kind, a file path or a page host, the time; never a command or any input, which
  could hold a token), at most 20 and 4 KB per turn with the rest counted ("+N more"). The row of the running list says "Needs you"
  with a card that has no buttons (the CLI is past it, and the work was not done) until the next message. The tray notifies once
  per turn ("N requests need you"), with a one-minute cooldown across turns (R92). It is never allowed on its own, and a request on a
  credential folder is not parked at all: the deny rules refuse it for good. A CLI without the flag, Ask and the isolated mode start
  as before. Where the CLI lists `--autocompact`, every launch but a side question compacts at its model's window (Haiku 150k,
  Sonnet 300k, Opus 400k, R150); side questions, unknown models and a CLI without the flag keep the CLI default.
- R97/R107 (now the isolated mode only): bypass permissions with native deny rules for `.git` edits, credential directories
  and key files; Codex uses `never` approvals with a workspace-only permission profile and sandbox network access disabled.
  Native deny rules [apply even in bypass mode](https://code.claude.com/docs/en/permission-modes#available-modes), but
  [command patterns are not an OS sandbox](https://code.claude.com/docs/en/permissions#what-a-bash-rule-doesnt-match).
  Explicit ask rules [still prompt in bypass mode](https://code.claude.com/docs/en/permission-modes#actions-no-mode-auto-approves)
  and reach Faber's automatic policy over stdio. It gates file reads/writes, read-only git and literal local deletions, resolving junctions and refusing
  external or `.git` writes. Build/test tools and project entrypoints run through the installed native Codex sandbox
  with the same profile, never through an unsandboxed fallback. Native Windows requires MXC support. Unsupported shell
  syntax and arbitrary programs are declined without a card. Secret reads
  are denied while environment templates and internal `.env` creation work. Git transports and GitHub authentication
  are disabled in child commands. Both areas disclose the mode; tightening it stops current turns.
  Don't ask loads only user settings to keep project env/hooks out of the host launcher. Chat also uses safe mode,
  strict MCP configuration and its fixed tool list; Ask Chat retains restricted mode. Read-only shell commands and
  deletions use the sandbox too; file tools retain their canonical path checks, including network-link guards.
  Host file decisions, CLI startup customizations and external integrations are separate from command isolation;
  see the verified findings and limits in [the R97 security review](../security-r97-review.md).
- Appearance is a draft, in the same cards and rows as the other sections. The first card, Look, has the six looks as small
  live renders (one look card, shared with the first run) and the accent with a hex field; the second, Text and layout, has
  the background, the two fonts, text size, density, corners and motion. A closed "Advanced" fold holds the measured contrast
  of text, faint text and the accent, the three effect switches, the fixed-meaning colours and the theme file. The controls
  change the draft, and the preview panel on the right wears the answer of `preview_look`. The panel
  says "Not applied yet" until "Use this look", which sits in a bar that stays in view. "Back to default" returns the
  draft to Dark Room. The three meaning colours are shown read-only, with their shapes and words.
- A refused accent, or a look with a failing pair, shows its sentence in the bar and "Use this look" stays as it was; a
  refused accent puts the draft back to the last one the shell accepted.
- An account counts as connected only when its CLI says someone is signed in; an installed CLI that says nobody is, or
  that cannot answer, is shown as such. The check runs each time the section opens.
- The preview draws the effect of the new conversation (a still picture of the beam, the loom or the line) and the two
  lights, so each of the three switches shows what it does before "Use this look".
- A theme file is written beside its destination and renamed over it only when the whole write worked, so a full disk
  leaves an old file as it was. A failed read or write shows words of Faber's own, never the system's.
- "Manage accounts" opens the account cards alone. Closing them (or Esc) brings Settings back on the same section, with
  the draft that was not applied.
- Accounts and limits has one card per AI (the former Connected accounts and Usage limits, one section): its mark, name,
  the plan with its first letter capital, and one status word in a pill that is amber only when a window is really near
  its limit (C002; fast use is said calmly). Then one row per quota window the vendor has reported (name, used share, a
  bar with the expected-pace mark, and the reset time written in full); an old reading, or none, shows "—". The card ends
  with Faber's own spend today in API value, written the language's way with two decimals. Every note about how the
  figures are read (counting delay, shared account, incomplete meters, learning, foreign use, extra usage, each window's
  projection and age) is behind one "Details" fold per card. "Manage accounts" appears once in the section.
- Privacy has three statements: telemetry is off, history stays on this PC, and Faber never reads the CLI logins.

## First run and tutorial

- The first run has three steps in a window with no sidebar: connected accounts, appearance, first conversation. A stepper
  at the top shows the place. Each step is a modal dialog: its heading takes focus when it opens, and Esc and the
  shortcuts of the shell (Ctrl N, Ctrl comma, Ctrl period) do nothing until a button is used.
- Step 1 is the account cards, with Continue and Skip; both go on to step 2 (or straight to the Chat home when a look is
  already saved). Opened later, from the no-AI state or from
  Settings, the same cards have no stepper and only a Close button. They save `setup` and nothing else, so reconnecting an
  account never restarts the first run or the tutorial.
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
