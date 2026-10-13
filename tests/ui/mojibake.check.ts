import { LANGUAGES } from '../../ui/i18n.ts';
import { mojibake } from '../ux/mojibake.ts';
import { check, ok } from './check.ts';

// UX sentinel T1, the part that needs no window. The screen suite runs the same function over the text on screen.

check('T1 mojibake: garbled text is found, and real accents, quotes and symbols are not', () => {
  // The planted twins: each of these is something that reached the owner's screen or could.
  for (const garbled of ['Chat â€º Pergunta', 'Aï¿½o', 'AÃ§Ã£o', 'Ã©', 'Â· ready', 'â€œquotedâ€\u009D', '�', 'Ãƒâ€š']) {
    ok(mojibake(garbled), `not found in ${JSON.stringify(garbled)}`);
  }
  for (const fine of ['Chat › Pergunta', 'Ação', 'Größe und Maße', 'Ça va ? « oui »', '日本語', 'a — b … c', 'Español ñ', 'ß', '€ 5', '']) {
    ok(!mojibake(fine), `wrongly found in ${JSON.stringify(fine)}: ${mojibake(fine)}`);
  }
});

check('T1 mojibake: no text of any language is garbled', () => {
  const found: string[] = [];
  for (const { code, messages } of LANGUAGES) {
    for (const [key, entry] of Object.entries(messages) as Array<[string, string | { one: string; other: string }]>) {
      for (const text of typeof entry === 'string' ? [entry] : [entry.one, entry.other]) {
        const why = mojibake(text);
        if (why) found.push(`${code} ${key}: ${why}`);
      }
    }
  }
  ok(found.length === 0, found.slice(0, 5).join('; '));
});

check('T1 mojibake: no source file holds garbled text', async () => {
  // Raw sources of the screen, the engine and the shell: strings the screen can get from Rust are in the last two.
  const files = {
    ...import.meta.glob('../../ui/**/*.{ts,tsx,css}', { query: '?raw', import: 'default' }),
    ...import.meta.glob('../../engine/src/**/*.rs', { query: '?raw', import: 'default' }),
    ...import.meta.glob('../../src-tauri/src/**/*.{rs,json}', { query: '?raw', import: 'default' }),
  } as Record<string, () => Promise<string>>;
  const found: string[] = [];
  for (const [path, load] of Object.entries(files)) {
    const lines = (await load()).split('\n');
    lines.forEach((line, at) => {
      const why = mojibake(line);
      if (why) found.push(`${path.replace('../../', '')}:${at + 1} ${why}`);
    });
  }
  ok(Object.keys(files).length > 20, `the scan found only ${Object.keys(files).length} files`);
  ok(found.length === 0, found.slice(0, 5).join('; '));
});
