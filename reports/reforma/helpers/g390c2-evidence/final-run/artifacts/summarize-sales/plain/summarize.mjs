import { readFileSync } from 'node:fs';

try {
  if (process.argv.length !== 3) throw new Error('Usage: node summarize.mjs <file.csv>');
  const lines = readFileSync(process.argv[2], 'utf8').split(/\r?\n/);
  if (lines.at(-1) === '') lines.pop();
  if (lines.shift() !== 'category,amount') throw new Error('Invalid header');

  const totals = new Map();
  const rowPattern = /^(?:"((?:[^"\r\n]|"")*)"|([^",\r\n]*)),(?:"((?:[^"\r\n]|"")*)"|([^",\r\n]*))$/;
  for (const [index, line] of lines.entries()) {
    const row = rowPattern.exec(line);
    if (!row) throw new Error(`Malformed CSV on line ${index + 2}`);
    const category = (row[1] ?? row[2]).replaceAll('""', '"');
    const amount = row[3] ?? row[4];
    if (!/^-?\d+\.\d{2}$/.test(amount)) throw new Error(`Invalid amount on line ${index + 2}`);
    const cents = BigInt(amount.replace('.', ''));
    totals.set(category, (totals.get(category) ?? 0n) + cents);
  }

  // Serialize integer cents directly so large totals never lose precision.
  console.log(`{${[...totals].map(([category, cents]) => `${JSON.stringify(category)}:${cents}`).join(',')}}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
