import { createSignal } from 'solid-js';
import { lang } from './i18n.ts';
import { html } from './markdown.tsx';

// Reading an answer aloud with the browser's own voices (Web Speech): nothing is downloaded and nothing leaves the PC. One answer
// speaks at a time; its button is a switch, and the same press stops it.
const [speaking, setSpeaking] = createSignal<string>();

/** The id of the message being read aloud now, if any. */
export { speaking };

export const canSpeak = () => typeof speechSynthesis !== 'undefined' && typeof SpeechSynthesisUtterance !== 'undefined';

// A press of "stop", the end of the last part and an error all end the same run; a run that was already replaced ignores them.
let run = 0;

const PART = 220;

/** What is worth hearing in an answer, in parts a voice can take: its words without the code blocks, cut at sentences. */
export function spoken(markdown: string): string[] {
  const page = new DOMParser().parseFromString(html(markdown), 'text/html');
  page.querySelectorAll('pre').forEach((block) => block.remove());
  const parts: string[] = [];
  for (const paragraph of (page.body.textContent ?? '').split(/\n+/)) {
    let part = '';
    for (const sentence of paragraph.trim().split(/(?<=[.!?…])\s+/)) {
      if (part && part.length + sentence.length > PART) {
        parts.push(part);
        part = '';
      }
      part += (part ? ' ' : '') + sentence;
    }
    if (part.trim()) parts.push(part);
  }
  return parts;
}

/** The voice that speaks the screen's language: the exact one (pt-BR) before another of the same language (pt-PT), one on this PC before one that needs the network. */
export function voiceFor(code: string, voices: SpeechSynthesisVoice[]): SpeechSynthesisVoice | undefined {
  const same = (v: SpeechSynthesisVoice) => v.lang.replace('_', '-').toLowerCase();
  const want = code.toLowerCase();
  const near = voices.filter((v) => same(v).split('-')[0] === want.split('-')[0]);
  return near.find((v) => same(v) === want && v.localService) ?? near.find((v) => same(v) === want) ?? near.find((v) => v.localService) ?? near[0];
}

export function stopSpeech(id: string): void {
  if (speaking() !== id) return;
  run++;
  speechSynthesis.cancel();
  setSpeaking(undefined);
}

/** Reads `markdown` aloud as message `id`, or stops it when that message is the one being read. */
export function toggleSpeech(id: string, markdown: string): void {
  if (!canSpeak()) return;
  if (speaking() === id) return stopSpeech(id);
  const parts = spoken(markdown);
  if (!parts.length) return;
  const mine = ++run;
  speechSynthesis.cancel();
  setSpeaking(id);
  const voice = voiceFor(lang(), speechSynthesis.getVoices());
  const over = () => {
    if (run !== mine) return;
    run++;
    speechSynthesis.cancel();
    setSpeaking(undefined);
  };
  parts.forEach((part, i) => {
    const utterance = new SpeechSynthesisUtterance(part);
    utterance.lang = voice?.lang ?? lang();
    if (voice) utterance.voice = voice;
    utterance.onerror = over;
    if (i === parts.length - 1) utterance.onend = over;
    speechSynthesis.speak(utterance);
  });
}
