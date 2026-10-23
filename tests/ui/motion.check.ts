import { createComponent, createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { animateDetails, animateList } from '../../ui/motion.ts';
import { check, equal, finished, frames, ok } from './check.ts';

// CI's machines ask Windows for reduced motion, and the stylesheet then cancels every transition and animation. The checks
// page hides that (tests/ui/system-motion.ts) and must never see it: the motion checks would stand aside or fail
// without saying so. A check of the reduced path sets it by itself.
check('motion: the test build never reports reduced motion, in the script or in the stylesheet', () => {
  ok(!matchMedia('(prefers-reduced-motion: reduce)').matches, 'the webview reports prefers-reduced-motion: reduce');
  const probe = document.createElement('div');
  probe.style.cssText = 'transition: opacity 1s; animation: none';
  document.body.append(probe);
  try {
    equal(getComputedStyle(probe).transitionDuration, '1s', 'the stylesheet cancels transitions as if the system asked for reduced motion');  } finally {
    probe.remove();
  }
});

check('motion: panel content staggers within 300ms and reduced motion disables all feedback', async () => {
  const root = document.createElement('div');
  root.className = 's-main';
  root.style.cssText = 'position:fixed;left:0;top:0;width:400px;height:300px';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<button>First</button><button>Second</button><button class="sw2">Third</button>';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    const animations = root.getAnimations({ subtree: true });
    equal(animations.length, 3);
    const timing = animations.map((a) => a.effect!.getComputedTiming());
    ok(Number(timing[1]!.delay) > Number(timing[0]!.delay) && Number(timing[2]!.delay) > Number(timing[1]!.delay));
    ok(timing.every((t) => Number(t.delay) + Number(t.duration) <= 300));
    for (const animation of animations) animation.finish();
    await finished(animations);
    root.dataset.motion = 'reduced';
    const button = root.querySelector('button')!;
    equal(getComputedStyle(button).transitionDuration, '0s');
    equal(getComputedStyle(root.querySelector('.sw2')!, '::after').transitionDuration, '0s');
    root.append(document.createElement('button'));
    button.remove();
    await new Promise((resolve) => setTimeout(resolve, 20));
    equal(root.getAnimations({ subtree: true }).length, 0);
    equal(document.querySelector('.motion-exit'), null);
  } finally {
    dispose();
    root.remove();
  }
});

check('motion: list exits honor the 80ms exit token and are shorter than their entrance', async () => {
  const root = document.createElement('div');
  root.className = 's-main';
  root.dataset.motion = 'full';
  root.style.cssText = 'position:fixed;left:10px;top:10px;width:240px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  // Keep the real timing tokens on the row so its exit copy retains them inside the isolated shadow tree.
  root.innerHTML = '<div data-motion-exit-timing style="height:40px;--dur-ui:220ms;--dur-exit:80ms">Timing probe</div>';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    const row = root.firstElementChild as HTMLElement;
    const entrance = row.getAnimations()[0];
    ok(entrance, 'the real list observer creates an entrance animation');
    const entranceMs = Number(entrance.effect!.getComputedTiming().duration);
    equal(entranceMs, 220, 'the entrance keeps its own token');
    entrance.finish();
    await finished([entrance]);
    row.remove();
    // MutationObserver delivers the exit before a rendering frame can finish the 80ms animation.
    await Promise.resolve();
    const copy = document.querySelector('.motion-exit')?.shadowRoot?.querySelector<HTMLElement>('[data-motion-exit-timing]');
    ok(copy?.inert && copy.getAttribute('aria-hidden') === 'true', 'inspect the real isolated inert exit copy');
    const exit = copy.getAnimations()[0];
    ok(exit, 'removing the row creates its real exit animation');
    exit.pause();
    const exitMs = Number(exit.effect!.getComputedTiming().duration);
    equal(exitMs, 80, `the 80ms exit token must not be raised by the entrance clamp (got ${exitMs}ms)`);
    ok(exitMs < entranceMs, `exit ${exitMs}ms is shorter than entrance ${entranceMs}ms`);
  } finally {
    dispose();
    root.remove();
  }
});

