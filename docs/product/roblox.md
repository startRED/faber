# Roblox development

Choose **Pipelines** in Catalog or search for Roblox. Open a card, copy its starter request, and add the idea, references or
current build in Projects. A short idea starts with concept and a testable hypothesis. The original
[Roblox game development skill](../../engine-projects/src/game-skills/roblox-game-dev/SKILL.md) reaches the agent through Faber's
existing guide. Opening a card or copying its request uses no model, tool installation, project draft, or new permission.

## Skills and tools

Before using Studio, the agent lists the official skills provided by the connected Studio and loads the one that matches
the task. It uses enabled Catalog skills for specialist help or file-based work, including the optional community
Roblox development guide, world building and project tooling skills. Install them through the normal Catalog confirmation.
If Studio is unavailable, the agent
uses project files and official documentation where possible and reports which Studio checks it could not run.

## Five paths, chosen by what the request means

The skill carries five named paths. The agent picks by the meaning of the request in any language and chains them as needed.
For a full game, settle a playable concept before expanding the map and economy. Qualify the style in one slice, then use it
to guide larger runs of assets. Test with players before scaling content, and prepare promotion around the resulting game.

| Path | Starts from | Produces | Stops before |
| --- | --- | --- | --- |
| Roblox game concept | A short idea | Research notes and a hypothesis: repeated action, choice, visible consequence, reason to replay. With permission, a paper or greybox experiment | Final art, uploads, generators |
| Roblox game production | A concept or existing build | An editable slice with map, art direction, character, original interface and interaction design, rights-cleared sound and SFX, animation and VFX, scripts, economy and saves, multiplayer and performance as needed | Scaling unproven content, publishing, purchases |
| Roblox 3D art | A model the game needs | A plan for the artist: silhouette and scale in the real camera, `.blend` plus FBX or GLB, UVs and PBR maps, scale, pivot and collision, rig and poses as needed, import with upload off, inspection | Upload to Roblox |
| Roblox playtest | A playable build | Technical proof, observed clarity and fun, hardware measurements when available, fixes and retests | Claims the available people or devices cannot establish |
| Roblox launch and discovery | A candidate build, audience and goals | Readiness checklist, purchase checks, icon, thumbnails, a trailer brief and ad drafts, plus a measurement plan | Uploads, publishing, purchases and ad spend without separate approval |

