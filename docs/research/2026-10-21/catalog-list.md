# Catalog list, step 2

Status: review corrections applied locally on catalog-list after 04801ffd; no new commit or native screen run.

## Selection

The catalog now has 114 skills (21 original plus 93 new), one unchanged server and no installable plugins.
The two full reviews were read and every row is resolved below. The first selection had 132 new skills; the systematic
re-audit removed 39, in addition to the original 206 exclusions from 338 candidates. No previously rejected item was revived.

Good start is for newcomers to a coding-agent app: debugging, simple code and interface design, plus general everyday
work. Its ten recommendations are systematic-debugging, test-driven-development, ponytail, design-taste-frontend,
improve-codebase-architecture, grill-me, skill-creator, emil-design-eng, break-ui and humanizer; animate repeats emil-design-eng and animation-performance is a narrow fix, so both are more.
All marketing, product-management, engine/framework-specific entries and ponytail-debt are more. A setup research verdict
meant setup for a chosen area; it never justified recommending that area to everyone.

## Systematic pinned-folder re-audit

Fetched each of the 132 new skills as its **whole folder at its catalog commit**, including references, scripts, assets,
evaluations and root dotfiles (humanizer has an empty repository path). The 686 files all matched the existing SHA-256
digests, using the store format: sorted relative path, NUL, byte length, NUL, complete file bytes. No upstream code ran.
Search covered all files for package/tool installation and execution, pip/npm/npx/cargo/brew/apt/uv/pub/dotnet commands,
curl/wget pipelines, downloaded executables, LATEST/self-updates, skill/MCP/plugin setup, all URLs and mutable branch/raw/latest
instruction references. Context review distinguished operational instructions from citations, test fixtures, API names,
existing-tool checks and release-author instructions. A second broad install/download/fetch scan caught additional
Godot C#, Unreal packaging and public-relations instructions missed by the first reviewers.

