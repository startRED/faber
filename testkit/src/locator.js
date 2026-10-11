// Sent to the page by the test client before it looks for anything. It finds elements the way a person describes them:
// by role and name, by label, by visible text or by CSS, narrowing step by step. It installs `window.__tk` once.
(() => {
  if (window.__tk) return;
  const norm = (s) => String(s ?? '').replace(/\s+/g, ' ').trim();
  const hidden = (el) => {
    for (let e = el; e; e = e.parentElement) {
      if (e.getAttribute('aria-hidden') === 'true') return true;
      const style = getComputedStyle(e);
      if (style.display === 'none' || style.visibility === 'hidden') return true;
    }
    return false;
  };
  const shown = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && !hidden(el);
  };
  const INPUT_ROLES = { checkbox: 'checkbox', radio: 'radio', button: 'button', submit: 'button', reset: 'button', range: 'slider' };
  const TAG_ROLES = { button: 'button', textarea: 'textbox', select: 'combobox', article: 'article', dialog: 'dialog', nav: 'navigation', ul: 'list', li: 'listitem' };
  const roleOf = (el) => {
    const given = el.getAttribute('role');
    if (given) return given;
    const tag = el.tagName.toLowerCase();
    if (/^h[1-6]$/.test(tag)) return 'heading';
    if (tag === 'input') return INPUT_ROLES[el.type] ?? 'textbox';
    if (tag === 'a') return el.hasAttribute('href') ? 'link' : '';
    return TAG_ROLES[tag] ?? '';
  };
  const textOfIds = (ids) => ids.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? '').join(' ');
  const nameOf = (el) => {
    if (el.getAttribute('aria-label')) return norm(el.getAttribute('aria-label'));
    if (el.getAttribute('aria-labelledby')) return norm(textOfIds(el.getAttribute('aria-labelledby')));
    if (el.labels?.length) return norm([...el.labels].map((l) => l.textContent).join(' '));
    return norm(el.textContent) || norm(el.getAttribute('title')) || norm(el.getAttribute('alt'));
  };
  const labelsOf = (el) => [
    el.getAttribute('aria-label'),
    el.getAttribute('aria-labelledby') ? textOfIds(el.getAttribute('aria-labelledby')) : null,
    ...(el.labels ? [...el.labels].map((l) => l.textContent) : []),
  ].filter(Boolean);
  // `want` is {text} (a case-insensitive substring, or the whole text when `exact`) or {re, flags}.
  const says = (text, want, exact) => {
    const seen = norm(text);
    if (want.re !== undefined) return new RegExp(want.re, want.flags ?? '').test(seen);
    const wanted = norm(want.text);
    return exact ? seen === wanted : seen.toLowerCase().includes(wanted.toLowerCase());
  };
  const inside = (els) => els.flatMap((e) => [...e.querySelectorAll('*')]);
  const step = (els, s) => {
    let found;
    if (s.k === 'css') found = els.flatMap((e) => [...e.querySelectorAll(s.sel)]);
    else if (s.k === 'role') found = inside(els).filter((e) => roleOf(e) === s.role && !hidden(e) && (!s.want || says(nameOf(e), s.want, s.exact)));
    else if (s.k === 'label') found = inside(els).filter((e) => !hidden(e) && labelsOf(e).some((t) => says(t, s.want, s.exact)));
    else if (s.k === 'text') {
      found = inside(els).filter((e) => !hidden(e) && says(e.textContent, s.want, s.exact) && ![...e.children].some((c) => says(c.textContent, s.want, s.exact)));
    } else if (s.k === 'has') found = els.filter((e) => norm(e.textContent).toLowerCase().includes(norm(s.text).toLowerCase()));
    else if (s.k === 'last') found = els.slice(-1);
    else throw new Error(`unknown locator step ${s.k}`);
    return [...new Set(found)];
  };
  const find = (steps) => steps.reduce(step, [document]);
  // The one element the steps name, or why there is none yet. `need`: 'attached', 'visible' or 'enabled'.
  const one = (steps, need) => {
    const els = find(steps);
    if (els.length === 0) return { why: 'no element matches' };
    if (els.length > 1) return { why: `${els.length} elements match, but one was expected` };
    const el = els[0];
    if (need !== 'attached' && !shown(el)) return { why: 'the element is not visible' };
    if (need === 'enabled' && (el.disabled || el.getAttribute('aria-disabled') === 'true')) return { why: 'the element is disabled' };
    return { el };
  };
  window.__tk = { find, one, shown };
})();
