# R97 security review verification

The review of `61a91ab` was checked against this worktree, Claude Code 2.1.296 (`--help`), Codex 0.162.1 (`--help` and its
locally generated app-server schema), and primary documentation. No model session or credential file was opened.

New regression checks failed before their fixes: external-read confinement and key denies, offline Codex start/resume,
missing write/command gates, overbroad command/environment patterns, missing remote child restrictions, tightening an
active turn, and the Projects label/disclosure. Tests use fake CLI pipes and temporary files, never real CLIs or git.

| Finding | Verified result and change/proof |
| --- | --- |
| C1 | Real: Codex explicitly enabled sandbox network; Claude lacked an external-read boundary and several key denies. Network is now explicitly false on start/resume; Claude sets `blockReadsOutsideWorkingDirectories`, denies the named credential/key files, and gates file tools and shell calls. Argument and serialized-protocol tests cover both. The signing key itself was neither read nor moved. |
| H1 | The scope claim is false: the original request explicitly kept Chat restricted and R97 applied to Projects. The ambiguous label was real; all five languages now say Permissions for Projects. Chat launch equality remains tested. |
| H2 | Real: bypass admitted external file writes. Host decisions resolve the project and existing ancestors, including Windows junctions, and deny external or `.git` targets. New paths, junction escapes and protocol decisions are tested. Unsupported unsandboxed programs are declined. |
| H3 | Real: `.git/config` and hook writes were unprotected. Native Edit denies and the host policy now block them, including recursive deletion. Child Git disables fsmonitor/hooks. The suggested change to Faber's own git runner is inapplicable: this worktree has no such runner yet. The Codex sandbox's actual Windows enforcement was not executed against a real CLI. |
| H4 | Real: remote actions and `gh` were not covered. Child environments forbid Git transports and remove GitHub token access; Claude additionally rejects remote CLIs and all git mutations. Codex shell network is off. `pushInsteadOf` alone is insufficient because explicit `pushurl` overrides it. Generated environments and serialized requests are tested. |
| M1 | Real: `rm` was denied everywhere. Literal internal deletion is now approved after canonical operand and descendant checks; external paths, `.git`, junction escapes, expansions and compound commands fail closed. Recursive inspection is capped at 10,000 paths. |
| M2 | Real: command-text wildcards matched harmless argument text. They are removed; argv selects the subcommand. `git log --grep push` remains allowed. Commit/stash mutations are deliberately refused by the requested `.git` write boundary, regardless of message text. |
| M3 | Real: the named destructive variants were missing. The policy permits only read-only git subcommands and known flags, so restore, checkout, stash deletion, forced switches/branches, reset, update-ref, reflog expiry and worktree removal are all denied. Table tests cover each review example, quoted/global-option evasions and abbreviated output/textconv options. |
| M4 | Real residual user-interaction requests are declined. This behavior is retained at the owner's instruction; no Chat or clarification-flow redesign is included. A synthetic AskUserQuestion fixture now tests this actual residual case. |
| M5 | Real: an active turn retained bypass after tightening. DontAsk to Ask now interrupts and closes active project sessions, settles their replies as stopped, and resumes future work under Ask. The active Chat session is untouched in the regression test. |
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

Native Windows Claude Code has no Bash sandbox. CLI authentication, user-installed startup helpers, plugins and trusted
hooks are outside this tool boundary; a malicious CLI/plugin or a concurrent filesystem replacement is not isolated by
canonicalization or inherited environment settings. Do not treat this feature as a sandbox for hostile projects. Full
OS enforcement, native sandbox runtime behavior, and live permission-rule matching were not tested: tests may not call
real CLIs. Capabilities were checked with help, schema and tagged source. An attempted `codex sandbox windows --help`
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
