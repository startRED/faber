# Projects work in folders with spaces, accents and long names

Area: Projects
Files: engine/tests/project.rs, engine/tests/project_git.rs, engine/src/project/copy.rs, engine/src/cli.rs
Depends: none
Size: about 200 changed lines

Why: many Windows users keep projects in `C:\Users\Joao Silva\Documentos\Meu Projeto (final)` or on a OneDrive path, often with accents (the owner writes Portuguese). Faber's tests use short ASCII temp names, so a break in quoting, the 8.3 short-name handling, a long path or a non-UTF-8 name would only show in a person's first project.

Build: tests, with fixes only for what they find. Create a project folder named with spaces, an accent, a parenthesis, an apostrophe and a path over 260 characters; open it as a project, make a separate copy, run a fake-CLI turn that writes a file, read Changes and remove the copy. Pass each path to child processes as an argument, never inside a shell line. Cover the same name in the working folder given to the CLI and the one stored in `projects.json`.

Leave out: other operating systems' naming rules and any Electron work.

Done: the new tests pass on Windows and fail if a path is split at a space or lost at 260 characters (show it by temporarily breaking the argument passing); each bug found is fixed in the same pull request. Extends "Projects". No cost outside tests.
