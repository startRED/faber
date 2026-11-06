import { readFileSync } from 'node:fs';

try {
  if (process.argv.length !== 3) throw new Error('Usage: node summarize.mjs <file.csv>');
  const lines = readFileSync(process.argv[2], 'utf8').split(/\r?\n/);
  if (lines.at(-1) === '') lines.pop();
  if (lines.shift() !== 'category,amount') throw new Error('Invalid header');
  const totals = new Map();
  const rowPattern = /^("(?:[^"\r\n]|"")*"|[^",\r\n]*),("(?:[^"\r\n]|"")*"|[^",\r\n]*)$/;
  const decode = (field) => field.startsWith('"') ? field.slice(1, -1).replaceAll('""', '"') : field;
  for (const [index, line] of lines.entries()) {
    const row = rowPattern.exec(line);
    if (!row) throw new Error(`Invalid CSV on line ${index + 2}`);
    const category = decode(row[1]);
    const amount = decode(row[2]);
    if (!/^-?\d+\.\d{2}$/.test(amount)) throw new Error(`Invalid amount on line ${index + 2}`);
    const cents = BigInt(amount.replace('.', ''));
    totals.set(category, (totals.get(category) ?? 0n) + cents);
  }
  console.log('{' + [...totals].map(([category, cents]) => `${JSON.stringify(category)}:${cents}`).join(',') + '}');
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
