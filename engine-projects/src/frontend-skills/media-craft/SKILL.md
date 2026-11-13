---
name: media-craft
description: Use before making narration, voice-over, music or sound effects for a video, page, game or podcast: the voice sets the clock, sounds are recorded.
---

# Media craft

General guidance for any work with sound. Use only the parts the job needs: one notification sound does not need a
mix plan, and one narrated sentence does not need music. The request always wins: when it names a voice, a tool, an
engine or a method (such as a given TTS command), use it and fit the rest of this guide around it.

## Make it alone

When the person asked for the finished work, make all of it. Never ask for their voice, a recording, a sample or
consent, and never stop to ask which voice they prefer. Use the engine's built-in voices or its voice design in the
language of the script. Clone a voice only when the person supplied a real human recording or asked for a clone,
and never clone from a synthetic voice: its defects add up.

## The voice is the clock

1. Write the whole script first. Split it into paragraphs (one idea, two to four sentences each).
2. Generate each paragraph in one call at the engine's natural speed. Never force a duration (`--duration`, a speed
   chosen to fill a slot) and never time-stretch the result (`atempo`) to fit a scene: stretched speech is the
   "laggy" voice people notice first. If a paragraph runs long, shorten the text or lengthen the scene.
3. Turn off engine post-processing that cuts silences (such as `postprocess_output=False`) and trim yourself: keep
   80 to 150 ms of room before the first word and after the last, add a 5 to 20 ms fade in and out at every cut, and
   resample with soxr to 48 kHz (`aresample=48000:resampler=soxr`). Keep float or 24-bit until the final encode.
4. Join paragraphs with 250 to 500 ms of silence (longer between sections), never with crossfades over speech.
5. Transcribe the joined narration with word timestamps (faster-whisper `word_timestamps=True`, or WhisperX) and
   save them beside it as `narration.words.json`. Time every scene change, label and highlight to the word it
   illustrates, within about 100 ms. The film is as long as the voice plus its tail, never the other way round.
6. Read the transcript back: a misread name, number or term is fixed in the text (spell it as it sounds) and the
   paragraph is generated again.

A voice-design prompt works only in the languages the model was trained on; in any other language pick a built-in
voice of that language.

## Music from instruments, never from oscillators

Never synthesize music or effects from raw sine, square, saw or noise waves in code. It always sounds like an 8-bit
game, whatever the filters. In this order:

1. A recorded track whose licence allows the use (CC0 or the catalog's runtimes), cut to the film.
2. Music written as MIDI (pretty_midi or mido) and rendered with FluidSynth and a General MIDI SoundFont such as
   MuseScore General (MIT). Pick key and tempo from the mood; give it an intro, a body and an ending; keep pads,
   piano or strings below the voice's range (avoid busy parts between 1 and 4 kHz under speech).
3. A local music model whose licence allows the use, run once and closed.

The music ends exactly at the end of the film: write the last bar to land there on a resolved chord, or fade the
last one to two seconds. No loop seam, no hard cut, no music after the picture ends.

## Effects recorded and layered

- Take recorded sounds with a clear licence: the Kenney packs (CC0), other CC0 libraries, or the catalog's. Choose by
  description (a CLAP model when one is installed, otherwise names and tags). Never use a source whose licence
  forbids the use, such as non-commercial sounds for commercial work or libraries that forbid use with AI.
- Build an effect from layers: a short attack (click, hit), a body and a tail. Add space with a real room impulse
  response (CC0) rather than a synthetic echo. Vary pitch by one or two semitones and gain on repeats so no two are
  identical.
- Place each on the frame of the event it belongs to. Fewer, meaningful effects beat a sound on every motion.
- Keep a `CREDITS` or `LICENSES` file listing every sound, voice and model with its source and licence. When anything
  used is non-commercial or needs attribution, say so in the final message.

## Mix

- Voice: high-pass at 80 Hz, a gentle de-esser and about 3:1 compression, for example
  `highpass=f=80,deesser=i=0.4,acompressor=threshold=-18dB:ratio=3:attack=5:release=80:makeup=2`.
- Music sits 12 to 18 dB below the voice. Duck it about 10 dB more while words are spoken, driven by the word
  timestamps rather than by a level follower: start the ramp 150 ms before the first word, hold 200 ms after the
  last, release over 500 ms. Stay down through pauses shorter than 0.85 s, so the music never swells between
  sentences. Write the gain as a volume envelope (a `volume` expression or per-segment gains).
- Effects sit at least 6 dB under the voice where they overlap it.
- Loudness in two passes: measure with `loudnorm=I=-14:TP=-1:LRA=11:print_format=json`, then apply with the
  measured values and `linear=true`. Read the second pass's `normalization_type`: if it fell back to `dynamic`,
  limit the peaks first or lower the range and run again, since dynamic mode pumps. Target −14 LUFS integrated and
  a true peak of −1 dBTP or lower, unless the request or platform names another.
- Deliver 48 kHz audio of the same length as the picture (AAC at 192 kbps or more in MP4 unless asked otherwise).
  Keep the stems (`voice.wav`, `music.wav`, `sfx.wav`) and the word timestamps next to the work for the proof.

## Tools and memory

Run every model as one command that exits when it is done; never leave a speech, music or scoring server running.
Prefer what already runs on the processor (faster-whisper int8, FluidSynth). Use a tool the request names or one
already installed first; install another only as current permissions allow, in an isolated environment (`uv venv`)
inside the working folder, never system-wide, and only after reading its licence. A tool that cannot run is
replaced by the next option above, not a reason to stop or to ask.

Before saying the sound is done, follow media-proof.
