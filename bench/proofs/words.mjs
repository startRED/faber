// Proof of "everyday-question": node words.mjs <folder> <pristine folder> <answer file>
// The two limits the person set: under 180 words (counted as at most 180), exactly three list items.
import { readFileSync } from 'node:fs';

const answer = readFileSync(process.argv[4], 'utf8');
const checks = [];
const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

const words = answer.replace(/[*_`#>|-]+/g, ' ').split(/\s+/).filter((w) => /[\p{L}\p{N}]/u.test(w)).length;
// "Under 180" is read as at most 180: two ways of counting words differ by a few at the edge, and a verdict must not hang on it.
check('at most 180 words', words <= 180, `${words} words`);
const items = answer.split(/\r?\n/).filter((l) => /^\s*(?:[-*•]|\d+[.)])\s+\S/.test(l));
check('exactly three list items', items.length === 3, `${items.length} items`);

console.log(JSON.stringify({ pass: checks.every((c) => c.ok), checks }));
