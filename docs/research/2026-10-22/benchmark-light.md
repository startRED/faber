# Light benchmark, first run (2026-10-22)

Erick's order: "Montar e rodar uma versão leve tipo 5 pedidos" (R410). Faber (the engine, Auto, Claude Code only) against plain Claude Code on 5 requests of the plan in [product/README.md](../../product/README.md#goals). How to run it: [dev/benchmark.md](../../dev/benchmark.md). One run, five requests, one judging pass each: a first look, not a measurement with an error bar.

## What was run

- **Faber side:** `Engine::send` with Auto (the engine's router picks the model), Full access, Chat or Projects (a separate copy for a project), the real Claude Code CLI on the person's own sign-in. **Plain side:** `claude -p --model claude-sonnet-5-5 --effort medium --permission-mode bypassPermissions`, the model Faber itself starts on when nothing picks one, in its own copy of the same starting folder. Both sides keep this PC's own Claude setup (hooks, user instructions, three MCP servers).
- **Judge:** a fresh Claude Code session, `claude-opus-5-5` at high effort, no tools, none of the person's setup, reading the two replies, the files and the proof's result as "A" and "B" (the sides alternate, the app's name is scrubbed), revealed after the vote. **The other vendor was not used:** Codex's quota is out until 2026-10-27 03:32, so the judge is the same vendor as both sides (models.md allows it in a fresh session; the review is weaker, and a Claude judge may lean toward Claude-style answers).
- **Proofs:** a Node script per request checks the result (listed below); a failed check is shown to the judge as a fact.
- **Cap:** US$ 8 API-equivalent or 3 million first-time tokens for a whole run, US$ 1.50 for one side of one request. Five medium requests were expected to cost about US$ 0.15 each per side (the Faber chats turned out far dearer, see below), plus about US$ 0.05 of judging each, so the cap is three times what a run was expected to spend. It never stopped a run. It is weaker than it sounds: Faber's cost is written only when a turn ends, so one long reply is never interrupted and a project's fix loop runs until the 20 minute run limit; Codex reports no cost, so the dollar caps do nothing for Codex.
- **Windows and priority:** no window opened: every program started through the engine's own start (no shell, no console, a job of its own) and the runner itself started by hand at below-normal priority (the code does not set it).

| # | Request | Kind | Proof (a script) |
| :-- | :--- | :--- | :--- |
| 1 | `organize-folder`: sort 29 mixed files into folders, keep every file, write an INDEX.md | Project | every file still there by hash, none copied, only INDEX.md loose, 3 to 10 folders, none empty or holding most files, INDEX.md names each folder |
| 2 | `plan-an-app`: plan a medicine-tracking app for a non-programmer as a conversation | Chat | none, the judge alone |
| 3 | `todo-app`: a dependency-free Node to-do CLI with tests and a README | Project | its own `node --test` passes, then the six commands of the request run in a copy, and bad input exits non-zero |
| 4 | `research-with-sources`: what changed from Node.js 22 to 24, the current LTS and its end of life, with links | Chat | at least four distinct links, each opens (401, 403 and 429 count as existing), no placeholders |
| 5 | `everyday-question`: why a phone drains faster in winter, under 180 words, exactly three tips | Chat | at most 180 words, exactly three list items |

## Result of the final run (`light-3`)

| Request | Kind | Judge (blind) | Faber proof | Plain proof | Faber s | Plain s | Faber tokens | Plain tokens | Faber US$ | Plain US$ |
| :--- | :--- | :--- | :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: |
| organize-folder | Project | plain | pass (8/8) | pass (8/8) | 29 | 23 | 28377 (187376 cached) | 26335 (130338 cached) | 0.144 | 0.126 |
| plan-an-app | Chat | Faber | none | none | 21 | 11 | 257070 (0 cached) | 25793 (54170 cached) | 1.032 | 0.110 |
| todo-app | Project | plain | pass (11/11) | pass (11/11) | 28 | 29 | 26996 (183990 cached) | 28819 (138704 cached) | 0.140 | 0.147 |
| research-with-sources | Chat | Faber | pass (3/3) | pass (3/3) | 60 | 52 | 15647 (767757 cached) | 30661 (179925 cached) | 0.194 | 0.185 |
| everyday-question | Chat | plain | pass (2/2) | pass (2/2) | 16 | 12 | 257498 (0 cached) | 22211 (16405 cached) | 0.259 | 0.092 |

| Totals | Faber | Plain |
| :--- | ---: | ---: |
| Judge wins (ties 0) | 2 | 3 |
| Time (s) | 155 | 127 |
| First-time tokens (input, output, cache writes) | 585588 | 133819 |
| Cache-read tokens | 1139123 | 519542 |
| API-equivalent US$ | 1.769 | 0.661 |
| Faber cost as a share of plain | 268% | 100% |

Spend of the run: US$ 2.43 for the sides, US$ 0.22 for the judge. Tokens count input, output and cache writes as first-time tokens. Time is the reply's own: the plain run, or the reply's `took` in Faber (router and copy included); with the engine's start and the wait for a project's checks Faber's wall time was 51, 30, 44, 70 and 24 s in the table's order (`results.json`, `wall_secs`).

Judge's A/B picks per run (the first answer shown): light-3 picked A in 5 of 5 requests, light-2 picked B in 4 of 5, light-1 picked B in 2 of 5. A in all five is what a pure lean to the first answer gives with this alternating deal (A is plain, Faber, plain, Faber, plain), and light-3 scored 8 against 7 on three requests, so one-point gaps are near-ties, not findings. Only the plan, the research and the todo app, stable over three runs with wide gaps, support a claim. The judge's reasons, one line each: the plain organize grouping was "more natural" (tax return under finance, fewer odd placements); Faber's plan came with targeted questions, the plain one asked a single question and gave no plan; the plain todo tests were "more thorough and isolated" (`node --test` ran 2 tests for Faber, 5 for plain); Faber's research added the OpenSSL 3.5 security-level change and disclosed a conflicting date; the plain answer to the winter question was more useful than the Haiku one Auto picked, which had a dubious claim.

Auto picked Sonnet 5.5 at medium for requests 1 to 3, Sonnet 5.5 at high for the research and Haiku 5.5 at medium for the everyday question.

## The two earlier runs, and what they say about the noise

Same requests, same code except where noted. `light-1` had two defects in my proofs, found by reading its results and fixed before `light-2`: a minimum of three tests that the request never asked for (it flipped the todo proof) and a 403 from a news site counted as an invented link. `light-2` changed only those proofs and how the runner logs each CLI's arguments; `light-3` added the reply's own time.

| Run | Judge wins (Faber - plain) | Proof passes (Faber, plain) | Faber cost as a share of plain |
| :--- | :--- | :--- | :--- |
| light-1 (proofs not yet fixed) | 4 - 1 | 3 of 4, 1 of 4 | 243% |
| light-2 | 3 - 2 | 4 of 4, 4 of 4 | 83% |
| light-3 | 2 - 3 | 4 of 4, 4 of 4 | 268% |

Per request, over the three runs: the plan and the research went to Faber each time, the todo app to plain each time, the organize folder to Faber, Faber, plain and the everyday question to Faber, plain, plain. The verdict of a request flipped between runs while both answers kept passing their proofs, so a single judging pass of five requests cannot rank the two; what held through three runs is the plan and research wins and the todo loss.

## What stands out

1. **Faber's Chat carries about 230,000 tokens of tool definitions on this PC, and that alone sets the cost.** A Chat conversation started with `--tools Read,Write,Edit,Glob,Grep,Bash,WebSearch,WebFetch,AskUserQuestion` and no `ToolSearch`, so Claude Code cannot defer the MCP tools of the servers set up here (rea, blender and rimworld, 221 tools) and puts every definition in the first request. Measured by replaying the Chat session's own arguments (the runner logs them in `spawns.log`): 233,000 tokens as Faber starts it; 37,000 with no `--tools` list; 14,000 with `ToolSearch` added to the list. A cold start writes the 230k to the cache (the 1.03 and 0.26 dollars above); a warm one reads it (`light-2`, where the same Faber chats cost 0.048 and 0.017 and the share fell to 83%). Plain Claude Code loads the same servers and defers them: a first request of about 37,000 tokens. Projects do not show it: their cached reads are close to plain's. The goal "at most 70% of the plain CLI's cost" is out of reach on a PC with many MCP servers until the Chat list carries `ToolSearch`; the fix, adding `ToolSearch` to Chat's tool list, is being shipped as its own change.
2. **Quality is close, not yet a win.** The judge split 2 - 3, 3 - 2 and 4 - 1 over three runs, and every proof passed for both sides once the proofs were fixed. The goal is a win or a tie on 9 of 10 requests, and Faber lost the todo app to a tidier, better tested plain answer in all three runs.
3. **Auto's cheapest pick lost its request.** On the everyday question it chose Haiku (US$ 0.017 warm against 0.077 to 0.092 for plain Sonnet) and the judge preferred the Sonnet answer in two of three runs. Cheaper per answer is not cheaper per answer worth keeping.
4. **Time:** Faber's reply took 1.2 times the plain CLI's (155 against 127 s), with the router and the project's copy inside it; the goal is about the first token, which this run did not measure.

## What could not run

- **Codex rows:** Codex's quota is out until 2026-10-27 03:32. `--plain codex` and `--vendors claude,codex` are written but never met a live account (the event reader follows Codex's documented format). Once the quota is back: `cargo run -p faber-engine --example bench -- --plain codex --vendors claude,codex`, and the judge becomes the other vendor's strongest model for the Claude rows.
- **Not verified:** request 4's facts (the current LTS today and its end of life): the proof only fetches the links (403 and 429 count as existing, so a made-up path on a site that blocks bots passes) and the judge has no tools or web. Request 2, a plan as a conversation, was judged on one single-turn reply per side, not a conversation.
- **Plain side unguarded:** it runs without Faber's secret-path deny list and loads this PC's hooks, instructions and MCP servers.
- **Creative requests** (thumbnail, video, 3D, mod), the full ~40, a second judging pass with the sides swapped, and the goals that are not quality (speed to first token, precision, memory).
- **Not counted in Faber's tokens:** the router's Haiku question and two small side processes the engine starts with a session (about a cent together). The judge's spend is counted apart.

## Cost of this work

Three runs cost US$ 6.26 API-equivalent (2.30, 1.31 and 2.65 with the judge); two one-request smoke runs, the four replays that found the tool list and two plain checks added about US$ 0.8. Together about US$ 7, inside the US$ 25 allowed for tests. The raw results of each run are in `E:/Temp/faber-bench/light-N/results.json` (not in the repository), next to its folders.
