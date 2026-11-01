# Screen check: a project agent on each AI hands a piece to the other, and the report shows in the thread

Area: Helper from the other AI
Files: testkit/src/screen/helper_both.rs, testkit/src/screen/mod.rs, tests/fixtures/claude/helper-call.jsonl, tests/fixtures/codex/helper-call.jsonl, testkit/screen-costs.json
Depends: none
Size: about 250 changed lines

Why: the helper is the leader agent's cross-vendor tool (R161), tested in the engine for both directions (the `r161_*` tests). On the screen the only helper scenario is "helper controls" on a recorded session. Nothing runs the whole round with the fake CLI: the call row, the helper's steps inside it, the report back and the cost joined to the reply. The leader must work with both AIs before the first public version.

Build: two scenarios in a project with both fake CLIs connected: a Claude agent calls the helper (a Codex helper runs), then the reverse. Reuse the existing fixtures where they fit.

Done: each scenario fails if the call row stays running after the helper ends, if the helper's steps do not show inside the call, if the agent's next words do not use the report, or if the reply's cost leaves out the helper's. Both directions in one run, within the memory limit. Extends "Helper, subagent". First check that the screen bench can connect both fake CLIs in one project; if it cannot, that is the first step.
