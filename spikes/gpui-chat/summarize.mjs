import { readFileSync } from 'node:fs';
const rows = readFileSync('out/measurements.txt', 'utf8').trim().split('\n').map(line => {
  const fields = Object.fromEntries([...line.matchAll(/(\w+)=([\d.]+)/g)].map(([, key, value]) => [key, Number(value)]));
  const stream = line.match(/stream_cpu_percent=Some\(([\d.]+)\)/);
  if (stream) fields.stream_cpu_percent = Number(stream[1]);
  return { label: line.split(' ')[0], ...fields };
});
for (const label of ['idle', 'bottom', 'top', 'stream']) {
  const runs = rows.filter(row => row.label === label);
  if (runs.length !== 3) { console.log(`${label}: ${runs.length}/3 runs`); continue; }
  const median = name => runs.map(row => row[name]).sort((a, b) => a - b)[1];
  console.log(`${label}: first marker ${median('process_to_marker_ms')} ms; private working set ${median('private_mb')} MiB; idle CPU ${median('idle_cpu_percent')}%${label === 'stream' ? `; streaming CPU ${median('stream_cpu_percent')}%` : ''}`);
}

