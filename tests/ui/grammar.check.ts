import themes from '../../src-tauri/src/themes.json';
import { check, equal, ok } from './check.ts';

// One grammar (docs/design/looks.md, "One grammar"): the same control heights, text sizes and timings everywhere, read from the real
// stylesheet, and the timers and observers that must not run while nobody looks.

/** Every style rule of the page, with the groups (media, supports, container) opened. */
function styleRules(): CSSStyleRule[] {
  const found: CSSStyleRule[] = [];
  const walk = (rules: CSSRuleList) => {
    for (const rule of rules) {
      if (rule instanceof CSSStyleRule) {
        found.push(rule);
        if (rule.cssRules?.length) walk(rule.cssRules);
      } else if (!(rule instanceof CSSFontFaceRule) && 'cssRules' in rule) walk((rule as CSSGroupingRule).cssRules);
    }
  };
  for (const sheet of document.styleSheets) walk(sheet.cssRules);
  return found;
}

/** The text of a rule's declarations, where a shorthand that holds a var() is kept whole. */
const body = (rule: CSSStyleRule) => rule.style.cssText;
const describe = (rule: CSSStyleRule) => `${rule.selectorText} { ${body(rule).slice(0, 120)} }`;

check('grammar: the stylesheet has no pixel text size and no calc() around a text size token', () => {
  const bad: string[] = [];
  for (const rule of styleRules()) {
    const text = body(rule);
    if (/(^|;\s*)font-size:[^;]*\d(\.\d+)?px/.test(text) || /(^|;\s*)font:[^;]*(^|[\s/])\d*\.?\d+px/.test(text)) bad.push(`a pixel text size in ${describe(rule)}`);
    if (/calc\(\s*var\(--fs-[a-z0-9]+\)\s*[-+]\s*[\d.]+px\s*\)/.test(text)) bad.push(`a calc() around a text size in ${describe(rule)}`);
  }
  equal(bad.join('\n'), '', 'text sizes come from --fs-xs, --fs-sm, --fs-ui and the reading, lead and heading tokens');
});