| Pinned repository | Audited folders | Files | Retained |
| :--- | ---: | ---: | ---: |
| [android/skills](https://github.com/android/skills/tree/a3f56b8fc83039c407e20a7b4789c2b6ea192d57) | 3 | 17 | 2 |
| [emilkowalski/skills](https://github.com/emilkowalski/skills/tree/2d228630ba1446365f614a108cb8252f1feda202) | 2 | 4 | 2 |
| [gamedev-skills/awesome-gamedev-agent-skills](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd) | 61 | 138 | 52 |
| [anthropics/skills](https://github.com/anthropics/skills/tree/dbd4588f9e1033efb41dad4bef2f7947c8993d44) | 1 | 83 | 0 |
| [coreyhaines31/marketingskills](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b) | 34 | 301 | 11 |
| [phuryn/pm-skills](https://github.com/phuryn/pm-skills/tree/07b34f64ee828df08953f505b7f747aef85ae7c6) | 15 | 15 | 15 |
| [cloudflare/skills](https://github.com/cloudflare/skills/tree/86505e44f87d801aa1864a3c4109e9a8ae5c409f) | 1 | 4 | 0 |
| [flutter/skills](https://github.com/flutter/skills/tree/6e590650bb4bab87f33187423dc10113d04d4836) | 3 | 3 | 1 |
| [greensock/gsap-skills](https://github.com/greensock/gsap-skills/tree/aed9cfd3277740755f6bfc1155c7aa645403b760) | 1 | 1 | 1 |
| [blader/humanizer](https://github.com/blader/humanizer/tree/8c307f367c91bd974002f10f2be2965d642c13df) | 1 | 14 | 1 |
| [kepano/obsidian-skills](https://github.com/kepano/obsidian-skills/tree/3ccff5338ea700537839b21900aa5358a0402c98) | 3 | 8 | 3 |
| [Jahrome907/minecraft-agent-skills](https://github.com/Jahrome907/minecraft-agent-skills/tree/30137f93fdbd4446910ceaa8776d561fd291aed1) | 2 | 8 | 2 |
| [DietrichGebert/ponytail](https://github.com/DietrichGebert/ponytail/tree/09c434ab73ca43761c5d8b0c5d341926c2fba710) | 1 | 1 | 1 |
| [Weizhena/Deep-Research-skills](https://github.com/Weizhena/Deep-Research-skills/tree/6ce38f60e3f8b22502c29873f96503a4e0c5addb) | 1 | 1 | 0 |
| [AvdLee/SwiftUI-Agent-Skill](https://github.com/AvdLee/SwiftUI-Agent-Skill/tree/204dba7c67256a8b707ee3fbc5759a824191974f) | 1 | 51 | 1 |
| [majidmanzarpour/threejs-game-skills](https://github.com/majidmanzarpour/threejs-game-skills/tree/8286774b22a2566bf894dbc825825c16921866af) | 1 | 33 | 0 |
| [quodsoler/unreal-engine-skills](https://github.com/quodsoler/unreal-engine-skills/tree/f3742d7b688690810df369802b90430324e380b9) | 1 | 4 | 1 |

### Additional exclusions (39)

Every removal below keeps its original pin as evidence; no skill was rewritten or repinned to hide an unsafe dependency.
A conditional supported workflow counts too: CRO routing, compressed model decoding and optional publishing cannot
fetch instructions or executable dependencies outside the checked folder. Versioned CDN code is still executable code
not already present on the person’s machine.

| Skill | Reason | Evidence at the original pin |
| :--- | :--- | :--- |
| `cro` | Uses the inbound-routing playbook on mutable main. | [`SKILL.md:196`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/cro) |
| `marketing-ads` | Routes implementation to the tools registry and integration guides on main. | [`SKILL.md:456-465`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/ads) |
| `marketing-analytics` | Routes analytics setup to mutable main integration guides; also overlaps the retained PM metrics workflow. | [`SKILL.md:349-357`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/analytics) |
| `marketing-churn-prevention` | Sends experiment setup and implementation to mutable main guides. | [`SKILL.md:373,394`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/churn-prevention) |
| `marketing-co-marketing` | Routes implementation to mutable main partner integration guides. | [`SKILL.md:295-301`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/co-marketing) |
| `marketing-cold-email` | Routes setup to the mutable main outbound registry and integrations. | [`SKILL.md:191-199`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/cold-email) |
| `marketing-content-strategy` | Routes CMS implementation to integration instructions on main. | [`references/headless-cms.md:192-194`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/content-strategy) |
| `marketing-conversion-tracking` | Routes CLI/MCP implementation to a mutable main integration guide. | [`references/server-side.md:95,165`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/conversion-tracking) |
| `marketing-emails` | Routes implementation to mutable main email provider guides. | [`SKILL.md:321-330`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/emails) |
| `marketing-influencer-marketing` | Routes implementation to the mutable main registry and integration guide. | [`SKILL.md:190-194`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/influencer-marketing) |
| `marketing-launch` | Uses a partner integration guide on mutable main. | [`SKILL.md:96`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/launch) |
| `marketing-marketing-loops` | Sends the agent to an operational SEO playbook on mutable main. | [`references/loop-catalog.md:61; references/seo-operator.md:88`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/marketing-loops) |
| `marketing-marketing-plan` | Installs agent-browser globally and adds an MCP server. | [`references/ops-stack-mapping.md:103-107`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/marketing-plan) |
| `marketing-programmatic-seo` | Routes implementation-platform selection to the mutable main partner registry. | [`references/implementation-platforms.md:34`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/programmatic-seo) |
| `marketing-prospecting` | Routes implementation and setup to mutable main integration guides. | [`SKILL.md:246-265; references/signal-plays.md:91`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/prospecting) |
| `marketing-referrals` | Routes implementation to mutable main referral integration guides. | [`SKILL.md:264-274`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/referrals) |
| `marketing-revops` | Routes implementation and enrichment to mutable main guides. | [`SKILL.md:325-338; references/inbound-routing.md:9`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/revops) |
| `marketing-sales-enablement` | Routes partner enablement to mutable main registry/integration instructions. | [`SKILL.md:410-414`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/sales-enablement) |
| `marketing-sms` | Routes implementation to mutable main SMS integration guides. | [`SKILL.md:328-339`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/sms) |
| `marketing-social` | Installs jq/libxml2-utils and routes publishing to mutable main integration guides. | [`references/listening.md:115-116; SKILL.md:333`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/social) |
| `marketing-directory-submissions` | Runs brew install mcp-publisher before initializing, signing in and publishing. | [`references/directory-list.md:186-188`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/directory-submissions) |
| `marketing-public-relations` | Requires the listening workflow tool installation: brew install jq. | [`references/newsjacking.md:147`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/public-relations) |
| `marketing-customer-research` | Duplicates customer interview/research jobs owned by pm-user-personas, pm-interview-script and pm-summarize-interview. | [`SKILL.md:3`](https://github.com/coreyhaines31/marketingskills/tree/e85f3cbc2e4a79bf33b2a12e9bdd9087d414735b/skills/customer-research) |
| `adaptive` | Instructs installing another skill with android skills add testing-setup. | [`references/android/studio/preview/compose-screenshot-testing-with-testsuites.md:23-25`](https://github.com/android/skills/tree/a3f56b8fc83039c407e20a7b4789c2b6ea192d57/jetpack-compose/adaptive) |
| `canvas-design` | Directs downloading and using arbitrary additional font binaries outside the checked folder. | [`SKILL.md:110`](https://github.com/anthropics/skills/tree/dbd4588f9e1033efb41dad4bef2f7947c8993d44/skills/canvas-design) |
| `durable-objects` | Fetches current external setup/API instructions and routes compatible package installation through them. | [`references/testing.md:5-15`](https://github.com/cloudflare/skills/tree/86505e44f87d801aa1864a3c4109e9a8ae5c409f/skills/durable-objects) |
| `flutter-setup-declarative-routing` | Installs go_router with flutter pub add. | [`SKILL.md:32-42`](https://github.com/flutter/skills/tree/6e590650bb4bab87f33187423dc10113d04d4836/skills/flutter-setup-declarative-routing) |
| `flutter-setup-localization` | Adds localization packages with flutter pub add. | [`SKILL.md:24-35`](https://github.com/flutter/skills/tree/6e590650bb4bab87f33187423dc10113d04d4836/skills/flutter-setup-localization) |
| `research-report` | Needs outline.yaml, fields.yaml and JSON output from absent research entry points and calls a client-specific question tool; not a standalone research entry point. | [`SKILL.md:13-37,86-91`](https://github.com/Weizhena/Deep-Research-skills/tree/6ce38f60e3f8b22502c29873f96503a4e0c5addb/skills/research-codex-en/research-report) |
| `game-asset-creation` | Installs the scripts requirements with pip when Pillow is absent. | [`SKILL.md:96-98`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/disciplines/create-game-assets) |
| `pygame-development` | Core setup installs pygame-ce with pip. | [`SKILL.md:31-32,170-171`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/other-engines/pygame-core) |
| `itch-publish` | Downloads and executes butler through LATEST and offers self-upgrade. | [`SKILL.md:34-36; references/butler-ci.md:23-32,93`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/workflows/itch-publish) |
| `steam-publish` | Downloads Steamworks SDK and bootstraps a self-updating steamcmd. | [`SKILL.md:41-42,66-67,133-135`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/workflows/steam-publish) |
| `threejs-gameplay-systems` | Scaffolder prescribes npm install; physics reference installs Rapier. | [`scripts/create_threejs_game.py:73; references/physics-engine-selection.md:21`](https://github.com/majidmanzarpour/threejs-game-skills/tree/8286774b22a2566bf894dbc825825c16921866af/skills/threejs-gameplay-systems) |
| `godot-export` | Core workflow downloads/installs matching export templates. | [`SKILL.md:27-29`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/godot/godot-export) |
| `godot-csharp` | Core workflow instructs installing the matching .NET SDK. | [`SKILL.md:27; references/csharp-setup-and-interop.md:8`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/godot/godot-csharp) |
| `unreal-packaging` | Troubleshooting instructs installing platform prerequisites. | [`SKILL.md:91-92`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/unreal/unreal-packaging) |
| `threejs-scene-setup` | The no-bundler core workflow loads and runs Three.js modules from a CDN instead of an existing local dependency. | [`SKILL.md:50-63`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/web-engines/threejs-scene-setup) |
| `threejs-gltf-loading` | The compressed-model workflow downloads and runs decoder JS/WASM from a CDN; a version pin does not make those executables preinstalled. | [`SKILL.md:113-125`](https://github.com/gamedev-skills/awesome-gamedev-agent-skills/tree/0a70cfc64672d512c5b5b6006b5dcb5cf90dbdbd/skills/web-engines/threejs-gltf-loading) |

### Retained boundary cases and prerequisites

- humanizer: README, CI and root AGENTS describe installation/publishing, but its prose-editing SKILL needs none of them.
- minecraft-modding: check-build.sh checks existing Java, prints an error and exits on missing/old Java; it never installs
  or downloads the JDK. This is the user’s permitted existing-tool prerequisite. Its needs line now includes Java 17/21/25.
- swiftui-expert-skill: RocketSim is optional with human confirmation/Xcode Previews fallbacks. Existing Mac/Xcode and
  Python 3 for trace operations are named. No installer is required for the core workflow.
- android-permissions-security: optional “learn more” sample links are bibliography, not a required mutable instruction
  source. adb installs the app under test, not a new agent prerequisite. Existing Android tools are named.
- animate: routing to pick-ui-library is explicitly for a component task **rather than animation**, outside its own job.
  Its animation workflow is self-contained with native CSS/WAAPI or an existing library; no library installer is prescribed.
- Ordinary reference links and live research data are not remote instructions. Local latest-apis.md and API fields named
  latest are inside the checked pin, not mutable dependencies. No excluded installation path was moved into needs text.

Prerequisites were checked against all retained folders and corrected individually in five hand-written locales.
Headless testing needs Godot’s executable, no testing addon. Godot targets 4.7; Unity C# uses the Editor, without a
standalone .NET SDK prerequisite; Unity Input System must already be enabled. LÖVE includes LuaJIT. Phaser, PixiJS and
Three.js need existing libraries, not Node/npm by definition. Design skills name an engine only for implementation.
Android intent checks name API 23+/AndroidX Core 1.9+. Raw A/B calculations need Python 3; drafting SQL needs schema and
dialect, not query access, and makes no read-only promise. Dashboard planning needs metric/source descriptions.

The retained new ids (93), all audited above:

`animate`, `break-ui`, `gsap-core`, `json-canvas`, `obsidian-bases`, `obsidian-markdown`, `ponytail-debt`, `humanizer`
`copywriting`, `copy-editing`, `pricing`, `create-prd`, `market-sizing`, `godot-gdscript`, `unity-csharp-scripting`, `roblox-luau`
`ue-cpp-foundations`, `minecraft-modding`, `minecraft-server-admin`, `swiftui-expert-skill`, `marketing-ab-testing`, `marketing-aso`, `marketing-community-marketing`, `marketing-competitors`
`marketing-events`, `marketing-free-tools`, `marketing-lead-magnets`, `marketing-seo-audit`, `pm-pre-mortem`, `pm-release-notes`, `pm-summarize-meeting`, `pm-ab-test-analysis`
`pm-cohort-analysis`, `pm-sql-queries`, `pm-user-personas`, `pm-interview-script`, `pm-metrics-dashboard`, `pm-prioritize-features`, `pm-summarize-interview`, `pm-business-model`
`pm-product-strategy`, `game-audio-design`, `game-camera-systems`, `game-dialogue-systems`, `game-ai-systems`, `game-feel`, `game-ui-ux`, `game-input-systems`
`game-level-design`, `game-performance-optimization`, `game-physics-tuning`, `game-procedural-generation`, `game-save-systems`, `game-shader-programming`, `game-card-design`, `game-fps-shooter`
`game-platformer-design`, `game-puzzle-design`, `game-roguelike-design`, `game-rpg-design`, `game-survival-crafting`, `game-tower-defense-design`, `game-visual-novel-design`, `godot-2d-movement`
`godot-3d-essentials`, `godot-animation`, `godot-audio`, `godot-gdscript-headless-testing`, `godot-multiplayer`, `godot-physics`, `godot-shaders`, `godot-tilemap`
`godot-ui-control`, `bevy-ecs`, `love2d-game-development`, `roblox-characters`, `roblox-datastores`, `roblox-networking`, `roblox-physics`, `roblox-ui`
`unity-animation`, `unity-build-pipeline`, `unity-input-system`, `unity-scriptableobjects`, `unreal-blueprints`, `phaser-arcade-physics`, `phaser-core`, `pixijs-rendering`
`game-prototype-fast`, `threejs-materials-lighting`, `android-intent-security`, `android-permissions-security`, `flutter-apply-architecture-best-practices`

## Review resolutions and delivery

Both fresh reviews are superseded by this whole-folder audit, not sampling. First-review rows: unsafe installers and
mutable instructions removed across the whole new list; headless requirements fixed in all five locales. Second-review
rows 1–2: real tiers restore Good start and checks assert its exact membership. Rows 3, 4 and 6: mutable marketing,
incomplete research-report and game-asset installation paths removed. Row 5: keep the PM research/metrics side and drop
marketing-customer-research/analytics. Row 7: competitor pages, language-neutral humanizer, SQL description/requirements
corrected; analytics removed, including its inaccurate text. Row 8: details show the pinned CLI name for 43 mismatches,
with one localized label; map and design docs agree. Row 9: 04801ffd is not amended because the user explicitly requested
no commit; the cost statement below is ready for the lead’s eventual commit body.

Do not add mcp-debugger: its pinned bundle accepts program paths outside project containment. Keep all twenty plugin
candidates out until checked portable delivery exists. No prerequisite installer, access, enablement, delivery or selection
behaviour changed. Skills remain original pinned folders from their original sources.

## Original candidate exclusions (206)

These exclusions preceded the re-audit. Coverage names in their original rationale describe that first comparison;
where the covering workflow is also excluded above, neither item is revived. The current retained set is listed above.

### Instructs installing additional animation packages; excluded from this skill-only step (1)

`animate-expo`.

### Narrow variant covered by the retained design, research or notes entry point (7)

`animation-vocabulary`, `ask-sonner`, `mobile-native`, `pick-ui-library`, `prototype`, `review-animations`, `write-swift`.

### GSAP sub-feature covered by the retained gsap-core entry and existing animation-performance (7)

`gsap-frameworks`, `gsap-performance`, `gsap-plugins`, `gsap-react`, `gsap-scrolltrigger`, `gsap-timeline`, `gsap-utils`.

### Command-specific notes helper; retain note formats and Bases without another command interface (1)

`obsidian-cli`.

### Research pipeline helper; retain research-report as the user-facing entry (2)

`research-add-items`, `research-add-fields`.

### Duplicate slice of marketing-ads (1)

`marketing-ad-creative`.

### Duplicate slice of marketing-analytics (1)

`marketing-attribution`.

### Duplicate slice of marketing-competitors (1)

`marketing-competitor-profiling`.

### Duplicate slice of canvas-design (1)

`marketing-image`.

### Duplicate slice of marketing-seo-audit (3)

`marketing-link-building`, `marketing-schema`, `marketing-site-architecture`.

### Duplicate slice of copywriting (2)

`marketing-offers`, `marketing-psychology`.

### Duplicate slice of cro (4)

`marketing-onboarding`, `marketing-paywalls`, `marketing-popups`, `marketing-signup`.

### Duplicate slice of marketing-marketing-plan (2)

`marketing-product-marketing`, `marketing-ideas`.

### Narrow planning variant covered by the retained product workflow; no separate job (44)

`pm-brainstorm-okrs`, `pm-outcome-roadmap`, `pm-prioritization-frameworks`, `pm-retrospective`, `pm-sprint-plan`, `pm-stakeholder-map`, `pm-strategy-red-team`, `pm-test-scenarios`, `pm-user-stories`, `pm-beachhead-segment`, `pm-growth-loops`, `pm-gtm-motions`, `pm-gtm-strategy`, `pm-ideal-customer-profile`, `pm-customer-journey-map`, `pm-market-segments`, `pm-sentiment-analysis`, `pm-user-segmentation`, `pm-north-star-metric`, `pm-positioning-ideas`, `pm-product-name`, `pm-feature-request-analysis`, `pm-experiments-existing`, `pm-experiments-new`, `pm-assumptions-existing`, `pm-assumptions-new`, `pm-opportunity-solution-tree`, `pm-prioritize-assumptions`, `pm-ansoff-matrix`, `pm-monetization-strategy`, `pm-pestle-analysis`, `pm-porters-five-forces`, `pm-product-vision`, `pm-swot-analysis`, `pm-value-proposition`, `pm-intended-vs-implemented`, `pm-shipping-artifacts`, `pm-dummy-dataset`, `pm-ideas-existing`, `pm-ideas-new`, `pm-grammar-check`, `pm-skills-pm-toolkit-skills-draft-nda`, `pm-skills-pm-toolkit-skills-privacy-policy`, `pm-skills-pm-toolkit-skills-review-resume`.

### Narrow engine recipe covered by the retained engine or game-system entry points (45)

`minecraft-minecraft-ci-release`, `minecraft-minecraft-commands-scripting`, `minecraft-minecraft-datapack`, `minecraft-minecraft-essentials-ops`, `minecraft-minecraft-imagegen`, `minecraft-minecraft-multiloader`, `minecraft-minecraft-plugin-dev`, `minecraft-minecraft-resource-pack`, `minecraft-minecraft-testing`, `minecraft-minecraft-world-generation`, `minecraft-minecraft-worldedit-ops`, `threejs-aaa-graphics-builder`, `threejs-debug-profiler`, `threejs-game-ui-designer`, `threejs-qa-release`, `ue-actor-component-architecture`, `ue-ai-navigation`, `ue-animation-system`, `ue-async-threading`, `ue-audio-system`, `ue-blueprint-cpp-interop`, `ue-character-movement`, `ue-data-assets-tables`, `ue-editor-tools`, `ue-game-features`, `ue-gameplay-abilities`, `ue-gameplay-cameras`, `ue-gameplay-framework`, `ue-gameplay-tags-messaging`, `ue-input-system`, `ue-mass-entity`, `ue-materials-rendering`, `ue-module-build-system`, `ue-mover`, `ue-networking-replication`, `ue-niagara-effects`, `ue-physics-collision`, `ue-procedural-generation`, `ue-project-context`, `ue-sequencer-cinematics`, `ue-serialization-savegames`, `ue-state-trees`, `ue-testing-debugging`, `ue-ui-umg-slate`, `ue-world-level-streaming`.

### Duplicate slice of game-ai-systems (1)

`game-ai-behavior-trees`.

### Duplicate slice of godot-gdscript (3)

`godot-nodes-scenes`, `godot-resources`, `godot-signals-groups`.

### Duplicate slice of roblox-luau (1)

`roblox-studio-workflow`.

### Duplicate slice of game-prototype-fast (1)

`game-jam-workflow`.

### Specialized platform service or enterprise-network recipe; defer beyond Workers basics and Durable Objects (10)

`workers-agent-sdk`, `basin-analytics`, `zero-trust-migrations`, `zero-trust-architecture`, `gateway-middleware-worker`, `k2-streams`, `sandbox-sdk-next`, `sandbox-sdk-stable`, `web-performance-audit`, `worker-profiling`.

### External generation services and runtime dependency installation need a separate checked integration (3)

`threejs-game-skills-skills-threejs-3d-generator`, `threejs-game-skills-skills-threejs-audio-generator`, `threejs-game-skills-skills-threejs-image-generator`.

### Narrow platform recipe; retain architecture, navigation, localization and security entry points (44)

`verified-email`, `testing-setup`, `play-billing-library-version-upgrade`, `litert-runtime`, `wear-compose-m3`, `engage-sdk-integration`, `media3-cast-integration`, `display-glasses-with-jetpack-compose-glimmer`, `appfunctions`, `play-policy-insights`, `migrate-xml-views-to-jetpack-compose`, `edge-to-edge`, `leanback-to-compose-tv-migration`, `restore-credentials`, `styles`, `navigation-3`, `agp-9-upgrade`, `camerax`, `glance-canonical-layouts`, `navigation-event`, `android-for-cars-parked-apps`, `wear-widgets`, `ml-kit-genai-prompt-api`, `flutter-implement-json-serialization`, `flutter-use-http-package`, `flutter-add-integration-test`, `dart-fix-runtime-errors`, `flutter-add-widget-preview`, `dart-use-pattern-matching`, `dart-use-doc-examples`, `dart-build-cli-app`, `dart-use-ffigen`, `dart-setup-ffi-assets`, `dart-run-static-analysis`, `dart-migrate-to-checks-package`, `dart-use-primary-constructors`, `dart-resolve-package-conflicts`, `flutter-fix-layout-issues`, `dart-write-documentation`, `dart-collect-coverage`, `dart-add-unit-test`, `dart-use-path-package`, `flutter-build-responsive-layout`, `dart-generate-test-mocks`.

### Unity Companion License restricts use; conditional engine-only terms stay out of this general skill store (21)

`localization`, `asset-transformer-toolkit`, `setup-vivox-voice-chat`, `create-tile-palette-or-rule-tiles`, `initialize-ai-navigation`, `ui-uitk`, `ui`, `sprite-segment-3x3grid`, `optimize-text-mesh-pro`, `build-gtk`, `ui-ugui`, `manage-sprite-atlas`, `levelplay-unity-integration`, `setup-multiplayer-services`, `physics-3d-collision`, `implement-in-app-purchases`, `2d-pixel-perfect`, `build-live-game`, `shader-graph-create-custom-node`, `validate-urp-render-graph-renderer-feature`, `ui-imgui`.

## Budgets, cost and validation

The catalog metadata is 34,992 bytes under a tightened 36 KiB budget. Tests fix the count at 114 skills and one server;
stored-job budgets are 32 engine games, 23 game design, 11 marketing and 14 for other jobs. All original source pins and
digests are unchanged. Metadata gains only recommendation tiers and the different pinned CLI names.

No process, timer, background network request or persistent DOM is added. Browsing filters 115 metadata rows and still
draws at most 48 cards. Closed, the tab unmounts. Static metadata and locale strings shrink with the removals; CLI-name
metadata and one conditional detail paragraph are added. No native working-set/CPU measurement or savings is claimed.
Cards checked: C006, C007, C009 and C011 ok by source review. Screenshots and the native screen suite were not run.

All Cargo commands used `CARGO_TARGET_DIR=E:/Temp/faber-target/c77e-list`. Requested rerun results:

| Proof | Result |
| :--- | :--- |
| npm run lint | Pass; existing App.tsx:1681 optional-chain warning remains. |
| npm run typecheck | Pass, including final five-language edits. |
| npm run docs | Blocked by environment: Node spawnSync git EPERM. The unchanged exported problems validator, with the exact git file list captured by PowerShell, passed: 39 living pages, zero problems. |
| cargo fmt --all --check | Pass. |
| cargo clippy -q --workspace --all-targets -- -D warnings | Pass. |
| cargo test -q -p faber-engine | 397 passed, 1 failed, 5 ignored. Stops in unchanged rewind_unfinished_redo_is_discoverable_even_without_files_done_or_live_expiry with error.rewind.storage, the same baseline failure already reproduced before this review correction. Integration suites are not reached by this exact command. |
| cargo test -q -p faber-engine --test engine tools:: | All 10 catalog/store checks pass. |
| Original 04801ffd catalog under the new metadata check | Fails as intended: 153 skills instead of 114. The corrected catalog was restored in a finally block and all 10 checks passed again. |
| git diff --check | Pass. |

Logs are E:/Temp/faber-tools/c77e-fix-{lint-final,typecheck-final,docs-final,docs-exported,fmt-final,clippy,engine,catalog-red,catalog-green}.log.
A fresh read-only source/code/coherence review found no exclusion or code blocker; its raw-data Python prerequisite
correction is applied in all five locales. No native app, screen, full-app memory/CPU or live external integration was
run. No commit was made; 04801ffd and its body are unchanged.

## Plugin delivery preflight (catalog-plugins)

Status at the first preflight: blocked before implementation at 0678303a. Superseded by the lead's private-session delivery decision below. The task explicitly says to stop and report a part that needs a
folder outside those the skill store already uses. No plugin, alternate installer, CLI-home write, product change,
release note or new product decision was made. The kits lane's owned files are unchanged.

The existing implementation does **not** install catalog files in the user's CLI skill folders. `Store::skills` in
`engine/src/tools.rs` writes `<data>/tools/plugin/skills/<id>/`; `Store::servers` writes `<data>/tools/servers/<id>/`.
`install_from` requires `SKILL.md` or a pinned npm entry, and `prune` removes everything beside `skills/` and Faber's
own `.claude-plugin/` manifest. The kits snapshot's `Delivery::verified_for_launch` in `engine/src/tools/kits.rs`
also refuses additional root entries. Claude receives that private plugin; current Codex session kits deliver checked
instructions, not personal skill-root writes. The task's description of writing each CLI's own agent/command folders
therefore cannot be satisfied by the existing delivery path unchanged.

Official documentation checked on 2026-10-21:

- [Codex subagents](https://developers.openai.com/codex/subagents), redirected to
  [Custom agents](https://learn.chatgpt.com/docs/agent-configuration/subagents#custom-agents): custom agents are standalone
  TOML files in `~/.codex/agents/` or project `.codex/agents/`. These are outside the current store's delivery roots.
- [Codex custom prompts](https://developers.openai.com/codex/custom-prompts), redirected to
  [Custom Prompts](https://learn.chatgpt.com/docs/custom-prompts): Markdown slash prompts require `~/.codex/prompts/`,
  explicit invocation and a restart. They are deprecated in favor of skills; the docs do not make a private skill
  snapshot a native prompt folder. No prompt was converted to a skill or silently omitted.
- [Codex skills](https://developers.openai.com/codex/skills), redirected to
  [Build skills](https://learn.chatgpt.com/docs/build-skills): repository/user discovery uses `.agents/skills/`, with
  plugins as a distribution option. This does not authorize writing new agent/prompt roots or changing CLI settings.

Supporting native commands or agents needs an explicit delivery-scope decision before building: either permit the
necessary additional roots within the existing store's checked delivery, or narrow the plugin parts delivered to each
CLI. This report makes neither decision for the owner. It does not claim those parts are unsupported by Codex.

### Recovered twenty-candidate inventory

The prior report mentioned twenty plugins but omitted their identities. Recovered from the earlier curation's local
`E:/Temp/faber-tools/catalog-pins.json` (`plugins`) and cross-checked against `catalog-candidates.json`. The table preserves
the prior pins and recorded reasons, **not a fresh whole-folder R362 audit or a verified digest**. No original source was
downloaded for this preflight and no upstream code ran. Zero candidates are admitted; all twenty remain withheld pending
delivery and the requested whole-folder audit. A prior exclusion is not reversed by having plugin packaging.

| Candidate | Original source at the prior commit | Prior reason or audit status |
| :--- | :--- | :--- |
| code-lsp-plugins | [anthropics/claude-plugins-official](https://github.com/anthropics/claude-plugins-official/tree/d1adba47161a271799de7b548225d929310046bd/.claude-plugin/marketplace.json) | Language-server configurations require external language servers; not a checked skill/server bundle. |
| code-modernization | [plugins/code-modernization](https://github.com/anthropics/claude-plugins-official/tree/d1adba47161a271799de7b548225d929310046bd/plugins/code-modernization) | Hooks and scripts; agent/command parts also need delivery work. |
| stitch-skills | [google-labs-code/stitch-skills](https://github.com/google-labs-code/stitch-skills/tree/0337446dadde6f8c94210444e2aa9d546126480f) (`plugins/stitch-build`, `stitch-design`, `stitch-utilities`) | Selected skills set `allowed-tools`; external MCP and sign-in prerequisite. |
| trailofbits-security-lenses | [trailofbits/skills](https://github.com/trailofbits/skills/tree/a3131740a39291e42fd15b9eac07e0b393ad71b1/plugins) (`differential-review`, `insecure-defaults`, `sharp-edges`, `variant-analysis`, `fp-check`) | Prior curation rejected CC-BY-SA terms; inventory also records hooks/scripts. No new license conclusion. |
| security-guidance | [plugins/security-guidance](https://github.com/anthropics/claude-plugins-official/tree/d1adba47161a271799de7b548225d929310046bd/plugins/security-guidance) | Hooks and scripts. |
| security-scan-team | [plugins/claude-security](https://github.com/anthropics/claude-plugins-official/tree/d1adba47161a271799de7b548225d929310046bd/plugins/claude-security) | Prior inventory records proprietary terms plus agents, commands and scripts. |
| document-skills-office | [anthropics/skills](https://github.com/anthropics/skills/tree/dbd4588f9e1033efb41dad4bef2f7947c8993d44/skills) | Prior inventory records all-rights-reserved terms and bundled scripts; not admitted. |
| knowledge-work-plugins | [small-business](https://github.com/anthropics/knowledge-work-plugins/tree/bc8f6922cfeea215a202cdcc9f0d299a990889a8/small-business) | Hybrid skills, agents and MCP manifest; portability not checked. |
| financial-services | [anthropics/financial-services](https://github.com/anthropics/financial-services/tree/574ed3624aebd0418c7e96cd101262f30210ab26) | Hooks and mixed agent/command/MCP packages; paid-data prerequisites recorded. |
| legal-practice-plugins | [anthropics/claude-for-legal](https://github.com/anthropics/claude-for-legal/tree/4a6c651889c97cc9140580363c73e0eb17379c2b) | Hooks and MCP manifests in the prior inventory. |
| unity-agent-plugin | [Unity-Technologies/unity-agent-plugin](https://github.com/Unity-Technologies/unity-agent-plugin/tree/421acb1d365cbab6506b7d77ce09a71602edbce1/skills) | No plugin-specific skip reason in the old cache; engine-only license and individual skill exclusions above remain. Whole bundle not audited. |
| universal-modder | [rehan-remade/universal-modder](https://github.com/rehan-remade/universal-modder/tree/c1dff21663665dd17af289744a301b66e369605e) | CLI binary, MCP, hooks and scripts. |
| elevenlabs-skills | [elevenlabs/skills](https://github.com/elevenlabs/skills/tree/269e5f0e5a4cf296a8a38beb166245b89d5e2c1c/skills) | Prior audit records automatic CLI installation in useful skills. |
| expo-skills | [plugins/expo](https://github.com/expo/skills/tree/55ec65cd16499f20f14fb10fe99a2e62901b00a1/plugins/expo) | Hooks and MCP configuration. |
| flutter-skills | [flutter/skills](https://github.com/flutter/skills/tree/6e590650bb4bab87f33187423dc10113d04d4836/skills) | No plugin-specific skip reason in the old cache; retained individual skills do not approve the whole bundle. Installer exclusions above remain. |
| android-skills | [android/skills](https://github.com/android/skills/tree/a3f56b8fc83039c407e20a7b4789c2b6ea192d57) | No plugin-specific skip reason in the old cache; retained individual skills do not approve the whole bundle. Installer exclusions above remain. |
| firebase-mcp-and-skills | [firebase/agent-skills](https://github.com/firebase/agent-skills/tree/92eed24c0976a79a66d5b8038e9eda8b441ef92d) | Agent and MCP configuration; R253 portability not established. |
| render-skills-and-tools | [render-oss/skills](https://github.com/render-oss/skills/tree/8675173fe30232384d51c8763d1eea4db0350419) | Hooks and remote MCP configuration. |
| sentry-plugin | [getsentry/plugin-claude](https://github.com/getsentry/plugin-claude/tree/0d42f6a5b013580110ea55c00f699782c03a99b0) | Hosted MCP configuration; R253 requires a checked dependency-free npm server. |
| posthog-skills-and-mcp | [PostHog/skills](https://github.com/PostHog/skills/tree/b6951f6ee5fa21856939456a35ab33d3733b9b8e/skills/posthog) | Hooks in the `all` plugin and external hosted integration. |

Historical: at this first preflight nothing had been built, so no product page, test or word changed. The delivery that
followed is described in the next section, and its proofs are in the last one.

Preflight validation uses `CARGO_TARGET_DIR=E:/Temp/faber-target/c77e-plugins`; logs are
`E:/Temp/faber-tools/c77e-plugins-{lint,typecheck,docs,docs-exported,build,fmt,clippy,engine}.log`.

| Requested proof | Preflight result |
| :--- | :--- |
| npm run lint | Pass; pre-existing optional-chain warning in `ui/App.tsx:1686`. |
| npm run typecheck | Pass. |
| npm run docs | Blocked: `spawnSync git EPERM`. The unchanged exported validator passed with the exact file list captured by git: 40 living pages, zero problems. |
| npm run build | Blocked: Vite's `externalize-deps` cannot spawn its Windows path-resolution helper (`EPERM`). |
| cargo fmt --all --check | Pass. |
| cargo clippy -q --workspace --all-targets -- -D warnings | Pass. |
| cargo test -q -p faber-engine | 502 passed, 1 failed, 8 ignored. The unchanged `rewind::validation_tests::rewind_unfinished_redo_is_discoverable_even_without_files_done_or_live_expiry` fails at `engine/src/rewind.rs:1856` with `error.rewind.storage`, the same named failure recorded above. Integration suites are not reached. |

The inventory check confirms all twenty ids and their full original commit pins appear in this report. A fresh read-only
review checked the preflight against the store, the candidate cache and the official docs and found no blocker in the
report; it did not re-audit upstream folders. `git diff --check` passed. No commit was made.

## Checked portable plugin delivery (catalog-plugins)

Status: store, Catalog controls and native session delivery are integrated with the landed session kits. The earlier preflight is historical,
not the current delivery decision. The lead explicitly authorized delivery only from Faber's own checked store: no
personal skill, agent or prompt folder and no CLI configuration is written.

A plugin is one pinned original-source folder plus declared skill, command, agent and portable-server parts. The original
folder uses R247's canonical tree digest; each skill uses that same tree algorithm, each standalone Markdown/JSON part
uses SHA-256 of its raw bytes, and each npm package has its own original-source version, commit and tree digest. Root
and part metadata, names, file types and exact layouts are checked. Native command/agent definitions cannot grant tools,
change models or permissions, execute inline shell expansions, or depend on another plugin-root layout. Shipped binaries,
scripts, hooks, package lifecycle scripts, unlisted native parts and non-portable MCP definitions are refused.

All parts publish through the existing staged install, lock and parent ledger entry. Off, Remove, journal recovery and
Undo move/check that same parent folder. Skills and servers expand to virtual namespaced resources for the existing kit
selector; children never get a second installer, ledger or selector. Tool servers remain Full-access-only.

The documented building blocks for the lead's session decision are the [Codex configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference):
`agents.<name>.config_file` accepts a role TOML layer and `agents.<name>.description` supplies role guidance. The
[app-server protocol](https://developers.openai.com/codex/app-server) provides per-thread configuration overrides, which
Faber already uses for MCP. Applying these two documented mechanisms to named agent roles is an integration inference,
not a live CLI result. A checked agent's instruction body can become a deterministic TOML layer containing only
`developer_instructions`, stored in the same private snapshot, with its original description and no permission override.
This is separate from the documented personal/project agent discovery folders.
[Custom prompts](https://learn.chatgpt.com/docs/custom-prompts) require the personal prompts folder and explicit
invocation; no corresponding per-thread prompt override is documented. Codex therefore receives plugin skills, supported
agent roles and eligible servers, with commands explicitly marked unavailable on the card/detail. No prompt is silently
converted into a skill. Claude uses the same private plugin folder with namespaced `commands/` and `agents/`; a missing
installed `--plugin-dir` capability is stated on the card/detail. Chat receives only the kit's picked skill text, with
no plugin commands or subagents. Native role refresh on a real resumed CLI thread still needs the lead's integration
validation; no real CLI or model was called by these tests.

### Twenty original bundles: exclusion evidence

Re-fetched the exact original GitHub commits, inspected the complete declared source scopes, dotfiles and supporting
files, and checked concrete blockers against the new delivery rules. ElevenLabs' old `skills/` inventory path does not
exist at its pin; its actual root layout was inspected instead. Flutter's root manifest was included, beyond the old
skills-only scope. The code-LSP candidate is a marketplace index, so its one-file scope does not approve the indexed
external packages. Every bundle has a concrete blocker below. None is admitted; previously rejected individual skills
are not revived as bundles. The existing 114 skills and one server keep their pins.

This is fail-fast exclusion evidence, not a positive security approval: after a decisive blocker, remaining operational
instructions were not all normalized individually. Searching every file is discovery; absence of a keyword is never a
pass. Full archive SHA-256 values below identify what was fetched, not R247 canonical unpacked-tree digests. No upstream
code ran, no prerequisites installed, and no candidate endpoint was contacted beyond its original GitHub archive.

| Candidate | Files in inspected scope | Concrete blocker at its original pin |
| :--- | ---: | :--- |
| code-lsp-plugins | 1 | External language-server binary requirement. [.claude-plugin/marketplace.json:863,866-868](https://github.com/anthropics/claude-plugins-official/blob/d1adba47161a271799de7b548225d929310046bd/.claude-plugin/marketplace.json#L863) |
| code-modernization | 107 | Requires command hooks, including telemetry. [plugins/code-modernization/hooks/hooks.json:8-9](https://github.com/anthropics/claude-plugins-official/blob/d1adba47161a271799de7b548225d929310046bd/plugins/code-modernization/hooks/hooks.json#L8) |
| stitch-skills | 81 | Host-specific allowed-tools and external Stitch MCP. [plugins/stitch-build/skills/react-components/SKILL.md:7-12](https://github.com/google-labs-code/stitch-skills/blob/0337446dadde6f8c94210444e2aa9d546126480f/plugins/stitch-build/skills/react-components/SKILL.md#L7) |
| trailofbits-security-lenses | 123 | Requires prompt hook; selected source is CC-BY-SA-4.0. [plugins/fp-check/hooks/hooks.json:3-10](https://github.com/trailofbits/skills/blob/a3131740a39291e42fd15b9eac07e0b393ad71b1/plugins/fp-check/hooks/hooks.json#L3) |
| security-guidance | 56 | Requires command hooks and hook bootstrap. [plugins/security-guidance/hooks/hooks.json:8-10](https://github.com/anthropics/claude-plugins-official/blob/d1adba47161a271799de7b548225d929310046bd/plugins/security-guidance/hooks/hooks.json#L8) |
| security-scan-team | 46 | Requires shell command hooks. [plugins/claude-security/hooks/hooks.json:9-10](https://github.com/anthropics/claude-plugins-official/blob/d1adba47161a271799de7b548225d929310046bd/plugins/claude-security/hooks/hooks.json#L9) |
| document-skills-office | 439 | Office skill terms prohibit third-party distribution. [skills/docx/LICENSE.txt:1,13-21](https://github.com/anthropics/skills/blob/dbd4588f9e1033efb41dad4bef2f7947c8993d44/skills/docx/LICENSE.txt#L1) |
| knowledge-work-plugins | 214 | Contains hosted MCP endpoint. [small-business/.mcp.json:2-5](https://github.com/anthropics/knowledge-work-plugins/blob/bc8f6922cfeea215a202cdcc9f0d299a990889a8/small-business/.mcp.json#L2) |
| financial-services | 363 | Vertical plugin contains hosted MCP endpoint. [plugins/vertical-plugins/financial-analysis/.mcp.json:4-5](https://github.com/anthropics/financial-services/blob/574ed3624aebd0418c7e96cd101262f30210ab26/plugins/vertical-plugins/financial-analysis/.mcp.json#L4) |
| legal-practice-plugins | 315 | Legal plugin contains hosted MCP endpoint. [commercial-legal/.mcp.json:4-5](https://github.com/anthropics/claude-for-legal/blob/4a6c651889c97cc9140580363c73e0eb17379c2b/commercial-legal/.mcp.json#L4) |
| unity-agent-plugin | 216 | Useful CLI skill runs mutable remote installer. [skills/unity-cli/SKILL.md:67,72](https://github.com/Unity-Technologies/unity-agent-plugin/blob/421acb1d365cbab6506b7d77ce09a71602edbce1/skills/unity-cli/SKILL.md#L67) |
| universal-modder | 781 | Requires command hook (also declares hosted MCP). [hooks/hooks.json:8-9](https://github.com/rehan-remade/universal-modder/blob/c1dff21663665dd17af289744a301b66e369605e/hooks/hooks.json#L8) |
| elevenlabs-skills | 75 | Hosted MCP and automatic mutable CLI installation. [agents/SKILL.md:13,21](https://github.com/elevenlabs/skills/blob/269e5f0e5a4cf296a8a38beb166245b89d5e2c1c/agents/SKILL.md#L13) |
| expo-skills | 131 | Requires command hook and hosted MCP. [plugins/expo/hooks/hooks.json:8-10](https://github.com/expo/skills/blob/55ec65cd16499f20f14fb10fe99a2e62901b00a1/plugins/expo/hooks/hooks.json#L8) |
| flutter-skills | 109 | Plugin manifest configures non-R253 Dart MCP. [.claude-plugin/plugin.json:18-22](https://github.com/flutter/skills/blob/6e590650bb4bab87f33187423dc10113d04d4836/.claude-plugin/plugin.json#L18) |
| android-skills | 412 | Reference requires installing another skill through Android CLI. [testing/testing-setup/references/android/studio/preview/compose-screenshot-testing-with-testsuites.md:23-25](https://github.com/android/skills/blob/a3f56b8fc83039c407e20a7b4789c2b6ea192d57/testing/testing-setup/references/android/studio/preview/compose-screenshot-testing-with-testsuites.md#L23) |
| firebase-mcp-and-skills | 113 | Server manifest uses npx to fetch and run package at runtime. [.mcp.json:4-10](https://github.com/firebase/agent-skills/blob/92eed24c0976a79a66d5b8038e9eda8b441ef92d/.mcp.json#L4) |
| render-skills-and-tools | 143 | Contains hosted MCP endpoint. [.mcp.json:4-5](https://github.com/render-oss/skills/blob/8675173fe30232384d51c8763d1eea4db0350419/.mcp.json#L4) |
| sentry-plugin | 222 | Plugin manifest wires hosted MCP endpoint. [.claude-plugin/plugin.json:18-21](https://github.com/getsentry/plugin-claude/blob/0d42f6a5b013580110ea55c00f699782c03a99b0/.claude-plugin/plugin.json#L18) |
| posthog-skills-and-mcp | 1446 | Selected all plugin requires command hook. [skills/posthog/all/hooks/hooks.json:7-9](https://github.com/PostHog/skills/blob/b6951f6ee5fa21856939456a35ab33d3733b9b8e/skills/posthog/all/hooks/hooks.json#L7) |

| Original candidate archive | Downloaded bytes | Archive SHA-256 (not the store tree digest) |
| :--- | ---: | :--- |
| code-lsp-plugins | 8259115 | `c77425c59df334c2a25a5a9c0050286a39acf7d57c8e4d3732767e4eb5c94ffd` |
| code-modernization | 8259115 | `c77425c59df334c2a25a5a9c0050286a39acf7d57c8e4d3732767e4eb5c94ffd` |
| stitch-skills | 227622 | `30c0e72dd7612c291050082b6299d69fb9d8b391f1043fd06490a8e2b20e944d` |
| trailofbits-security-lenses | 4070446 | `bb695b9a489f38682e9acc192d2c44b27970cac461fd511d0279af2249948fd7` |
| security-guidance | 8259115 | `c77425c59df334c2a25a5a9c0050286a39acf7d57c8e4d3732767e4eb5c94ffd` |
| security-scan-team | 8259115 | `c77425c59df334c2a25a5a9c0050286a39acf7d57c8e4d3732767e4eb5c94ffd` |
| document-skills-office | 4343662 | `2f81fe0297244f9e4fa7673f17a0196009657a08bd1094e9e673c70163ce7d6e` |
| knowledge-work-plugins | 3377609 | `31d74c403bc26d5b3a0a1dccac30a341fbf95c84b7ca20c7f15616b995bab6de` |
| financial-services | 976940 | `9ac1bd211ef4c377778c8531cae5813fe20737c74e6a0fe9633bc9ce5f3c2e83` |
| legal-practice-plugins | 1395242 | `891f16b9bcc9b53fcb3ea54c660ea06ea8d0fab9400ac6e46f319006f34ab73d` |
| unity-agent-plugin | 845751 | `06e5ce799ed625cd388e288b6997a26b89430358f05c72d055b962144dc9fa41` |
| universal-modder | 28493111 | `460f67aafbfa89c9420f8e60c156f3c2114fa777134846a3be4f991610c8f40d` |
| elevenlabs-skills | 521156 | `c919f332ab412470c9d94f504d589ab9092ad0af0a24ee6597a0e10ef6cb51f0` |
| expo-skills | 3489454 | `1e0474718324a14d2f42ce7cde374dcb1302e3e74963d8637a8aa90d2f6d365b` |
| flutter-skills | 240480 | `2b63c84e5e132b90f3be08557277353fdd53ce967cd39edb0f4105efdce57bf0` |
| android-skills | 1288466 | `2d712a3bc6a87c4258809d345649be0d2b6aa23521ce43ce0210a95fc037a2ef` |
| firebase-mcp-and-skills | 235160 | `14cacc977556480f282918e6962761ef3453c39780fed7c980a420386225f5a6` |
| render-skills-and-tools | 344820 | `bf661c40c2af11125e106c97fa110024a60eac02a221ae6f0ab0452676cab0ab` |
| sentry-plugin | 429549 | `a83923b6eac3e3773883b6fafbef4aac3195c140c418132a38c12e6b8c31d205` |
| posthog-skills-and-mcp | 8606217 | `90fc8b13f9e70d7ad7a16418c24843334482f5da34ccf7ea09182ed64ff92e6c` |

### Proofs of the delivered state (after two reviews)

| Proof | Result |
| :--- | :--- |
| npm run lint, typecheck, docs, build | Pass (the `ui/App.tsx:1686` warning is older). |
| cargo fmt --all --check, cargo t --claims, cargo clippy -q --workspace --all-targets -- -D warnings | Pass. |
| cargo test -q -p faber-engine | 506 unit and 944 integration tests pass, 0 fail. |
| cargo t --all --filter catalog / plugins | 58 / 24 pass, 0 fail (screen part not run in this sandbox). |
| Not checked | A real Claude Code or Codex run (the `agents.<name>.config_file` thread override is documented but not run live), memory and processor of the full app. |