check('motion: closing a column keeps the surviving main width and uses the 80ms exit token', async () => {
  const root = document.createElement('div');
  root.className = 'shell';
  root.dataset.motion = 'full';
  root.style.cssText = 'position:fixed;left:10px;top:10px;width:600px;height:160px;grid-template-columns:0 minmax(0,1fr) 240px;transition:none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<main class="main" style="grid-column:2;margin:0;transition:none;--dur-ui:220ms;--dur-exit:80ms"></main><aside class="runcol" style="grid-column:3;margin:0;--dur-exit:80ms"></aside>';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    const main = root.querySelector<HTMLElement>('.main')!;
    const column = root.querySelector<HTMLElement>('.runcol')!;
    const before = main.getBoundingClientRect();
    const resting = { shadow: getComputedStyle(main).boxShadow, radius: getComputedStyle(main).borderRadius };
    ok(before.width > 0 && column.getBoundingClientRect().width > 0, 'both real grid columns have measurable width');
    equal(main.getAnimations().length, 0, 'the survivor starts at rest');
    root.style.gridTemplateColumns = '0 minmax(0,1fr) 0';
    column.remove();
    // Measure the new layout before MutationObserver installs the real closing-column animation.
    const target = main.getBoundingClientRect();
    ok(target.width > before.width, 'removing the right column gives the same main more room');
    await Promise.resolve();
    const animation = main.getAnimations()[0];
    ok(animation, 'the real list observer animates the surviving main');
    animation.pause();
    equal(root.querySelector('.main'), main, 'the same main owns the animation');
    equal(document.querySelector('.motion-exit')?.shadowRoot?.querySelector('.main'), null, 'the main is never cloned');
    const duration = Number(animation.effect!.getComputedTiming().duration);
    equal(duration, 80, 'closing-column survivors honor the exit token');
    ok(duration < 220, 'closure remains shorter than the entrance token');
    const keys = (animation.effect as KeyframeEffect).getKeyframes();
    ok(Math.abs(parseFloat(String(keys[0]!.width)) - before.width) < 0.5, 'width starts at the measured old size');
    ok(Math.abs(parseFloat(String(keys.at(-1)!.width)) - target.width) < 0.5, 'width ends at the measured target');
    const from = new DOMMatrix(String(keys[0]!.transform));
    ok(Math.abs(from.m41 - (before.x - target.x)) < 0.5, 'translation matches the actual layout delta, including zero');
    ok(new DOMMatrix(String(keys.at(-1)!.transform)).isIdentity, 'travel ends without translation or glyph scaling');
    const exits = document.querySelector('.motion-exit')?.shadowRoot?.querySelector<HTMLElement>('.runcol')?.getAnimations() ?? [];
    for (const active of [animation, ...exits]) active.finish();
    await finished([animation, ...exits]);
    equal(main.getAnimations().length, 0, 'the animation releases the surviving main');
    ok(Math.abs(main.getBoundingClientRect().width - target.width) < 0.5, 'layout owns the final width');
    equal(main.style.width, '', 'no inline animation width remains');
    equal(getComputedStyle(main).boxShadow, resting.shadow, 'the resting shadow returns');
    equal(getComputedStyle(main).borderRadius, resting.radius, 'the resting corners return');
    equal(document.querySelector('.motion-exit'), null, 'the removed column copy is cleaned up');
  } finally {
    dispose();
    root.remove();
  }
});

check('motion: removing a row has inert exits with full motion and stays instant with reduced motion', async () => {
  const preference = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches')!;
  try {
    for (const reduced of [false, true]) {
      Object.defineProperty(MediaQueryList.prototype, 'matches', { ...preference, get(this: MediaQueryList) {
        return this.media === '(prefers-reduced-motion: reduce)' ? reduced : preference.get!.call(this);
      } });
      const root = document.createElement('div');
      root.style.cssText = 'position:fixed;left:10px;top:10px;width:240px;display:grid';
      root.dataset.motion = 'full';
      for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
      root.innerHTML = '<div style="height:40px">first</div><div style="height:40px">second</div>';
      document.body.append(root);
      const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
      const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
      try {
        await settle(20);
        const row = root.lastElementChild as HTMLElement;
        const y = row.getBoundingClientRect().y;
        root.firstElementChild!.remove();
        await settle(20);
        const ghost = document.querySelector<HTMLElement>('.motion-exit');
        if (reduced) equal(ghost, null, 'reduced motion makes no exit copy');
        else {
          ok(ghost?.inert, 'the exit copy cannot receive clicks or focus');
          equal(ghost.getAttribute('aria-hidden'), 'true');
          ok(row.getAnimations().length > 0, 'the surviving row has a FLIP animation');
          ok(row.getBoundingClientRect().y > 10 && row.getBoundingClientRect().y <= y, 'the row moves through intermediate positions');
          const copy = ghost.shadowRoot!.querySelector<HTMLElement>('.motion-exit')!;
          await finished([...row.getAnimations(), ...copy.getAnimations()]);
          await settle(0);
          equal(document.querySelector('.motion-exit'), null, 'the copy is removed after exit');
        }
        equal(row.getAnimations().length, 0);
        equal(row.getBoundingClientRect().y, 10);
      } finally {
        dispose();
        root.remove();
      }
    }
  } finally {
    Object.defineProperty(MediaQueryList.prototype, 'matches', preference);
  }
});

