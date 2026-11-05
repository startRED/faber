# Classify directory searches without guessing their contents

Area: Agent's plan and closing box
Files: engine/src/permissions.rs, engine/src/engine/plan.rs, engine/tests/plan.rs, docs/dev/plan-boundary-qualification.md
Depends: none
Size: about 400 changed lines

Why: task 500 ships a conservative boundary: directory searches and directory deletes always require a risky
contract. The removed scanner could miss protected contents while classifying the operation as small or normal.
Keep that fallback until an inspection can match the actual tool's reach with a bounded cost.

Build:

- Specify the different traversal rules: rg respects ignore rules and hidden-file settings, including its config;
  grep -r/-R can read everything. Do not assume a generated or dependency directory is ignored.
- Define cap behaviour that cannot approve an uninspected subtree under a weaker contract. Cover credentials
  inside skipped folders and beyond the cap, and directory deletes containing .git or .env.
- Resolve hard-link aliases conservatively without reading credentials; cover Glob descendants below ** instead
  of checking only the literal prefix. Keep existing symbolic-link, junction and network-path guards.
- Measure directory inspection cost under the engine lock and avoid blocking unrelated work. Test secret() in
  downstream guards when the project lives below .claude/worktrees ancestors, with relocated credential roots.
- Add failing regression checks for the findings in srp.review5.md, srp.review6.md and srp.review6b.md before
  replacing the fallback. Obtain the independent security reviews required by product-rules.md.

Leave out: changing permission modes, intercepting tools that never request approval, a general shell interpreter,
relaxing single-file path protections, and claiming a point-in-time check prevents later link substitution.

Done: the inspector matches the requested tool's actual reach, never downgrades an uninspected boundary, and has
measured lock-time and memory bounds. Checks cover ignore rules, caps, skipped credentials, directory deletes,
hard links, recursive globs and worktree ancestors. Until then every directory search/delete remains risky.
