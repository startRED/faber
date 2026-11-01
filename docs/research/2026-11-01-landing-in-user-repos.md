# Landing work in a person's own repository

Proposal, nothing built. AI, 2026-11-01, for the owner to decide (the three questions at the end). It asks whether what
sped up Faber's own landing should also work for people whose repositories Faber's agents change. Faber's own
automation (`scripts/land.mjs`, `.github/workflows/ci.yml` and `queue.yml`, [landing.md](../dev/landing.md)) is used
only as a source of ideas. People's repositories differ, so nothing here copies it as it is.

## Why

Faber's own repository measured what a queue of agent deliveries costs when every delivery is checked and landed alone:

- 31 of 38 queue runs were cancelled and started again, because each landing moved the base under all the others
  (R497, 2026-11-01). One train of up to four deliveries, checked in one run, fixed it.
- 43% of queue runs were green checks thrown away when they were queued again, though the base had moved only in files
  they did not touch (R496).
- Three flaky screen checks kept the queue red for hours (R505).
- 441 session-minutes went to full test runs on the PC that CI would repeat anyway (R183), and the PC now checks only
  what a change touches (R203, R217).

A person's repository with two or three of Faber's conversations working at once (R198, R504) runs into the same
problems at a smaller scale: pull requests that conflict, CI re-run on every base move, a red main nobody can explain,
and private repositories paying for GitHub Actions minutes on every re-run.

## What exists today, to extend

A second way to do any of this would be a bug (product-rules). Each mechanism below builds on one of these:

| Existing part | What it does today | What this proposal adds |
| :--- | :--- | :--- |
| Separate copy (R201) | Each project conversation works in its own worktree on `faber/<words>-<id>`, never on the person's branch; Faber never merges | The copy's branch is the delivery; nothing else changes |
| Checks and `verify` (R137, R203) | The project's own checks; `fast` runs only what changed since the merge base, `full` runs all of them | A `full` run on the main branch after a landing, when the repository has no CI that does it |
| Followed pull request (R200, R478) | Faber follows a pull request the agent opened and wakes the agent on red checks or a conflict | It also says when GitHub's merge queue or Faber's train holds the pull request |
| File claims (R155, planned) | Proposed for missions with several workers | Claims for conversations of one project that run at the same time |
| Slice size (R490) | A delivery is the biggest coherent slice one AI does well | The same rule in the brief for a person's project |
| Git modes (R83, not on screen) | Safe, Balanced, Free; Free merges alone only after tests and the other AI's review | Trains and merge-queue use exist only where the mode lets Faber merge |
| Automatic archive (R222) | Archives a conversation once its pull request is merged or closed | Unchanged; a train landing counts as merged for each delivery in it |

## Ground rules

1. **Faber never changes how a repository is set up without asking.** That covers workflow files, branch protection and
   rulesets, turning on the merge queue or auto-merge, required checks, and test quarantine lists. When a change would
   help, Faber shows one card that says what it changes and why. The person makes the change, or Faber makes it as a
   pull request the person reviews like any other.
2. **Merging follows the git mode (R83).** In Safe and Balanced, Faber opens pull requests and the person merges.
   Trains and merge-queue entry exist only where the person has let Faber merge.
3. **Reading is free, writing is granted.** Faber reads the repository's setup with the person's own `gh`, hidden, as
   R200 already does. It never reads a token itself (AGENTS.md rule on credentials).
4. **The strictest reading wins when Faber cannot tell.** A `gh` sign-in that cannot read branch protection (a
   fine-grained token without admin read, or an organization policy) is treated as "protected, team repository".

## Step 1: Faber reads what kind of repository it is

This runs once when the project is added and again at most once a day while the project has a conversation working.
It costs a handful of `gh api` calls of about a second each, and no process stays alive afterwards. Faber reads:

- whether a GitHub remote exists, its visibility, and whether a person or an organization owns it;
- the default branch's protection or rulesets: required checks, required reviews, "must be up to date", and the
  `merge_queue` rule;
- which merge methods and auto-merge the repository allows;
- whether any workflow runs on `pull_request`, on `push` to the default branch, and on `merge_group`;
- whether other people pushed to the default branch in the last 30 days (solo or team).

The result is a profile stored in Faber's data for the project, never in the repository, and shown as one line in
Settings > Projects ("Solo repository with automatic tests on GitHub"). Settings is where it belongs, never a work
screen (C011).

## Step 2: defaults per profile

