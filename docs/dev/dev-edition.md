# Faber Dev

The owner's own copy of Faber for trying what landed, next to the installed Faber and never touching it
(`scripts/dev-edition.mjs`).

## What it is

- **A separate program.** It is built from the newest `phase-1-base` commit, which CI lets in only when every system is
  green, in its own clone of the repository (never the working checkout). A build that fails keeps the last working
  program. It is a debug build, so the page's DevTools open with a right click.
- **A new person at every start.** Each start gets a new, empty session folder for both Faber's data (`FABER_DATA_DIR`)
  and the page's browser profile (`WEBVIEW2_USER_DATA_FOLDER`), so it opens on the first-run screen with no
  conversation, project or local storage from before; older sessions are deleted. As in tests, it never updates itself,
  never registers to start with Windows and never holds the single-instance lock, so it runs next to the installed
  Faber. The AIs are the real CLIs with their own sign-in; nothing reads or copies it.
- **A hub page.** Changes by day (the what's-new lines in Portuguese, then the `feat`, `fix` and `perf` commits, then the
  rest folded), and every screen the newest green CI run photographed, grouped from first run to settings.

## Commands

All in `FABER_DEV_HOME` (default `E:\Apps\FaberDev`): `src` (the clone), `target`, `app`, `data`, `hub`.

```bash
node scripts/dev-edition.mjs install
```

builds it, writes the hub page and puts three shortcuts on the desktop:

- **Faber Dev** (`start`): opens it as a new person, then looks for a newer green build in the background at idle
  priority; that build is used from the next start (Windows cannot replace a running program).
- **Faber Dev continuar** (`start --keep`): reopens the newest session instead.
- **Faber Dev novidades** (`hub`): refreshes and opens the hub page.

`update --force` builds again even when nothing new landed. The shortcuts run the copy in `FABER_DEV_HOME`, which each
update replaces with the one from the clone.
