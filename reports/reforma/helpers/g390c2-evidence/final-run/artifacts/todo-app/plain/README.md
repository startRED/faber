# Command-line to-do tool

Requires Node.js 18 or newer. No dependencies or installation needed.

## Usage

```sh
node todo.js add buy milk
node todo.js add "walk the dog"
node todo.js list
node todo.js done 1
node todo.js remove 1
```

`list` prints tasks as `1. [ ] buy milk`, or `1. [x] buy milk` when done.
An empty list prints nothing. Removing a task renumbers the remaining tasks.
Invalid commands, missing text, and invalid or out-of-range numbers print an
error to stderr and exit with a non-zero code.

Tasks are saved in `tasks.json` in the current working directory. Running the
tool from a different folder uses a separate list. Invalid existing data is
reported without overwriting it. Run one command at a time; simultaneous edits
to the same list are not supported.

## Tests

```sh
node --test
```
