# Catalog

Faber's original frontend-craft and frontend-proof instruction skills are part of its existing first-party guide
delivery, not catalog entries or a new tool installation path. They add no cards, switches or screen layout; third-party
design skills keep their existing source, installation and selection rules ([contract](../product/frontend.md)).

A browsing and management tab beside Chat and Projects (R344). It extends the pinned skill/server store, moving its
controls out of Settings > Projects. It is not a work area: no conversation, composer, project instructions or AI picker.

## Browsing

Search is the main action, focused with the shared Ctrl+F (Command+F on macOS) from `ui/keys.ts`, also named in the
shortcut sheet. All, Skills, Tool servers and Installed are native pressed buttons; Plugins joins them only when the
catalog lists a plugin, so while it ships none no pill leads to an empty list. The catalog has 114 skills, one unchanged
server and zero plugins; the twenty rejected candidates have no placeholder cards. Good start offers debugging, simple code and interface design, plus general everyday skills.
Its ten recommended-tier entries include the original debugging, test-driven development, ponytail and interface
design entry points; area-specific tools and ponytail-debt stay under their job groups. Its picks move to the front before paging, so
recommendations outside the first source page are reachable and no item repeats on a later page. The ten plain groups
are code, interface design, planning, data, web services/payments, mobile apps, notes/research/writing, marketing,
building games and game design. Legacy job identifiers stay intact; browsing merges their narrower groups. Each item appears
once. Cards name the item, describe its job, name owner/repo and licence, and state whether it is installed/on/off or
needs Node. Search includes the name, job, description and repository. Empty searches and first-run installs have calm
plain words. Installed search matches personal/project skills, plugin records and reported servers as well as catalog
cards; its empty state waits for the full selected snapshot. Download failures keep a retry beside the error; browsing
uses the local catalog even offline. The sidebar keeps New conversation in place while Catalog is open, so tabs never
jump under the pointer. The command box includes Go to Catalog and calls the same navigation as the sidebar. Its running
action returns to the previous work area and shows the column, including when it was already open behind Catalog.
When removal empties the last Installed page, the list returns to its last remaining page instead of leaving a blank list.

Each card shows a drawing for its job (`ui/Glyphs.tsx`; plugins a puzzle piece, servers a globe) tinted with the accent while the item is on, and a dot before its state words. Beside the card, outside its button so the card stays one control: a select box (top right), the source page in the system browser, and Install or the on/off switch (bottom right), which save through the same store calls as the detail. The select box and the source link stay hidden at rest and show on hover, on keyboard focus, on every card once anything is selected, and always where there is no hover. A selection survives filters, search and pages; Select all shown adds or removes the visible cards. While anything is selected a bar floats at the bottom of the list with the count and only the actions that fit the selection (Install, Turn on, Turn off, Remove, each with its count), run one item after another with "Working on 2 of 5". Removing several asks first and says they can be installed again: Undo keeps only one single removal. Items that fail stay selected and the notice offers Try again. Escape leaves the question, then clears the selection.

Cards use the look's surface, ink, line and radius tokens. Hover changes surface and border only; press changes the same
properties, with no travel, scaling or blur. Reduced/no motion removes the transition. At 720 px cards become one column,
filters wrap, and long names, pins and paths wrap. The ordinary narrow sidebar remains a modal dialog with its existing
focus trap and Escape close. Heading focus enters the same-tab detail; Back and Escape return focus to the card.

## Detail and actions

Detail opens with a large drawing, the name, the job and the state, and Open the source page. "What you get" holds the item's description; the actions come next; "How it works in Faber" lists installing, when the agent uses it and what it can reach, each with its drawing. Detail names what the item does, when the agent can use it, access implications, source repository, licence and pinned
commit/package version. New items with prerequisites name them in the five locales, under usage and before Install;
these are requirements for use, not a second installer. When the pinned SKILL.md name differs from the card id,
one localized “Name in the AI tool” line shows it before installation. Repository and Node download links open through the existing checked system-browser command.
Skills guide the agent's existing file, command, network and browser powers; they create no new sandbox. The current
server controls a headless Chrome browser, with Faber's telemetry/update/file-navigation denies preserved.

A plugin is one checked item containing skills, slash-command prompts, subagent definitions and/or R253-portable
servers. Its card summarizes the parts and names delivery limits; its detail lists each part and its access. Commands
guide the project agent within its current permissions; subagents inherit those permissions; servers need Full access.
The Claude Code limit line appears only for a plugin that has a command or subagent, with Claude connected and its flags
already read. A plugin's parts have no intent words of their own: a task that matches the plugin's job loads all its skills
and servers together, within the same six-item budget as other items.
A bundled server requires Node before Install, like a standalone server. Plugins stay out of the Skills filter.
Install, On/Off, Remove and Undo apply to the whole plugin through the existing store, lock, ledger and journal;
parts never acquire independent installation rows or switches. Installed opens the parent plugin detail.

