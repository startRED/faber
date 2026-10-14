---
name: second-opinion
description: Use when the work needs an independent read from the other vendor's CLI (Codex): reviewing a diff, a plan or a security-sensitive change before it is trusted. Runs Codex read-only, then checks its findings instead of repeating them.
disable-model-invocation: true
---

# Second opinion from Codex

One model's confidence is not proof. For security-sensitive changes two independent reviews are the minimum; this is the
second one. It reviews. It does not edit, commit or push.

## Before running

1. `command -v codex`. If missing or not signed in, say so and stop; never pretend a review ran.
2. `git status --short` and `git diff --stat`. Decide exactly what is under review (a diff, a file list or a plan file) and
   write it in the prompt as paths, not pasted secrets or unrelated private text.
3. Never read or pass CLI credentials or session tokens (`~/.codex`, `~/.claude`, Gemini OAuth). The prompt carries the
   task and paths only.

## Run

The prompt goes through stdin from a quoted heredoc, so nothing in it is interpreted by the shell. Read-only is the
default; there is no flag here that skips approvals or the sandbox.

```sh
codex exec --sandbox read-only --cd '<absolute project path>' - <<'TASK'
Review <what>. Do not edit files. Follow AGENTS.md. For each finding give file:line, the evidence, what breaks and for
whom, and how sure you are. Say "no findings" if there are none; do not pad.
TASK
```

Ask for what matters in this project: correctness, the rules in `AGENTS.md` (no shell for child processes, no
credentials, text only in `ui/locales/`, memory budget), and for a security change the attack path, not a style list.

## After

- Open the code at every cited line and decide: confirmed, wrong or unclear. Report only the first kind as findings, the
  rest as "checked and dismissed" in one line each.
- Findings that disagree with the first review are the valuable ones: say who said what.
- Fixes are a separate step, done by one agent at a time. Two agents never edit the same files together; parallel work
  gets separate worktrees and a named owner per file.
- Do not commit, push or deploy because of the review alone.

Report what was reviewed, the confirmed findings, what was dismissed, and anything that could not be checked.