check('motion: a tutorial ring keeps its own position when it appears', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px;height:400px';
  root.dataset.motion = 'full';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return undefined; }, root);
  try {
    const ring = document.createElement('div');
    ring.className = 'tour-ring';
    ring.style.cssText = 'width:100px;height:100px;transform:translate(180px,90px)';
    root.append(ring);
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(ring.getBoundingClientRect().x, 180, 'arrival must not replace the anchor transform');
    equal(ring.getBoundingClientRect().y, 90);
    // The ring fades in with its own opacity animation (styles.css); the list never animates its transform.
    equal(ring.getAnimations().filter((a) => (a.effect as KeyframeEffect).getKeyframes().some((k) => 'transform' in k)).length, 0);
  } finally {
    dispose();
    root.remove();
  }
});

// The stylesheet may move the sidebar by a transition of its own; this one stands without it, so what is left is the list.
check('motion: sidebar CSS motion is not replaced by list reflow', async () => {
  const root = document.createElement('div');
  root.className = 'shell';
  root.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
  root.dataset.motion = 'full';
  root.innerHTML = '<aside class="side" style="transition:none;transform:translateX(-40px)"></aside><main class="main"></main>';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    const side = root.querySelector<HTMLElement>('.side')!;
    side.style.transform = 'translateX(-30px)';
    root.append(document.createElement('div'));
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(side.getAnimations().length, 0, 'the list must leave the sidebar transform alone');
    equal(getComputedStyle(side).transform, 'matrix(1, 0, 0, 1, -30, 0)');
  } finally {
    dispose();
    root.remove();
  }
});

check('motion: container resize preserves FLIP until child layout really changes', async () => {
  const preference = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches')!;
  Object.defineProperty(MediaQueryList.prototype, 'matches', { ...preference, get(this: MediaQueryList) {
    return this.media === '(prefers-reduced-motion: reduce)' ? false : preference.get!.call(this);
  } });
  const root = document.createElement('div');
  root.className = 'welcome-grid';
  root.dataset.motion = 'full';
  root.style.cssText = 'position:fixed;left:10px;top:10px;width:700px;display:grid;grid-template-columns:600px 80px;gap:0';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<div style="height:40px">greeting</div><div style="height:40px">resume</div>';
  document.body.append(root);
  const dispose = render(() => { animateList(root); return [...root.children] as HTMLElement[]; }, root);
  try {
    await frames();
    root.style.gridTemplateColumns = '200px';
    root.style.width = '690px';
    await frames();
    const row = root.lastElementChild as HTMLElement;
    const animation = row.getAnimations()[0];
    ok(animation, 'a real container breakpoint starts FLIP');
    // Observer bookkeeping is independent of frame cadence, covered by the native screen check.
    animation.pause();
    animation.currentTime = 40;
    root.style.width = '680px';
    await frames();
    equal(row.getAnimations()[0], animation, 'visual travel is not another layout change');
    root.style.gridTemplateColumns = '600px 80px';
    root.style.width = '670px';
    await frames();
    const replacement = row.getAnimations()[0];
    ok(replacement && replacement !== animation, 'a genuine layout change retargets the active FLIP');
    await finished([replacement]);
    equal(row.getAnimations().length, 0);
    equal(row.getBoundingClientRect().x, 610);
    equal(row.getBoundingClientRect().y, 10);
  } finally {
    dispose(); root.remove();
    Object.defineProperty(MediaQueryList.prototype, 'matches', preference);
  }
});