Projects receives checked commands and agents in Claude Code's existing private session plugin, alongside skills,
when the installed CLI declares its plugin flag. Otherwise the card and detail say that commands and agents are
unavailable and skills use the existing instruction fallback. Codex skills keep their existing delivery; subagents
use checked role TOML files through per-session agent configuration. Codex gets no plugin slash-command prompts,
which the card and detail say plainly. Faber never writes personal command/agent folders or CLI configuration.
Chat receives only task-picked catalog skill text, including plugin skills; plugin commands and agents stay in Projects.
Bundled servers use the existing project picker and saved preferences with namespaced identities. Turning the parent
off disables their project switches without replacing saved choices. Every plugin source folder and delivered part is
checked again before use; hooks, binaries, shipped scripts, postinstall and unsupported parts keep a candidate out.

Install checks the original source's pinned files and never executes shipped scripts. Install, Remove and installed
on/off are separate actions. Turning off retains checked files outside every CLI skill root. Remove asks in place,
with Cancel, then offers Undo. Undo restores the previous on/off state locally after a pin/digest check. The screen saves
the removed item and its on/off choice together only after successful removal; a failed removal of another item leaves
that Undo unchanged. Retry recognizes when recovery completed that exact restore, even during another store action or
session launch, and updates the screen. A small completed receipt remains until the next successful removal, Catalog
entry or engine restart; it retains no removed files and is checked against the restored copy before acknowledgement. An intent
record precedes the file move; interrupted restores complete before launch pruning or cleanup, and a failed ledger
save leaves the recoverable copy and intent in place. Only the latest manual removal is shown for Undo; opening Catalog or restarting
the engine expires unreachable copies after recovery. An agent removal uses this same Undo notice and retains its prior on/off choice; its receipt survives opening Catalog so the person can reach Undo, then expires at engine restart. Restoring one offers the next pending agent removal through that same notice, without reopening Catalog. Disabled copies are checked by name, ledger and folder type on
launch; their files are hashed when enabling. Future sessions in Chat and Projects follow the saved choice and existing
permission boundaries. The session kit selects verified installed instructions and servers for each task; Claude
Projects also discovers descriptions of all enabled catalog skills. Chat receives only selected catalog instructions,
never personal or external plugin skills; servers there need an explicit file-free, setup-free catalog review and Full access.
Existing conversation kits retain their checked files; these choices apply to new sessions.

Every shipped item has one hand-written `catalog.name.*` name in each of the five locales. Cards, details, installed
catalog rows, removal messages, conversation work logs and the context panel's list of a conversation's tools use that same source; CLI aliases remain a separate detail.
Codex Projects also receives a lazy index of all enabled skills, using their checked, shortened descriptions. Its 32 KiB
cap is separate from the 16 KiB selected kit, so discovery cannot crowd out servers or pins. Chat skips oversized
instructions and takes the next matching item that fits. Claude Projects names native plugin skills without outside
file reads when its CLI supports the plugin flag; older CLIs and helpers receive the lazy Read fallback. An unchanged
native resume, including warm-up, sends no repeated index.

An installed item with a failed source digest check stays listed with one localized reinstall line on its card, detail
and installed catalog row. It is unavailable to new sessions; healthy items remain available. The existing Install action
is offered as the explicit repair, never an automatic download. Install errors keep their retry; transient snapshot I/O
errors do not mark healthy source files as needing reinstall. Catalog returns its current cached status without waiting
for snapshot preparation. A `catalog` publication event refreshes mounted cards and installed catalog rows without polling;
the listener closes with the tab, including when registration completes after closure. These controls and status never
return to Settings.

An installed server's detail offers the project picker and existing per-project MCP preference. It reads/writes the same
projects.json field, reports each connected CLI's last known state, and never treats global availability as a second
project preference. A server must be installed and globally on for its project switch to be available. Off or Remove
shows that switch off and disabled while preserving both its saved project preference and last reported session state;
turning it on or undoing removal shows that same preference again.

## Installed sources and limits

