import { check, equal, ok } from './check.ts';

// The checks that look at the real stylesheet (styles.css is part of the test build): a piece of markup is placed on the
// page for a moment, asked what the browser makes of it, and removed.

function onPage<T>(html: string, look: (root: HTMLElement) => T): T {
  const root = document.createElement('div');
  root.innerHTML = html;
  document.body.append(root);
  try {
    return look(root);
  } finally {
    root.remove();
  }
}

check('ambient: the promoted glow is bounded by its visible mask and keeps the beam aligned', () => {
  for (const width of [720, 1440]) {
    onPage(`<div class="ambient" style="position:relative;inset:auto;width:${width}px;height:900px;--amb-beam:block">
      <div class="amb amb-beam"><div class="beam"><i></i><i></i></div></div>
      <div class="amb amb-glow"><div class="amb-beam"><div class="beam"><i></i><i></i></div></div></div>
    </div>`, (root) => {
      const glow = root.querySelector<HTMLElement>('.amb-glow')!;
      const bounds = glow.getBoundingClientRect();
      // The existing mask's 170 px circle, centred 60 px above the stage, ends at y = 110.
      ok(bounds.width <= 340 && bounds.height <= 110, 'transparent mask pixels must not promote a stage-sized surface');
      const beam = root.querySelector('.amb-beam .beam')!.getBoundingClientRect();
      const highlight = glow.querySelector('.beam')!.getBoundingClientRect();
      for (const key of ['x', 'y', 'width', 'height'] as const) {
        ok(Math.abs(beam[key] - highlight[key]) < 0.5, `the clipped glow keeps the original beam ${key}`);
      }
    });
  }
});

check('sidebar: the "+" buttons are hidden by opacity, so the keyboard can still reach them', () => {
  onPage('<section class="projects"><div class="group-h"><button class="plus">+</button></div><div class="prow"><button class="plus">+</button></div></section>', (root) => {
    for (const plus of root.querySelectorAll<HTMLElement>('.plus')) {
      const style = getComputedStyle(plus);
      equal(style.visibility, 'visible', 'visibility:hidden would take it out of the tab order');
      equal(style.opacity, '0', 'it is only invisible until the pointer or the focus comes near');
      plus.focus();
      equal(document.activeElement, plus, 'it takes the focus');
    }
  });
});

check('sidebar: the search box shows a focus outline for the keyboard', () => {
  // A hidden test window never has the focus, so :focus-within cannot be triggered; the rule itself is read instead.
  const rules = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]).filter((r): r is CSSStyleRule => r instanceof CSSStyleRule);
  const rule = rules.find((r) => r.selectorText === '.search:focus-within');
  ok(rule, 'a rule for the focused search box');
  // With a var() inside, the shorthand stays whole in the style sheet.
  const outline = rule.style.getPropertyValue('outline');
  ok(/[2-9]px solid/.test(outline) && outline.includes('--accent-text'), `a visible outline in the accent colour, not "${outline}"`);
});

check('thread: a numbered list keeps the number it starts at', () => {
  onPage('<div class="md"><ol start="5"><li>five</li><li value="9">nine</li></ol></div>', (root) => {
    const li = root.querySelector('li')!;
    // The marker is drawn from the list item's own counter, which follows `start` and `value`; a counter of the list's own
    // would begin at 1 whatever the list says.
    ok(getComputedStyle(li, '::before').content.includes('list-item'), getComputedStyle(li, '::before').content);
    ok(!getComputedStyle(root.querySelector('ol')!).counterReset.includes('step'));
  });
});

check('first run: the primary button stays inside the stage at 1440x900 and at the minimum window, however tall the step is', () => {
  for (const [w, h] of [[1440, 900], [720, 480]] as const) {
    // The stage is the window minus the 52px stepper row; a step far taller than it must scroll under a footer that stays.
    onPage(
      `<div style="position:fixed;left:0;top:0;width:${w}px;height:${h}px;display:grid"><div class="fr-stage"><div class="fr-top"></div><div class="firstrun"><div style="height:2000px"></div><div class="first-foot"><button class="btn primary big">Continue</button></div></div></div></div>`,
      (root) => {
        const frame = root.firstElementChild!.getBoundingClientRect();
        const button = root.querySelector('.btn.primary')!.getBoundingClientRect();
        ok(button.top >= frame.top && button.bottom <= frame.bottom, `${w}x${h}: the button is at ${button.top}-${button.bottom}, the window at ${frame.top}-${frame.bottom}`);
      },
    );
  }
});
