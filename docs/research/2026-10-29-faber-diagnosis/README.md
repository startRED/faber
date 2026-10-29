# Faber diagnosis against T3 Code, Claude and Grok (2026-10-29)

Dated research, history after its day. The owner asked for a deep look at T3 Code (pingdotgg/t3code, MIT, commit
4092c4ac) to take what helps Faber, and for a diagnosis of Faber itself: is it easy to understand, can the person follow
work with helpers, does the answer use technical words it should not, and why do sessions lag or go in circles without
saying so. He also pointed at the Claude desktop app and Grok's web chat as easier to read.

This builds on the same day's first T3 study, which covered output rendering, helper progress, providers and general
UX (`docs/research/2026-10-29-t3code-study.md` where it lands; its notes live in `E:\Dev\re-corpus\t3code\notes\`).
Nothing here repeats it. The full helper reports, with file and line references and the measuring scripts, stay
outside the repository because they quote the owner's private conversations: `E:\Dev\re-corpus\faber-diagnosis\`.

How it was made: six Codex helpers (gpt-6.1-terra high for the code audits and the data forensics, gpt-6-luna for T3's
desktop app and the areas the first study skipped) and three Haiku helpers (words on screen, words in real answers,
T3's own wording), each on a copy of the code and of the owner's 34 saved conversations. Every number below was measured
on that copy or on the installed app; the claims that matter were re-checked by hand on `origin/phase-1-base` (c889460d).

## What the person feels, and why

### 1. Lag: every helper step rewrites the whole conversation (measured)

- `Core::keep_child_event` (`engine/src/engine.rs`, around line 1440) saves the conversation after every kept helper
  event, and `Store::save_checked` writes the whole conversation as one JSON file and renames it into place. Nothing
  coalesces those saves, and the shell calls the engine with its lock held, so every other conversation waits.
- The owner's largest conversation is 102 MB: 1,362 messages and 35,902 tool records; 92.6 MB of it is tool input and
  output. Parsing it takes 445 ms, writing it 279 ms plus 510 ms to write and flush (Node on the owner's PC, a lower
  bound for the same work in the engine). So each helper step in that conversation costs about 0.8 s of locked engine.
- Four conversations are over 10 MB (102, 45, 17, 15 MB). Bytes per turn reach 150 KB.
- The installed app, after 55 minutes with two project sessions working: `faber.exe` 644 MB (634 MB private), its six
  WebView2 processes 617 MB, the engine child 43 MB: about 1.3 GB, against the 500 MB ceiling of R125 (the CLIs are not
  in that number).

### 2. Silent stalls and circles: four ways to wait forever, two ways to loop (code audit, re-checked)

| Where | What happens | What the person sees |
| :--- | :--- | :--- |
| `engine/src/engine/silence.rs` (`waits`) | Any open tool, permission or compaction resets the silence clock, so a command or tool server that never returns keeps the turn "working" for ever | A running step, no notice |
| `engine/src/engine/compact.rs` | Compaction has no deadline of its own and is excluded from the silence watch; queued messages and keep-going wait behind it | "Working", nothing else |
| `engine/src/engine/stuck_watch.rs` | One pending approval resets every other helper's stall clock and clears its sign | The question only; a stuck sibling goes unflagged |
| `stuck_watch.rs`, `stuck.rs` | Background helpers are not watched at all; foreground ones only get a sign after 10 min (30 with a command open), and the sign stops nothing | "2 coisas ainda rodando" |
| `engine/src/engine/keep_going.rs` | Short non-empty idle answers rest 2 to 5 min and repeat with no cap; the 200-turn cap counts only working turns | A run that never ends |
| `keep_going.rs` (`silent_twice`) | Two empty answers stop the run but are labelled as done | "Continuou até não sobrar nada" with no proof |

The saved conversations confirm the pattern, not a freeze rate: in one conversation two identical answers alternate 19
times each in four minutes under keep-going; 255 automatic turns ended with neither text nor tools; a main agent ran
the same failing command 18 and 19 times in two sessions (7 and 8 failures), which the helper-only stall watch cannot
see; 59 web searches failed with empty saved inputs; one answer took 196 minutes with 1,091 tool steps and 10 helpers.
Nine unclean exits are on record (not all crashes). What already works and must stay: the 15-minute silence stop with
PC-sleep handling, repeat/failure/silence signs on foreground helpers, bounded keep-going failures and provider moves,
save failures that are surfaced and retried, 30-per-second event coalescing, open-snapshot generations.

### 3. Hard to follow: six status lines say "working" at once

The owner's screenshot of a running project turn stacks, under one answer: "Interrompida." / "Trabalhou sozinho · 15 min
· 3 turnos · plano 1 de 4" / the quota-switch sentence / "Enquanto você estava fora (1 h): 2 turnos de trabalho, 1 troca
para a outra IA." / "Trabalhando · 1 etapa · 1 comando" / "Executando · sleep 580; tail -8 …; gh pr view 383 …" / the
"Plano 1 de 4" checklist / the cheaper-helpers note / "Trabalhando · atualizado há 8 min" / "2 coisas ainda rodando ·
detalhes". Counted: six places answer "is it working?" (`ui/Run.tsx`, `ui/Thread.tsx` work log and away line,
`ui/foot.ts`, `ui/Details.tsx`), two count turns over different periods, two show the same plan count, two report the
same provider switch, four clocks measure four different things with no label. One shows a raw shell line. All are 12
or 13 px muted text (`fs-xs`, `fs-sm`, `ink-3`) under a 15.5 px answer, so the important state looks like a footnote.

Helpers: of 1,155 helper titles shown in 34 conversations, 1,144 were English (the CLI's `description` field or the
helper `title`, shown untranslated by `ui/worklog.ts`), for example "recorder analysis readiness". "pronto" means the
tool returned, not that the work was checked. "Dizer algo…" pre-fills `Sobre "<English title>": ` in the composer, which
looks like text already written and does not say who receives it.

Header and sidebar: the title is the first prompt cut to 60 characters, often a bare URL; the header shows the copy's
branch name ("faber/https-github-com-pingdotgg-185a"); the 43 % ring is the conversation's context use but says so only
on hover; the quota bars show "≥ 24 %" and "16 %" used with the vendor only in a tooltip; three header icons have no
visible label.

### 4. Words: answers got much plainer; the leftovers have known causes

Measured on 1,521 visible answers (129,289 words): technical terms per 1,000 words were 40 to 106 until 2026-10-19 and
3 to 10 from 2026-10-20, when `persona()` started asking for everyday words (rule of task 017/020). 73.5 % of answers
have none. What still leaks:

- The rule has a door: technical words are allowed "unless the person used those technical words first in this
  conversation". The owner pastes long prompts, links and other AIs' output, so the door is usually open.
- The persona says "never write that you are waiting on [background work]", which pushes the agent to stay silent
  exactly when the person needs to know what is waiting (it conflicts with the plain-progress goal).
- Helper titles are never mentioned in the rule, so they stay English.
- The screen itself: the 2026-10-29 audit of `ui/locales/pt-BR.ts` (see the helper report) found strings like "galho",
  "papel", "cache quente", "CLI", "sessão", "grafts, clone raso ou alternates", and a 90-word security text in Settings.

## What the reference apps do better (from the owner's screenshots)

- **Claude desktop:** one grey line per batch of work, past tense with counts ("Recuperou 1 memória, executou 10
  comandos ›"); answer text larger and high contrast; one plan card; background work, outputs and context use live in
  a side panel with labels ("Contexto 31 % usado"), each running task with a timer and a stop square. Nothing repeats.
- **Grok:** while working, one row per action with an icon ("Ran 3 searches", "Opened page …", a present-tense live
  row, "Working for 14s"); when done, the list folds to "Worked for 24s ›". The answer starts with the conclusion in
  bold, uses the person's questions as headings, ends with "Resumo direto", then sources, one row of actions and three
  suggested next questions.
- **T3 Code wording** (helper report `t3-copy.md`): past tense plus count; "Working for 1m 05s" then "Worked for 4m
  12s"; limits as pace and time to reset; errors that admit uncertainty and give a next step; status text that waits
  400 ms before showing and stays at least 400 ms, so it never flickers.

## What T3 Code adds beyond the first study

- **Reliability.** T3 has no better stall detector than Faber's: no heartbeat, no per-turn deadline, no loop detector
  (its 30-minute idle timer skips busy turns). Its strengths are elsewhere: a provider stream that ends unexpectedly
  becomes a failure the person sees; after a server restart, every run that was live is marked cancelled instead of
  shown as running, and an optional "continue where you left off" is capped at three in five minutes; Codex
  `serverOverloaded` retries five times with 2 to 30 s backoff using the saved history, never resending the message;
  failures are typed (usage limit, provider, transport, permission, validation, unknown) with a reset time when known,
  and "Resume at reset" is offered, off by default.
- **Storage.** SQLite with one row per message and events appended, history served in pages (10 user turns or 75
  items, 1 MiB per page), completed tool output loaded only when opened, groups over 150 items not inlined.
- **Desktop (useful for the Electron move).** Main process, a separate Node server and a sandboxed renderer
  (`contextIsolation`, `sandbox`, no Node in the page, strict CSP, a small typed preload); the window shown only when
  ready; the renderer reloaded at most three times after a crash; Node's compile cache at start.
- **Evaluations of agent behaviour** (`t3/evals/`): each change to instructions or tool descriptions comes with a case
  that failed before and passes after, plus a normal-use control, several samples against the same model, graders
  unit-tested, paid runs kept out of CI. This is how Faber can prove a persona change makes answers plainer.

## Ranked fixes

1. **Stop rewriting the whole conversation per helper step** (lag, memory): coalesce saves, keep tool output out of
   the conversation file, page history to the screen. Task `100-save-without-rewriting`.
2. **Never wait or loop without telling the person**: the four waits and two loops above, plus repeated failing
   commands of the main agent. Task `110-never-stuck-silently`.
3. **One calm status while work runs**: one live sentence at reading size, a helper list, raw commands only in details,
   one line after. Task `120-one-calm-status`.
4. **Helpers and answers in the person's words**, proven by a small behaviour evaluation. Task `130-plain-helpers-and-answers`.
5. **Plain screen texts** from the locale audit. Task `140-plain-screen-texts`.
6. **A header and sidebar that explain themselves**. Task `150-clear-header-and-sidebar`.
7. **Honest restart and limits** (T3's typed failures, cancelled-on-restart, resume at reset). Task `160-honest-restart-and-limits`.

The first study's tasks (needs-you-first list, readable turns, palette and queue, Antigravity) stay as they are; task
120 is the live state and 040's fold is what remains after, so whichever lands second extends the other.