| Profile | How a delivery is proven | How it lands | Grouped landing |
| :--- | :--- | :--- | :--- |
| A. No GitHub remote | `verify fast` while working, `full` before it is offered back | The agent says the branch is ready; the person brings it in. In Free mode Faber fast-forwards the local branch after a `full` pass and the other AI's review | No: there is nothing to queue |
| B. GitHub, no CI | `verify full` on the copy before the pull request, since no one else will check it | Pull request per delivery. In Free mode Faber merges after `full` passes on the branch as it will land | Faber's train, checked by `verify full` on the PC |
| C. GitHub with CI, no merge queue (most personal repositories) | `verify fast` on the PC, then the repository's own CI on the pull request confirms | Pull request per delivery; in Free mode Faber merges once CI is green | Faber's train when two or more deliveries wait (below) |
| D. Team repository (other authors, or required reviews) | As C | Faber never merges on others' behalf, even in Free mode; it asks for review and keeps its pull request fixed (R200) | No train, since riders would merge without each other's reviewers. Faber orders its own pull requests so they do not conflict |
| E. GitHub merge queue on | As C, plus the queue's own run | In Free mode, `gh pr merge --auto` puts the pull request in GitHub's queue and GitHub groups and lands it | GitHub's queue, never Faber's train. Faber's train would be a second way of doing the same thing |

GitHub's merge queue is offered only for repositories that an organization owns (public, or private on a paid
organization plan). Repositories owned by a person cannot use it, as R162 found for Faber itself. That is why
profile C, the most common for a solo person, needs Faber's train. If a profile E repository has no workflow that runs
on `merge_group`, its required checks never report and every entry waits forever. Faber detects this and says it once
on a card. It never edits the workflow.

## Step 3: the mechanisms

**Small deliveries that do not overlap.** The brief Faber writes for a project's agent (building.md, "A brief in five
parts") keeps R490's rule: one delivery is one coherent feature or fix with its tests. When the agent starts children
(R504), each child gets its own piece and its own copy.

**File claims per conversation (extends R155).** Before a conversation writes, it states the files it will change.
Faber keeps the claims in its data folder, never in the repository. It refuses a write outside the claim, and when two
claims overlap the second conversation waits for the first to land or asks the person. In profile D it also reads the
files of open pull requests by other people, so its agent does not start on a file someone else is changing. A claim
nobody renewed for a day is stale, as R154 rules for Faber's own lanes.

**Test what changed, then check the whole main branch.** While working, `verify fast` runs only the checks whose
inputs changed (R203). Once deliveries land, the main branch gets one full check. If the repository has CI on `push`
to main, that run is the check and Faber follows it like a pull request. If not, Faber runs `verify full` on a fresh
copy of main, below normal priority, and only when no conversation of the project is answering. A red main wakes the
conversation that landed last. When a train landed, the run splits the train to find the culprit (next paragraph).
This is R496's rule applied to people's repositories: a delivery that is green on its own may land on a base that moved
in other files, and main is still checked before anyone builds on it.

**Grouped landing, Faber's train (profiles B and C, Free mode only).** When two or more deliveries are green on their
own and wait to land, and their files do not overlap, Faber stacks up to four of them on a branch `faber/train-<id>`.
It opens that branch as a draft pull request, so the repository's existing `pull_request` CI checks the whole stack in
one run without any change to the CI. On green it merges each delivery's own pull request in order, with the
repository's allowed method. The end result is the same tree the train checked, and each pull request shows as merged
with its own title and history. Then it closes the train pull request and deletes `faber/train-<id>`, the only branch
Faber ever deletes, since it made it and nothing else points at it. On red, the first half goes again until one rider
fails alone, as R497 does. A train costs one CI run where it would otherwise cost one run per delivery plus a re-run
for each base move. On a private repository those are Actions minutes the person pays for.

**Flaky checks.** When a check fails and then passes on the same commit, Faber notes it as flaky in the project's data
and in the conversation's Technical details. It never adds the check to a quarantine list in the repository, because
that changes their CI (rule 1). A train whose only failure is a noted flaky check is retried once before it is split.
After three notes Faber offers a card ("this test failed by chance 3 times this week") with a pull request that fixes
or skips it, for the person to review.

**Landing on a moved base.** In profile C without "must be up to date", a green delivery whose files the base did not
touch merges without a new CI run (R496), and the main-branch check catches what the two together break. With "must be
up to date" required, GitHub decides, and Faber updates the branch and waits for CI. That is one of the reasons to
suggest the merge queue to a repository an organization owns.

## What it costs

- Idle: nothing. No process, timer or watcher. `gh` runs only for the daily profile read, the existing pull-request
  follow (R200: about one second, 41 MB while it runs), and a train's few calls.
- PC: `verify full` on main once per landing in profiles A and B, below normal priority and never while a game is in
  front (R191).
