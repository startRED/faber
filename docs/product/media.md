# Media craftsmanship

How Faber's project agent makes narration, music and sound effects on its own, and checks them without hearing.

## Why

Narrated explainer videos made by Faber on 2026-11-13 had stretched, "laggy" speech and chiptune-like effects. The
causes, found by three research rounds that day:

- one TTS call per sentence with a forced duration, then silence trimming, so each sentence restarted its intonation;
- a voice cloned from a synthetic reference;
- music and effects synthesized from sine waves and noise in code;
- music released between sentences, so it swelled in every pause, and audio that was measured but never judged.

## Original skills, existing delivery

Two more original instruction skills ride the same first-party `guide-plugin` delivery as the
[frontend pair](frontend.md): Claude Code in Projects discovers them as native skills (descriptions only until
used), Codex and Chat sessions get one more sentence in the guide's index line, and the text fallback carries a short
media checklist. They add no setting, card, tool, process or permission.

| Skill | When it applies | Source |
| :--- | :--- | :--- |
| media-craft | Before making narration, voice-over, music or sound effects | [Instructions](../../engine-projects/src/frontend-skills/media-craft/SKILL.md) |
| media-proof | Before saying narration, music or effects are done | [Instructions](../../engine-projects/src/frontend-skills/media-proof/SKILL.md) |

The request always wins: a voice, tool or method it names is used. The agent never asks the person for their voice,
a recording or consent; it uses the engine's built-in voices or voice design, and clones only from a real recording
the person supplied.

## The recipe (media-craft)

- The voice is the clock: whole script first, one TTS call per paragraph at natural speed, no forced duration or
  time-stretch, engine silence-trimming off, 5 to 20 ms fades, soxr resampling to 48 kHz. Word timestamps
  (faster-whisper or WhisperX) time every scene change; the film follows the voice.
- Music from instruments: a recorded track with a fitting licence, or MIDI rendered with FluidSynth and a General MIDI
  SoundFont (MuseScore General, MIT), ending exactly with the picture. Never raw oscillators.
- Effects from recorded CC0 sounds (Kenney packs), layered attack, body and tail, with real room reverb; a credits
  file lists every source and licence, and non-commercial material is named in the final message.
- Mix: voice high-pass, de-esser and 3:1 compression; music 12 to 18 dB under the voice, ducked about 10 dB on the
  word timestamps and held through pauses shorter than 0.85 s; two-pass loudnorm to −14 LUFS and −1 dBTP, checking
  that the second pass stayed linear.

## The proof (media-proof)

Results and command output go to `.faber/verify/listen` in the working folder, the same sandbox-readable place as
`.faber/verify/look`. In order: signal checks with ffmpeg (loudness, clipping, clicks, length, pumping), word checks
with faster-whisper or WhisperX (missing words, gaps over 0.6 s inside a sentence, cut edges, sync), then advisory
scores (DNSMOS through speechmos on the processor; UTMOSv2 and Audiobox Aesthetics when they fit). A check that
cannot run is written as not run and the agent goes on; it never stops the job. The scores stay advisory until they
are calibrated on known good and bad examples. `report.md` lists each problem with its exact time for the reviewer.

## Kit matching

Requests about narration, voice-over, music, sound effects, podcasts or audiobooks in the five screen languages carry
the `audio` job (`engine-projects/src/tools/kits/matching.rs`), so catalog items of that job join the kit on their own.
A video or page without sound does not. Audio runtimes are installed on demand through Catalog, from their original
sources with exact sizes and SHA-256 pins. The session kit lists each selected capability, absolute command and licence;
the skills use the installed one first. Processor tools join audio jobs; CUDA tools join only when the cached available
video memory fits. Unknown memory keeps them out. See [catalog design](../design/catalog.md#audio-runtimes).

The processor tier offers faster-whisper with a local multilingual tiny model and int8 processing, official Windows
FluidSynth, MuseScore General SF2, four recorded Kenney CC0 packs, and DNSMOS through speechmos. The CUDA tier offers
UTMOSv2, Audiobox Aesthetics and Chatterbox Multilingual with a built-in voice. Scorers emit advisory results until
calibrated. Audiobox weights require CC-BY-4.0 attribution, carried in both the kit and the score output.

Python tools use the person's uv and Python 3.12. Faber never installs uv or Python: an absent uv opens its own installation
page. Dependencies are downloaded and checked before an offline, hash-locked uv installation in the item's private,
relocatable environment. No shell, build backend or model service runs. Weights and auxiliary files are pinned individually;
launchers load the checked local files and disable automatic model downloads. No voice sample or other material is requested.

## Memory and limits

The skill bodies are static text read only when used; discovery grows by two descriptions or one index sentence. Every
model the skills mention runs as a one-shot command that exits (R125). Delivery checks
(`engine-projects/src/guide.rs`, `engine/src/engine/kits.rs`) establish that both AIs receive the skills, not that the
sound is better: that needs the paired video runs in [benchmark](../dev/benchmark.md#request-coverage).
