# R97 security review verification

> R121 (2026-10-14) made the user-facing "Don't ask" full access. Everything below describes the isolated mode that only Faber
> uses now; the last section, "R121", says what changed, the new risk and what each CLI can still block.

The review of `61a91ab` was checked against this worktree, Claude Code 2.1.296 (`--help`), Codex 0.162.1 (`--help` and its
locally generated app-server schema), and primary documentation. No model session or credential file was opened.

New regression checks failed before their fixes: external-read confinement and key denies, offline Codex start/resume,
missing write/command gates, overbroad command/environment patterns, missing remote child restrictions, tightening an
active turn, and the Projects label/disclosure. Tests use fake CLI pipes and temporary files, never real CLIs or git.

| Finding | Verified result and change/proof |
| --- | --- |
| C1 | Real: Codex explicitly enabled sandbox network; Claude lacked an external-read boundary and several key denies. Network is now explicitly false on start/resume; Claude sets `blockReadsOutsideWorkingDirectories`, denies the named credential/key files, and gates file tools and shell calls. Argument and serialized-protocol tests cover both. The signing key itself was neither read nor moved. |
| H1 | The scope claim is false: the original request explicitly kept Chat restricted and R97 applied to Projects. The ambiguous label was real and was corrected. R107 now explicitly extends the default to Chat; both areas are named in Settings and share the protection tests. |
| H2 | Real: bypass admitted external file writes. Host decisions resolve the project and existing ancestors, including Windows junctions, and deny external or `.git` targets. New paths, junction escapes and protocol decisions are tested. Unsupported unsandboxed programs are declined. |
| H3 | Real: `.git/config` and hook writes were unprotected. Native Edit denies and the host policy now block them, including recursive deletion. Child Git disables fsmonitor/hooks. The suggested change to Faber's own git runner is inapplicable: this worktree has no such runner yet. The Codex sandbox's actual Windows enforcement was not executed against a real CLI. |
| H4 | Real: remote actions and `gh` were not covered. Child environments forbid Git transports and remove GitHub token access; Claude additionally rejects remote CLIs and all git mutations. Codex shell network is off. `pushInsteadOf` alone is insufficient because explicit `pushurl` overrides it. Generated environments and serialized requests are tested. |
| M1 | Real: `rm` was denied everywhere. Literal internal deletion is now approved after canonical operand and descendant checks; external paths, `.git`, junction escapes, expansions and compound commands fail closed. Recursive inspection is capped at 10,000 paths. |
| M2 | Real: command-text wildcards matched harmless argument text. They are removed; argv selects the subcommand. `git log --grep push` remains allowed. Commit/stash mutations are deliberately refused by the requested `.git` write boundary, regardless of message text. |
| M3 | Real: the named destructive variants were missing. The policy permits only read-only git subcommands and known flags, so restore, checkout, stash deletion, forced switches/branches, reset, update-ref, reflog expiry and worktree removal are all denied. Table tests cover each review example, quoted/global-option evasions and abbreviated output/textconv options. |
| M4 | Real residual user-interaction requests are declined. This behavior is retained at the owner's instruction; R107 also applies the automatic decline to Chat; conversational text remains available for clarification. A synthetic AskUserQuestion fixture now tests this actual residual case. |
| M5 | Real: an active turn retained bypass after tightening. DontAsk to Ask now interrupts and closes active Chat and Project sessions, settles their replies as stopped, and resumes future work under Ask. The R107 regression checks that both active areas stop; idle sessions restart with the saved mode. |
| M6 | False as a defect: failing outside the sandbox and adapting without approval is the specified behavior. It is not silent: command failures become ToolResult errors, which the regression test verifies. No writable user-cache roots were added. |
| L1 | Real: native `.env.*` Read denies also blocked templates and Write. The host now protects secret reads while allowing the four named templates and internal `.env` creation. Tests cover both. |
| L2 | Partly real coverage gap: replay keys intentionally ignore variable params; separate existing parameter tests already covered modes. Tests now inspect actual serialized start/resume requests and replay a synthetic project policy/AskUserQuestion fixture. The fixture is a hand-authored protocol scenario, not a claimed live recording. |
| L3 | The migration-default objection is false: the owner explicitly chose Don't ask for existing users too. Missing disclosure was real; the project screen now displays the current mode before a turn, without consent or changing that default. |

