import { check, equal, frames, ok } from './check.ts';

check('motion: narrow sidebar stays visible until its exit finishes', async () => {
  const frame = document.createElement('iframe');
  frame.style.cssText = 'position:fixed;left:0;top:0;width:720px;height:480px;border:0';
  document.body.append(frame);
  try {
    const page = frame.contentDocument!;
    const view = frame.contentWindow!;
    const style = page.createElement('style');
    // Use the loaded product rules, including the checks page's system-motion override.
    style.textContent = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules].map((rule) => rule.cssText)).join('\n');
    page.head.append(style);
    page.body.innerHTML = '<div class="shell"><aside class="side shown"></aside></div>';
    const side = page.querySelector<HTMLElement>('.side')!;
    for (const mode of ['full', 'reduced', 'none']) {
      page.documentElement.dataset.motion = mode;
      side.classList.add('shown');
      equal(view.getComputedStyle(side).visibility, 'visible', `${mode}: opening is immediately visible`);
      side.getAnimations().forEach((animation) => animation.finish());
      await frames();
      side.classList.remove('shown');
      view.getComputedStyle(side).opacity;
      const animations = side.getAnimations();
      if (mode === 'full') {
        ok(animations.some((animation) => (animation as CSSTransition).transitionProperty === 'transform'), 'full motion must actually transition');
        for (const animation of animations) {
          animation.pause();
          animation.currentTime = Number(animation.effect!.getComputedTiming().endTime) / 2;
        }
        equal(view.getComputedStyle(side).visibility, 'visible', 'the closing sidebar must remain painted during its exit');
        const opacity = Number(view.getComputedStyle(side).opacity);
        ok(opacity > 0 && opacity < 1, 'the checked frame is inside the fade');
        for (const animation of animations) animation.finish();
      } else {
        equal(animations.length, 0, `${mode}: closing must be instant`);
      }
      equal(view.getComputedStyle(side).visibility, 'hidden', `${mode}: a completed exit is hidden`);
      equal(view.getComputedStyle(side).opacity, '0');
    }
  } finally {
    frame.contentDocument?.getAnimations().forEach((animation) => animation.cancel());
    frame.remove();
  }
});