Installed includes the catalog plus each CLI's known personal/project skill folders and plugin installation records,
with source and location. Personal/project installs are read-only. Faber never starts a CLI, opens skill bodies, follows
plugin install paths or reads broad home configuration. A linked personal skill appears by the link's own name and path,
marked Linked, not inspected, without resolving or opening its target. Project links and linked roots remain refused.
Display paths omit Windows verbatim prefixes; plugin rows say Plugin record instead of exposing a manifest filename.
Unknown external enablement is left unknown. Unsupported
sources/overrides and unsafe/unreadable records are stated. In particular, Codex plugin state cannot be inferred from
its cache. Project MCP names/statuses still come from the existing live-session snapshot rules (R334 through R337).

The installed Codex 0.162.1 protocol has plugin reports, but they are not passive: even a local-only `plugin/list`
can start catalog refresh tasks, while `plugin/installed` starts bundle synchronization. Opening Installed must not
update tools (R50), so Faber leaves this state explicitly unreported instead of calling either method or reading broad
home configuration. The screen explains why those plugins are absent. Evidence:
[versioned plugin request processor](https://github.com/openai/codex/blob/rust-v0.162.1/codex-rs/app-server/src/request_processors/plugins.rs#L733).

The tab and its resources unmount on close. There is no interval, background process, network on browse, kept card DOM
or per-frame work. Catalog pages draw at most 48 cards; inventory pages draw at most 48 rows per CLI. Discovery bounds
folder entries/depth and manifest bytes before building its snapshot. CPU work is finite filtering and one snapshot per
open/project/action. Filesystem commands run on blocking workers rather than the window's command thread. Inventory
returns an unavailable state within two seconds; one scanner owns its permit until it actually finishes, even after
timeout, so retries cannot accumulate stalled readers. Disk cost includes disabled items, manual and pending agent Undo copies and leased snapshots.
Metadata is capped at 36 KiB; stored job limits are 32 for building games, 11 for marketing, 23 for game design
and 14 for any remaining job, checked in engine tests. Only metadata and localized words ship. Actual working-set/CPU
measurements and the native screen suite belong to the lead's unsandboxed validation; no measured saving is claimed.

Checks: `tests/ui/catalog.check.ts`, `tests/ui/plugins.check.ts`, `engine/tests/plugins.rs`,
`engine/src/tools/plugins.rs`, `engine/src/tools/plugin_delivery.rs`, `tests/ui/kits.check.ts`, `engine/tests/locales.rs` (every shipped name in every locale),
`engine/src/engine/kits.rs` (native resume and oversize discovery), `engine/tests/tools.rs`, `engine/tests/tool_inventory.rs`, and existing MCP checks.
Cards checked in code review: C007 (localized UTF-8), C009 (shared context menu policy), C011 (management placement).
Catalog is not a C005 work home; no composer is added to satisfy that card. Visual screenshots remain to be verified.

## Cost statement for the pull request

No new process, background timer or persistent catalog DOM. While open, render at most 48 catalog cards and 48 inventory
rows per AI page; keep one bounded metadata snapshot and at most one inventory worker. A timed-out filesystem call may
retain that one worker until the OS returns, without polling or queuing retries. Closed, the tab releases its nodes and
listeners. Disabled tools are hashed on activation. Snapshot preparation and cleanup use one store worker; ready sessions
rehash the whole folders they may use before launch, outside the engine lock, including supporting files and server code.
One launch check runs at a time per engine, so simultaneous conversations cannot multiply its file buffers.
Transient preparation failures back off from one to sixty seconds; a catalog change allows an immediate retry. A snapshot shares
verified files through hardlinks where supported, with streamed-copy fallback and a 256 MiB aggregate cap. Leases keep old
states alive while needed; startup cleanup expires all Undo files; entry cleanup expires only manual Undo files from the previous visit. Native
working-set/processor measurements were not available in this sandbox, so no measured MB or CPU improvement is claimed.
Undo metadata is one item id and its prior on/off choice; a restore attempt adds a bounded receipt read before the
existing checks. Completed receipts add no per-session hashing or writes. No background work or process is added.

## Agent requests

Agent removals use the existing self-change card with separate removal consent. Its effect line says Undo remains in
Catalog until restart and current conversation tools stay available.

Agent-requested update and repair use that same card rather than another Catalog flow.
Each action has its own grant and shows the original and selected curated pins. The card states that local edits and
an earlier backup are replaced; the current files become the one retained backup outside all launch roots. It keeps
the tool's on/off state, and running conversations keep their leased kits. An incomplete journal is recovered before
new catalog preparation, while unknown or changed recovery files remain preserved and block new delivery.

Replacement hashes stream through a 64 KiB buffer, with the existing 64 MiB tool-file cap, and run during explicit
capture/publication and the preparation worker. Ready-cache reads gain no filesystem scan. No new timer, process or
persistent screen is added. Runtime memory and processor costs still require the queued commit's measurements.
