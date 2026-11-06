# To-do tool

A small command-line task list using Node.js built-in modules. No installation or dependencies required. Use Node.js 18 or newer.

## Usage

Run from this folder:

```sh
node todo.js add buy milk
node todo.js add "walk the dog"
node todo.js list
node todo.js done 1
node todo.js remove 1
```

`list` prints one line per task, such as `1. [ ] buy milk` or `1. [x] buy milk`. An empty list prints nothing. Numbers start at 1 and shift after removal. Marking a completed task done again is harmless.

Tasks are saved in `tasks.json` in the current working folder. To use a different folder, run the script by its absolute path from there. Missing text, invalid or nonexistent numbers, unknown commands, and storage errors print an error and exit with a non-zero code. Task text must fit on one line.

Run one command at a time; simultaneous changes to the same task file are not supported.

## Tests

```sh
node --test
```

Tests run the actual commands in temporary folders and leave your tasks untouched.