## Second review of `59703b0` and `5f3ecb2`

All five findings are real. New checks failed before the fixes, using synthetic temporary files, fake CLI stdio and
pure Windows prefix checks. The prefix test never resolves a network path or contacts a host. Installed versions remain
Claude Code 2.1.296 and Codex 0.162.1. Capability evidence also includes the latter's tagged configuration schema,
app-server thread processor and standalone sandbox implementation. No model session or credential file was opened.

| Finding | Verified result and change/proof |
| --- | --- |
| H1 | Real: Git implicitly enters no-index mode for an outside positional path. All positionals now pass the canonical read policy, including after `-C`; option values such as `--grep push` remain arguments. Regression cases cover external diff inputs, secret names, `--` paths and a junction under a selected subdirectory. |
| H2 | Real: network-off workspace-write still allowed disk-wide reads and provider-bound tool output. Projects now select a fresh workspace-only named profile (`:root = deny`, `:minimal = read`, workspace writes, read-only git metadata, explicit credential denies, network off). Legacy `sandbox` is omitted so it cannot override the profile. Serialized start/resume checks inspect this profile. The hint now discloses provider transmission and the separate integration boundary. No signing key was read, moved, encrypted or replaced: that file is outside this worktree and credential access is forbidden. |
| H3 | Real: the former command allowlist blocked builds and tests. Common build/test fronts, project entrypoints, local scripts and `rg` now receive a rewritten command through the installed **native** Codex sandbox, with the same restricted profile. npm shims resolve to native executable/script argv. PowerShell uses its call operator; every argument is quoted. Missing helpers, unsupported syntax and unsupported platforms fail closed. Unit and fake-stdio checks assert the rewritten command, zero permission cards and no unsandboxed fallback. WebSearch and TodoWrite are available; arbitrary MCP, Skill and Agent execution remain gated because they can act outside the command sandbox. Recursive native Grep and deletion keep their existing inspection limit; sandboxed `rg` and project tools provide the build/search route. |
| M1 | Real: forward and mixed slash UNC spellings reached filesystem lookup. Windows prefixes are checked before lookup: only an absolute disk path or a relative path is admitted; UNC, device, verbatim and drive-relative forms are refused. Link targets are also checked before canonicalization, with a bounded walk; relative parent-traversal links fail closed. The pure regression table includes each spelling and a mocked network junction, without probing the host. |
| M2 | Real: project environment could supply Git's higher-priority parameters channel. Both child environment policies clear `GIT_CONFIG_PARAMETERS`. Don't ask also loads only user settings, so project env/hooks cannot poison the unsandboxed launcher with PATH, BASH_ENV or loader variables. Ask keeps its prior sources. Every project shell approval, including read-only git and deletion, is sandbox-wrapped; native helpers are required, while JavaScript helpers are declined. Both missing protections reproduced with red tests. |

The earlier false verdicts stand: first-review H1 contradicted the explicit Projects scope; M6 is the specified
fail-and-adapt behavior, not a silent failure; L3's migration objection contradicted the owner's existing-user default.
The first review's C1/H3/H4 closures were too broad: network-off and inherited environment alone did not constrain reads
or project code. The command profile and positional validation address those gaps; they do not isolate trusted CLI
startup customizations or non-command integrations.

## R107: the same default in Chat (2026-10-13)

R107 supersedes the original Projects-only scope. Both drivers now select the same shared policy for Don't ask in
both areas; there is no second Chat policy. Claude uses bypass with the existing native denies, external-read setting,
catch-all host gate and sandbox command wrapper. Codex selects the same workspace-only read/write profile, `never`
approvals, network off and Windows MXC. Residual requests are answered automatically, without permission cards.
Ask retains its previous normal-session behavior; card/queue screen scenarios explicitly save Ask before starting.
Changing from Don't ask to Ask stops active turns in both areas, and idle sessions restart with the new choice.
Chat's unattended launch also disables Codex startup hooks regardless of the Projects hooks switch; a fake-launch
regression first reproduced that switch leaking into Chat. Mode changes retire both cached hook configurations.

