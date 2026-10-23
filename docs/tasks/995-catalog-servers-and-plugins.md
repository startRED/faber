# The catalog holds the tool servers and plugins a software project needs

The owner wants the first stable version to have a catalog with skills, plugins and tool servers that install, uninstall,
turn on and off, and that sessions pick by themselves (2026-10-23). Today the Catalog tab has 114 skills, one tool server
and no plugins (product map, "Catalog tab"); `docs/research/2026-10-21/catalog-list.md` says why each candidate was kept
or left out, and its "Plugin delivery preflight" says what a plugin must pass.

- Curate tool servers a software project uses every day (browser automation, the code host, docs lookup, databases and
  the like), each from its original source, pinned, with its licence shown, free to use with no key Faber must hold, and
  checked against the same containment and refusal rules as the kept server. Leave out anything that needs a paid key or
  an account Faber would have to store.
- Look again at plugins under the preflight: only whole plugins whose every part passes the checked delivery; a
  rejection is never reversed by packaging alone.
- Never bundle anything (AGENTS.md): the catalog lists, the install downloads from the source.

Done means AGENTS.md "Done means", the curation's reasons per candidate in the research note, a check per kept server
that installs, starts and removes it with the fake source, and session kits picking a kept server for a task that needs
it (`docs/product-map.md` "Session kits").
