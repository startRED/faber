# Context7 catalog preflight

Status: not admitted. Source-manifest preflight only; no package was installed or executed, and no catalog entry changed.
This is evidence for task 995, not a completed whole-package audit or runtime proof.

## Examined source

The original `@upstash/context7-mcp` 4.4.4 manifest at commit
`c7f80ce5af1ff1403d167579b76784482cf7f3e2` declares MIT licensing and thirteen runtime dependencies. Its npm entry
points to compiled JavaScript in `dist`; the normal build compiles TypeScript without bundling those dependencies.
The separate MCPB build bundles a different output, so that script does not establish that the published npm entry is
self-contained. The manifest lists no install lifecycle script.

Sources: [original package manifest](https://github.com/upstash/context7/blob/c7f80ce5af1ff1403d167579b76784482cf7f3e2/packages/mcp/package.json)
and [npm version metadata](https://registry.npmjs.org/@upstash%2Fcontext7-mcp/4.4.4). The registry metadata independently
confirms version, license, thirteen runtime dependencies, the entry point and no install lifecycle script. No tarball
was downloaded.

## Decision under the existing installer

R253 accepts checked, dependency-free npm servers; it does not run a package manager to resolve a dependency graph.
This candidate fails that existing delivery requirement. Faber must not silently install those dependencies, rebuild
the upstream package, or substitute the MCPB output as though it were the pinned npm artifact.

The [original README](https://github.com/upstash/context7/blob/c7f80ce5af1ff1403d167579b76784482cf7f3e2/packages/mcp/README.md)
documents basic use without a required API key. A remote documentation service alone is not a rejection under task
995's requirements; Faber holding a paid key or account would be. Anonymous service availability was not exercised.
The [telemetry configuration](https://github.com/upstash/context7/blob/c7f80ce5af1ff1403d167579b76784482cf7f3e2/packages/mcp/src/lib/telemetry-config.ts)
provides an OpenTelemetry hard-off switch. That possible safeguard does not resolve the dependency blocker.

## Scope and cost

This corrects the reason recorded for withholding the candidate: the npm delivery shape is a concrete blocker, rather
than the remote backend by itself. It changes no earlier plugin rejection and grants no new permission. No tarball
digest, published-file audit, containment proof, live tool call or session-kit acceptance is claimed. Reconsideration
needs a compatible original artifact and its complete existing admission checks.

Documentation only: no app allocation, process, timer, memory or processor behavior changes.
