import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { AccountsSection } from '../../ui/Accounts.tsx';
import { api, type Message, type RunningItem, type VendorUsage } from '../../ui/api.ts';
import { applyLook } from '../../ui/appearance.ts';
import { LANGUAGES, t, templates, useLanguage } from '../../ui/i18n.ts';
import { vendorName } from '../../ui/models.ts';
import { Running } from '../../ui/Running.tsx';
import { seedQuotas } from '../../ui/sidebar.ts';
import { Thread } from '../../ui/Thread.tsx';
import { STOPS } from '../../ui/api.ts';
import { sala } from './appearance.check.ts';
import { check, equal, ok } from './check.ts';

type Tokens = Record<string, string>;
const css = themes.css as unknown as { themes: Record<string, Tokens>; presets: Record<string, Tokens> };
// The three looks and the three presets, high contrast among them: each as the shell would send it.
const LOOKS = [
  ...['sala', 'oficina', 'bancada'].map((name) => ({ name, vars: css.themes[name]!, scheme: 'dark' as const })),
  ...['manha', 'alto', 'cobalto'].map((name) => ({ name, vars: css.presets[name]!, scheme: name === 'manha' ? ('light' as const) : ('dark' as const) })),
];

const channels = (color: string): [number, number, number] => {
  const n = (color.match(/-?[\d.]+/g) ?? []).map(Number);
  const scale = color.startsWith('color(') ? 255 : 1;
  return [n[0]! * scale, n[1]! * scale, n[2]! * scale];
};
const luminance = (color: string) => {
  const [r, g, b] = channels(color).map((v) => { const c = v / 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};
const contrast = (a: string, b: string) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
};

/** A host that wears one look, as a preview does: its variables reach everything drawn inside it. */
function wear(look: (typeof LOOKS)[number]) {
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;inset:0;overflow:auto;background:var(--bg);';
  document.body.append(host);
  const base = sala();
  void applyLook({ ...base, vars: { ...base.vars, ...look.vars }, scheme: look.scheme, appearance: { ...base.appearance, base: look.name } }, host);
  return host;
}

/** The mark is a triangle (the others are never one), readable at 14 px, and 3:1 against every surface it sits on in this look. */
function judge(host: HTMLElement, where: string, within: Element) {
  const mark = within.querySelector<HTMLElement>('.mk.gemini');
  ok(mark, `${where}: Antigravity has its own mark`);
  const style = getComputedStyle(mark);
  ok(style.clipPath.startsWith('polygon'), `${where}: a drawn triangle, not the neutral dot (${style.clipPath})`);
  ok(mark.getBoundingClientRect().width >= 8, `${where}: wide enough to read`);
  const ink = style.backgroundColor;
  for (const surface of ['--bg', '--s1', '--s2', '--s3']) {
    const probe = document.createElement('i');
    probe.style.background = `var(${surface})`;
    host.append(probe);
    const ratio = contrast(ink, getComputedStyle(probe).backgroundColor);
    probe.remove();
    ok(ratio >= 3, `${where}: the mark holds ${ratio.toFixed(2)}:1 on ${surface}, 3:1 is the least for a shape`);
  }
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('antigravity mark: a project reply, the running row and the usage bars draw their own mark and the name Antigravity, in every language and look', async () => {
  const was = api.setupCheck;
  api.setupCheck = async (vendor) => ({ vendor, installed: true, account: 'signedIn', canInstall: false });
  const question: Message = { id: 'q', role: 'user', text: 'Fix the date parsing', at: 0 };
  const answer: Message = { id: 'a', role: 'assistant', text: 'Fixed.', at: 1, status: 'done', vendor: 'gemini' };
  const running: RunningItem = { conversationId: 'c1', turnId: 'm1', title: 'Fix the date parsing', area: 'project', projectId: 'p1', state: 'working', since: 0, permissions: [], vendor: 'gemini' };
  const usage = (over: Partial<VendorUsage>): VendorUsage[] => [{
    vendor: 'gemini', plan: null, state: 'unknown', learning: false, relearning: false, foreignUse: false, overageOn: false, billedPerUse: false,
    billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0, counting: false, windows: [], ...over,
  }];
  // The same states the other AIs have: no reading, billed per use, out.
  const states: Array<[string, VendorUsage[], ReturnType<typeof seedQuotas>, 'usage.pill.unknown' | 'usage.pill.billed' | 'usage.pill.out']> = [
    ['no reading', usage({}), {}, 'usage.pill.unknown'],
    ['billed', usage({ billedPerUse: true, state: 'out' }), {}, 'usage.pill.billed'],
    ['out', usage({ state: 'out', windows: [{ name: 'seven_day', used: 1, elapsed: 0.5, expected: 0.5, resetsAt: 4_000_000_000, readAt: 1 }] }), seedQuotas([{ vendor: 'gemini', window: 'seven_day', used: 1, resetsAt: 4_000_000_000 }]), 'usage.pill.out'],
  ];
  try {
    for (const { code } of LANGUAGES) {
      for (const look of LOOKS) {
        useLanguage(code);
        const where = `${code} · ${look.name}`;
        const host = wear(look);
        const stops: Array<() => void> = [];
        try {
          const reply = document.createElement('div');
          const composer = document.createElement('div');
          composer.className = 'composer';
          host.append(reply, composer);
          stops.push(render(() => createComponent(Thread, { messages: [question, answer], from: 0, conversationId: 'antigravity-mark', options: [], onBranch: () => {} }), reply));
          const header = reply.querySelector('.model .vm');
          ok(header, `${where}: the reply header names Antigravity`);
          equal(header.querySelector('b')!.textContent, 'Antigravity', `${where}: a reply with no model named reads as the provider`);
          judge(host, `${where} · reply`, header);
          const name = getComputedStyle(header.querySelector('b')!).color;
          const surface = getComputedStyle(host).backgroundColor;
          ok(contrast(name, surface) >= 4.5, `${where}: the name reads at 4.5:1 (${contrast(name, surface).toFixed(2)})`);

          const column = document.createElement('div');
          host.append(column);
          stops.push(render(() => createComponent(Running, { items: [running], projects: [], onOpen: () => {}, onClose: () => {}, modal: false, ref: () => {} }), column));
          const row = column.querySelector('.ri .vm');
          ok(row, `${where}: the running row knows Antigravity`);
          equal(row.querySelector('b')!.textContent, 'Antigravity', `${where}: the running row's name`);
          judge(host, `${where} · running`, row);

          for (const [state, rows, quotas, pill] of states) {
            const cards = document.createElement('div');
            host.append(cards);
            const stop = render(() => createComponent(AccountsSection, { vendors: ['gemini'], quotas, usage: rows, now: 1_000_000, onManage: () => {} }), cards);
            try {
              await settle();
              const card = cards.querySelector<HTMLElement>('.acct[data-vendor="gemini"]');
              ok(card, `${where} · ${state}: the usage card`);
              equal(card.querySelector('h3')!.textContent, 'Antigravity', `${where} · ${state}: the card's name`);
              equal(card.querySelector('.acct-pill')!.textContent!.trim(), t(pill), `${where} · ${state}: the same state words as the others`);
              judge(host, `${where} · usage ${state}`, card.querySelector('h3')!);
              if (state === 'out') ok(card.querySelector('.urow'), `${where}: the bar of the out window`);
            } finally { stop(); cards.remove(); }
          }
        } finally {
          stops.forEach((stop) => stop());
          host.remove();
        }
      }
    }
  } finally {
    api.setupCheck = was;
    useLanguage('en');
  }
});

check('antigravity mark: the tray words sent in every language leave the AI to the engine, which names the third one Antigravity', () => {
  // The tray's text lives outside the page: the screen sends templates with {ai}, and `stopped_text` in host/src/tray/decide.rs
  // (tested there) fills in the name. Here: every template that names an AI keeps its slot, and the name it will get has no Gemini in it.
  for (const { code } of LANGUAGES) {
    for (const why of STOPS) {
      const text = templates(`tray.stopped.${why}`, code).other;
      if (!['pace', 'quota', 'billed'].includes(why)) continue;
      ok(text.includes('{ai}'), `${code}: tray.stopped.${why} names the AI`);
      const filled = text.replace('{ai}', vendorName('gemini'));
      ok(filled.includes('Antigravity') && !/gemini/i.test(filled), `${code}: ${filled}`);
    }
  }
});