- Quota: no model calls of its own. A red train or a red main wakes one conversation, under Auto's pacing as R200 does.
- GitHub: fewer CI runs than today in profiles C and E, and no new ones (next section). Profile B moves the checking to the PC.

## GitHub Actions minutes

The owner asked whether CI minutes are a problem for people. Public repositories run GitHub's hosted runners for free
with no monthly limit. Private repositories on the free plan get 2,000 minutes a month (Pro gets 3,000). A Windows
minute counts as two and a macOS minute as ten. Faber's own way costs about 22 jobs per delivery, on Windows and macOS,
and would use up a private repository's free minutes in a few days. So in people's repositories:

- **Faber never starts a CI run the repository did not already have.** It adds no workflow, job, schedule or
  `workflow_dispatch` call, and it adds no macOS or Windows runner. The runs it causes are the ones the repository's
  own `pull_request` and `push` triggers start anyway. A train's draft pull request is one such run, standing in for
  one run per delivery.
- **The light check is the default on private repositories.** On the PC, `verify fast` runs only the checks a change
  needs (R203), and a delivery is pushed only after it passes, so a red run on GitHub is the exception. When Faber offers
  the person a CI workflow (question 3), the offer is Linux only and selective: only the jobs the changed files need,
  with no macOS unless the person asks for it.
- **The minutes show before grouped landing is turned on.** Faber estimates what one delivery costs from the
  repository's recent runs (`gh run list` and each job's time and runner, weighted ×2 for Windows and ×10 for macOS).
  The switch in Settings > Projects then says, for a private repository, how many deliveries a month the free minutes
  cover, with and without trains. When a month's runs pass 80% of the plan's minutes, as far as Faber can read them,
  it pauses trains and says so on one card. It never runs extra checks to "make sure".

## Risks

- **Trust.** Merging into someone's repository is the biggest step Faber can take there. Mitigated by R83's Free mode
  only, tests plus the other AI's review, never in team repositories, and never with force or history rewrites.
- **A train hides which delivery broke.** Splitting finds it, at the cost of a few extra runs on a red train.
- **Two green changes that break together.** File-disjoint deliveries can still break each other's behavior. The full
  check on main catches it within one run, and the conversation that landed last is woken.
- **Branch protection that forbids it.** Required reviews, signed commits or linear history can refuse Faber's merge.
  Faber reads the refusal, says in plain words what the repository asks for, and leaves the pull request open. It
  never asks for bypass rights.
- **Noise in shared repositories.** A draft train pull request and its runs are visible to collaborators. That is why
  team repositories get no train.
- **GitHub changes or plan limits.** Merge-queue availability, rulesets and API permissions change. A profile Faber
  cannot read falls back to the strictest one (rule 4).
- **Public repositories and forks.** Workflows of fork pull requests run without secrets. Faber pushes only to the
  person's own repository and never opens a train from a fork.

## Order, if approved

1. The profile read and its line in Settings > Projects, plus file claims for conversations running at once. This part
   only reads and refuses, so every git mode gets it.
2. The full check of main after a landing (CI follow, or `verify full`), and flaky notes.
3. Free mode on screen for solo repositories (R83), then Faber's train for profiles B and C.
4. GitHub merge-queue entry for profile E.

Each step adds its rows to the product map and its decision when it lands. No step changes a repository's setup.

## Perguntas para o dono

Três decisões que só o Erick pode tomar. Cada uma vale sozinha.

1. **O Faber pode juntar o trabalho no ramo principal do repositório da pessoa sozinho?** Hoje ele só prepara o
   trabalho e quem junta é a pessoa. Opções: (a) nunca, a pessoa sempre aprova cada entrega; (b) só em repositórios em
   que a pessoa trabalha sozinha, se ela ligar isso nas configurações, e só depois de os testes passarem e a outra IA
   revisar (recomendado); (c) também em repositórios de equipe.
2. **Quando várias entregas estão prontas ao mesmo tempo, o Faber testa todas juntas de uma vez?** Isso economiza tempo
   e minutos pagos do GitHub, mas quando algo quebra ele precisa de mais uma ou duas rodadas para achar qual entrega
   foi. Opções: (a) sim, por padrão, quando a opção 1 permite e as entregas não mexem nos mesmos arquivos (recomendado);
   (b) só quando a pessoa pedir.
3. **Num repositório sem testes automáticos no GitHub, o Faber oferece criar esses testes?** Opções: (a) ele testa no
   computador da pessoa e oferece uma vez, num cartão, criar os testes no GitHub como uma entrega que a pessoa aprova
   (recomendado); (b) só testa no computador e nunca oferece; (c) cria sozinho. A opção (c) vai contra a regra de
   nunca mudar a configuração da pessoa sem perguntar.
