import type { Results } from './api.ts';
import { duration, lang, money, t } from './i18n.ts';
import type { Key } from './locales/en.ts';

const time = (ms: number) => {
  const seconds = Math.round(ms / 1000);
  if (seconds < 3600) return duration(seconds);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const h = new Intl.NumberFormat(lang(), { style: 'unit', unit: 'hour', unitDisplay: 'short' }).format(hours);
  return minutes ? `${h} ${duration(minutes * 60)}` : h;
};

export function delivered(results: Results): string {
  const d = results.delivered;
  if (!d.files && !d.edits && !d.checksPassed && !d.commits && !d.prs && !d.artifacts && !d.answers && !d.concluded) {
    return t('results.nothing');
  }
  const parts: string[] = [];
  for (const [value, key] of [[d.files, 'results.files'], [d.checksPassed, 'results.checksPassed'], [d.checksFailed, 'results.checksFailed'], [d.commits, 'results.commits'], [d.prs, 'results.prs'], [d.artifacts, 'results.artifacts'], [d.answers, 'results.answers']] as [number, Key][]) {
    if (value) parts.push(t(key, { n: value }));
  }
  if (d.concluded) parts.push(t('results.concluded'));
  return parts.join(', ');
}

export function cost(results: Results): string {
  const { unpriced, turns, usd, workMs, byVendor } = results.cost;
  const vendors = (['claude', 'codex'] as const).flatMap((vendor) => {
    const item = byVendor[vendor];
    return item ? [`${vendor === 'claude' ? 'Claude' : 'Codex'} ${money(item.usd)}`] : [];
  });
  // Nothing priced at all: a "$0.00" would read as free.
  if (unpriced >= turns && usd === 0) return t('results.costDetailNone', { time: time(workMs) });
  const split = vendors.length > 1 ? `: ${vendors.join(', ')}` : '';
  return t(unpriced ? 'results.costDetailAtLeast' : 'results.costDetail', { time: time(workMs), amount: money(usd) }) + split;
}

export const resultsLine = (results: Results) => t('results.line', { delivered: delivered(results), cost: cost(results) });
