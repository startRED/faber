# Anonymous sharing

R583 permits optional anonymous usage and crash sharing. Sharing is off until a person explicitly says yes. The local
crash file remains on the computer and is not sent: Faber makes a separate report containing only the fields below.

A build without `FABER_TELEMETRY_URL` has no sharing feature. Where it is available, first start asks once with equal
choices, including after an update; Privacy in Settings changes the answer, shows the exact next report, and deletes the
waiting reports when switched off. A refusal is never sent. If saving the first choice fails, Faber shows an error and
continues; the next start asks again.

## Report fields

The following fields match the type in `engine/src/sharing.rs`; its documentation check compares the field names against
this page. A report is a JSON object with these fields:

- `version`: Faber version; `commit`: its build commit; `channel`: stable, preview, or other.
- `os`: operating system family; `osMajor`: major version or null; `architecture`: CPU family; `language`: interface language.
- `daysSinceLastReport` and `daysSinceLastUse`: days or null. These are numbers, not identifiers.
- `counters`: `appStarts`; `chatConversations` and `projectConversations` by CLI; `turns` by CLI and outcome
  (`completed`, `interrupted`, `failed`); `failures`, keyed only by a known `error.*` or `reason.*` key (at most 128 keys); and
  `updateInstalled` and `updateFailed`.
- `cliVersions`: installed version of each CLI, or `other` when not recognized.
- `crashes`: at most 16 entries, each with `kind`, `version`, `commit`, `sourceFile` and `line`. The source is restricted
  to a compiled list of Faber source file names; an unrecognized one is `other`. The message is never included.

The CLI fields are `claude`, `codex` and `gemini` (the last is the stored name for Antigravity). The engine does not
currently receive these CLI versions during normal work, so each is `other`; sharing never starts a process to find them.
Version strings have three numeric components of at most four digits each, otherwise `other` (Faber's numeric
`-preview.N` suffix is also recognized, with at most four digits); commits have at most 12 hexadecimal characters or are
`unknown`. Error keys must appear in the fixed list derived from the engine and the
English interface dictionary. The lists are refreshed with `node scripts/sharing-lists.mjs` and checked by engine tests.
The type contains no free-text field and has no place for an installation or machine identifier. Channel follows the
version's preview suffix, or a build may identify itself with `FABER_RELEASE_CHANNEL`; that value does not enable sharing.

Never send prompts, answers, user files or paths, project or folder names, host names, URLs, user names, email addresses,
account or plan details, catalog names, model output, panic messages, or IP-derived data. The receiver must not retain
IP addresses, including in access logs.

## Receiver contract

The build supplies one fixed HTTPS address as `FABER_TELEMETRY_URL`, the same for everyone using that build. The app POSTs a UTF-8 JSON report of at most 32 KB as
`application/json`; a 2xx status means it was accepted. It sends after consent or on use at most once per day, with one
additional opportunity after a crash. A failed report is retried at most once later, without a loop. Requests run away
from the screen and turns, and never from a crashing process. Redirects and proxies are disabled. The receiver does not
exist yet, and this contract has not been tested against one. A request already on its way when sharing is switched off
may still arrive at the receiver.

## Cost

Sharing keeps at most two logical reports, brief copies while writing, and up to 256 recent turn IDs in memory to avoid
double-counting; those IDs never enter a report. Its JSON file is capped at 128 KB and at most 32 recording tasks wait at
once. Sharing starts no processes. Its background writes and HTTPS request use memory and processor time; their peak cost
has not been measured. No sharing process or timer stays alive between events.
