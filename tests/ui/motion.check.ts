import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { animateDetails, animateList } from '../../ui/motion.ts';
import { check, equal, ok } from './check.ts';

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
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) {
      equal(animations.length, 3);
      const timing = animations.map((a) => a.effect!.getComputedTiming());
      ok(Number(timing[1]!.delay) > Number(timing[0]!.delay) && Number(timing[2]!.delay) > Number(timing[1]!.delay));
      ok(timing.every((t) => Number(t.delay) + Number(t.duration) <= 300));
    }
    for (const animation of animations) animation.finish();
    await Promise.all(animations.map((a) => a.finished));
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
          await Promise.all([...row.getAnimations(), ...copy.getAnimations()].map((animation) => animation.finished));
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

check('motion: sidebar CSS motion is not replaced by list reflow', async () => {
  const root = document.createElement('div');
  root.className = 'shell';
  root.style.cssText = 'position:fixed;left:0;top:0;width:1200px;height:600px';
  root.dataset.motion = 'full';
  root.innerHTML = '<aside class="side" style="transform:translateX(-40px)"></aside><main class="main"></main>';
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
  const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  try {
    await frame(); await frame();
    root.style.gridTemplateColumns = '200px';
    root.style.width = '690px';
    await frame(); await frame();
    const row = root.lastElementChild as HTMLElement;
    const animation = row.getAnimations()[0];
    ok(animation, 'a real container breakpoint starts FLIP');
    // Observer bookkeeping is independent of frame cadence, covered by the native screen check.
    animation.pause();
    animation.currentTime = 40;
    root.style.width = '680px';
    await frame(); await frame();
    equal(row.getAnimations()[0], animation, 'visual travel is not another layout change');
    root.style.gridTemplateColumns = '600px 80px';
    root.style.width = '670px';
    await frame(); await frame();
    const replacement = row.getAnimations()[0];
    ok(replacement && replacement !== animation, 'a genuine layout change retargets the active FLIP');
    await replacement.finished;
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
    if (!matchMedia('(prefers-reduced-motion: reduce)').matches) ok(details.getAnimations().length > 0);
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
