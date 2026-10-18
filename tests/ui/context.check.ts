import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { ContextMeter } from '../../ui/Context.tsx';
import type { ContextFill } from '../../ui/api.ts';
import { NEAR, near, percentOf, share } from '../../ui/context.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
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
  const dispose = render(() => createComponent(ContextMeter, { get fill() { return fill(); } }), root);
  try {
    await settle();
    const meter = () => root.querySelector<HTMLElement>('[data-testid=context]')!;
    equal(meter().textContent?.trim(), t('ctx.share', { pct: 50 }));
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
  } finally {
    useLanguage('en');
    dispose();
    root.remove();
  }
});
