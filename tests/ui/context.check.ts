import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { ContextMeter } from '../../ui/Context.tsx';
import { api, type ContextFill } from '../../ui/api.ts';
import { heardParts, NEAR, near, percentOf, share } from '../../ui/context.ts';
import { catalogName, t, useLanguage } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('context: the share is the tokens held over the window, capped, and amber starts at the point the CLI compacts near', () => {
  equal(percentOf({ used: 150_000, window: 300_000 }), 50);
  equal(percentOf({ used: 15_002, window: 1_000_000 }), 2);
  equal(share({ used: 400_000, window: 300_000 }), 1, 'never beyond full');
  equal(share({ used: 5, window: 0 }), 1, 'a window of nothing counts as full, not as empty');
  ok(!near({ used: 254_000, window: 300_000 }), 'quiet below 85%');
  ok(near({ used: 255_000, window: 300_000 }), 'amber from 85%');
  equal(NEAR, 0.85);
});

check('context: the meter says the percent, keeps the tokens in its tooltip and turns amber only near the end', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [fill, setFill] = createSignal<ContextFill>({ used: 150_000, window: 300_000 });
  const dispose = render(() => createComponent(ContextMeter, { id: 'c1', busy: false, onFail: () => {}, get fill() { return fill(); } }), root);
  try {
    await settle();
    const meter = () => root.querySelector<HTMLElement>('[data-testid=context]')!;
    equal(meter().textContent?.trim(), 'Context 50%', 'the word beside the number, not only in the tooltip');
    equal(meter().title, t('ctx.tip', { used: '150,000', window: '300,000' }), 'the tokens held and the window');
    equal(meter().getAttribute('aria-label'), meter().title, 'read out the same way');
    ok(!meter().classList.contains('near'), 'half full is quiet (C002)');
    equal(meter().querySelector('.arc')?.getAttribute('stroke-dasharray'), '50 100', 'the ring is the same share');
    setFill({ used: 270_000, window: 300_000 });
    await settle();
    ok(meter().classList.contains('near'), 'near the point where it compacts itself, it turns amber');
    equal(meter().textContent?.trim(), t('ctx.share', { pct: 90 }));
    useLanguage('de');
    await settle();
    ok(meter().title.startsWith('Kontext:'), 'in the language of the screen');
    equal(meter().textContent?.trim(), 'Kontext 90 %');
  } finally {
    useLanguage('en');
    dispose();
    root.remove();
  }
});

check('context: a click opens what fills the context, part by part, and Compact now waits while the AI works', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [busy, setBusy] = createSignal(true);
  const dispose = render(
    () => createComponent(ContextMeter, { id: 'c2', fill: { used: 59_000, window: 150_000 }, onFail: () => {}, get busy() { return busy(); } }),
    root,
  );
  try {
    await settle();
    equal(root.querySelector('.ctx-panel'), null, 'closed until asked');
    const meter = root.querySelector<HTMLButtonElement>('[data-testid=context]')!;
    meter.click();
    await settle();
    const panel = root.querySelector('.ctx-panel')!;
    equal(meter.getAttribute('aria-expanded'), 'true');
    equal(panel.querySelector('.ctx-parts'), null, 'no parts before the CLI counted them');
    heardParts('c2', [{ key: 'ctx.part.system', tokens: 3500 }, { key: 'ctx.part.conversation', tokens: 40_000 }]);
    heardParts('other', [{ key: 'ctx.part.tools', tokens: 1 }]);
    await settle();
    equal(panel.querySelector('.ctx-parts'), null, 'parts of another conversation are not these');
    heardParts('c2', [{ key: 'ctx.part.system', tokens: 3500 }, { key: 'ctx.part.conversation', tokens: 40_000 }]);
    await settle();
    equal([...panel.querySelectorAll('.ctx-parts dt')].map((d) => d.textContent).join('|'), [t('ctx.part.system'), t('ctx.part.conversation')].join('|'));
    const compact = panel.querySelector<HTMLButtonElement>('.ctx-act button')!;
    equal(compact.textContent?.trim(), t('ctx.compact'));
    ok(compact.disabled, 'not while the AI works');
    equal(panel.querySelector('.ctx-hint')?.textContent, t('ctx.compactBusy'));
    setBusy(false);
    await settle();
    ok(!compact.disabled);
    equal(panel.querySelector('.ctx-hint')?.textContent, t('ctx.compactHint'));
    // Esc closes it and gives focus back to the meter.
    compact.focus();
    compact.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await settle();
    equal(meter.getAttribute('aria-expanded'), 'false');
    equal(document.activeElement, meter);
  } finally {
    dispose();
    root.remove();
  }
});

check('context: the panel names the catalog tools the conversation has now, why each was picked, and opens its Catalog detail', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const real = api.toolsInUse;
  const asked: string[] = [];
  let kit: [string, string][] = [['systematic-debugging', 'debugging'], ['design-taste-frontend', 'design']];
  api.toolsInUse = async (id) => { asked.push(id); return kit; };
  const opened: string[] = [];
  const dispose = render(
    () => createComponent(ContextMeter, { id: 'c3', fill: { used: 9_000, window: 150_000 }, busy: false, onFail: () => {}, onCatalog: (id: string) => opened.push(id) }),
    root,
  );
  try {
    await settle();
    equal(asked.length, 0, 'nothing is read before the panel opens');
    const meter = root.querySelector<HTMLButtonElement>('[data-testid=context]')!;
    meter.click();
    await settle();
    const panel = () => root.querySelector('.ctx-panel')!;
    equal(panel().querySelector('.ctx-head')?.textContent, t('ctx.tools'));
    const links = [...panel().querySelectorAll<HTMLButtonElement>('.ctx-kit .ctx-tool')];
    equal(links.map((b) => b.textContent?.trim()).join('|'), [catalogName('systematic-debugging'), catalogName('design-taste-frontend')].join('|'), 'the Catalog names');
    equal([...panel().querySelectorAll('.ctx-why')].map((s) => s.textContent).join('|'), [t('ctx.toolsWhy', { job: t('catalog.job.debugging') }), t('ctx.toolsWhy', { job: t('catalog.job.design') })].join('|'), 'why each was picked');
    equal(links[0].title, t('ctx.toolsOpen', { name: catalogName('systematic-debugging') }));
    await settle();
    equal(asked.join(','), 'c3', 'read once when it opens: no polling');
    links[1].click();
    await settle();
    equal(opened.join(','), 'design-taste-frontend', 'a name opens its Catalog detail');
    equal(meter.getAttribute('aria-expanded'), 'false', 'and the panel closes');
    kit = [];
    meter.click();
    await settle();
    equal(asked.join(','), 'c3,c3', 'each opening reads the tools again');
    equal(panel().querySelector('.ctx-kit'), null);
    equal(panel().querySelector('.ctx-none')?.textContent, t('ctx.toolsNone'), 'a calm line when none is in use');
    useLanguage('pt-BR');
    await settle();
    equal(panel().querySelector('.ctx-none')?.textContent, 'Nenhuma ferramenta do Catálogo em uso nesta conversa.', 'in the language of the screen');
  } finally {
    useLanguage('en');
    api.toolsInUse = real;
    dispose();
    root.remove();
  }
});
