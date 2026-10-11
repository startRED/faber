import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { LANGUAGES, templates } from '../../ui/i18n.ts';
import { check, deepEqual, ok } from './check.ts';

const LOCALES = { 'pt-BR': ptBR, es, fr, de };
const slots = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
const forms = (entry: string | { one: string; other: string }) => (typeof entry === 'string' ? [entry] : [entry.one, entry.other]);

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

// The tray, the tooltip and the notification are filled in by the shell, so the screen must send them with their placeholders.
check('i18n: the shell texts reach the tray with {n} and {title} intact, in every language', () => {
  for (const { code } of LANGUAGES) {
    for (const form of ['one', 'other'] as const) {
      ok(templates('tray.running', code)[form].includes('{n}'), `${code} tray.running ${form} lost {n}`);
      ok(templates('tray.needsYou', code)[form].includes('{n}'), `${code} tray.needsYou ${form} lost {n}`);
    }
    ok(templates('tray.notify', code).other.includes('{title}'), `${code} tray.notify lost {title}`);
  }
});
