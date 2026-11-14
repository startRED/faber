# Plan: audio runtimes in the catalog

Status: built in `lane/audio-runtimes`. The implementation follows this contract; [media](../product/media.md)
and [catalog design](../design/catalog.md#audio-runtimes) describe the delivered behavior, platform exclusions,
download limits and verification. Hash-locked requirements, source/weight pins and immutable wheel integrity metadata
live beside `engine-projects/src/tools.json`. All downloads are on demand and nothing third-party is bundled.

Plan for the third part of the media work ([media craftsmanship](../product/media.md)). The skills and the `audio`
job shipped first; the catalog cannot yet install anything that is not a GitHub folder or an npm server, so the agent
today installs faster-whisper or FluidSynth itself, under its permissions, as media-craft tells it. This page is the
precise build order for a lane that takes it. It touches stored data, downloads and processes, so it needs the
cross-vendor review of AGENTS.md before shipping.

## What a runtime item is

A new section `runtimes` in `engine-projects/src/tools.json`, beside `skills`, `servers` and `plugins`, read by
`tools::catalog()` like the others and carrying the existing `Item` fields (`id`, `job: "audio"`, `tier`, `license`,
`repo`, `commit`) plus a `runtime` object:

| Field | Meaning |
| :--- | :--- |
| `kind` | `python` (a uv environment from a pinned lock) or `file` (one download, such as a SoundFont or a sound pack) |
| `source` | Exact URL at a pinned revision: a PyPI version, a GitHub release asset or a Hugging Face file at a commit |
| `sha256`, `size` | Checked before anything is unpacked; a mismatch deletes the download and reports `error.tools.digest` |
| `weights_license` | SPDX of model weights when it differs from the code's (OmniVoice: code Apache-2.0, weights CC-BY-NC-4.0) |
| `capability` | `tts`, `align`, `music`, `sfx` or `qa`, for the kit and the skills' index line |
| `min_vram_mb` | Absent for processor items; a GPU item joins a kit only when the detected free VRAM fits |
| `command` | The one-shot command the agent runs (`faster-whisper`, `fluidsynth`, `speechmos`), relative to the environment |

## Install path

1. `engine-projects/src/tools/runtime.rs`: download into the store's temporary copy (`TemporaryCopy`), check size and
   sha256, then for `python` run the user's `uv` (resolved like npm in `engine-store/src/cli.rs`, never a shell) with
   `uv venv` and `uv pip install --require-hashes -r <pinned lock>` inside the item's own folder, offline after the
   download. No `uv` on the PC: the card says so and offers the uv installer page; Faber never installs uv silently.
2. Reuse the store lock, ledger, selection, journal and Undo exactly as plugins do (`tools/plugins.rs`), so
   uninstall, off and recovery need no new code paths. Removal deletes the environment folder whole.
3. Nothing runs at install except uv; models download their weights on first use only when the item lists them as
   files with their own sha256, so a first use is still checked.
4. VRAM: read once per kit build through `winproc` (DXGI adapter memory on Windows, `nvidia-smi --query-gpu` where
   present elsewhere), cached for the engine's life; unknown VRAM keeps GPU items out.

## Delivery to the agent

The kit already carries the `audio` job. A selected runtime adds one line to the session's kit text: its capability,
the absolute command path and its licence. Media-craft's "use one already installed first" then finds it. Nothing
stays running: each command exits (R125); measure an idle engine before and after to state the memory cost.

## First items (processor tier)

| id | What | Licence | Note |
| :--- | :--- | :--- | :--- |
| faster-whisper | Word timestamps and transcript checks | MIT | int8 on the processor; model files pinned by Hugging Face commit |
| fluidsynth | MIDI to audio | LGPL-2.1-or-later | Official release binary per OS, not rebuilt |
| musescore-general-sf2 | General MIDI SoundFont | MIT | One file, about 200 MB, pinned by URL and sha256 |
| kenney-audio | Recorded CC0 effect packs (Impact, Interface, UI, Digital) | CC0-1.0 | Zip files from kenney.nl, each pinned |
| speechmos | DNSMOS speech quality scores | MIT | Processor only |

GPU tier after that (only when VRAM fits): Chatterbox Multilingual (MIT), ACE-Step 1.5 (Apache-2.0 code), UTMOSv2.
Never listed: TangoFlux, AudioLDM2, MMAudio (non-commercial) and Sonniss (forbids use with AI).

## Screen

- A runtime card shows source, licence, size and, for GPU items, the memory it needs; installs run in the background
  like skills. New words go to `ui/locales/en.ts` first, then each language by hand.
- Non-commercial licences (`*-NC-*` in `license` or `weights_license`) get a warning line on the card and in its
  details before install, key `catalog.license.nonCommercial`: "For personal, non-commercial use only." Attribution
  licences (CC-BY, such as Audiobox Aesthetics) get `catalog.license.attribution`.
- GPLv3 tools (Pedalboard) are listed only as downloaded on request, never in a pack.

## Checks that must fail without it

- `engine/tests/tools.rs`: a runtime with a wrong sha256 leaves nothing installed; a right one installs, turns off,
  turns on and uninstalls through the existing journal; uv is called with arguments only, never a shell line.
- `engine-projects/src/tools/kits.rs`: an `audio` request selects processor runtimes; a GPU runtime is left out with
  unknown or too little VRAM and selected when it fits.
- `tests/ui/catalog.check.ts`: the NC warning shows on an NC card in all five languages; no warning on MIT.
