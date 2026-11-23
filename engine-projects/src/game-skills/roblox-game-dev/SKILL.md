---
name: roblox-game-dev
description: Develop Roblox games in any language. Use for concept, production, 3D art, playtests or release with Studio, Rojo, Blender or Figma.
---

# Roblox game development

Deliver what the requested path calls for, with an editable place or project when building. Respect existing style and
scope. Ask when the intended place, a costly direction, or an external action needs the person's decision. Choose reversible
details yourself. When the request spans stages, use the existing Projects plan to show decisions, dependencies and proof.

## Pick the path by what the person means

Match the meaning of the request in any language, not keywords. Combine paths in order when the request spans them. A short
idea ("a game about X") starts at **Roblox game concept**.

### Roblox game concept
1. Research first: read Roblox's [Prototyping](https://create.roblox.com/docs/production/game-design/prototyping) and
   [Core loops](https://create.roblox.com/docs/production/game-design/core-loops) and study comparable games. Use supplied
   screenshots, firsthand play, recordings or official pages. Label what was observed, what is inferred and what needs a
   test. Compare the first interaction, incentive and replay in two relevant references when available. Create original assets
   and rules, and treat popularity as context.
2. Brief: name the player, fantasy, repeated **action**, **choice**, visible **consequence** and reason to **replay**. Map the
   first session and one later change in challenge, expression or mastery. Show two directions if the core is open, choose
   one with a reason, and pause for the person's taste when it would change what gets built. A concept-only request ends in a
   brief and test design.
   Cues worth testing, not copying: guidance drawn on the ground, creatures or targets that stand out, a visible
   economy, a separate social lobby, a priced choice with its range, a stated objective, a confirmation before starting,
   a warning when the pace changes and a reward shown on screen. Test whether new players read orientation and consequence.
3. When building is requested, make a cheap paper or greybox experiment with one loop and a test that would make you change
   it. Observe a new player before scaling out final art, content or economy. Without a player, continue reversible technical
   work and leave the play hypothesis unconfirmed.

### Roblox game production
- Build one representative slice before scaling content. Specify the input, action, result, fallback and audio/visual/HUD
  feedback for the central interactions. Match the genre: a roleplay session may work through expression and social
  interaction, while a round-based game needs a clear ending and restart. Start with local assets.
- Art direction: define framing, palette, silhouettes, materials and a small list of hero assets; approve the look in one
  representative scene with the real HUD, lights, character and camera. Repeat the visual rules in the next area.
- Map: greybox the route, put objective or hazard at spawn, test scale against an avatar, then dress it. Review the real
  spawn view: lighting shows walkable surfaces, the first action is discoverable, landmarks separate routes. A room of
  identical targets or a goal only in a floating label needs another pass.
- Interface: original HUD for touch, gamepad and keyboard inside safe areas; color-independent and sound-independent
  feedback; readable text size. Figma frames are reference only: read layout, variants, type and spacing through the
  official Figma MCP or an authorized export, build `ScreenGui`, compare a Studio screenshot and test real input.
- Sound: list ambience, music, interaction SFX and warning cues by purpose. Check the mix in headphones or speakers and
  make critical information usable muted. Record source, license and assignment for each asset.
- Animation, VFX and feel: bind motion and effects to the actual contact, collect, reward or failure event. Inspect
  anticipation, timing, readability, and how the character recovers. Check the low-effects and reduced-motion paths when
  applicable. Keep effects clear of the aim, path and action controls.
- Economy and saves: server-owned currency and progress, test data separate from live data, clear sinks and starter amounts.
- Multiplayer: the server validates actions, distance, permissions and values; test join, leave and simultaneous actions.
- Performance: budget for the target phone and measure there; the importer's 20,000-triangle ceiling is not a goal.
- Label every claim with an evidence level: built or linted, ran in Studio, ran with two clients, ran on a real device,
  observed with new people, measured live.

### Roblox 3D art
Sketch the function and silhouette before modeling. Check frontal, side and three-quarter views beside an avatar, then judge
the object at play distance. Deliver editable `.blend` plus FBX or GLB, texture sources and an in-game-scale preview. Inspect
topology, triangle budget, UVs and applicable PBR maps (color, OpenGL normal, roughness, metalness). Check transforms, facing,
pivot and the simplest collision that works. If animated, finish the mesh before the rig and exercise extreme poses. Use Blender
in background mode with script
auto-execution off and a local script you can read. Import with upload off in the
[Studio importer](https://create.roblox.com/docs/studio/importer), then inspect size, pivot, material, hierarchy and
collision in a disposable place and render a preview. Verify and record license and source of third-party assets.

### Roblox playtest
- Technical proof (scripts, AI or emulator input, multiple clients, logs, device simulation) shows the build works. It
  never shows clarity or fun.
- Clarity and fun need new people. Give a concrete action ("play for five minutes") without guiding them. Record where
  they hesitate, what they think the goal is, whether they restart by choice and what they tell a friend.
- Exercise recovery, repeat play, access needs and the supported input methods. Test on real hardware before frame-rate or
  memory claims. Capture findings by action and device, fix the biggest friction first, then retest the same situation.

### Roblox launch and discovery
- Make a readiness list tied to the build and its authorized audience. Check rights, accessibility, moderation and the
  supported devices. If monetization is requested, show the benefit and current purchase price before confirmation. Test
  repeated and interrupted purchase receipts, and grant rewards durably on the server.
- Prepare an icon, thumbnails, trailer and ad drafts from actual gameplay. Check the promise at small size, the moment of
  play the materials show, localization and consistency with the first session. Plan a promotion hypothesis and measurement
  before spend. Keep publishing, uploading, ad campaigns and payments behind separate authority.
- After authorized publication, evaluate acquisition by source, the first meaningful action and retention by device using real
  analytics. Report missing data and sample size. A local test cannot measure reach or retention.

## Choose a source of truth

1. Inspect the workspace for the place, manifest, scripts and test commands, and use the existing setup. For a new game use
   the connected local Studio place if intended for this work, otherwise Rojo files.
2. With Studio MCP, pick the target by ID, file and mode. Before acting, call `list_roblox_skills` and load the guide
   for the task with `get_roblox_skill` (or `skill` when exposed). Choose a matching enabled Catalog skill or tool;
   use `roblox-dev-skill` for broader Luau and Studio reference. If disconnected, use files and official docs.
   Back up before editing, inspect scripts first, and persist changes in Edit mode.
3. With Rojo, keep mapped scripts in source files; live sync overwrites mapped items, so edit through the owner. Pin tools via
   Rokit. `rojo build -o Game.rbxl` makes a local place. If Rojo is missing, prepare the source and name the prerequisite.
   If a place uses Studio's [Script Sync](https://create.roblox.com/docs/ai/coding-harness), keep its folders and do not
   attach Rojo to them.

For setup prefer the [built-in Studio MCP](https://create.roblox.com/docs/studio/mcp), enabled by the person in Assistant ->
Manage MCP Servers. In a trusted project with Full access, run the current `StudioMCP.exe` as a stdio server: on Windows
resolve it beside Studio via `HKCU\Software\Roblox\RobloxStudio` `ContentFolder` (one directory above `content`), verify the
Roblox signature and re-resolve after updates; on macOS use the app-bundle path. Use the client's project config and a fresh
conversation. Claude: stdio entry in `.mcp.json` under `mcpServers`. Codex: `[mcp_servers.roblox-studio]` in
`.codex/config.toml`. Set `command` to the executable, keep `args` empty, preserve other entries, keep machine paths local and
configure the copy the conversation actually runs in. Faber's guarded Claude Projects modes ignore the project MCP file, so
continue through files. Never edit global client or account files. File tools: [Rojo/Rokit, StyLua, Selene, Wally,
Lune](https://create.roblox.com/docs/projects/external-tools); use only what the project needs.

## Verify the actual play path

Play from a clean spawn through the first action, reward or failure and restart; check output and record inputs. A file
build proves no gameplay. Check desktop and phone layouts in
[test modes](https://create.roblox.com/docs/studio/testing-modes), two clients for shared state and gamepad focus.

## Boundaries

Uploads, publishing, purchases, paid generation, sharing and sign-ins stay behind approval, even though live tools may have
cloud effects. Use a supplied Figma frame through its official MCP or authorized export and implement native `ScreenGui`.
