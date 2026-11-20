# Permissions

How the Settings and composer permission choices behave.

Readable cards keep the original request in a disclosure. Commands preserve argument boundaries and network requests show their destination; edits show proposed changed lines from the shared change reader, including all lines for assistive technology. Connected tools show the server, tool and the first 1,000 characters of arguments, with a marker and full details when longer. Direction and control characters are shown as code points throughout the request. Codex reason and working folder are retained. Native controls keep keyboard order and readable content stays still.

Mode details distinguish requested CLI policy from machine isolation, which remains unverified. No new process or idle timer is kept. Full access retains its existing stored value and default; remembered-rule withdrawal and denial feedback need the separate host-command change.

A decision the person wrote and kept as "never touch `path`" (R575) needs no card of its own: in every mode Claude Code's file
tools are denied the path, a guarded Codex sandbox keeps it read-only, and an approval that names it is declined before any card,
with a line telling the agent to leave it as it is. In Full access a shell command is not stopped; nothing on screen claims more.
The paths are read when a session starts (a rule kept mid-conversation holds from the next start), each on its own; a path
through a link, outside the project or with pattern characters (in it or in the project's folder) protects nothing, at most 32
count within 2,048 characters, and a decision list that cannot be read keeps the paths last read in this run (none on the first
start). Only path fields (`source` and `destination` too) and command lines are compared, never a description; a command line
that only mentions the name, as in a commit message, is refused too.

- R140 (Chat; the person's own project sessions take the same modes, and mission roles never do): the Permissions choice is a
  menu of five modes, each with one plain line and its long text behind the closed "?" of the row, which is not in the page
  while closed. The composer of Chat carries the same choice as a chip. Mode by CLI (the CLI that lacks a mode runs the
  nearest safer one and the reply says so, `mode.fallback.auto`):

  | Mode | Claude Code `--permission-mode` | Codex approvalPolicy / sandbox / approvalsReviewer |
  |---|---|---|
  | Ask | `default` | `untrusted` / named writable profile / `user` |
  | Accept edits | `acceptEdits` | `on-request` / named writable profile / `user` |
  | Plan | `plan` | `on-request` / named read-only profile / `user` |
  | Auto | `auto` (else `acceptEdits`) | `on-request` / named writable profile / `auto_review` |
  | Full access (default, stored `dontAsk`) | `bypassPermissions` | `never` / `danger-full-access` |

  Sandboxed file tools deny writes to the actual Store root, including `FABER_DATA_DIR`. Codex profiles keep it and
  nested workspaces read-only, preserving allowed reads and approval policies. Both drivers refuse recognized writes
  before a card, resolving existing ancestors so Windows short names and junction aliases cannot hide new files.
  Quoted read commands keep working with Windows short names and verbatim prefixes. Guarded Claude commands add
  OS enforcement: Bash and PowerShell always ask the host before executing, even with a native remembered grant.
  The original input stays on the permission card and remains the key for Faber's remembered grants. Every allowed
  response replaces it with the installed native sandbox invocation, using a fresh permission profile that denies
  reads and writes to every Store root, including aliases and paths computed inside a script. Plan uses read-only
  project access; the other guarded modes permit project writes. Network remains enabled. PowerShell scripts use
  UTF-16LE encoded input so native argument forwarding preserves quotes. Windows requires MXC; a WSL bash launcher,
  missing or prefixed runner, missing shell or unresolved root is refused. A failed native start never retries the
  original command. Monitor is denied because it has no checked replacement protocol. Isolated keeps its existing
  command policy. Full access is unchanged; its five warnings name conversations and settings.
  This is the owner-authorized fail-closed fallback (R554), with limits: native Auto classification and automatic
  plain-read approvals now reach Faber's cards unless a host grant matches; approval choices and mode flags remain.
  Literal native project entrypoints (Node, npm, Cargo and the existing build tools) run directly inside the sandbox. Shell syntax requires a supported native shell; a shell that cannot start is refused. Read-only sandbox rules cover CLI configuration, Git metadata and check settings. Native missing-path rules do not prevent every directory rename, so guarded launches additionally disable hooks even after opt-in, load only user settings and use strict Faber MCP configuration; project/local settings and servers cannot plant a host startup command. The existing hook hint and help disclose this in all five languages. A working folder inside a Store is refused. Commands start in the project folder, so a change of directory does not persist across calls, and background
  execution stays off. An installed native Codex sandbox is required; shell syntax additionally needs a native shell. None is downloaded or bundled.
  Before every guarded approval, including a remembered one, a metadata-only scan of every Store refuses symbolic
  links, multiply linked files, unknown or unreadable entries, and more than 10,000 entries. No file contents are read.
  This conservative fallback also refuses commands when catalog snapshots legitimately contain hardlinks; it does
  not silently exempt those files. A Store nested in the workspace denies its entire first workspace child, so
  renaming an allowed ancestor cannot reopen it; other files under that child become unavailable too. A separate
  unsandboxed process mutating links after preflight is outside this boundary. Full access is unchanged.
  Fake-process checks prove replacement and refusal, not OS enforcement. Native qualification must separately show
  a successful project command and denied computed-path reads/writes of both approval records through real aliases.
  No control, layout or motion changes.

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
  Its existing help adds that visual pages go online only with Live. A page takes the choice when opened or switched
  to another version; the reply itself always keeps the screen's closed resource policy.
  Cached is offered only while Codex is connected. Codex reasoning summary uses `model_reasoning_summary`; reply style
  also sets `model_verbosity` to low for Short and high for Explained, otherwise leaving the model's own length.
- R121 (it replaces the next item for the two choices on screen): "Don't ask" is full access. No approval cards, commands
  run without Faber's sandbox and with network, WebFetch and WebSearch work, any folder can be the working folder, and the
  person's own MCP servers, plugins and hooks stay on, as in the CLI itself. Claude Code runs in bypass mode. Since R564 its
  file tools reach every folder, the person's key folders (`.ssh`, cloud and GitHub sign-ins), the tools' own settings
  folders and `.git` included, as commands and Codex already did. The deny rules it keeps (`claude::full_access_deny`) close
  only the AI tools' own sign-in files (`.credentials.json`, `.claude.json`, Codex `auth.json`, Gemini `oauth_creds.json`,
  also where a documented variable moved their folder), the release signing key and changes to Faber's approval records. A
  token a file tool read would be saved in the conversation and sent to the model's maker, which is why those stay closed;
  the rules bind the file tools, not arbitrary commands. Codex runs with `danger-full-access` and `never` approvals and blocks
  nothing (see the R121 section of [the R97 security review](../security-r97-review.md)). Ask names its modes (Claude
  `default`, Codex `untrusted`), keeps the folder-wide credential denies and refuses a forwarded request that names a
  credential folder; it asks except for what the person pre-allowed in the tool's own settings. The system and credential folders are refused as working folders, judged
  by their resolved path (R124, R124b), and Faber's own requests never run the person's hooks. A CLI and everything it started
  end together (a Windows job object made before the CLI runs, failing closed; R124b, R124c). Known residual: the CLI is
  created suspended and joins its job a moment later (about 100 ms, measured on Windows 11), so if Faber ends in that
  moment the CLI stays suspended, outside any job, until the PC restarts. It never runs a single instruction. Std Rust
  cannot create a process inside a job yet (`raw_attribute` is unstable), so Faber starts as few CLIs as it can: Codex is
  asked for its models only while Faber has none, one asking at a time. Settings shows the
  details; the thread shows nothing about it (R168). The isolated mode below is `Permissions::Isolated`: Faber's
  own model list, router, internal checks and self-development roles use it, and the person cannot pick it.
- No prompt can block a full-access turn (R564). The host answers the CLI's permission requests in every mode (R160), so a
  question the agent asks the person is a card. Whatever else Claude Code still holds back in bypass mode (a protected path,
  a delete it finds risky, the person's own ask rules) is answered yes at once, on every CLI version: refusing it left the work
  undone behind a note with nothing to allow. Only a request that names one of the sign-in files or approval records above
  is refused, with no card (`permissions::touches_sign_in`: what a request writes is not judged, `auth.json` counts only
  beside its folder, and a command line is matched by its text, so best effort). A reply saved before R564 may still carry
  such a held-back request ("Needs you", a card with nothing to allow) as safe metadata only (tool, a kind, a file path or a
  page host, for a command one fixed word, the time; never a command or any input), until its "Got it". Where the CLI lists `--autocompact`, every launch but a side question compacts at its model's window (Haiku 150k,
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