check('grammar: no transition or animation holds a literal duration or curve; the timing tokens carry them', () => {
  const bad: string[] = [];
  const timing = /(^|;\s*)((transition|animation)(-duration|-delay|-timing-function)?):\s*([^;]*)/g;
  for (const rule of styleRules()) {
    for (const [, , property, , , written] of body(rule).matchAll(timing)) {
      // `animation: none` and `transition: none` are written out by the browser with the initial values: nothing literal was written.
      const value = written!.replace(/auto ease 0s 1 normal none running none|all 0s ease 0s/g, '');
      if (/(?<![\w.-])(?!0s\b)\d*\.?\d+m?s\b/.test(value)) bad.push(`a literal duration in ${property} of ${describe(rule)}`);
      if (/cubic-bezier\(|(^|[\s,])ease(-in|-out|-in-out)?(?=[\s,;]|$)/.test(value)) bad.push(`a literal curve in ${property} of ${describe(rule)}`);
    }
  }
  equal(bad.join('\n'), '', 'durations are var(--dur-*), curves var(--ease*) (steps() and linear are no curve)');
});

check('grammar: a row or a button does not brighten on hover, and only the scrim blurs what is behind it', () => {
  const bad: string[] = [];
  for (const rule of styleRules()) {
    if (/brightness\(/.test(body(rule))) bad.push(`a brightness filter in ${describe(rule)}`);
    if (/backdrop-filter/.test(body(rule)) && rule.selectorText !== '.scrim') bad.push(`a backdrop blur behind a near-opaque panel in ${describe(rule)}`);
  }
  equal(bad.join('\n'), '');
});

const HEIGHTS: [string, string][] = [
  ['.btn', '--h-md'], ['.btn.big', '--h-lg'], ['.ap-bar .btn', '--h-lg'], ['.iconbtn', '--h-md'], ['.chip', '--h-sm'], ['.chip.choose', '--h-sm'],
  ['.pill', '--h-sm'], ['.send', '--h-md'], ['.seg-ctl', '--h-md'], ['.field', '--h-md'], ['.field.sm', '--h-sm'], ['.field.hex', '--h-sm'],
  ['.newchat', '--h-lg'], ['.run-btn', '--h-md'], ['.plus', '--h-sm'], ['.toast button', '--h-sm'], ['.dot', '--h-sm'], ['.lt', '--h-sm'],
  ['.item', '--h-sm'], ['.nav .item', '--h-md'], ['.area', '--h-md'],
];

check('grammar: every control takes its height from one of the three tokens', () => {
  const rules = styleRules();
  const bad: string[] = [];
  for (const [selector, token] of HEIGHTS) {
    const own = rules.filter((r) => r.selectorText === selector && /(^|;\s*)height:/.test(body(r)));
    if (!own.length) bad.push(`${selector} has no height`);
    for (const rule of own) if (!new RegExp(`(^|;\\s*)height:[^;]*var\\(${token}\\)`).test(body(rule))) bad.push(`${selector} is not ${token}: ${describe(rule)}`);
  }
  // No control picks a height of its own beside the three.
  const control = /(^|[\s,>])(button|select|\.btn|\.chip|\.pill|\.field|\.send|\.iconbtn|\.seg-ctl|\.newchat|\.search|\.run-btn)\b/;
  for (const rule of rules) {
    const last = rule.selectorText.split(',').map((part) => part.trim().split(/[\s>+~]+/).pop()!);
    if (!last.some((compound) => control.test(compound))) continue;
    if (/(^|;\s*)(min-)?height:\s*[\d.]+px/.test(body(rule))) bad.push(`a pixel height in ${describe(rule)}`);
  }
  equal(bad.join('\n'), '');
});

function onPage<T>(html: string, look: (root: HTMLElement) => T): T {
  const root = document.createElement('div');
  root.style.cssText = '--dens: 1; position: fixed; left: 0; top: 0; width: 600px; display: grid; gap: 8px; justify-items: start';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.innerHTML = html;
  document.body.append(root);
  try {
    return look(root);
  } finally {
    root.remove();
  }
}

check('grammar: the controls drawn on the page are 28, 32 or 40 px high, and the ones side by side line up', () => {
  const sizes: [string, number][] = [
    ['<button class="btn">Go</button>', 32], ['<button class="btn big">Go</button>', 40], ['<button class="btn primary">Go</button>', 32],
    ['<button class="iconbtn">x</button>', 32], ['<span class="chip">Chip</span>', 28], ['<span class="chip choose"><select><option>One</option></select></span>', 28],
    ['<select class="pill"><option>One</option></select>', 28], ['<button class="send">Send</button>', 32],
    ['<div class="seg-ctl"><button>A</button><button aria-pressed="true">B</button></div>', 32], ['<input class="field">', 32],
    ['<input class="field sm">', 28], ['<label class="field search"><input type="search"></label>', 32], ['<button class="newchat">New</button>', 40],
    ['<div class="run-btn"><span class="seg">1</span></div>', 32], ['<button class="plus">+</button>', 28],
  ];
  for (const [html, height] of sizes) {
    onPage(html, (root) => {
      const box = root.firstElementChild!.getBoundingClientRect();
      equal(Math.round(box.height), height, html);
    });
  }
  // A button, a field and a segmented control in one row of Settings share their height.
  onPage('<div style="display:flex;gap:8px;align-items:center"><button class="btn">Go</button><input class="field"><div class="seg-ctl"><button>A</button><button>B</button></div></div>', (root) => {
    const heights = [...root.firstElementChild!.children].map((el) => Math.round(el.getBoundingClientRect().height));
    equal(new Set(heights).size, 1, `heights ${heights.join(', ')}`);
  });
});

check('grammar: every text field outside the composer is the one .field class', () => {
  const rules = styleRules().map((r) => r.selectorText);
  for (const gone of ['input.rename', '.env-tray input', '.ask .q-other', '.name-in', '.ny-custom input'])
    ok(!rules.includes(gone), `${gone} has no style of its own`);
});

check('grammar: a text field is quiet at rest like the search, and the Privacy facts sit inside their card with an option row\'s padding', () => {
  const rules = styleRules();
  const rule = (selector: string) => rules.find((r) => r.selectorText === selector);
  ok(rule('.field')?.style.boxShadow.includes('var(--line)'), 'the resting line of a field is the quiet one');
  ok(rule('.field:hover, .field:focus-within')?.style.boxShadow.includes('var(--control-line)'), 'the stronger line shows on hover and while typing');
  equal(rule('.st-card > .facts li')?.style.padding, rule('.st-opt')?.style.padding, 'a fact row and an option row share their padding');
});
