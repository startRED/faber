# Nightly CLI candidate checks

Phase 4's `.github/workflows/cli-qualification.yml` checks the newest official stable npm versions each day and on
manual dispatch. Windows x64, Linux x64 and macOS arm64 use fresh runners. Official native platform packages are
installed outside the checkout with lifecycle scripts disabled; no downloaded tool is bundled with Faber.

`scripts/cli-qualification.mjs` starts the native executable directly with no shell, in a disposable folder and an
allowlisted environment whose account/configuration folders are empty. Commands have a 30-second timeout and a
1 MiB output limit. It checks exact version identity and Faber's required launch options. Codex additionally generates
its version-specific JSON schema locally; required request methods must be actual schema branches, not prose.
It also sends initialize to the real Codex app server, requires the matching successful response, sends initialized
and closes stdin, requiring a clean parent exit. Combined startup output is capped at 1 MiB; the 30-second deadline
ends the probe even if inherited pipes remain open. Neither parent exit nor killing it proves process-tree cleanup.
No model turn, login, account request, personal settings or production signing is used.

The JSON receipt records the binary SHA-256, exact candidate version, platform, architecture, time and each check's
pass/failure category. Raw output, configuration, generated schemas and binaries are never uploaded. Reports remain
for 14 days. A command failure or missing interface fails the job and preserves its report when it can be written.
A failed download has no binary receipt and fails the artifact step too, rather than suggesting a checked candidate.

Every receipt says `qualified: false`, even when all contract checks pass. A passing Codex startup removes only its
protocol-startup gap. Claude startup, authenticated turns, cancellation and process-tree cleanup still lack evidence.
These checks detect interface drift; they cannot
establish complete compatibility, authorize an installation, update a manifest, sign metadata or promote a release.
Real-binary authenticated qualification and the owner's production signing/publication setup remain unfinished.

The workflow adds no process, polling, memory or processor work to the app. Its runners consume resources only during
the scheduled check. Local test fixtures validate missing options/methods, version mismatch, environment filtering
and the unqualified status; they are never presented as real-binary compatibility evidence.

Sources: [Claude CLI reference](https://code.claude.com/docs/en/cli-reference),
[Codex version-specific schema generation](https://developers.openai.com/codex/app-server), and the official npm
metadata for [Claude Code](https://registry.npmjs.org/@anthropic-ai/claude-code/latest) and
[Codex](https://registry.npmjs.org/@openai/codex/latest). Candidate versions are resolved afresh for each run.
