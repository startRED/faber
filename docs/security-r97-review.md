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

## Boundary and limits

This is a host tool policy, not OS isolation. File decisions use canonical paths and refuse protected targets. Shell
approval deliberately admits only literal local deletion and read-only git inspection; interpreters, build scripts,
wrappers, git mutations, arbitrary programs and remote CLIs require Ask mode. Recursive search/deletion also checks
descendants and refuses large or unresolvable trees. Git diff/show need `--no-ext-diff --no-textconv`.

Codex's workspace-write sandbox still permits reads outside the project. Disabling sandbox network prevents shell
network access; it does not prevent the CLI from sending tool output to its model provider. No new read sandbox or
provider isolation is claimed. The host file policy above applies to Claude tool requests.

Native Windows Claude Code has no Bash sandbox. CLI authentication, user-installed startup helpers, plugins and trusted
hooks are outside this tool boundary; a malicious CLI/plugin or a concurrent filesystem replacement is not isolated by
canonicalization or inherited environment settings. Do not treat this feature as a sandbox for hostile projects. Full
OS enforcement, native sandbox runtime behavior, and live permission-rule matching were not tested: no model session
was authorized. The Windows junction test uses a Windows-only **dev dependency** because stable Rust cannot create
junctions and this PC lacks symlink privilege; no test shells out and the dependency does not ship in Faber.

Primary references: [Claude permission rules](https://code.claude.com/docs/en/permissions),
[external read setting](https://code.claude.com/docs/en/settings-reference#permissions-blockreadsoutsideworkingdirectories),
[native Windows sandbox limitation](https://code.claude.com/docs/en/sandboxing),
[Git protocol environment](https://git-scm.com/docs/git#Documentation/git.txt-GITALLOWPROTOCOL),
[pushInsteadOf caveat](https://git-scm.com/docs/git-config#Documentation/git-config.txt-urlltbasegtpushInsteadOf),
[Codex subprocess environment](https://learn.chatgpt.com/docs/config-file/config-reference).
