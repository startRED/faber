# Adaptive workflow pilots

The [lead agent](../product/lead-agent.md) can use these examples to choose tools and checks suited to the request. Evaluate quality first, cost second and time third. The software and thumbnail comparisons each have one recorded pair. The Blender proof has a local round trip, and the thumbnail preflight includes a clipped control.

## Choose a path and evidence

Record the outcome, starting artifact, constraints, available tools and what would count as success. Use a reversible copy when appropriate. Choose the least expensive check that can establish the result, and increase coverage as the risk or evidence warrants. Keep the artifact, tool version, inputs, check, result and limitations together outside the build cache so another person can reproduce it. Use screenshots for appearance, interactions for behavior, and tests for the exact cases they exercise. Advance work that is independent of a blocked check and record what remains unverified.

| Request | Example of real proof | Boundary |
| :--- | :--- | :--- |
| Software | Reproduce the bug through its normal entry point. Run the same check after the change and retain both results. | Exercise the app itself when the outcome depends on how the pieces interact. |
| Roblox Studio | Playtest the place. For a change that synchronizes players, run a server with at least two clients and record what each sees. | Select the Studio mode for the behavior the task changes. [Studio testing modes](https://create.roblox.com/docs/studio/testing-modes) |
| Blender to Roblox | Export an asymmetric sample with a marked front face. Import it into Studio and compare orientation, scale and materials. Save the source, export and importer warnings. | Verify the output on both sides of the transfer. [Blender guidance](https://create.roblox.com/docs/art/blender) · [Importer](https://create.roblox.com/docs/studio/importer) |
| Interface design | Exercise a prototype at intended desktop and phone sizes. Check keyboard access and relevant empty and error states. | Match prototype fidelity to the decision and record the interactions exercised. [Making prototypes](https://www.gov.uk/service-manual/design/making-prototypes) |
| YouTube thumbnail | Open the exported image. Check dimensions and format, then compare the same reduced sizes for cropping, legibility and subject recognition. | A private review checks the image. An eligible published-video experiment with the owner's permission can compare performance by watch time. [Image guidance](https://support.google.com/youtube/answer/72431?hl=en) · [YouTube's A/B test](https://support.google.com/youtube/answer/16391400?hl=en-GB) |

## Stop and review by risk

For a reversible draft, inspect it against the criteria and retain the previous version. For changes to stored data, permissions, processes or security, follow the project's review rules and give the independent reviewer enough evidence to repeat the check. Bound a review-and-repair loop by the scope, available quota and a concrete success condition. If the evidence conflicts, isolate the disagreement before changing more. Use the project's existing authorization path for irreversible actions.

## Two inexpensive comparisons

Use the same brief, starting materials, criteria and available tools for both sides. Record the model, settings, elapsed time, known cost, failures and evidence. Alternate presentation order, conceal which approach produced each result, and use a reviewer independent of the authors. Apply the [benchmark's](../dev/benchmark.md) isolation and cost limits. Report unknown cost as unknown and confine any conclusion to the tested cases.

1. **Software.** Give a direct attempt and an adaptive attempt the same bug and fixture. Run the same independent regression check on both. Compare cost and time between results that pass the quality criteria.
2. **Thumbnail.** Use the same brief and source assets with a single-candidate approach and an approach that compares inexpensive drafts first. Inspect both at identical small sizes and blind-review legibility and fidelity to the brief. Record any later audience experiment as separate evidence.

### Invoice pair on Windows, 2026-11-19

The first complete pair, `invoice-20261119-safe-5`, used only subscription quota with GPT-6.2 Sol at low effort. Earlier trials stopped during setup or when the first conversation waited for a person. Both sides used the same fixture and project permissions in disposable folders. Standard ran first. The guided transcript showed `Get-Content` on the proof skill. The standard run had no such receipt. Neither run showed delegation. The captured figures remain below. The original run folder was in disposable build storage and is no longer in the copy.

| Prompt | Independent proof | Fresh input tokens | Output tokens | API-price equivalent | Elapsed |
| :--- | :--- | ---: | ---: | ---: | ---: |
| Standard | 3 of 3 | 41,480 | 1,631 | US$0.12403 | 75.27 s |
| Guided | 3 of 3 | 42,050 | 1,759 | US$0.12672 | 74.64 s |

The independent program checked that the README was intact, the invoice handled its contract and invalid inputs, and each submitted regression test caught the original zero-quantity charge. Each saved conversation recorded one refusal of grouped PowerShell receipts and then separate checks, so the total elapsed times include that work. The standard and guided sides reported 229,248 and 231,936 cached input tokens. The provider did not report cache-write counts. The amounts are API-price equivalents for the reported tokens. The run used subscription quota.

In the recorded blind verdicts, one judge and one reader called it a tie. Another reader preferred the guided submission narrowly because its test also covered a positive quantity with a zero unit price. The implementation was equivalent on inspection. One additional judge returned no valid verdict. The guided run used about 2.2% more at API prices and finished 0.63 seconds sooner.

The live project sessions used normal guarded read access, including installed instructions and skills outside the project. The preflight denied writes to a neighboring fixture and denied reads and writes to its test file inside the engine data folder. The guided session later read the installed proof guide through normal access. The preflight did not establish isolation from ordinary sibling reads or network access. No recorded command used the other candidate. The independent proof itself ran offline in its own restricted process. With that boundary, one fixture, one order and no replication, the differences are exploratory and do not determine a default workflow.

### Shell proof on Windows, 2026-11-20

Windows PowerShell 5.1.26100.9818 and PowerShell 7.6.7 both ran ripgrep 15.2.0. Searching for `fn waiting_command(` with `engine/src/engine/*.rs` as a path failed with Windows error 123 and ripgrep exit 2. Passing `engine/src/engine` as the directory and `-g '*.rs'` as the filter found one matching source file and exited 0. A valid search for an absent string exited 1. [ripgrep guide](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md) · [ripgrep exit codes](https://github.com/BurntSushi/ripgrep/blob/master/CHANGELOG.md)

A child PowerShell started with `-Command` exited 0 in both versions when `Write-Output 'finished'` followed the failed search. Capturing `$LASTEXITCODE` immediately after ripgrep and explicitly returning it after that message preserved exit 2. In an automated check, record the expected match or test count and preserve the native exit code before formatting or printing a summary. [PowerShell automatic variables](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_automatic_variables) · [PowerShell command options](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_pwsh?view=powershell-7.6)

### Blender export round trip on Windows, 2026-11-19

The proof creates a body, an off-center front marker with its own material, and an unselected object. It exports the selected meshes to FBX and GLB. The FBX preset uses the Roblox guide's unit scaling and axes for ordinary meshes. Avatar assets have separate export requirements. [Roblox Blender guide](https://create.roblox.com/docs/art/blender)

In Blender 5.0.1, a fresh Blender process for each file recovered both selected meshes with the expected world dimensions, positions and material names. The unselected object stayed out of both exports. In the control export at 1% scale, the check found a body of 0.04 × 0.02 × 0.01 where it expected 4 × 2 × 1 and exited with code 3. The Studio import and appearance checks remain open.

Run from the project root in PowerShell with Blender available as `blender`:

```powershell
$fixture = Join-Path ([System.IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString('N'))
$proof = @('--background', '--factory-startup', '--disable-autoexec', '--python-exit-code', '3', '--python', 'bench/proofs/blender-export.py', '--')
blender @proof emit $fixture
blender @proof verify (Join-Path $fixture 'asset.fbx')
blender @proof verify (Join-Path $fixture 'asset.glb')
blender @proof verify (Join-Path $fixture 'wrong-scale.fbx')
```

The first three commands should exit 0. The last should exit 3 with the scale discrepancy. Retain the fixture, the two successful check results and the rejected control together. For the target-app check, use a disposable Studio place with **Upload to Roblox** disabled. Record the hierarchy, size and orientation shown by the importer. [Studio importer](https://create.roblox.com/docs/studio/importer)

### Roblox gameplay proof in a disposable place

For a shared collectible, start with one original pickup and one respawn rule. Record which players are eligible, the expected number of awards, and what each player should see after the item returns. Reuse the project's existing assertions for rules that can run independently of Roblox, then run the engine checks for the behaviors that rely on replication, input, or physics.

- With the owner's Studio test place already open and connected, confirm its instance identity before editing or playing. Run a short solo playtest, capture the visible result and console, and end the test. The built-in connector targets an open Studio instance and exposes separate state, playtest, and console commands. [Roblox Studio MCP](https://create.roblox.com/docs/studio/mcp)
- When players share state, run one server and two clients. Trigger both pickup attempts, confirm that the server grants one award, and check both clients after the respawn. A Studio plugin can script the setup and collect a result through `StudioTestService`. Keep the server assertion and both client observations. [Studio testing modes](https://create.roblox.com/docs/studio/testing-modes) · [StudioTestService](https://create.roblox.com/docs/reference/engine/classes/StudioTestService)
- For an already authorized test place, Open Cloud can execute Luau against a named place version and return task logs. That receipt can prove a server-side assertion. A client or visual claim needs its corresponding playtest evidence. [Open Cloud Luau Execution](https://create.roblox.com/docs/cloud/reference/features/luau-execution)

When only the local file and Studio command are available, `RunScript` can run a smoke script after the file loads and write a log. Roblox documents the `--background` flag for Mac only. Confirm that the chosen launch fits the session's window permissions before using it. No Studio, multiplayer, or Open Cloud run was performed for this proposal. [Studio command-line interface](https://create.roblox.com/docs/studio/cli)

### Thumbnail preview check on Windows, 2026-11-19

The fixture was a long-form tutorial thumbnail using a render of the asymmetric Blender sample and two text lines. Blender 5.0.1 rendered the source object without opening the editor. The local export was a 3,840 × 2,160 PNG of 406,254 bytes. That resolution and aspect ratio follow YouTube's current recommendation for long-form videos. Preview widths of 384 and 192 pixels were local test sizes. [YouTube image guidance](https://support.google.com/youtube/answer/72431?hl=en)

The check reopened the PNG and measured text and object bounds. It accepted the complete image and flagged a control whose text started 180 pixels left of the canvas. An independent reviewer opened both versions at both preview sizes, labeled A and B, before seeing which was the control. The reviewer read both lines in the complete image and spotted the cropped letters in the control. The object remained distinct in all four previews. The render, preview images, result and review key are in the local `.faber/verify/thumbnail-proof` folder.

The paired pilot reused that object and brief, with the same copy, typeface and colors on both sides. Python 3.12.10 and Pillow 13.0.0 produced one complete left-text layout directly. The other route screened three 384 px layouts and selected by smallest line height, then subject height. Draft order broke the remaining tie. Both deliverables reopened as 3,840 × 2,160 PNGs. They passed bounds checks for text, subject and a locally reserved bottom-right area. At 192 px, the smaller line measured 8.45 px in the direct export and 9.2 px in the selected export. The subject measured about 41 px in height in both.

An independent reviewer saw the two 192 px previews before the two 384 px previews, in reverse of export order and without the selection key. They read both lines in both previews, saw the same 3D object, found no clipped content and called it a tie. The small render showed the object as a slab with a small marker. Its connection to Roblox came from the words. Export and checks took 342 ms for the direct route and 383 ms for the draft route in that run. Both routes generated locally without model calls. These times exclude composing the layouts and the review. The script, small previews, run receipt and review are in the local proof folder. This one pair shows how to compare layouts before export. It leaves model effort, other briefs and audience performance open.

### Interface proof in a local browser on Windows, 2026-11-20

The fixture was the existing [local signup form](../../bench/browse/signup.html), using invented names and addresses.
Headless Chrome 158.0.8127.36 loaded the fixture and a copy with a viewport declaration at 1,280 × 900 and at an
emulated 390 × 844 mobile viewport. The quick check found a loaded page and a form in all four runs.

| Variant | Desktop layout width | Mobile layout width | Mobile visual scale |
| --- | ---: | ---: | ---: |
| Existing fixture | 1,280 CSS px | 980 CSS px | 0.398 |
| Copy with the viewport declaration | 1,280 CSS px | 390 CSS px | 1.000 |

The keyboard run entered an invalid address, selected a plan, accepted the terms and tried submitting. The browser
blocked the invalid address. With a corrected address it reached the confirmation in all four runs. The mobile
screenshots showed the original text and controls scaled down. The copy rendered at the requested width.

To reproduce, open the fixture and a copy with `<meta name="viewport" content="width=device-width, initial-scale=1">`
in the head. Compare the sizes above using mobile rendering for the narrow viewport. Record `innerWidth`,
`visualViewport.scale`, screenshots and the invalid and valid keyboard outcomes. The pilot covers one browser, one
form and mobile emulation. A physical phone, touch and a screen reader remain untested. [Chrome device mode](https://developer.chrome.com/docs/devtools/device-mode)

The local receipt and captures are in `.faber/verify/viewport-pilot-2026-11-20-b`.

## A checked example in Faber

The diagnostic change reproduced an agent request for numbered tool steps that the old conversation tool rejected. The same integration case passed after the change and delivered the reason from a simulated failed search to the conversation that requested it. Separate checks covered long messages, access across conversations and clipped or sensitive diagnostic lines. This establishes the tested behavior in an isolated fixture. The Studio import pilot remains unrun.

## Research basis and limits

Anthropic describes progressive handoffs and testing across sessions. OpenAI describes repository guidance and feedback in one development project. NIST illustrates peer review and testing in an example development process. Use these as design inputs and measure effects in Faber's own tasks. [Anthropic's study](https://www.anthropic.com/research/effective-harnesses-for-long-running-agents) · [OpenAI's account](https://openai.com/index/harness-engineering/) · [NIST's reference model](https://pages.nist.gov/nccoe-devsecops/example-implementation-1-2/notational-reference-model.html)
