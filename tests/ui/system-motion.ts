// CI's machines ask Windows for reduced motion, and the stylesheet then cancels every transition and animation. The checks
// page hides that preference: system-motion.js makes every script read "no preference" from the first line of the page, and
// this switches the stylesheet's reduced-motion rules off. A check of the reduced path still sets `data-motion` or mocks the
// media query itself. Only the checks page does this; the scenarios drive the real page and set the preference through the
// DevTools port (`system_motion`).

const REDUCE = '(prefers-reduced-motion: reduce)';

function mute(rules: CSSRuleList): void {
  for (const rule of rules) {
    if (rule instanceof CSSMediaRule) {
      if (rule.media.mediaText === REDUCE) rule.media.mediaText = 'not all';
      else mute(rule.cssRules);
    } else if ('cssRules' in rule) mute((rule as CSSGroupingRule).cssRules);
  }
}

export async function ignoreSystemMotion(): Promise<void> {
  // The stylesheet is a link in the page: wait for it before reading its rules.
  const deadline = performance.now() + 5000;
  while (performance.now() < deadline && [...document.querySelectorAll('link[rel="stylesheet"]')].some((link) => !(link as HTMLLinkElement).sheet)) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  for (const sheet of document.styleSheets) mute(sheet.cssRules);
}
