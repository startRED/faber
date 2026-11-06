# Three tool-server delivery preflights

Status: none admitted. Source and published-metadata review for task 995; no installation, execution or catalog change.
R253 accepts a pinned original npm package without dependencies. These findings apply that existing boundary; they
do not change it or complete the catalog task.

## GitHub repository tools

The official `github/github-mcp-server` is a Go project. Its documented local delivery is a Docker image or a compiled
executable, rather than the dependency-free npm artifact the current installer accepts. Docker is needed for the
container option; Go is needed to build from source, not necessarily to run a prebuilt executable. The repository
license is MIT. Exclude this local delivery under R253 without building, repackaging or adding a binary installer.

The documented local authentication options include user OAuth authorization and a supplied personal access token.
An account requirement alone does not establish that Faber must hold an account or a paid key. Neither authentication,
the hosted alternative nor plan-specific access was exercised; those do not resolve the local delivery blocker.

Sources at observed commit `f38526264abb14961b32cea386e38ec6d3cf674f`:
[README](https://github.com/github/github-mcp-server/blob/f38526264abb14961b32cea386e38ec6d3cf674f/README.md),
[Go manifest](https://github.com/github/github-mcp-server/blob/f38526264abb14961b32cea386e38ec6d3cf674f/go.mod),
[license](https://github.com/github/github-mcp-server/blob/f38526264abb14961b32cea386e38ec6d3cf674f/LICENSE).

## SQLite reference server

The official reference implementation is in `modelcontextprotocol/servers-archived`, whose repository is archived.
Its source manifest names `mcp-server-sqlite` 0.6.2, Python >=3.10, the entry point
`mcp-server-sqlite = mcp_server_sqlite:main`, and runtime dependency `mcp[cli]>=1.6.0`. This is a Python distribution,
not an npm artifact compatible with R253. No current official npm replacement was established or selected.

The local setup names a database path, without an API key requirement. The repository carries MIT licensing, but this
review did not establish a separate per-project license grant. Neither point removes the incompatible delivery and
maintenance concerns. Exclude this archived candidate without substituting a similarly named third-party package.

Sources at observed commit `9be4674d1ddf8c469e6461a27a337eeb65f76c2e`:
[Python manifest](https://github.com/modelcontextprotocol/servers-archived/blob/9be4674d1ddf8c469e6461a27a337eeb65f76c2e/src/sqlite/pyproject.toml),
[project README](https://github.com/modelcontextprotocol/servers-archived/blob/9be4674d1ddf8c469e6461a27a337eeb65f76c2e/src/sqlite/README.md),
[repository archive notice](https://github.com/modelcontextprotocol/servers-archived).

## Sequential Thinking

The official npm package `@modelcontextprotocol/server-sequential-thinking` 2.0.0 declares four runtime dependencies:
`zod`, `chalk`, `yargs`, and `@modelcontextprotocol/server`. Its executable points to `dist/index.js` and its Node
requirement is >=24. Exclude it under R253 because the original published package has runtime dependencies; do not
strip them, bundle them locally or silently invoke a package manager.

The project README states MIT licensing; registry metadata says `SEE LICENSE IN LICENSE`. The published archive's
license file and source-to-archive provenance were not inspected. Its documented local setup shows no API key or
account requirement, but no live invocation was made.

Sources: [version-specific npm metadata](https://registry.npmjs.org/@modelcontextprotocol%2fserver-sequential-thinking/2.0.0)
and [original README](https://github.com/modelcontextprotocol/servers/blob/51b6382a3e827aceb6bce430b1208d69b93d95ea/src/sequentialthinking/README.md).
The observed repository commit is a source-reading reference, not a verified provenance claim for npm 2.0.0.

## Limits and cost

No upstream archive or executable was installed or run. No credentials were accessed. Package digest, complete
published-file audit, MCP Registry identity, containment, startup/removal and session-kit acceptance remain unverified.
The kept browser server's separate runtime investigation remains unresolved; this report does not claim MCP success.
Earlier plugin exclusions and the catalog inventory are unchanged.

Documentation only: no app allocation, process, timer, memory or processor behavior changes.
