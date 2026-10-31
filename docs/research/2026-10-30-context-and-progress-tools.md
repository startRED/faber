# Context and progress tools, measured against what Faber has

Date: 2026-10-30. Sources: each project's README and docs and the papers named below, read by two research sessions. No
project was cloned or run, every number is its authors' own unless a source says otherwise, and no repository's last
commit date was seen. GSD and BMAD are covered in depth by `docs/superpowers/specs/2026-10-22-faber-framework/`; this
page adds the context tools and the progress-board question the owner asked on 2026-10-30.

## What was built from it

The board (docs/dev/workflow.md#board): one generated page of doing, landing, next, waiting and landed, never written
by hand, every list capped at ten rows. It takes GSD's "state file under 100 lines, a digest, not an archive", CCPM's
status scripts that cost no tokens, BMAD's status derived from what exists, and Backlog.md's "ready when every
dependency is done" (already `Depends:`), and avoids the trap all four hit: a shared state file that writers edit by hand
(GSD needs a lockfile, BMAD a never-regress rule, Task Master's single `tasks.json` conflicts across branches).
`npm run docs` also caps what agents read whole before they start: AGENTS.md at 14,000 bytes, a task brief at 6,000.

## Context tools

| Project | Mechanism | Published result | Fit for Faber |
| :-- | :-- | :-- | :-- |
| microsoft/TokenEconomizer | Repo facts confirmed by three sessions are injected at start; unused tools pruned in the CLI's own settings; a judge nudges a stuck turn | SWE-Bench: cost down 18.4-30.9% in all 30 cells; pass@1 up in 4 of 5 agent-model pairs, down 1.0 in one | Best measured and closest to Faber's lessons (`lessons.md`, task 920). Needs Python; Windows not stated. Measure before adopting |
| headroomlabs-ai/headroom | Compresses tool output by type, keeps the original retrievable | 20% fewer tokens for coding agents, 60-95% on JSON (vendor) | The main path wraps the CLI in a proxy, which Faber refuses. Copy the idea: big output summarized, original on request |
| thedotmack/claude-mem | Hooks record tool use, an LLM summarizes it, SQLite FTS5 and Chroma serve it back | "~10x" with no benchmark found | Skip: a worker, a vector database and a summarizer that spends quota or needs a key |
| Graphify-Labs/graphify | tree-sitter code graph with no LLM, plus a small report file | 82.0% key-fact coverage against 70.8% for grep and read on one repository | The idea is already here: graft indexes this repo, and the `map` tool serves projects |
| infiniflow/ragflow | Document RAG engine | No benchmark shown | Skip: Docker, Elasticsearch, about 16 GB of memory and model keys |
| milind-soni/OpenMausBot | Desktop chat whose bots drive the user's own `claude` and `codex` | None | A product comparison only: the closest in shape to Faber, with no context mechanism |
| CopilotKit/OpenBot | Self-hosted coworker bots in containers behind a policy gateway | None | Skip: Docker, PostgreSQL, a license and a model key |
| oraios/serena | Language-server symbol tools over MCP | Anecdotal 60-80%; one study (arXiv 2608.13568) found a similar server cost 6-118% more tokens than grep and was used 0-6% of the time | Never on by default; at most opt-in on large repositories |
| mksglu/context-mode | Runs big output in a sandbox, keeps only what is printed; BM25 over docs | 98% on its own case, 25% on an independent 20 KB log test | Same idea as headroom's, without the proxy |

## What the studies say about always-loaded files

- ETH Zurich (arXiv 2602.11988): repository context files gave no significant success gain and cost over 20% more.
- Khatri (arXiv 2607.27250): none, always-on and selective retrieval did not differ in correctness.
- Lulla et al. (arXiv 2601.20404): AGENTS.md lowered runtime and output tokens over 124 pull requests; the question is open.
- Vercel's eval, as reported by a third party: an always-present 8 KB index scored 100% on unseen APIs; a skill the agent
  had to choose to open scored 53%, and was never opened in 56% of runs.

Read together: the always-loaded file stays small and holds only what the code cannot show plus where to look; anything
optional needs a pointer that says when to open it, because agents skip optional lookups.

## Ranked, for a harness that drives unmodified CLIs

1. A small always-loaded file and an index that says when to read each page: done (start card, `docs/README.md`, the new caps).
2. Large tool output summarized with the original retrievable, without a proxy: partly queued (`big-tool-input-in-pages`).
3. Section-level keyword search over the project's Markdown, no vector database: `memory_search`.
4. A code map built without an LLM within a token budget: graft and `map`.
5. Facts confirmed across sessions before they are injected: closest to `lessons.md`; TokenEconomizer is the measured
   reference to compare against.
6. Symbol servers only where a repository is large, never by default.

## Traps

Proxies in front of the CLI; always-on language servers; long generated context files; memory tools whose summarizer
spends quota or needs a key; heavy RAG stacks; headline percentages from the authors' best case; prompt compressors on
code, where one dropped token breaks a diff; repository maps that drop low-ranked call sites (9 of 40 diffs failed to
compile in one reported run).
