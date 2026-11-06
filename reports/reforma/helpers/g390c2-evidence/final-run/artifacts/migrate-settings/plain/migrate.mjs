import { mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

try {
  if (process.argv.length !== 3) throw new Error('Usage: node migrate.mjs <settings.json>');
  const file = resolve(process.argv[2]);
  const settings = JSON.parse(readFileSync(file, 'utf8'));
  if (settings?.version !== 1 && settings?.version !== 2) {
    throw new Error('Unsupported settings version');
  }
  if (settings.version === 1) {
    if (typeof settings.theme !== 'string') throw new Error('Version 1 requires a string theme');
    settings.appearance = { theme: settings.theme };
    delete settings.theme;
    settings.version = 2;

    // Replace only after the complete migrated file has been written successfully.
    const temporary = mkdtempSync(join(dirname(file), '.migrate-'));
    try {
      const output = join(temporary, 'settings.json');
      writeFileSync(output, JSON.stringify(settings, null, 2) + '\n');
      renameSync(output, file);
    } finally {
      rmSync(temporary, { recursive: true, force: true });
    }
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
