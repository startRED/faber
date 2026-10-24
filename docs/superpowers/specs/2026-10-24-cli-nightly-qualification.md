# Nightly CLI contract qualification

Status: In progress. This slice detects candidate interface drift; it does not qualify installation.

Extend phase 4's nightly version tests with a scheduled, read-only workflow. Install official npm packages only in
disposable runners, with lifecycle scripts disabled. Run native platform binaries directly, without a shell, in an
empty project with an allowlisted environment and fresh home/config folders. Never use sign-in, model requests,
personal configuration, production signing, publication, automatic updates or promotion.

Check exact version output, the launch options Faber needs, and Codex's locally generated ClientRequest schema for
initialize, thread/start, thread/resume, turn/start and turn/interrupt. Bound each command's time and output.
Save a small JSON report binding package/binary identity, platform, architecture, checks and failure categories.
Do not upload binaries, configuration, raw subprocess output or generated schemas.

Every report remains qualified=false. Help and schema checks cannot prove a recorded model turn, cancellation or
process-tree cleanup. Those missing real-binary proofs and the production publication setup remain separate work.
A failed probe exits nonzero while preserving its report; a passing probe never changes the app's supported versions.

Acceptance: missing launch options or request methods fail; method names in descriptions cannot pass; malformed
versions and failed commands fail; environment filtering drops credentials; reports never claim full qualification.
The workflow runs on Windows x64, Linux x64 and macOS arm64 and keeps reports only for a bounded retention period.
