// UX sentinel T1: text that was UTF-8 once, read as Windows-1252 and saved again (`â€º` for `›`, `Ã§` for `ç`), or
// that holds the replacement character. The same function runs in the `ui` suite over every string of the app and in
// the screen suite over the text on screen (`window.__faberMojibake`, test build only).

const decoder = new TextDecoder('windows-1252');
const strict = new TextDecoder('utf-8', { fatal: true });
// Character -> byte, built from the browser's own table so that 0x80 to 0x9F (`€`, `‚`, `…`, `™`...) are right.
const byteOf = new Map<string, number>();
for (let byte = 0; byte < 256; byte++) byteOf.set(decoder.decode(Uint8Array.of(byte)), byte);

/** Why `text` is garbled, or `undefined` when it reads fine. */
export function mojibake(text: string): string | undefined {
  if (text.includes('�')) return 'holds the replacement character U+FFFD';
  // A garbled letter is a run of characters that Windows-1252 has: a UTF-8 lead byte and its continuation bytes, which
  // decode to fewer characters than they were written with. Real accents (`ß`, `ã`) make a run of one, or bytes that
  // are no UTF-8 at all.
  for (const [run] of text.matchAll(/[^\u0000-\u007F]+/g)) {
    const chars = [...run];
    const bytes = chars.map((char) => byteOf.get(char));
    if (chars.length < 2 || bytes.some((byte) => byte === undefined)) continue;
    let fixed: string;
    try {
      fixed = strict.decode(Uint8Array.from(bytes as number[]));
    } catch {
      continue;
    }
    if ([...fixed].length < chars.length) return `${JSON.stringify(run)} reads as ${JSON.stringify(fixed)}`;
  }
  return undefined;
}
