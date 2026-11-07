# Faber product demo

An original, finite project-conversation illustration: request, work, checks, then a result opened in the preview.
It is a synthetic demo, visibly labelled as one. It neither runs an AI nor proves the illustrated project works.
Dark Room tokens, Archivo, Curioso and the icons come from Faber's current sources at preparation time.

## Run

From Faber, with Node.js 24+, npm and git installed:

```sh
npm ci
npm run demo -- --lang pt-BR
```

Open `http://127.0.0.1:4187/faber.html`. Nothing opens a browser or console by itself. The first run downloads the
catalog-pinned Stageshow source from its original GitHub repository, verifies its complete source digest and installs
its locked dependencies with lifecycle scripts disabled. The source, dependencies, generated assets and build all
remain under ignored `target/stageshow/`; none enter the shipped app or its dependency tree.

The standalone runtime uses upstream Stageshow (Node >=20.19); this launcher also imports Faber's TypeScript locales
directly, so it requires Faber's Node >=24. It uses the real npm JavaScript executable and never a shell or `.cmd` shim.
Downloaded source is verified again before each launch. A modified or interrupted cache fails with a recovery message;
move `target/stageshow` aside before retrying, preserving any local work there.
Stop a running demo preview with Ctrl+C before preparing or building it again; npm reinstalls its verified runtime.

Options are `--lang en|pt-BR|es|fr|de`, `--port 4187`, `--prepare` and `--build`. Build output is the downloaded
source's `stageshow/dist-faber/`. Serving this folder preserves the relative iframe and asset URLs. Its `notices/`
folder carries Faber, Stageshow MIT/asset notices, Archivo's SIL Open Font License and the runtime dependency licenses.

## Playback and composition

The viewer supplies keyboard-accessible pause/play, next frame, replay and 0.25x slow motion controls over Stageshow's
iframe clock. Playback stops while the page is hidden. Reduced motion shows the completed poster with no cursor or
animation. The illustration plays once and holds its result; replay is its explicit reset.

The wide composition is 960 by 510. Below 600 px the 390 by 570 composition drops the sidebar and opens the preview
over the conversation, keeping the content at a readable size instead of shrinking a desktop screenshot.
Every scene and viewer string comes from the existing five Faber locales. The viewer discloses the simulated story.

## Rehearse

A local Chrome/Chromium installation is required for upstream film capture; it is not bundled by Faber. From the
Faber root, `node demos/faber/verify.mjs` checks the running preview's keyboard playback, complete story at both widths,
reduced motion and failed-load recovery, then writes screenshots and page-only measurements to the ignored audit folder.
This is an explicit browser integration check, not a unit test. It re-verifies the downloaded source before importing
the upstream browser launcher; it requires the preview running at port 4187.
It also traces 15.5 seconds of native browser frames at each width: a captured native clock, no sampling gap over
100 ms, all four story beats in order, bounded authored spatial changes and a stable result for the final two seconds. Typing
must append the real request while its field keeps its width. Raw observations survive a failed assertion. This
bounded independent proof never changes an inconclusive upstream film result into an upstream pass.
Native time establishes sampling coverage. The separate animation clock establishes authored trajectory velocity;
instantaneous native velocity remains a diagnostic. These checks do not certify constant physical frame rate.

For film capture, from the
prepared source's `stageshow/` directory, while the preview is running:

```sh
node scripts/film.mjs "http://127.0.0.1:4187/?capture=faber" --select "#example" --gate --count 32 --every 500 --width 960 --out audit/faber-wide.png
node scripts/film.mjs "http://127.0.0.1:4187/?capture=faber" --select "#example" --gate --count 32 --every 500 --width 390 --out audit/faber-compact.png
```

Read the generated JSON coverage, motion findings and captured frames, as well as the printed verdict. Missing
samples or inconclusive coverage do not establish a pass. Capture the poster separately with `--still --reduced`.
The original [`scene.mjs`](scene.mjs) is the tested story; [`REHEARSAL.md`](REHEARSAL.md) records evidence and limits.

This uses the concepts and runtime of upstream Stageshow, not a committed copy of its kit or example code.
Upstream MIT and asset notices remain in the downloaded source. Only Faber-owned icons, fonts and marks are used in
the authored composition. No background demo process exists unless this command is running.
