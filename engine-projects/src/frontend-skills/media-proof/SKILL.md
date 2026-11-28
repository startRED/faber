---
name: media-proof
description: Use before saying narration, music or sound effects are done: measure gaps, cut words, clicks, clipping, loudness and speech quality without hearing.
---

# Media proof

You cannot hear the result, so measure it, then judge what a person would notice. Scale the checks to the job: one
short clip needs the loudness, clipping and transcript checks only.

## Where results go

`<listen>` is `.faber/verify/listen` inside the working folder (create `.faber/verify` with a `.gitignore` holding
`*` when missing). Send every command's output to a file in `<listen>` and read the file, since a sandbox can
swallow printed output. Keep the report; delete large intermediate files when finished.

## Fall through, never stop

Run the checks in order. When one cannot run (a tool is missing, no graphics card, no network, the sandbox), write
"not run" and the reason, then go on to the next. A check that could not run never stops the job, is never a
finding about the audio, and is never a reason to ask the person to listen instead. Finish every check you can run
before the closing message.

## 1. Signal checks (ffmpeg and a short script)

Measure the final file and each stem.

- Loudness: `ffmpeg -i <file> -af loudnorm=I=-14:TP=-1:print_format=json -f null -`. Integrated within 1 LU of the
  target (−14 LUFS unless the request named another), true peak at or below −1 dBTP.
- Clipping: `-af astats` reports the peak and the samples at full scale; any sample at or above −0.1 dBFS is a defect.
- Clicks: read the samples (numpy or soundfile) and flag a jump between neighbouring samples larger than 0.3 of full
  scale where the surrounding level is low, and every clip start or end that is not faded to zero. List each time.
- Length: audio and picture lengths match within 50 ms; the music ends with the picture, with no hard cut (the last
  20 ms below −50 dB) and no sound after it.
- Pumping: in each pause shorter than 0.85 s, the music level must not rise more than 3 dB over the spoken parts.

## 2. Word checks (faster-whisper or WhisperX)

Transcribe the voice stem with word timestamps on the processor (int8, a small or medium model; WhisperX when
installed for tighter alignment), save it in `<listen>`, and compare it with the script.

- Missing, extra or misread words are defects; fix the text and generate that paragraph again.
- A gap longer than 0.6 s inside a sentence is a defect (stretched or broken speech). So is a word lasting more than
  about three times the median for its length.
- Edges: the first word starts at least 80 ms after its clip begins and the last ends at least 100 ms before it ends,
  so nothing is cut.
- Sync: every scene change or label tied to a word lands within 150 ms of that word.

## 3. Scores (advisory)

Use the installed one: when the session kit lists `speechmos`, `utmosv2` or `audiobox-aesthetics`, run each installed
command on the relevant generated stem and save its JSON output in `<listen>`. DNSMOS takes the voice stem;
UTMOSv2 takes the voice stem when its GPU fits; Audiobox takes the music/effects stems and final mix. Use the absolute
command from the kit, including its isolated Python flags and launcher, followed by the audio path. Never substitute
an unpinned download or ask the person for source material. If an installed scorer fails, record its error as not run
and continue. Copy Audiobox's attribution line into the credits file.

- DNSMOS through `speechmos` on the processor: signal, background and overall quality of the voice.
- UTMOSv2 for naturalness, only when a graphics card fits it.
- Audiobox Aesthetics for the production quality of music and effects (its licence asks for attribution).

These scores are not yet calibrated against known good and bad examples. Report them and compare versions of the
same job (a lower score after a change is a reason to look again), but never pass or fail delivery on a score alone
and never invent a threshold. Defects measured in parts 1 and 2 are real defects.

## Report

Write `<listen>/report.md`: for each stem, every problem with its exact time (mm:ss.mmm), what it is and the measure
that found it; then the scores; then the checks not run and why. Give this file to the reviewer. What a person would
notice (a click, a gap inside a sentence, a cut word, music swelling in a pause, clipping, a hard stop) is a problem
to fix whatever its size. Fix it, measure again, and stop after two rounds unless a defect remains. In the closing
message say what was measured, what was fixed and what stayed unmeasured.
