import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

writeFileSync(join(process.argv[2], 'settings.json'), JSON.stringify({ version: 1, theme: 'dark', shortcuts: { save: 'Ctrl+S' }, zoom: 0, experimental: { enabled: false } }, null, 2) + '\n');
writeFileSync(join(process.argv[2], 'notes.txt'), 'These settings belong to the user. Preserve unknown preferences.\n');