The first path avoids spending on final art before anyone has been seen playing. Roblox's own
[Prototyping](https://create.roblox.com/docs/production/game-design/prototyping) page says prototypes should be fast and
cover only the aspects under test, and its [battle royale series](https://create.roblox.com/docs/education/battle-royale-series/project-setup)
warns that polishing art for an untested map can be wasted when the map must change.

### Technical proof versus people

Technical proof covers scripts running, AI or emulator input, several clients and logs. It shows the build works. Clarity and
fun are observed with new people: give a concrete action (“play for five minutes”), do not guide, and record where they hesitate,
what they believe the goal is, whether they restart by choice and what they say about it. One agent run, a screenshot or a
passing emulator test is never evidence of fun. The [hardware page](https://create.roblox.com/docs/performance-optimization/test-on-hardware)
states the emulator is useful for aspect ratio and controls but inaccurate for memory.

The skill records separate evidence for a build, Studio execution, multiple clients, real hardware, observed new players and
live analytics. The project keeps working on reversible fixes while a participant or target device is unavailable.

### Compare the process on the same brief

1. Use the same original brief, starting files, time and tool access for a plain harness run and a pipeline run. Keep the
   prompts, build, used tools and cost so a later run is reproducible. Compare different genres over time.
2. Give unlabeled builds to players who have not seen them. Vary which one they try first. Record the first action, what they
   thought the goal was, an actual choice and its consequence, and whether they choose to continue. Record device, sample
   size, constraints and technical failures. A reproducible comparison can surface design differences with a small group.
3. Compare play observations separately from visual craft, crashes, device performance and effort. Prioritize a recurring
   failure that changes what a player does. Test that edit with new people on the same task. Claims about how the process
   compares with other creators need representative comparisons. Retention and reach need actual post-launch observations.

## Pick the source of truth

- For a place maintained in Studio, confirm the intended window and place, use Edit mode for changes, and test a local copy.
  A playtest runs in a separate data model and changes made there are temporary. Record what the person can open again.
- For a file-based project, keep scripts and mapped objects in the repository. Rebuild with its pinned Rojo version and
  open the resulting local `.rbxl` file for playtesting. If using live Rojo sync, write mapped changes to the source files.
  Back up an existing place before adopting a mapping. If the project already uses Studio's
  [Script Sync](https://create.roblox.com/docs/ai/coding-harness), keep its linked script folders and Studio place and
  avoid syncing the same paths with Rojo. Keep handwritten and Studio-only assets clearly owned.
- The [Star Salvage example](../../demos/roblox/README.md) is a separate local place with an original short round, a pinned
  file build and a checklist for validating solo play, multiplayer and touch. It establishes a repeatable check for the
  file-based workflow. It is not a default game template or an evaluation of a live model's first attempt.

## Tool choices

| Tool | Use | Setup and limits |
| --- | --- | --- |
| [Roblox Studio MCP](https://create.roblox.com/docs/studio/mcp) | Inspect the game tree and scripts, edit Studio-owned objects, run a playtest, inspect output and capture the actual viewport. | Built into Studio. Enable inside Studio and configure the native AI client for the project. It needs a running Studio with the intended place available. Confirm actual tool names and capabilities before relying on them. |
| [Rojo and Rokit](https://create.roblox.com/docs/projects/external-tools) | Keep files editable and versioned, build a local place. | Pin the versions per project. Live sync additionally needs the Studio plugin. A built place only proves the mapping and artifact generation. |
| [StyLua, Selene and Wally](https://create.roblox.com/docs/projects/external-tools) | Format Luau, check Roblox globals, and manage packages when needed. | Reuse the repository configuration. StyLua and Selene catch source problems. Wally is useful when a package earns its place. |
| [Luau tools](https://luau.org/getting-started/) and [Lune](https://lune-org.github.io/docs/) | Run or check standalone Luau and pure game rules locally. | Roblox-specific services or player input still require Studio. Use a host-side test tool when there is logic independent of the engine. |
| [Blender](https://docs.blender.org/manual/en/latest/advanced/command_line/arguments.html) and [Studio 3D Importer](https://create.roblox.com/docs/studio/importer) | Make reusable low-poly models, render previews, export and inspect GLB/glTF or another supported format. | Prefer Blender's local background mode for repeatable assets. Keep source, export settings and rights. Validate the imported scale, pivot, material and collision in Studio. An already authorized live Blender tool can help with art direction. |
| [Official Figma MCP](https://developers.figma.com/docs/figma-mcp-server/) | Read reference frames, states, typography, spacing and exports for HUDs and menus. | The official remote endpoint uses native-client sign-in. Build with Roblox `ScreenGui` and input APIs, and check against the reference in Studio. Design exports do not prove Roblox input or layout. |

Faber's Catalog already has Roblox Luau, UI, networking, data stores, physics and characters, plus general game design and
performance skills. The Catalog's supported server installer uses verified npm packages. Studio MCP ships with Studio,
Figma's remote connector uses sign-in, and a live Blender add-on belongs to Blender. Those use their own existing setup
paths. A Blender bridge that executes arbitrary Python, uses paid generators or sends telemetry needs separate evaluation
before curation. Roblox's official [agent plugin](https://github.com/Roblox/roblox-studio-agent-plugin) can help native-client
setup, while Faber's own skill handles the prompt-to-playable workflow and the file fallback.
The [older standalone Roblox server](https://github.com/Roblox/studio-rust-mcp-server) remains a reference; start with the
built-in connector for a current Studio. A service such as [Thrixel](https://thrixel.com/learn/build-a-roblox-game) can
generate assets for a later art pass, subject to account access and per-project approval. It is outside the local starter path.

## Connect Studio to a dedicated project

1. Open an editable local place in Studio. In Assistant, use **Manage MCP Servers**, then enable Studio as an MCP server.
2. Use the [official client instructions](https://create.roblox.com/docs/studio/mcp) and keep the configuration in the game
   project when the client supports it. On Windows the signed `StudioMCP.exe` lives alongside `RobloxStudioBeta.exe` in the
   active version folder. It can speak stdio directly. The Studio installation's `ContentFolder` registry value identifies
   that folder's `content` subdirectory, so re-resolve the path if Studio updates. On macOS use the official `StudioMCP`
   binary in the app bundle. Never copy a command path belonging to a different machine.
3. Use a fresh Project conversation after setup. In Faber's guarded Claude Projects modes the project MCP file is filtered
   out. Use the file workflow in these modes. For live Studio changes, use an approved connection in a trusted project with
   Full access, then list Studio instances and check that the intended one is available and in the expected mode.

For a native Claude project use a `roblox-studio` entry under `mcpServers` in `.mcp.json` with `type: "stdio"`, `command`
set to that exact binary and `args: []`. For a trusted native Codex project use `[mcp_servers.roblox-studio]` in
`.codex/config.toml` with the same `command` and `args = []`. Antigravity supports project stdio servers via
`.agents/mcp_config.json`; when using it, qualify the live connection in that client. Merge with the existing project
entries. Leave machine-specific paths untracked. A separate Faber conversation copy does not carry the base folder's
untracked configuration. Configure the copy that will run the conversation, or use a dedicated project with **A separate
copy for each conversation** off. Keep native sign-in with its own client. For Figma, follow the
[remote setup](https://developers.figma.com/docs/figma-mcp-server/remote-server-installation/)
in the chosen client when an authorized frame is part of the work. Figma is optional for a first round.

## Prove play and quality separately

Start with one round from a clean join to reward or failure and a restart. Record the first meaningful action and its
time, a failure and recovery, and whether the next round starts cleanly. For multiplayer, run at least two clients and
check a simultaneous action or a player leaving. For the targeted devices, use Studio's
[test modes](https://create.roblox.com/docs/studio/testing-modes) for touch, controller focus and readable HUDs. Record the
actual device on any [performance measurement](https://create.roblox.com/docs/performance-optimization/test-on-hardware).
Check the console and measure frame time and memory before claiming a performance gain. Keep test and production saves
separate. Retain the place or source, concise test results and visible evidence where obtained.

The [video reference](https://www.youtube.com/watch?v=FnsIL4zPCQg) supplies five design hypotheses: recognizable fantasy,
purposeful first actions, meaningful play with friends, simple controls with depth, and moments that naturally make a good
clip. Use them to ask what a playtester can observe, and test those observations separately from a technically sound game.
For a launched experience, distinguish [acquisition](https://create.roblox.com/docs/production/analytics/acquisition) from
[onboarding](https://create.roblox.com/docs/production/game-design/onboarding) and
[retention](https://create.roblox.com/docs/production/analytics/retention). Instrument specific steps when needed and compare
by input platform. Roblox [funnel events](https://create.roblox.com/docs/production/analytics/funnel-events) require a
published experience. Local playtests cannot measure audience retention, discovery or concurrent-player outcomes.

## Research behind the paths

Sources were read on 2026-11-21 with the Keenable search and page tools. Roblox pages are primary sources; the game pages
below give only what their own description says.

| Topic | Source | What the skill takes from it |
| --- | --- | --- |
| Prototyping | [Prototyping](https://create.roblox.com/docs/production/game-design/prototyping) | Paper and Studio prototypes; test the core loop, UI, rules and edge cases; keep scope below a full feature |
| Core loops | [Core loops](https://create.roblox.com/docs/production/game-design/core-loops) | Minute-to-minute interaction, most repeated actions and a progression engine |
| Onboarding | [Onboarding](https://create.roblox.com/docs/production/game-design/onboarding) | Teach the essentials, get to the fun quickly, leave players wanting more |
| Artists | [Onboarding for artists](https://create.roblox.com/docs/onboarding/artists), [Mesh specifications](https://create.roblox.com/docs/art/modeling/specifications) | FBX export, PBR maps (color, OpenGL normal, roughness, metalness), simplest collision that works, at most 4 bones per vertex, MeshPart size up to 2,048 studs |
| Triangle limit | Same pages | A mesh cannot exceed 20,000 triangles. This is a ceiling, not a target; budget by what the target device sustains |
| Blender and import | [Importer](https://create.roblox.com/docs/studio/importer), [Blender command line](https://docs.blender.org/manual/en/latest/advanced/command_line/arguments.html) | The importer reads FBX, OBJ and glTF/GLB, and “Upload to Roblox” can be turned off to iterate locally |
| Hardware | [Test on hardware](https://create.roblox.com/docs/performance-optimization/test-on-hardware) | Real devices for memory, frame rate, thermal limits, input and latency; use MicroProfiler, Performance Stats and the Developer Console |
| Metrics | [Acquisition](https://create.roblox.com/docs/production/analytics/acquisition), [Retention](https://create.roblox.com/docs/production/analytics/retention) | Day 1 retention depends on core loop, first-time experience and performance |

### Official cases, observations and hypotheses

Five games serve as study cases for orientation and consequence. The facts column is each game's own page description; the
observations column is what the product owner saw in their screenshots; the last column is a hypothesis a playtest can check.
None of this claims why a game became popular, confirms any player figure (including a reported 300,000), or licenses copying
an interface, identity or number.

| Case | Page description says | Owner's observation | Hypothesis to test |
| --- | --- | --- | --- |
| [Grow a Garden](https://www.roblox.com/games/126884695634066/Grow-a-Garden) | Buy seeds, plant, wait, harvest for profit; the garden grows while offline | Not recorded | A short wait with a visible return is clear to a first-time player |
| [Brookhaven](https://www.roblox.com/games/4924922222/Brookhaven-RP) | Roleplay: houses, cars, city, friends | Not recorded | Open-ended play needs few rules but clear places to meet |
| [DOORS](https://www.roblox.com/games/6516141723/DOORS) | Horror about doors; enter without a guide and treat each death as a lesson | Not recorded | Learning by failing works when each death shows what happened |
| [Throw Food For Animals](https://www.roblox.com/games/120219252362013) | Throw food far, attract and collect animals, earn money, buy better food, train throw power | Direction shown on the floor, animals highlighted, economy visible | Players grasp throw, collect, improve range without text |
| [Tower Defense Simulator](https://www.roblox.com/games/3260590327/Tower-Defense-Simulator) | Place units to defend against zombies and team up for bosses | Separate social lobby, tower choice with price and range, objective as life, wave and time, confirmation to begin, notice at the second wave, reward money on screen | Players understand what a tower costs and covers before placing it, and what changes at wave two |

Page counts such as active players or visits move every minute and were not recorded as evidence.

### Further production and launch references

- Production: [environment art](https://create.roblox.com/docs/tutorials/curriculums/environmental-art/develop-polished-assets),
  [cross-platform controls](https://create.roblox.com/docs/projects/cross-platform),
  [accessibility](https://create.roblox.com/docs/production/publishing/accessibility),
  [animation editor](https://create.roblox.com/docs/animation/editor),
  [particle emitters](https://create.roblox.com/docs/effects/particle-emitters), and [audio](https://create.roblox.com/docs/audio).
- Release: [developer products](https://create.roblox.com/docs/production/monetization/developer-products),
  [regional pricing](https://create.roblox.com/docs/production/monetization/regional-pricing),
  [thumbnails and videos](https://create.roblox.com/docs/production/publishing/thumbnails),
  [experience icons](https://create.roblox.com/docs/production/publishing/experience-icons), and
  [acquisition by source](https://create.roblox.com/docs/production/analytics/acquisition). Draft promotion from real gameplay,
  review the message at small sizes and use [Ads Manager](https://create.roblox.com/docs/production/promotion/ads-manager)
  only within approved campaign and spending limits.
