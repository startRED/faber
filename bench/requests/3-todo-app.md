---
id: todo-app
title: A small complete app with tests
kind: project
proof: todo
---
Build a small command-line to-do tool in Node.js with no dependencies, in this folder. Run it as `node todo.js <command>`:

- `add <text>` adds a task and prints a short confirmation.
- `list` prints one line per task, numbered from 1, like `1. [ ] buy milk`, and `[x]` instead of `[ ]` once it is done.
- `done <number>` marks a task done.
- `remove <number>` deletes a task; the numbers of the tasks after it shift down.

Tasks live in `tasks.json` in the folder where the command runs, so they survive between runs. A missing text, a bad number or an unknown command prints a clear message and exits with a non-zero code. Write tests that run with `node --test`, run them, and make sure they pass. Finish with a README that has a short usage section.