Installed Claude Code 2.1.296 help and the [official permission-mode documentation](https://code.claude.com/docs/en/permission-modes#bypass-permissions)
confirm that `--restricted` refuses bypass. Chat therefore uses `--safe-mode`, a fixed tool list and strict MCP
configuration in Don't ask, plus the shared user-only setting source and forced hook disable. Safe mode disables
customizations while preserving normal authentication, built-in tools and permissions, as described in the
[CLI reference](https://code.claude.com/docs/en/cli-reference). Project/local environment files cannot poison the
launcher. Ask Chat retains restricted mode. No real model session or credential read was used to check these flags.

The person can choose Chat's working folder in General Settings **before the first message**. Faber stores its
canonical local path on the conversation and preserves it across restart and branching. Once messages exist the
choice is fixed, so historical attachment/output paths cannot silently refer to another folder. With no choice,
Chat uses Faber-owned `data_dir/chat/<conversation-id>` scratch. Its temporary `.faber/tmp` is removed after a turn;
visible files remain with their conversation. Discarding a draft or deleting its conversation removes owned scratch,
never a chosen folder. Startup removes orphan UUID scratch folders with no record; even an unreadable existing record
keeps its files. This implements the scratch lifecycle portion of R106; the broader build-slot/disk-budget work
remains outside this permission change. Non-English R107 locale wording is handwritten and marked for native review.

Red tests reproduced missing Chat launch confinement, network restrictions, residual declines, sandboxed build
routing, active-turn tightening, persisted folder selection, orphan cleanup and the Settings scope. They also exposed
host helpers outside the CLI boundary: file discovery/export admitted protected files or external links, and private
branch copying included synthetic credentials. Those helpers now reuse the shared read policy, skip links and exclude
internal `.git`/`.faber` artifacts. An attachment-junction regression also reproduced host writes outside the working
folder. Upload directories and each filename now pass the shared write policy; failed or partial saves stop the send
with an error instead of claiming nonexistent attachments. Tests use fake stdio, synthetic files and Windows junctions;
no actual secret is read.
Shared profile/argv and serialized start/resume tests exercise both areas, while Ask and side-question tests remain.

The merged internal Codex question path also retained legacy `read-only`, which permits reads outside its working
folder even with approvals disabled. A new parameter regression failed before this fix. Internal questions now use
the same profile factory with their existing helper folder read-only: root deny, minimal runtime reads, credential
denies, network off, never approvals and ephemeral sessions. There are no workspace write grants or legacy sandbox
parameters that could override the restricted reads. Normal Ask turns retain their previous parameters.

Windows CI exposed two Git-policy regressions when a trusted temporary root uses an 8.3 spelling with `~`.
A literal-tilde working-root test reproduced the same denial locally. Git now validates relative operands against
the canonical root rather than treating its trusted prefix as shell syntax. User-supplied expansion/traversal and
network paths remain rejected, with the existing external-read and `-C` directory tests retained.

Windows CI also exposed attachment writes under a trusted 8.3 temporary root being rejected as shell
expansion. A literal-tilde root test reproduced the failure before the fix. Host attachment paths now
use relative operands under their trusted root; outside directories, traversal, junctions and `.git`
remain refused by the same file policy. Duplicate-name handling is covered by the regression too.

The subsequent base refresh adds queued/steered Chat messages and the UX sentinel. Queued uploads
use the same working root and host attachment checks as ordinary messages; a refused credential
upload never enters the queue. The permission-card sentinel reproduced a missing-card timeout under
the new default and now explicitly selects Ask before commands before measuring its animation.

## Boundary and limits

This is a host tool policy, not OS isolation. File decisions use canonical paths and refuse protected targets. Shell
approval validates literal local deletion and read-only git inspection, then wraps them in the OS sandbox too.
Recognized project build/test commands and local entrypoints are similarly wrapped; no bare executable is approved
outside it. Explicit background requests become foreground commands. Git mutations, inline interpreters,
compound shell syntax, remote CLIs and unknown tool capabilities are declined. Recursive native search/deletion checks
descendants and refuses large or unresolvable trees; `rg` can search through the sandbox. Git diff/show require
`--no-ext-diff --no-textconv` and validated positionals. Project temporary files live under `.faber/tmp`.

The new command profile denies outside reads except the CLI's minimal runtime paths. Native Windows selects MXC
explicitly and refuses an unavailable or policy-blocked backend rather than falling back to unelevated mode. Claude
builds require an installed native Codex CLI with sandbox support; no Codex sign-in or model call is used by the runner,
and Faber neither bundles nor silently installs it. Other platforms use that CLI's native sandbox. Missing dependencies,
external writable caches, detached-server assumptions and unsupported sandbox policies can still make commands fail.
Profiles retain vendor safeguards and bound glob expansion to three levels, as the configuration reference requires.
Config/argv checks do not prove every platform's runtime enforcement or future-file glob behavior.

Native Windows Claude Code has no Bash sandbox. CLI authentication and user-installed startup helpers are outside
this tool boundary; unattended plugins/hooks are disabled. A malicious CLI or a concurrent filesystem replacement is not isolated by
canonicalization or inherited environment settings. Do not treat this feature as a sandbox for hostile projects. Full
OS enforcement was initially checked only through help, schema and tagged source. The live probes below supersede
that limit for this Windows host; automated tests still never call real CLIs. An attempted `codex sandbox windows --help`
was interpreted as a command because there is no `windows` subcommand, and failed to start that program; it was not a
successful sandbox probe or a model session. The Windows junction test uses a Windows-only **dev dependency** because stable Rust cannot create
junctions and this PC lacks symlink privilege; no test shells out and the dependency does not ship in Faber.

Primary references: [Claude permission rules](https://code.claude.com/docs/en/permissions),
[external read setting](https://code.claude.com/docs/en/settings-reference#permissions-blockreadsoutsideworkingdirectories),
[native Windows sandbox limitation](https://code.claude.com/docs/en/sandboxing),
[Git protocol environment](https://git-scm.com/docs/git#Documentation/git.txt-GITALLOWPROTOCOL),
[pushInsteadOf caveat](https://git-scm.com/docs/git-config#Documentation/git-config.txt-urlltbasegtpushInsteadOf),
[Codex subprocess environment](https://learn.chatgpt.com/docs/config-file/config-reference),
[workspace-only profiles and glob expansion](https://learn.chatgpt.com/docs/permissions),
[Windows MXC compatibility and fail-closed selection](https://learn.chatgpt.com/docs/windows/windows-sandbox),
[implicit no-index diff](https://git-scm.com/docs/git-diff),
[Git configuration precedence](https://git-scm.com/docs/git-config#Documentation/git-config.txt-GITCONFIGCOUNT),
[installed-version config loading](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/core/src/config/mod.rs),
[installed-version thread start/resume](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/app-server/src/request_processors/thread_processor.rs),
[installed-version sandbox command](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/cli/src/debug_sandbox.rs).

## Independent Chat reviews: verified findings and live probes

This section supersedes the earlier integration and runtime limitations where noted. Both independent reviews
targeted `4b6339f`. Each newly fixed gap first failed a regression assertion or the authorized live probe.

| Review finding | Verdict and change | Regression evidence |
|---|---|---|
| Opus H1: missing mutation coverage | Real. Both areas assert the catch-all ask rule, unknown/external-tool rejection and home/workspace credential denies. Nine source mutations each failed an assertion. | `review3_catch_all_routes_untrusted_tools_to_the_host_in_both_areas`; `review3_codex_credential_denies_survive_in_both_areas_and_on_resume` |
| Opus H2: unattended integrations can push | Real. Apps/plugins/browser/computer use and automatic MCP dependency installation are disabled. An empty `mcp_servers` table does not clear inherited tables: before start/resume, the CLI's config API supplies names and Faber overrides each server with `enabled=false`. Values are skipped with `serde::de::IgnoredAny`, never forwarded or recorded. Both CLIs also disable hooks/startup integrations in Don't ask. Ask retains its setup. | `review3_user_mcp_servers_are_disabled_before_start_and_resume_without_copying_values`; `review3_unattended_startup_cannot_run_integration_hooks_even_after_opt_in` |
| Opus M1: overly broad folders | Real. Don't ask rejects canonical home folders, drive roots and Windows/Program Files/AppData trees, including existing chosen folders on launch. Ask permits them. Only Faber-owned per-conversation scratch is exempt from the AppData restriction. Settings explains the restriction. | `review3_sensitive_chat_roots_are_refused_only_without_prompts`; `review3_sensitive_roots_include_canonical_home_drive_and_system_subtrees` |
| Opus M2 / Gemini H1: Windows backend | The launch ambiguity was real; the claim that thread config is ignored is false. Installed-version `ConfigManager::load_with_cli_overrides` appends request overrides after server overrides, so the thread's MXC selection wins. Official docs also say unelevated applies a restricted token and ACL boundaries; it is not unrestricted, although it lacks denied reads. Startup now pins MXC too, removing ambiguity. Final canary reads/writes failed with actual filesystem `EPERM` errors under MXC. | `review3_codex_unattended_projects_disable_integrations_and_pin_mxc`; `review3_live_captures_contain_actual_denied_file_operations_and_git_requests` |
| Gemini H2: git mutations | Real command-policy gap, not proof of a successful published-history rewrite. Read-only `.git` protects history but is not a git command allowlist: `clean` can delete worktree files. Don't ask now disables native Codex shell tools and exposes a host command tool using the same validator/sandbox argv as Claude. Direct push/reset/rebase/clean requests are declined. Existing sessions restart once with visible history to install that tool; gated sessions resume normally. | `review3_command_gate_declines_git_mutations_and_runs_builds_only_in_the_sandbox`; `review3_legacy_sessions_restart_with_history_and_gated_sessions_resume` |
| Gemini M1: project-local auth files | Real. `.credentials.json`, `credentials.json` and `auth.json` are now protected by the shared file policy and sandbox profile. | `review3_project_local_auth_files_are_unreadable_and_denied_in_the_profile` |
| Gemini M2: Claude command dependency / MCP unavailable | Real dependency, intentional safety boundary. Claude commands still require an available native Codex sandbox runner; there is no unsandboxed fallback. Settings now states the dependency and integration restrictions. No MCP tool is auto-approved. | existing `review2_h3` sandbox-wrapper tests; `review3: Settings discloses sandbox dependencies, integration limits and forbidden folders` |
| Gemini L1: incomplete disclosure | Real; the five locales now state the helper dependency, integration/hook restrictions and forbidden folders. Four handwritten locales await native review. | `review3: Settings discloses sandbox dependencies, integration limits and forbidden folders` |
| Gemini L2: fake tests do not prove runtime enforcement | Real validation limit; supplemented by owner-authorized subscription probes. Captured failures must be real filesystem denials, not startup failures. CI tests only replay/inspect trimmed captures. | `review3_live_captures_contain_actual_denied_file_operations_and_git_requests` |

Mutation list: remove `*`; approve WebFetch/Agent; remove profile `~/.claude`; remove profile `~/.codex`;
remove workspace `.claude`; remove workspace `.codex`; remove workspace `.gemini`; remove native
`Read(~/.claude/**)`; remove native `Read(~/.codex/**)`. Every mutation failed the focused `review3` assertions;
the original files were restored after each check. No mutation test starts a CLI.

The probes used Claude Code 2.1.296 and Codex 0.162.1, subscription authentication, fresh owned scratch folders,
two outside canaries, and a nested throwaway repository with two commits and a fake published upstream.
Each agent attempted read, write and `git -C repo reset --hard HEAD~1` once per probe scenario. Initial probes
found additional integration bugs: the required `--permission-profile` flag was missing; arbitrary read-access
globs such as `**/.git` are rejected by the installed CLI; Windows native argument forwarding broke quoted TOML
tables. Failing regressions and before-captures precede those fixes. Existing nested Git metadata now gets exact
read-only bindings; an incomplete/bounded metadata scan makes the workspace read-only rather than granting writes.
`review3_unrepresentable_git_bindings_make_the_workspace_read_only` first reproduced a skipped non-UTF-8 subtree;
unrepresentable bindings now also make the workspace read-only.
That filesystem regression runs on Windows/Linux; [APFS requires valid UTF-8 names](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html).
Legacy extra writable roots are explicitly cleared and runtime workspace roots are fixed to the chosen folder.

After correcting those failures, Python's Windows Store alias could not run in MXC. Node initially failed when
canonicalizing an ancestor drive, so child `NODE_OPTIONS` uses Node's supported symlink-preserving loader flags,
without granting ancestor reads. The final local Node script ran under both CLIs: opening the read canary and
opening the write canary each returned `EPERM`; reset was declined by Faber; neither canary nor the published
head changed. Final and before captures live in `tests/fixtures/security-probes/{claude,codex}{,-before}.json`, with paths
scrubbed and no account data. `engine/examples/security_probe.rs` is manual-only, never an automated test.
A broad `:workspace` MXC capability check also failed while enumerating this host's unrecognized `G:` volume;
the restricted final profile succeeded. No volume, credential file or account configuration was changed by Faber.

These probes verify this host and these operations, not every platform, runtime, future-file glob or hostile project.
Node loader flags can change symlink/module identity behavior; unsupported interpreters and external caches may fail.
Native sandbox support remains mandatory. The earlier malicious-CLI/auth-helper and filesystem-race limits still
apply; unattended plugins/hooks are now disabled. One owned probe directory remains under ignored `target` after
automatic approval review rejected its cleanup as `blocked by policy`.

CI follow-up: macOS found a Windows-only mutable binding in the manual probe helper; its mutation now lives
inside the Windows branch. Windows CI correctly refused four tests' chosen working folders under AppData temp.
A synthetic AppData/TMP tree reproduced `r107_chat_folder_choice_is_canonical_persisted_and_fixed_before_the_first_message`
failing before the fixture fix. Those four tests now choose owned folders under ignored `target/test-workspaces`;
the forbidden-folder policy is unchanged. The complete engine suite passes with the synthetic AppData/TMP environment.

Additional primary evidence: [installed config override order](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/app-server/src/config_manager.rs),
[installed table merging](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/config/src/merge.rs),
[installed filesystem grammar](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/core/src/config/permissions.rs),
[Node loader flags](https://nodejs.org/api/cli.html#--preserve-symlinks-main).

## R121: "Don't ask" is full access (2026-10-14)

Decision R121 replaces the user-facing sandbox of R97, R107 and R109. The owner chose it after the AI could not reach
GitHub, saying that a mode called "Don't ask" that silently denies things is wrong. Everything above in this file now
describes the **isolated mode** (`Permissions::Isolated`), which only Faber itself uses: the Codex model list and limits
reading, the router and pulse questions, internal checks and the self-development mission roles. The person can pick only
Ask before commands or Don't ask (full access). The stored setting still reads `dontAsk`, so nobody's choice changes name.

### What full access gives, in plain words

The AI works without asking. It runs commands outside any Faber sandbox, with the network and with the person's own
sign-ins in the environment (for example the GitHub CLI's), it can use WebFetch and WebSearch, it can work in any folder
(the home folder, a drive root, Windows or Program Files), and the person's own MCP servers, plugins and hooks load, as in
the CLI itself.

**The new risk.** A web page, a file, an email or a tool result that the AI reads can carry instructions that steer it.
With full access nothing stops it from acting on them: it can change or delete files, run programs, use the network and
use the person's own sign-ins for other services (GitHub, cloud tools), without asking first. The old mode contained
this by denying the network and by writing only inside the working folder. Full access does not. Use Ask before commands
for anything whose source you do not trust. The owner saw this risk and chose it.

### What each CLI still blocks, and what it cannot

| | Claude Code | Codex |
|---|---|---|
| Arguments | `--permission-mode bypassPermissions` and a settings JSON that carries only the deny rules (`claude_args`, engine/src/claude.rs). No `--safe-mode`, `--strict-mcp-config` or `--setting-sources`. Chat keeps its fixed tool list. | `codex app-server` with no flags (`codex_args`, engine/src/codex.rs). thread/start and thread/resume carry `sandbox: danger-full-access` and `approvalPolicy: never`, with no `config` and no dynamic tools (`codex_thread`). |
| Credential folders | **Enforced for Claude's file tools**: Read and Edit deny rules (Write follows Edit) for `~/.claude`, `~/.codex`, `~/.gemini` (the Antigravity CLI keeps its state in `~/.gemini/antigravity-cli`), `~/.ssh`, `~/.aws`, `~/.docker`, `~/.azure`, `~/.kube`, the GitHub CLI folders, `~/.claude.json*`, `~/.git-credentials`, `~/.npmrc` and updater keys. A folder moved with CLAUDE_CONFIG_DIR, CODEX_HOME, GEMINI_CLI_HOME, GH_CONFIG_DIR, DOCKER_CONFIG, AZURE_CONFIG_DIR or an AWS file variable is denied where it now is. | **Not enforced.** Codex can read them. |
| Commands (Bash, PowerShell) | **Partly.** Claude Code applies the same rules to the plain file commands it recognises (`cat`, `head`, `tail`, `sed`, `tee`, redirections), and not to a script or program that opens files itself. | **Not enforced.** |
| Requests the CLI still forwards | Refused by the engine in **both modes** with no card when the request names a credential folder or file in any spelling (case, slashes, `~`, `%USERPROFILE%`), or runs in or from one (Codex sends the command's folder; Claude's requests are judged from the working folder). Full access answers yes to the rest. **Best effort, not a guarantee** (see below). | The same, decided in the driver (a refusal is always safe) and again in the engine. |
| Network, integrations, hooks | On | On |

The deny rules travel in the settings JSON (`permissions.deny`), one string each, not after `--disallowedTools`. The installed
CLI (2.1.296) kept a rule with a space intact on the command line too, so the claim that it splits was not reproduced; the
JSON array still removes the doubt and a test checks every rule. Matching on Windows was checked against the installed CLI
with decoy folders: a backslash path, a lower-case spelling of a folder, a relative path from inside the folder and a `~`
rule were all refused, so one rule form per folder is enough. The docs say Windows paths are normalised to `/c/Users/...`
and do not mention case; the checks above are the evidence.

Why Codex cannot block them: in Codex 0.162.1 the only filesystem deny rules live in a permission profile. On Windows, a
profile that grants the whole disk and denies a few folders failed here in every backend that can be selected. The default
Windows sandbox (`mxc`) refuses it ("MXC cannot verify it does not alias a denied object") as soon as one drive letter is
unreachable; this PC has an unreachable `G:`, and any empty card reader or disconnected network drive would do the same.
The unelevated backend answers "Restricted read-only access requires the elevated Windows sandbox backend", and the
elevated one failed with "setup refresh had errors". Per-drive path keys such as `C:\` are rejected as "cannot be
represented losslessly". These probes ran `codex sandbox` against a decoy folder in a scratch directory, never against a
credential file. So full access sends no profile, and the Settings text says so.

**The command-line match is a net, not a wall.** A shell can build a name out of pieces the text never shows: quote splicing
(`cat .co""dex/auth.json`), a variable, a glob, a script that opens the file itself. No text match follows those, and the code
says so (`touches_credentials`). The protection that holds is Claude's deny rules for its file tools and the recognised file
commands. The screen claims nothing beyond that.

**Pre-approvals in the tools' own settings.** Ask asks before commands and file changes, except the ones the person already
allowed in the tool's own settings (Claude's `permissions.allow`, Codex's execution rules). Faber never reads or strips those
files, so such a command runs without a card. The Ask text in Settings says so. The credential deny rules still apply to
Claude in Ask (they travel in the settings JSON in both modes), and a forwarded request that names a credential is refused.

What this means for the person: in full access both tools can still reach the sign-in and key folders through commands, and
Codex can read them directly. Do not keep secrets in folders you give the AI in this mode, and use Ask before commands for
material you do not trust. The Settings text says exactly this.

### Changes that follow

- Working folders: the home folder and drive roots stay allowed in both modes. Refused with a calm message
  (`error.workingFolder`, `permissions::forbidden_folder`): the Windows folder, Program Files, a credential folder and any
  folder inside one (R124). The check runs when a project is added, when a Chat folder is chosen and again when a turn starts
  (for a Chat folder too, since R124b). R124b: the folder is resolved to its real path first (links, junctions, 8.3 names, the
  verbatim `\\?\` form and administrative-share aliases of this PC such as `\\localhost\C$`), and both the written and the resolved
  path are compared in one normal form (forward slashes, lower case). A folder that cannot be resolved for a reason other than
  being absent is refused. R124c: a network path to an administrative share (a share name ending in `$`, any host) is refused,
  since aliases such as the PC's LAN address cannot all be recognised; the check also runs on every turn, also when the process of
  the earlier turn is reused (the process is started again if the folder now resolves elsewhere), and the CLI starts in the
  resolved path that was checked (`permissions::checked_folder`; the `\?` prefix is dropped only if the shorter spelling resolves to the very same folder, since `safe.` would otherwise become `safe`; the default private Chat folder is checked like a chosen one, R124d). Between the check and the start the folder could still be swapped
  by someone with write access to its parents; that window is not closed. This protects the folder the person picks; it does not
  limit what a command run inside it can reach, and the Settings text says the blocking of those folders for commands is best effort.
- Hooks: the Settings switch applies only to Ask before commands. With full access the person's hooks always run.
- Faber's own requests (the router, any side question, the `/usage` reading) start Claude with `--safe-mode`, `--setting-sources ""`,
  `--strict-mcp-config`, no tools and `disableAllHooks`, whatever mode the person chose. Before this a user hook ran during
  background routing (checked against the installed CLI with a decoy project hook).
- Relocated sign-in variables accept an absolute local path or a network share (`\\host\share`, `\\?\UNC\host\share`); a
  relative value is still ignored (R124b). Claude's rule for a network path is written in two spellings because the CLI's own could
  not be checked here.
- Ask names its mode. Claude Code gets `--permission-mode default` in Chat and in Projects, so a `defaultMode` in the person's
  CLI settings can no longer loosen it, and Chat no longer auto-accepts edits. Codex gets `approvalPolicy: untrusted` with the
  workspace-write sandbox, which asks before every command that is not a plain read and before every file change inside the
  working folder (checked against the installed Codex; `on-request` let commands inside the folder run unasked). Both tools
  may skip a short list of plain reading commands, and the Ask text says so.
- Full access never shows a card. A request the CLI still forwards goes to the engine, which answers with the mode in force
  at that moment, under the lock that also saves the mode. A request that was on its way when the person switched to Ask finds
  the session gone and is never accepted. Fail-closed answers remain for the isolated mode and for side questions.
- The router and the model list never use the person's mode. The router's Codex thread keeps its read-only profile and its
  shell off in every mode, and it runs on a Codex process of its own started with the `Isolated` arguments (one extra idle
  process once Auto routes through Codex; its memory cost was not measured here).
- Switching from Don't ask to Ask stops running turns. R124b: on Windows every CLI the drivers start joins a job object with
  kill-on-close and no breakaway (`proc::spawn_command`, the safe `win32job` crate). Stopping ends
  the job first and closes the input after, so a CLI that exits when its input closes cannot orphan its workers, and a CLI that
  quits on its own leaves none either. If the system refuses the job, `taskkill /T` (run directly, never through a shell) is
  the fallback and a failure is logged. Round 3 (R124c) closed the startup race: the CLI is created suspended
  (`CREATE_SUSPENDED`, with `CREATE_NO_WINDOW`), put in the job, and only then resumed (`proc::spawn_command`), so nothing it
  starts can be outside the job. Resuming needs the thread handle, which std and tokio do not give, so the thread is found through a
  toolhelp snapshot in the one unsafe call of Faber, in its own crate (`winproc/`, outside the workspace's `forbid(unsafe_code)`).
  If the job cannot be made or joined, the process is killed before it ran a single instruction and the spawn fails with the
  usual start error: no CLI runs uncontained on Windows, so the `taskkill` fallback only serves test doubles and other systems.
  The engine asks the drivers to stop and does not wait for the end: the live entry is removed first, so any late event is
  dropped; with the job, the end is the kernel's work once the handle closes. Elsewhere only the CLI itself is ended; a process
  group would be the next step. Switching to Don't ask does not touch a running turn.
- A Codex session saved with the isolated host command tool (`commandGate`) restarts once, with its visible history,
  because that tool no longer exists in any mode the person can choose.
- Code that wants the old sandbox must say `Permissions::Isolated`. The variant `Permissions::DontAsk` no longer exists
  (it is `FullAccess`, stored as `dontAsk`), so a branch that used it for the sandbox, such as the self-development
  mission roles, fails to compile after the merge instead of silently getting full access.
