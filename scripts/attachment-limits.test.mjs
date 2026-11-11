import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const limit = (path, name) => {
  const match = read(path).match(new RegExp(`(?:const|static) ${name}(?:\\s*:\\s*usize)?\\s*=\\s*(\\d+)\\s*\\*\\s*1024\\s*\\*\\s*1024`));
  assert.ok(match, `${path} declares ${name} in MiB`);
  return Number(match[1]) * 1024 * 1024;
};

test('attachment budget agrees across the screen and engine and fits the smallest transport', () => {
  const bytes = limit('ui/Composer.tsx', 'MAX_ATTACHMENTS_BYTES');
  assert.equal(bytes, limit('engine-core/src/workspace.rs', 'MAX_ATTACHMENTS_BYTES'));
  const wire = Math.min(limit('electron/sidecar.ts', 'MAX_LINE'), limit('host/src/wire.rs', 'MAX_LINE'));
  const frame = JSON.stringify({ id: 1, cmd: 'send', args: { req: {
    area: 'chat', text: 'A message with an attachment.', choice: { vendor: 'auto' }, images: [],
    files: [1, 2].map((part) => ({ name: `Quarterly financial report part ${part}.pdf`, base64: Buffer.alloc(bytes / 2).toString('base64') })),
  } } });
  assert.ok(Buffer.byteLength(frame) < wire, 'a permitted attachment must fit the Electron/host request boundary');
});
