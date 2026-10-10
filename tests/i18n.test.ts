import { test } from 'node:test';
import assert from 'node:assert/strict';
import { de } from '../ui/locales/de.ts';
import { en } from '../ui/locales/en.ts';
import { es } from '../ui/locales/es.ts';
import { fr } from '../ui/locales/fr.ts';
import { ptBR } from '../ui/locales/pt-BR.ts';

const LOCALES = { 'pt-BR': ptBR, es, fr, de };
const slots = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
const forms = (entry: string | { one: string; other: string }) => (typeof entry === 'string' ? [entry] : [entry.one, entry.other]);

// The type checker already makes every language list every key; this checks what types cannot see.
test('every language fills every text and keeps the same placeholders as English', () => {
  for (const [code, messages] of Object.entries(LOCALES)) {
    for (const [key, source] of Object.entries(en)) {
      const entry = messages[key as keyof typeof en];
      const wanted = slots(forms(source).at(-1)!);
      for (const text of forms(entry)) {
        assert.ok(text.trim(), `${code} ${key} is empty`);
        for (const slot of slots(text)) assert.ok(wanted.includes(slot), `${code} ${key} has {${slot}}, which English does not`);
      }
      assert.deepEqual(slots(forms(entry).at(-1)!), wanted, `${code} ${key} lost a placeholder`);
    }
  }
});
