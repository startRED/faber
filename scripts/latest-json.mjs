import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PLATFORMS, parse } from './channel.mjs';

// Refuse partial or duplicate entries so a release cannot publish an unusable manifest.
export function merge(version, notes, pubDate, entries) {
  if (!parse(`v${version}`) || !Number.isFinite(Date.parse(pubDate))) throw new Error('Invalid version or publication date');
  const platforms = {};
  for (const { platform, signature, url } of entries) {
    if (!PLATFORMS.includes(platform) || platforms[platform] || typeof signature !== 'string' || !signature.trim()) throw new Error(`Invalid entry: ${platform}`);
    const asset = new URL(url);
    if (asset.protocol !== 'https:') throw new Error(`Invalid URL: ${url}`);
    platforms[platform] = { signature, url };
  }
  if (PLATFORMS.some((platform) => !platforms[platform])) throw new Error('Missing platform');
  return { version, notes, pub_date: pubDate, platforms };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [version, notesFile, directory] = process.argv.slice(2);
  const entries = readdirSync(directory).filter((name) => name.endsWith('.json')).map((name) => JSON.parse(readFileSync(path.join(directory, name), 'utf8')));
  process.stdout.write(`${JSON.stringify(merge(version, readFileSync(notesFile, 'utf8'), new Date().toISOString(), entries), null, 2)}\n`);
}
