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

for (const [name, markup, selector] of [
  ['recoverable question errors', '<div class="ask"><p class="note err">Try again</p></div>', '.ask .note.err'],
  ['update entries including their stagger', '<div class="notes"><section class="notes-group"><h2 style="--i:0">Changes</h2><ul><li style="--i:20">Updated</li></ul></section></div>', '.notes-group h2, .notes-group li'],
]) {
  check(`motion: ${name} finish within 300ms in every factory theme`, () => {
    for (const [theme, overrides] of Object.entries(themes.css.themes)) {
      for (const motion of ['full', 'reduced', 'none']) {
        const root = document.createElement('div');
        root.dataset.base = theme;
        root.dataset.motion = motion;
        for (const [key, value] of Object.entries({ ...themes.css.base, ...overrides })) root.style.setProperty(`--${key}`, value);
        root.innerHTML = markup;
        document.body.append(root);
        try {
          for (const el of root.querySelectorAll(selector)) {
            const style = getComputedStyle(el);
            if (motion === 'full') {
              const total = (parseFloat(style.animationDuration) + parseFloat(style.animationDelay)) * 1000;
              ok(total > 0 && total <= 300, `${theme} ${selector} takes ${total}ms including stagger`);
            } else equal(style.animationName, 'none', `${theme} ${motion} disables ${selector}`);
          }
        } finally { root.remove(); }
      }
    }
  });
}

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