check('motion: disclosures animate both ways, survive interruption and toggle instantly with reduced motion', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:10px;top:10px;width:240px';
  root.dataset.motion = 'full';
  root.innerHTML = '<details><summary>Details</summary><div style="height:80px">Content</div></details>';
  document.body.append(root);
  const dispose = render(() => { animateDetails(root); return [...root.children] as HTMLElement[]; }, root);
  const details = root.querySelector('details')!;
  const summary = root.querySelector('summary')!;
  const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  try {
    summary.click();
    equal(details.open, true);
    await settle(40);
    ok(details.getAnimations().length > 0);
    summary.click();
    await settle(250);
    equal(details.open, false, 'a click during entry closes the disclosure');
    equal(details.getAnimations().length, 0);
    root.dataset.motion = 'reduced';
    summary.click();
    equal(details.open, true);
    equal(details.getAnimations().length, 0);
    summary.click();
    equal(details.open, false);
  } finally {
    dispose();
    root.remove();
  }
});

check('motion: a hover never moves a row and no card arrives in more than 300ms', () => {
  // Cards a person picks from (themes, presets) may lift; anything in a list only changes colour.
  const lifts = new Set(['.tcard:hover', '.preset:hover .thumb']);
  const moving: string[] = [];
  const walk = (rules: CSSRuleList) => {
    for (const rule of rules) {
      if (rule instanceof CSSStyleRule && rule.selectorText.includes(':hover') && /translate|scale/.test(rule.style.transform)) {
        if (!rule.selectorText.split(',').every((s) => lifts.has(s.trim()))) moving.push(rule.selectorText);
      } else if ('cssRules' in rule && !(rule instanceof CSSStyleRule)) walk((rule as CSSGroupingRule).cssRules);
    }
  };
  for (const sheet of document.styleSheets) walk(sheet.cssRules);
  equal(moving.join(' | '), '');

  const root = document.createElement('div');
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<div class="toast"></div><div class="ask"></div><div class="dv-card"></div><div class="notes"></div><div class="ask-more"></div>';
  document.body.append(root);
  try {
    for (const el of root.children) {
      const ms = parseFloat(getComputedStyle(el).animationDuration) * 1000;
      ok(ms > 0 && ms <= 300, `${el.className} arrives in ${ms}ms`);
    }
  } finally {
    root.remove();
  }
});

check('motion: a disclosure chevron turns with a transform that eases, and nothing else about it changes', () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:10px;top:10px;width:240px';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = '<details class="disclosure"><summary>Work log</summary><div>Content</div></details>';
  document.body.append(root);
  try {
    const details = root.querySelector('details')!;
    const chevron = () => getComputedStyle(root.querySelector('summary')!, '::after');
    equal(chevron().transitionProperty, 'transform');
    ok(chevron().transitionDuration !== '0s', 'it takes time');
    const margin = chevron().marginTop;
    details.open = true;
    equal(chevron().marginTop, margin, 'opening moves it by the transform alone, not by its margin');
    root.dataset.motion = 'reduced';
    equal(chevron().transitionDuration, '0s', 'reduced motion turns it at once');
    // At rest, closed and open, so nothing is mid-way: the open one sits 2 px lower, where its old margin put it.
    equal(new DOMMatrix(chevron().transform).f, 2, 'open: the 2 px drop is in the transform');
    details.open = false;
    equal(new DOMMatrix(chevron().transform).f, 0, 'closed: no drop');
  } finally {
    root.remove();
  }
});

check('motion: the tutorial leaves with a fade of its own instead of vanishing', async () => {
  // Imported here, not at the top: as a static import of this file it kept the checks page from starting at all (no check ran).
  const { Tutorial } = await import('../../ui/Tutorial.tsx');
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:600px';
  root.dataset.motion = 'full';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const [open, setOpen] = createSignal(true);
  const dispose = render(() => {
    animateList(root);
    return createComponent(Show, { get when() { return open(); }, keyed: true, children: () => createComponent(Tutorial, { onEnd: () => {} }) });
  }, root);
  const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  try {
    await settle(30);
    ok(root.querySelector('.tour'), 'the tutorial is there');
    setOpen(false);
    await settle(30);
    equal(root.querySelector('.tour'), null, 'it is out of the screen at once, so nothing under it is blocked');
    const ghost = document.querySelector('.motion-exit')?.shadowRoot?.querySelector('.tour');
    ok(ghost, 'but a copy of it is still fading');
    equal(ghost!.getAnimations().length, 1);
    await settle(400);
    equal(document.querySelector('.motion-exit'), null, 'then the copy goes too');
  } finally {
    dispose();
    root.remove();
  }
});
