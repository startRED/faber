# Auto's task guide

What Auto's router is told about each kind of work, how scope decides which model starts it, and the makers' guidance
both follow (read 2026-11-11). The bars, the table and the rules of a conversation are in [models.md](models.md#auto-a-blind-router).

## Levels

Difficulty follows the work, not message length; ties use the harder level. The router sees no model or maker names.
Writing includes documents, slides and spreadsheets. Visual work is building something judged by eye (a page or
interface design, an animation, a video or explainer film made with code, a designed deck): code's bars, never past
hard, so it starts on the everyday model (R568). Claude leads it while its quota has room, Sol otherwise (R569).

- Easy: greetings and short facts; sorting, tagging, extracting and summarising supplied text; finding files and reading
  logs; one-line rewrites or renames; simple arithmetic.
- Medium: everyday documents, emails, slides and spreadsheets; translations; scoped code (a named bug, one function or
  file, a clear spec or a small script); routine investigations, reviews and explanations.
- Hard: engineering across files, unknown bugs, features across parts and large reviews; many screenshots or computer
  operation; financial models and documents from many sources; proofs; costly subtle errors in concurrency, security,
  legal, medical and money work.

## Scope

Scope decides who starts the work. Tight: the finished result can be stated in one sentence (a named bug or one with clear
steps to reproduce, one file or function, a spec, one deliverable, a feature whose parts are already decided even across
several files). Open: explore and decide, choices nobody has written down, many services, sources or milestones, a migration, major refactor, architecture, deep research or a long
unattended run; a clear spec does not make a migration tight. Anthropic's own split, from its launch video for Fable 5.2
(transcript read 2026-11-11): most work starts on Opus at medium; Sonnet takes well-defined work where speed and cost
matter; Haiku takes high-volume sorting, routing and summarising; Fable takes ambitious, open-ended work and can lead
while Sonnet does the well-defined pieces; and when a result falls short, raise effort before changing model. So at
neutral quota, with only Claude: medium tight work runs on Sonnet high, medium open work on Opus medium, medium open
research nobody can check on Fable medium; with Codex: Terra medium, Sol medium and Sol high. Not measured: how often
the router calls real requests medium and open, and what that costs in quota.

## Sources

Read **2026-11-11**: [Anthropic tasks](https://claude.dev/blog/which-claude-model-should-you-build-with/)
and [cost and intelligence guidance](https://platform.claude.com/docs/en/about-claude/models/optimizing-for-cost-and-intelligence),
and [OpenAI models](https://developers.openai.com/codex/models) and
[reasoning](https://developers.openai.com/api/docs/guides/reasoning). Measured floors and quota gates still apply.
