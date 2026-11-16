# A stopped helper wakes its lead, and the lead checks before it says "running"

Area: project
Files: engine-core and engine-projects (helpers, lead messages), host table, `docs/product-map.md`
Depends: none
Size: one pull request

Why: On 2026-11-15 at 21:39 local time, the helper "Adaptar controle à base atual" of conversation 2c436503 (Faber em execução com meus dados) ended after 1 minute with "The helper stopped before it finished: it was stopped", in the middle of a `git rebase --onto` with conflicts in five files. Nothing woke the lead. The lead's last message still said the adaptation was running in the background, and the screen showed "esperando o comando dele terminar" for about 25 minutes, until a person wrote to it. No helper process was running. Why the helper was stopped is not known: check whether it was the app, a restart, a budget or quota limit, or a person, and record the reason on the helper's result. Evidence: the last assistant message of `conversations/2c436503-*.json` (tools `Helper`, `commandExecution`).

Build: (1) A helper that ends as stopped, failed or dead wakes the conversation that started it, with the reason and, for a helper that worked in a folder, the state it left (clean tree, rebase or merge in progress, conflicted files). (2) A lead's statement that something is running is checked against the live helpers and background commands before the turn ends: when none runs, the wake tells the AI so instead of letting it wait. (3) The reason a helper was stopped is always recorded and shown.

Leave out: restarting helpers automatically; the AI decides what to do after the wake.

Done: A test fails without the change: a fake helper that exits as stopped wakes its lead with the reason and the folder state, and a lead that ends a turn waiting for a helper that is not running is woken. A real run with the cheapest models stops a helper mid-task and shows the lead recovering without a person. Memory and processor cost is in the commit body.
