import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, VendorUsage } from '../../ui/api.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { Settings } from '../../ui/Settings.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { ageOf, freshness } from '../../ui/sidebar.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const NOW = 1_800_000_000_000;
const at = (secondsAgo: number) => NOW / 1000 - secondsAgo;

const usage = (over: Partial<VendorUsage> & { readAt?: number; uncounted?: number | null } = {}): VendorUsage[] => [{
  vendor: 'codex', plan: 'pro', state: 'ok', learning: false, relearning: false, outsideShare: null, foreignUse: false,
  overageOn: false, billedPerUse: false, billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0,
  counting: over.counting ?? false,
  windows: [{ name: 'seven_day', used: 0.44, elapsed: 0.5, expected: 0.5, resetsAt: NOW / 1000 + 86_400, readAt: over.readAt ?? at(240), uncounted: over.uncounted ?? null }],
}];

const quotas = { 'codex:seven_day': { vendor: 'codex' as const, window: 'seven_day', used: 0.44, resetsAt: NOW / 1000 + 86_400 } };

function sidebar(u: VendorUsage[], withQuota = true) {
  const host = document.createElement('div');
  const dispose = render(() => createComponent(Sidebar, {
    conversations: [], projects: [], vendors: ['codex'], version: 'test', place: { area: 'chat' }, running: [],
    quotas: withQuota ? quotas : {}, usage: u, now: NOW,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), host);
  return { host, dispose, text: () => host.querySelector('.usage')!.textContent ?? '' };
}

check('quota: a reading says its age in plain words', () => {
  deepEqual(ageOf(at(10), NOW), { key: 'quota.age.now', n: 0 });
  deepEqual(ageOf(at(240), NOW), { key: 'quota.age.min', n: 4 });
  deepEqual(ageOf(at(2 * 3600 + 120), NOW), { key: 'quota.age.hour', n: 2 });
  deepEqual(ageOf(at(3 * 86_400), NOW), { key: 'quota.age.day', n: 3 });
  deepEqual(ageOf(at(-500), NOW), { key: 'quota.age.now', n: 0 }, 'a clock that went back is not an age from the future');
});

check('quota: freshness takes the newest window, shows a number only where there is a conversion', () => {
  const none = freshness(usage({ counting: true }), 'codex');
  deepEqual([none.counting, none.uncountedPoints], [true, undefined], 'the marker without a number');
  const some = freshness(usage({ counting: true, uncounted: 0.02 }), 'codex', 'seven_day');
  equal(some.uncountedPoints, 2);
  equal(freshness(usage({ counting: true, uncounted: 0.001 }), 'codex').uncountedPoints, undefined, 'under half a point is not a number');
  deepEqual(freshness([], 'codex'), { counting: false });
});

check('quota: the sidebar shows the age, the marker and the not-yet-counted estimate (pt-BR)', () => {
  useLanguage('pt-BR');
  const s = sidebar(usage({ counting: true, uncounted: 0.02 }));
  try {
    const text = s.text();
    ok(text.includes('44%'), text);
    ok(text.includes('+~2%'), `the estimate sits beside the number: ${text}`);
    ok(text.includes('há 4 min'), text);
    ok(text.includes('ainda contando'), text);
  } finally { s.dispose(); }
});

check('quota: without recent work the number is final, and without a conversion no estimate is invented', () => {
  useLanguage('pt-BR');
  const calm = sidebar(usage({ readAt: at(10) }));
  try {
    ok(calm.text().includes('agora'), calm.text());
    ok(!calm.text().includes('ainda contando'), calm.text());
    ok(!calm.text().includes('+~'), calm.text());
  } finally { calm.dispose(); }
  const marker = sidebar(usage({ counting: true }));
  try {
    ok(marker.text().includes('ainda contando'), marker.text());
    ok(!marker.text().includes('+~'), 'the marker alone: no estimate without a measured conversion');
  } finally { marker.dispose(); }
  const unread = sidebar([], false);
  try {
    ok(!unread.text().includes('há'), 'a vendor never read has no age');
  } finally { unread.dispose(); }
});

check('quota: Settings spells out the estimate and the age', () => {
  useLanguage('pt-BR');
  const host = document.createElement('div');
  const props = { start: 'usage', look: { appearance: { base: 'sala' }, vars: {}, scheme: 'dark', motion: 'none', effects: {}, checks: [], notices: [] }, saved: {}, vendors: ['codex'],
    quotas, now: NOW, usage: usage({ counting: true, uncounted: 0.02 }), onClose: () => {}, onSave: async () => true, onOpenSetup: () => {} } as unknown as Parameters<typeof Settings>[0];
  const dispose = render(() => createComponent(Settings, props), host);
  try {
    const text = host.textContent!;
    ok(text.includes('+ ~2% ainda vai entrar na conta'), text);
    ok(text.includes('Atualizado há 4 min'), text);
    ok(text.includes('O número ainda pode subir'), text);
  } finally { dispose(); }
});

check('quota: Settings gives each window its own age, never the newest one', () => {
  useLanguage('pt-BR');
  const two = usage({ readAt: at(10) });
  two[0].windows.push({ name: 'five_hour', used: 0.1, elapsed: 0.5, expected: 0.5, resetsAt: NOW / 1000 + 3600, readAt: at(3 * 3600), uncounted: null });
  const host = document.createElement('div');
  const props = { start: 'usage', look: { appearance: { base: 'sala' }, vars: {}, scheme: 'dark', motion: 'none', effects: {}, checks: [], notices: [] }, saved: {}, vendors: ['codex'],
    quotas: { ...quotas, 'codex:five_hour': { vendor: 'codex' as const, window: 'five_hour', used: 0.1, resetsAt: NOW / 1000 + 3600 } }, now: NOW, usage: two, onClose: () => {}, onSave: async () => true, onOpenSetup: () => {} } as unknown as Parameters<typeof Settings>[0];
  const dispose = render(() => createComponent(Settings, props), host);
  try {
    const ages = [...host.querySelectorAll('.urow .age')].map((e) => e.textContent);
    deepEqual(ages.sort(), ['Atualizado agora', 'Atualizado há 3 h'].sort());
  } finally { dispose(); }
});

check('quota: Settings says running low only for a nearly used window, and fast use calmly (C002)', () => {
  const stateText = (state: VendorUsage['state']) => {
    const u = usage();
    u[0].state = state;
    const host = document.createElement('div');
    const props = { start: 'usage', look: { appearance: { base: 'sala' }, vars: {}, scheme: 'dark', motion: 'none', effects: {}, checks: [], notices: [] }, saved: {}, vendors: ['codex'],
      quotas, now: NOW, usage: u, onClose: () => {}, onSave: async () => true, onOpenSetup: () => {} } as unknown as Parameters<typeof Settings>[0];
    const dispose = render(() => createComponent(Settings, props), host);
    try { return host.querySelector('.usage-state')!.textContent!; } finally { dispose(); }
  };
  useLanguage('pt-BR');
  equal(stateText('fast'), 'Gastando rápido');
  equal(stateText('low'), 'Cota acabando');
  for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
    useLanguage(language);
    ok(!/acabando|running low|poca cuota|faible|knapp/i.test(stateText('fast')), `${language}: "${stateText('fast')}" must not alarm`);
  }
});

check('quota: Auto never alarms about quota that is far from the limit (C002)', () => {
  const reply = (kind: 'fast' | 'low') => ({ id: 'reply', role: 'assistant', text: 'Answer', at: 1, status: 'done', vendor: 'claude',
    reason: 'reason.route.code.medium', pace: { kind, vendor: 'codex' } }) as Message;
  const said = (kind: 'fast' | 'low') => {
    const host = document.createElement('div');
    const dispose = render(() => createComponent(Thread, { conversationId: 'chat', messages: [reply(kind)], from: 0, options: [], onBranch: () => {} }), host);
    try { return host.querySelector('.pace')!.textContent!; } finally { dispose(); }
  };
  useLanguage('pt-BR');
  equal(said('fast'), 'O Codex está gastando rápido esta semana; o Automático usou o Claude para poupar.');
  equal(said('low'), 'O Codex está perto do limite; o Automático usou o Claude.');
  for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
    useLanguage(language);
    ok(!/acabando|running out|running low|agotando|épuis|ausgeht/i.test(said('fast')), `${language}: "${said('fast')}" must not alarm`);
  }
});
