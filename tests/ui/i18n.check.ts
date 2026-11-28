import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { amount, bytes, duration, fromEngine, lang, LANGUAGES, money, templates, useLanguage } from '../../ui/i18n.ts';
import { keysOf } from '../../ui/keys.ts';
import { mac } from '../../ui/WindowControls.tsx';
import { check, deepEqual, ok } from './check.ts';

const LOCALES = { 'pt-BR': ptBR, es, fr, de };
const slots = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
const forms = (entry: string | { one: string; other: string }) => (typeof entry === 'string' ? [entry] : [entry.one, entry.other]);

check('i18n: permission failures are readable in every language', () => {
  const before = lang();
  try {
    for (const { code, messages } of LANGUAGES) {
      useLanguage(code);
      for (const key of ['error.permission.stale', 'error.permission.undelivered']) {
        ok(fromEngine(key) !== key, `${code} displays the raw engine key ${key}`);
        const entry: unknown = Object.getOwnPropertyDescriptor(messages, key)?.value;
        ok(typeof entry === 'string' && entry.trim(), `${code} has no text for ${key}`);
      }
    }
  } finally {
    useLanguage(before);
  }
});

// The type checker already makes every language list every key; this checks what types cannot see.
check('i18n: every language fills every text and keeps the same placeholders as English', () => {
  for (const [code, messages] of Object.entries(LOCALES)) {
    for (const [key, source] of Object.entries(en)) {
      const entry = messages[key as keyof typeof en];
      const wanted = slots(forms(source).at(-1)!);
      for (const text of forms(entry)) {
        ok(text.trim(), `${code} ${key} is empty`);
        for (const slot of slots(text)) ok(wanted.includes(slot), `${code} ${key} has {${slot}}, which English does not`);
      }
      deepEqual(slots(forms(entry).at(-1)!), wanted, `${code} ${key} lost a placeholder`);
    }
  }
});

check('i18n: sizes, times, amounts and key names are written the way each language writes them', () => {
  const before = lang();
  try {
    useLanguage('en');
    ok(bytes(1536).startsWith('1.5'), bytes(1536));
    useLanguage('pt-BR');
    ok(bytes(1536).startsWith('1,5'), bytes(1536));
    ok(duration(200).startsWith('3') && duration(200).includes('20'), duration(200));
    ok(amount(0.0123, 4) === '0,0123', amount(0.0123, 4));
    useLanguage('de');
    ok(keysOf('new') === (mac ? '⌘N' : 'Strg N') && money(2) === '2,00 US$', `${keysOf('new')} ${money(2)}`);
  } finally {
    useLanguage(before);
  }
});

// The tray, the tooltip and the notification are filled in by the shell, so the screen must send them with their placeholders.
check('i18n: the shell texts reach the tray with {n} and {title} intact, in every language', () => {
  for (const { code } of LANGUAGES) {
    for (const form of ['one', 'other'] as const) {
      ok(templates('tray.running', code)[form].includes('{n}'), `${code} tray.running ${form} lost {n}`);
      ok(templates('tray.needsYou', code)[form].includes('{n}'), `${code} tray.needsYou ${form} lost {n}`);
    }
    ok(templates('tray.notify', code).other.includes('{title}'), `${code} tray.notify lost {title}`);
    const many = templates('tray.notifyMany', code).other;
    ok(many.includes('{title}') && many.includes('{n}'), `${code} tray.notifyMany lost {title} or {n}`);
  }
});

// An encoding slip once turned the accented letters of pt-BR, es, fr and de into "?" ("Permiss?es", "aprova??o", "bac ? sable").
// A real question mark ends a sentence or follows a space before a capital; it never sits inside a word.
const damaged = (text: string) => /\p{L}\?+\p{L}/u.test(text.replace(/https?:\/\/\S+/g, '')) || /\p{L} \?+ \p{Ll}/u.test(text);

check('i18n: no text has a "?" where an accented letter was lost', () => {
  const found: string[] = [];
  for (const { code, messages } of LANGUAGES) {
    for (const [key, entry] of Object.entries(messages) as Array<[string, string | { one: string; other: string }]>) {
      for (const text of forms(entry)) if (damaged(text)) found.push(`${code} ${key}: ${text.slice(0, 60)}`);
    }
  }
  ok(found.length === 0, found.slice(0, 5).join('; '));
});

check('i18n: the "?" check finds the damage it was written for and leaves real questions alone', () => {
  for (const bad of ['Permiss?es no Chat', 'aprova??o antes', 'Faber n?a pas pu', 'le bac ? sable', 'Passer ? ce mode']) ok(damaged(bad), bad);
  for (const fine of ['Ça va ? Oui.', 'Delete this chat?', 'Wirklich löschen?', '¿Seguro?', 'Quoi ?', 'https://example.com/a?b=c']) ok(!damaged(fine), fine);
});
