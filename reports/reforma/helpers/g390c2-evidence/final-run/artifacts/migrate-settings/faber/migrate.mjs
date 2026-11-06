import { readFileSync, writeFileSync, renameSync, unlinkSync, statSync } from 'node:fs';
import { randomUUID } from 'node:crypto';

try {
  if (process.argv.length !== 3) {
    throw new Error('Usage: node migrate.mjs <settings.json>');
  }

  const path = process.argv[2];
  const settings = JSON.parse(readFileSync(path, 'utf8'));
  if (!settings || Array.isArray(settings) || typeof settings !== 'object') {
    throw new Error('Settings must be a JSON object');
  }
  if (settings.version !== 1 && settings.version !== 2) {
    throw new Error('Unsupported settings version');
  }
  if (settings.version === 1) {
    if (typeof settings.theme !== 'string') {
      throw new Error('Version 1 requires a string theme');
    }
    settings.appearance = { theme: settings.theme };
    delete settings.theme;
    settings.version = 2;

    // Replace only after the complete migrated file has been written.
    const temporary = `${path}.${randomUUID()}.tmp`;
    let created = false;
    try {
      writeFileSync(temporary, `${JSON.stringify(settings, null, 2)}\n`, {
        flag: 'wx', mode: statSync(path).mode,
      });
      created = true;
      renameSync(temporary, path);
    } finally {
      if (created) {
        try { unlinkSync(temporary); } catch (error) {
          if (error.code !== 'ENOENT') throw error;
        }
      }
    }
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
