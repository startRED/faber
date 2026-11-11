// One action of the agent on the page in Faber's browser (R161 step 7), run by `pane/mod.rs` in a world of its own inside
// the page (the page's scripts cannot change what this one calls): the function after `export default`, called with the
// request; the screen's checks import it as it is. It finds what to act on by the words a person sees, never by the page's
// code, and does nothing on a page that asks for a login, a captcha or a payment: that is the person's turn. It answers
// { ok, turn?, why?, seen? }, where `why` is a code `pane/mod.rs` turns into words and `seen` the words of the candidates.
export default (request) => {
  const norm = (text) => String(text || '').replace(/\s+/g, ' ').trim().toLowerCase();
  const seen = (el) => {
    const box = el.getBoundingClientRect();
    const style = getComputedStyle(el);
    return box.width > 0 && box.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
  };
  // Every element of the page (or of `from`), its open shadow roots and the frames of its own site, since a field can hide in
  // any of them.
  const roots = (from = document) => {
    const found = [];
    const walk = (root) => {
      found.push(root);
      for (const el of root.querySelectorAll('*')) {
        if (el.shadowRoot) walk(el.shadowRoot);
        if (el.tagName === 'IFRAME' || el.tagName === 'FRAME') {
          try {
            if (el.contentDocument) walk(el.contentDocument);
          } catch {
            // Another site's frame: judged by where it comes from, below.
          }
        }
      }
    };
    walk(from);
    return found;
  };
  const all = (selector, from = document) => roots(from).flatMap((root) => [...root.querySelectorAll(selector)]);
  const parts = (el) =>
    [
      ...[...(el.labels || [])].map((l) => l.innerText),
      el.getAttribute('aria-label'),
      ...(el.getAttribute('aria-labelledby') || '').split(' ').map((id) => document.getElementById(id)?.textContent),
      el.title,
      el.placeholder,
      el.name,
      el.id,
      el.getAttribute('autocomplete'),
    ].join(' ');
  const labelOf = (el) => norm(parts(el));
  // The same words with the joints of names opened (user_password, userPassword), so whole words can be matched.
  const wordsOf = (el) => norm(parts(el).replace(/([a-z])([A-Z])/g, '$1 $2').replace(/[_.-]+/g, ' '));
  const tokens = (el) => norm(el.getAttribute('autocomplete')).split(' ');
  // Whole words only: "Pass" is a secret, "Passenger name" on a travel form is not.
  const SECRET = /\b(password|passwd|pwd|pass|passcode|passphrase|passwort|senha|contrase[ñn]a|mot de passe|kennwort|pin|otp|one[- ]?time|2fa|verification code|token|secret|ssn|(api|access|auth|private|recovery|backup|sms|authentication) ?(key|code|phrase))\b/;
  const CARD = /card.?(number|no)|cc.?(num|number)|cvc|cvv|csc|security code|expir|n[uú]mero do cart[aã]o|tarjeta|carte bancaire|kartennummer|pr[uü]fnummer/;
  const PAY_HOST = /(^|\.)(stripe\.com|paypal\.com|paypalobjects\.com|braintreegateway\.com|braintree-api\.com|adyen\.com|klarna\.com|squareup\.com|checkout\.com|mercadopago\.com|pagseguro\.uol\.com\.br|razorpay\.com)$/;
  const CAPTCHA = /recaptcha|hcaptcha|turnstile|captcha|arkoselabs|funcaptcha/;
  const hostOf = (src) => {
    try {
      return new URL(src, location.href).hostname;
    } catch {
      return '';
    }
  };
  const isSecret = (f) =>
    f.type === 'password' ||
    tokens(f).some((t) => t === 'current-password' || t === 'new-password' || t === 'one-time-code') ||
    (getComputedStyle(f).webkitTextSecurity || 'none') !== 'none' ||
    SECRET.test(wordsOf(f));
  const isCard = (f) => tokens(f).some((t) => t.startsWith('cc-')) || CARD.test(labelOf(f));
  // Another site's frame cannot be read from here, so a large one inside a form may hold a card form.
  const unreadable = (f) => {
    try {
      return !f.contentDocument;
    } catch {
      return true;
    }
  };
  const bigForeign = (f) => {
    const box = f.getBoundingClientRect();
    return seen(f) && unreadable(f) && box.width >= 200 && box.height >= 100;
  };
  // The form an element belongs to, through the shadow roots it sits in.
  const formOf = (el) => {
    for (let n = el; n; ) {
      const form = n.form || n.closest?.('form');
      if (form) return form;
      n = n.getRootNode?.()?.host;
    }
    return null;
  };
  // The words that commit money: the person's turn even with a saved card and no card field.
  const COMMIT = /^(pay|buy|purchase|place (your )?order|complete (the )?(order|purchase)|confirm (the )?(order|purchase|payment)|checkout|check out|subscribe|donate|transfer|send money|pagar|comprar|finalizar (a )?compra|confirmar (o )?pagamento|assinar|doar|transferir|payer|acheter|commander|valider (la )?commande|s.abonner|bezahlen|kaufen|jetzt kaufen|bestellen|zahlungspflichtig bestellen|abonnieren)\b/;
  const commits = (el) => [el.innerText, el.textContent, el.value, el.getAttribute('aria-label'), el.title, el.alt].some((w) => COMMIT.test(norm(w)));
  // A control that sends its form: a button with no type, or one that is not valid, sends it too.
  const isSubmit = (el) => (el.tagName === 'BUTTON' && el.type === 'submit') || (el.tagName === 'INPUT' && (el.type === 'submit' || el.type === 'image'));
  // Whether sending this form pays: a submit control of it (in a shadow root, or tied by its `form` attribute too) uses the
  // words that commit money, or a large frame of another site is in it.
  const paysForm = (form) =>
    [...new Set([...form.elements, ...all('button, input', form), ...all('button, input').filter((c) => c.form === form)])].some((c) => isSubmit(c) && commits(c)) || all('iframe, frame', form).some(bigForeign);
  const turnOf = () => {
    // What is on view: a form a page keeps hidden cannot be typed in, and once it shows, the next action sees it. Payment
    // scripts leave hidden frames on ordinary pages, so only a frame on view is a payment; the commit words cover the rest.
    const fields = all('input, select, textarea, [contenteditable=""], [contenteditable="true"]').filter(seen);
    const frames = all('iframe, frame').filter(seen);
    // A large frame of another site in a form may hold a card form that cannot be read from here.
    if (fields.some(isCard) || frames.some((f) => PAY_HOST.test(hostOf(f.src)) || (bigForeign(f) && formOf(f)))) return 'a payment';
    const captcha = frames.some((f) => CAPTCHA.test(f.src || '') && !/size=invisible/.test(f.src || ''));
    if (captcha || all('.g-recaptcha:not([data-size=invisible]), .h-captcha, .cf-turnstile').some(seen)) return 'a captcha';
    if (fields.some(isSecret)) return 'a login';
    return '';
  };
  // The address the agent read, up to an empty fragment: a page that moved since is not the one it chose from.
  const strip = (url) => (String(url).indexOf('#') === String(url).length - 1 ? String(url).slice(0, -1) : String(url));
  const moved = () => typeof request.at === 'string' && strip(request.at) !== strip(location.href);
  const gone = () => ({ ok: false, why: 'moved', seen: [location.hostname] });

  // Where the agent acted (R161 step 9, browsing): a ring around the thing and a pointer at it, over the page, for the person
  // who takes the pane and for the picture taken after the action; it follows the thing as the page scrolls or the pane
  // changes size, and goes at the agent's next action. It is drawn once the action is done, so nothing of the page runs
  // between the checks and the action; from plain elements (never markup, never a custom element a page could define) under
  // a closed shadow root the page's scripts and Faber's reading cannot see into, in the top layer (over a modal dialog too),
  // in the document of the thing (a frame of the page's own site too); nothing in it animates. Each action runs in a world
  // of its own, so the ring carries the shell's random name and only a node with it is removed. Drawing it never stops an
  // action.
  const named = typeof request.mark === 'string' && /^[0-9a-f]{16}$/.test(request.mark) ? request.mark : '';
  const unmark = () => {
    try {
      if (named) for (const old of all(`div[data-faber-mark="${named}"]`)) old.remove();
    } catch {
      // A ring that cannot be removed is only a drawing.
    }
  };
  const mark = (el) => {
    unmark();
    if (!named || !el.isConnected || !seen(el)) return;
    try {
      const doc = el.ownerDocument;
      const view = doc.defaultView;
      const host = doc.createElement('div');
      host.setAttribute('data-faber-mark', named);
      host.setAttribute('popover', 'manual');
      host.style.cssText = 'all: initial; position: fixed; inset: 0; pointer-events: none; z-index: 2147483647;';
      const shadow = host.attachShadow({ mode: 'closed' });
      const ring = doc.createElement('div');
      const svg = 'http://www.w3.org/2000/svg';
      const pointer = doc.createElementNS(svg, 'svg');
      pointer.setAttribute('viewBox', '0 0 20 20');
      const path = doc.createElementNS(svg, 'path');
      path.setAttribute('d', 'M2 2 L2 16 L6 12 L9 18 L11.5 17 L8.5 11 L14 11 Z');
      path.setAttribute('fill', '#4c8dff');
      path.setAttribute('stroke', '#fff');
      path.setAttribute('stroke-width', '1.5');
      path.setAttribute('stroke-linejoin', 'round');
      pointer.append(path);
      shadow.append(ring, pointer);
      const stop = new AbortController();
      const place = () => {
        if (!host.isConnected) return stop.abort();
        const r = el.getBoundingClientRect();
        ring.style.cssText = `position: fixed; left: ${r.left - 4}px; top: ${r.top - 4}px; width: ${r.width + 8}px; height: ${r.height + 8}px; box-sizing: border-box; border: 2px solid #4c8dff; border-radius: 6px; background: rgba(76, 141, 255, 0.12);`;
        pointer.style.cssText = `position: fixed; left: ${r.left + r.width / 2}px; top: ${r.top + r.height / 2}px; width: 20px; height: 20px; overflow: visible;`;
      };
      doc.documentElement.append(host);
      host.showPopover?.();
      place();
      view.addEventListener('resize', place, { signal: stop.signal });
      view.addEventListener('scroll', place, { signal: stop.signal, capture: true, passive: true });
    } catch {
      // A page that refuses the drawing still gets the action.
    }
  };

  // The controls of the page as the agent reads them: each has a number that holds from one read to the next while it keeps
  // its role and its name, so the agent clicks and types by number and a click never lands on a look-alike. Faber keeps the
  // numbers between calls (`memo`, carried by the host per tab and answered back whole); nothing of it lives in the page.
  const flat = (text) => String(text || '').replace(/\s+/g, ' ').trim();
  const clip = (text, max) => (text.length > max ? `${text.slice(0, max - 1)}…` : text);
  const KINDS = new Set(['button', 'link', 'tab', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'checkbox', 'radio', 'switch', 'option', 'combobox', 'textbox', 'searchbox', 'slider', 'spinbutton', 'treeitem']);
  const FIELDS = new Set(['textbox', 'searchbox', 'spinbutton', 'combobox', 'slider']);
  const CONTROLS = 'a[href], button, summary, input, select, textarea, [role], [contenteditable=""], [contenteditable="true"], [tabindex], [onclick]';
  const MAX_CANDIDATES = 2000;
  const MAX_LISTED = 250;
  const MAX_BLOCK = 16000;
  const roleOf = (el) => {
    const given = norm(el.getAttribute('role')).split(' ')[0];
    if (KINDS.has(given)) return given;
    const tag = el.tagName;
    if (tag === 'A') return el.hasAttribute('href') ? 'link' : '';
    if (tag === 'BUTTON' || tag === 'SUMMARY') return 'button';
    if (tag === 'SELECT') return 'combobox';
    if (tag === 'TEXTAREA') return 'textbox';
    if (tag === 'INPUT') {
      const type = String(el.type || 'text').toLowerCase();
      if (type === 'hidden') return '';
      if (/^(checkbox|radio)$/.test(type)) return type;
      if (/^(button|submit|reset|image)$/.test(type)) return 'button';
      if (type === 'range') return 'slider';
      if (type === 'number') return 'spinbutton';
      if (type === 'search') return 'searchbox';
      return type === 'file' ? 'file' : 'textbox';
    }
    const editable = norm(el.getAttribute('contenteditable'));
    if (editable === '' || editable === 'true') return el.hasAttribute('contenteditable') ? 'textbox' : '';
    return el.hasAttribute('onclick') || (el.hasAttribute('tabindex') && el.tabIndex >= 0) ? 'clickable' : '';
  };
  const nameOf = (el) => {
    const ids = flat(el.getAttribute('aria-labelledby')).split(' ').filter(Boolean);
    const root = el.getRootNode();
    let text = ids.map((id) => root.getElementById?.(id)?.textContent || '').join(' ');
    text = flat(text) || flat(el.getAttribute('aria-label'));
    if (!text && el.labels?.length) text = flat([...el.labels].map((l) => { const c = l.cloneNode(true); c.querySelectorAll('select, input, textarea, button').forEach((x) => x.remove()); return c.textContent; }).join(' '));
    if (!text && el.tagName === 'INPUT' && /^(button|submit|reset)$/.test(el.type)) text = flat(el.value);
    if (!text && !/^(INPUT|SELECT|TEXTAREA)$/.test(el.tagName)) text = flat(el.innerText || el.textContent);
    if (!text) text = flat(el.getAttribute('alt') || el.querySelector?.('img[alt]')?.getAttribute('alt') || el.title || el.placeholder || el.getAttribute('name'));
    return clip(text, 80);
  };
  // The words of the row, card or form a control sits in: told apart by them when several controls read the same.
  const rowOf = (el) => {
    const holder = el.closest('tr, li, [role=row], [role=listitem], article, fieldset, section, form');
    if (!holder) return '';
    const head = holder.querySelector('h1, h2, h3, h4, h5, h6, [role=heading]');
    return clip(flat(head ? head.innerText : holder.innerText), 40);
  };
  const valueOf = (el, role) => {
    if (!FIELDS.has(role) && role !== 'textbox') return '';
    const field = /^(INPUT|TEXTAREA)$/.test(el.tagName);
    // A password, a card or a one-time code never leaves the page: only that there is something in it (a custom box too).
    if (isSecret(el) || isCard(el)) return (field || el.tagName === 'SELECT' ? el.value : el.textContent) ? '[hidden]' : '';
    if (el.tagName === 'SELECT') return flat(el.selectedOptions?.[0]?.text);
    return clip(flat(field ? el.value : el.textContent), 60);
  };
  const marksOf = (el) => {
    const marks = [];
    const aria = (name) => el.getAttribute(name);
    if (el.disabled || aria('aria-disabled') === 'true') marks.push('disabled');
    if (el.checked === true || aria('aria-checked') === 'true') marks.push('checked');
    else if (aria('aria-checked') === 'mixed') marks.push('mixed');
    if (aria('aria-selected') === 'true') marks.push('selected');
    const open = aria('aria-expanded') ?? (el.tagName === 'SUMMARY' ? String(Boolean(el.parentElement?.open)) : null);
    if (open === 'true') marks.push('expanded');
    else if (open === 'false') marks.push('collapsed');
    if (el.required || aria('aria-required') === 'true') marks.push('required');
    if (el.readOnly) marks.push('readonly');
    if (aria('aria-invalid') === 'true') marks.push('invalid');
    if (el.getRootNode().activeElement === el) marks.push('focused');
    return marks;
  };
  // Every control on view, in page order, with the key that holds it from one read to the next: role and name, and the
  // words of its row when another control reads the same (then also its place among them).
  const controls = () => {
    const found = [];
    const chosen = new Set();
    for (const el of all(CONTROLS).slice(0, MAX_CANDIDATES)) {
      const role = roleOf(el);
      if (!role || !seen(el)) continue;
      const name = nameOf(el);
      // A button inside a link that reads the same is the same control.
      const up = el.parentElement?.closest('a[href], button, [role=button], [role=link]');
      if (up && chosen.has(up) && nameOf(up) === name) continue;
      chosen.add(el);
      found.push({ el, role, name });
    }
    const count = new Map();
    for (const c of found) count.set(`${c.role}|${c.name}`, (count.get(`${c.role}|${c.name}`) || 0) + 1);
    const place = new Map();
    for (const c of found) {
      const base = `${c.role}|${c.name}`;
      c.row = count.get(base) > 1 ? rowOf(c.el) : '';
      const stem = c.row ? `${base}|${c.row}` : base;
      const nth = place.get(stem) || 0;
      place.set(stem, nth + 1);
      c.key = `${stem}|${nth}`;
    }
    return found;
  };
  const line = (c) => {
    const value = valueOf(c.el, c.role);
    return `${c.role}${c.name ? ` "${c.name}"` : ''}${c.row ? ` in "${c.row}"` : ''}${marksOf(c.el).map((m) => ` ${m}`).join('')}${value ? ` = "${value}"` : ''}`;
  };
  // A small hash: enough to tell a changed line from an unchanged one.
  const hash = (text) => {
    let h = 2166136261;
    for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 16777619);
    return h >>> 0;
  };
  const MAX_LINES = 3000;
  const view = () => {
    const before = request.memo && typeof request.memo === 'object' ? request.memo : {};
    const same = typeof before.href === 'string' && before.href.split('#')[0] === location.href.split('#')[0] && before.refs && typeof before.refs === 'object';
    const refs = same ? before.refs : {};
    let next = Number.isInteger(before.next) && before.next > 0 ? before.next : 1;
    const listed = [];
    let size = 0;
    let hidden = 0;
    const fresh = {};
    for (const c of controls()) {
      if (listed.length >= MAX_LISTED || size > MAX_BLOCK) {
        hidden++;
        continue;
      }
      if (refs[c.key] === undefined) refs[c.key] = next++;
      fresh[c.key] = refs[c.key];
      c.n = refs[c.key];
      c.text = line(c);
      size += c.text.length + 8;
      listed.push(c);
    }
    // A custom box that holds a secret is a control, never text: its words do not come back as a line either.
    const names = new Set([...listed.map((c) => c.name), ...all('[contenteditable], [role=textbox]', request.within || document).filter((e) => isSecret(e) || isCard(e)).map((e) => flat(e.textContent))]);
    const scope = request.within || document.body;
    const lines = (scope ? String(scope.innerText || '') : '')
      .split('\n')
      .map((l) => l.replace(/\s+/g, ' ').trim())
      .filter((l) => l && !names.has(l))
      .slice(0, MAX_LINES);
    const seenControls = {};
    for (const c of listed) seenControls[c.n] = hash(c.text);
    const memo = { href: location.href, next, refs: fresh, c: seenControls, t: lines.map(hash) };
    // What changed since the read before, when it was this same page: the controls whose line differs or are new, the numbers
    // that went, the text lines that are new and how many went.
    const changed = same && before.c ? listed.filter((c) => before.c[c.n] !== seenControls[c.n]) : listed;
    const gone = same && before.c ? Object.keys(before.c).filter((n) => seenControls[n] === undefined) : [];
    const left = new Map();
    if (same && Array.isArray(before.t)) for (const h of before.t) left.set(h, (left.get(h) || 0) + 1);
    const added = [];
    for (const l of lines) {
      const h = hash(l);
      if (left.get(h) > 0) left.set(h, left.get(h) - 1);
      else added.push(l);
    }
    const dropped = [...left.values()].reduce((a, b) => a + b, 0);
    const total = listed.length + lines.length;
    const diffed = same && before.c && Array.isArray(before.t) && changed.length + gone.length + added.length + dropped < total * 0.6;
    const out = [];
    if (diffed) {
      if (!changed.length && !gone.length && !added.length && !dropped) out.push('Nothing changed since your last read.');
      else {
        out.push(`Same page as your last read: only the changes follow, and every number holds. Unchanged: ${listed.length - changed.length} controls, ${lines.length - added.length} text lines.`);
        if (changed.length) out.push('Controls new or changed:', ...changed.slice(0, 60).map((c) => `[${c.n}] ${c.text}`), ...(changed.length > 60 ? [`(${changed.length - 60} more)`] : []));
        if (gone.length) out.push(`Controls gone: ${gone.slice(0, 40).join(', ')}${gone.length > 40 ? ` and ${gone.length - 40} more` : ''}.`);
        if (added.length && !request.controls) out.push('Text new:', ...added.slice(0, 80));
        if (added.length > 80 && !request.controls) out.push(`(${added.length - 80} more new lines)`);
        if (dropped && !request.controls) out.push(`Text gone: ${dropped} lines.`);
      }
    } else {
      out.push(listed.length ? 'Controls (click or type by `ref`, the number in brackets; a number holds while its control keeps its role and name):' : 'Controls: none on view.');
      for (const c of listed) out.push(`[${c.n}] ${c.text}`);
      if (hidden) out.push(`(${hidden} more controls not listed; scroll or open the part you need.)`);
      if (!request.controls && lines.length) out.push('Text:', ...lines);
    }
    const max = Number.isInteger(request.max) && request.max > 0 ? request.max : 24001;
    return { view: out.join('\n').slice(0, max), memo, diff: Boolean(diffed) };
  };
  // The control a number stands for on this page, or why none: a number that is not in the last read, or one whose control
  // is not there any more (it changed its role or its name), is never moved to another.
  const byRef = () => {
    const memo = request.memo;
    if (!memo || typeof memo.href !== 'string' || !memo.refs) return { why: 'noref' };
    if (strip(memo.href) !== strip(location.href)) return { why: 'moved', seen: [location.hostname] };
    const key = Object.keys(memo.refs).find((k) => memo.refs[k] === request.ref);
    const hit = key && controls().find((c) => c.key === key);
    return hit ? { one: hit.el, role: hit.role } : { why: 'ref' };
  };

  const turn = turnOf();
  if (request.kind === 'read') {
    const shown = request.view ? view() : {};
    if (request.watch) {
      // A picture is taken after this: what changes in the page until `settle` makes the picture unfit to keep.
      globalThis.faberWatch?.o.disconnect();
      const w = { n: 0, href: location.href, o: new MutationObserver((records) => (w.n += records.length)) };
      // Every root the checks read: the document, its open shadow roots and its own site's frames. A root added later comes
      // with a change in one already watched.
      for (const root of roots()) w.o.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
      globalThis.faberWatch = w;
    }
    return { ok: true, turn, ...shown };
  }
  if (request.kind === 'settle') {
    const w = globalThis.faberWatch;
    globalThis.faberWatch = undefined;
    let changed = true;
    if (w) {
      const pending = w.o.takeRecords().length;
      w.o.disconnect();
      changed = w.n + pending > 0 || w.href !== location.href;
    }
    return { ok: true, turn, changed };
  }
  if (request.kind === 'fits') {
    // Whether the page fits the screen's width: how far it runs past it, so it scrolls sideways, and the words of the first
    // few things a person sees cut by the right edge (partly on screen; one wholly off it is a closed menu or a slide).
    const width = document.documentElement.clientWidth || window.innerWidth;
    const wider = Math.max(0, Math.round(document.documentElement.scrollWidth - width));
    const cut = [];
    for (const el of document.body ? document.body.querySelectorAll('*') : []) {
      if (cut.length >= 3) break;
      if (el.children.length || !seen(el)) continue;
      const box = el.getBoundingClientRect();
      const words = String(el.innerText || el.getAttribute('aria-label') || el.getAttribute('alt') || '').replace(/\s+/g, ' ').trim().slice(0, 60);
      if (box.right > width + 1 && box.left < width && words && !cut.includes(words)) cut.push(words);
    }
    // Its pictures reach the agent, so a page with a form or a button on view that commits money is a payment here too,
    // even with a saved card and no card field: it gets no picture.
    const pays =
      all('form').some((f) => seen(f) && paysForm(f)) ||
      all('button, input[type=submit], input[type=image], [role=button]').some((c) => seen(c) && commits(c));
    return { ok: true, turn: turn || (pays ? 'a payment' : ''), wider, cut };
  }
  if (request.kind === 'scroll') {
    unmark();
    window.scrollBy(0, (request.down ? 1 : -1) * window.innerHeight * 0.8);
    return { ok: true, turn };
  }
  if (request.kind === 'back') {
    unmark();
    history.back();
    return { ok: true, turn };
  }
  if (turn) return { ok: false, turn };
  // A redirect between the agent's reading and its action must not send the action to another page.
  if (moved()) return gone();
  const want = norm(request.target);
  // The words of what sits around a thing, to tell apart things that read the same (two "Delete" buttons of two records).
  const near = (el) => norm(el.closest('tr, li, [role=row], [role=listitem], article, fieldset, section, form')?.innerText).slice(0, 60);
  // One match, or none: words that fit several different things are not a choice the page makes for the agent, whether the
  // whole words or a part of them, and however alike the words are.
  const pick = (things, words) => {
    if (!want) return {};
    for (const hits of [things.filter((el) => words(el) === want), things.filter((el) => words(el).includes(want))]) {
      // A thing that holds another that matches (a button inside a link) is the same choice.
      const alone = hits.filter((el) => !hits.some((o) => o !== el && el.contains(o)));
      if (alone.length > 1) {
        const names = alone.map(words);
        const apart = new Set(names).size === names.length;
        return { many: [...new Set(alone.map((el, i) => (apart ? names[i] : near(el) || names[i])))].slice(0, 5) };
      }
      if (alone.length) return { one: alone[0] };
    }
    return {};
  };
  if (request.kind === 'click') {
    let one;
    if (request.ref !== undefined) {
      const found = byRef();
      if (!found.one) return { ok: false, why: found.why, seen: found.seen };
      one = found.one;
    } else {
      const things = all('a, button, summary, label, input[type=submit], input[type=button], [role=button], [role=link], [role=tab], [role=menuitem], [role=checkbox], [role=option]').filter(seen);
      const words = (el) => norm(el.innerText || el.value || el.getAttribute('aria-label') || el.title);
      const { one: hit, many } = pick(things, words);
      if (many) return { ok: false, why: 'many', seen: many };
      if (!hit) return { ok: false, why: 'click' };
      one = hit;
    }
    // What the click reaches: the control a label stands for, or the button something inside it sits in; if that sends a form,
    // the click pays when the form does.
    const target = (one.tagName === 'LABEL' && one.control) || one;
    const sender = [target, target.closest('button')].find((e) => e && isSubmit(e));
    if (commits(one) || commits(target) || (sender?.form && paysForm(sender.form))) return { ok: false, turn: 'a payment' };
    // A picture of one control wants its box, disabled or not.
    if (request.box) {
      one.scrollIntoView({ block: 'center' });
      const b = one.getBoundingClientRect();
      return { ok: true, rect: [b.x, b.y, b.width, b.height].map(Math.round), x: scrollX, y: scrollY };
    }
    // Before the click: on view, enabled, and still. Faber reads the box with `probe`, waits a moment and clicks with the box it read.
    if (!seen(one)) return { ok: false, why: 'hidden' };
    if (target.matches(':disabled') || target.getAttribute('aria-disabled') === 'true') return { ok: false, why: 'disabled' };
    one.scrollIntoView({ block: 'center' });
    if (moved()) return gone();
    const box = one.getBoundingClientRect();
    const rect = [box.x, box.y, box.width, box.height].map(Math.round);
    if (request.probe) return { ok: true, rect };
    if (request.rect && request.rect.some((v, i) => Math.abs(v - rect[i]) > 1)) return { ok: false, why: 'moving' };
    one.click();
    mark(one);
    return { ok: true };
  }
  if (request.kind === 'label') {
    // The numbers of the last read drawn over their controls for one picture; `unlabel` takes them off again.
    if (!request.memo?.refs) return { ok: false, why: 'noref' };
    const layer = document.createElement('div');
    layer.id = request.mark + '-labels';
    layer.style.cssText = 'position: fixed; left: 0; top: 0; width: 100vw; height: 100vh; pointer-events: none; z-index: 2147483647;';
    let count = 0;
    for (const c of controls()) {
      const n = request.memo.refs[c.key];
      const b = c.el.getBoundingClientRect();
      if (n === undefined || !seen(c.el) || b.bottom < 0 || b.right < 0 || b.top > innerHeight || b.left > innerWidth) continue;
      const box = document.createElement('div');
      box.style.cssText = `position: fixed; left: ${b.left}px; top: ${b.top}px; width: ${b.width}px; height: ${b.height}px; outline: 2px solid #e11d48; outline-offset: -1px; box-sizing: border-box;`;
      const tag = document.createElement('span');
      tag.textContent = String(n);
      tag.style.cssText = 'position: absolute; left: 0; top: 0; background: #e11d48; color: #fff; font: 700 11px/1 sans-serif; padding: 2px 4px; border-radius: 0 0 4px 0;';
      box.append(tag);
      layer.append(box);
      count++;
    }
    document.documentElement.append(layer);
    return { ok: true, count };
  }
  if (request.kind === 'unlabel') {
    document.getElementById(request.mark + '-labels')?.remove();
    return { ok: true };
  }
  if (request.kind === 'at') {
    // A point the agent names by position is only for what has no control (a map, a drawing, a drag): a control is clicked by its number.
    if (turn) return { ok: false, turn };
    const hit = document.elementFromPoint(request.x, request.y);
    if (!hit) return { ok: false, why: 'point' };
    const control = hit.closest('a[href], button, input, select, textarea, summary, label, [role=button], [role=link], [role=checkbox], [role=radio], [role=textbox], [role=tab], [role=menuitem], [role=option], [contenteditable=""], [contenteditable=true], iframe');
    if (control) return { ok: false, why: 'control', seen: [clip(norm(control.innerText || control.value || control.getAttribute('aria-label') || 'a frame'), 40)] };
    return { ok: true };
  }
  if (request.kind === 'has') {
    const text = norm(document.body.innerText);
    return { ok: text.includes(norm(request.text)) };
  }
  if (request.kind === 'type') {
    if (!want && request.submit && request.ref === undefined) return { ok: false, why: 'submit' };
    const open = all('input, textarea, [contenteditable=""], [contenteditable="true"]')
      .filter(seen)
      .filter((f) => f.isContentEditable || !/^(hidden|password|submit|button|checkbox|radio|file|image|reset|range|color)$/.test(f.type));
    let field;
    if (request.ref !== undefined) {
      const found = byRef();
      if (!found.one) return { ok: false, why: found.why, seen: found.seen };
      field = found.one;
      if (field.tagName === 'SELECT') {
        // A list takes the option whose words (or value) read this, whole first, then a part of them.
        const words = norm(request.text);
        const options = [...field.options];
        const option = options.find((o) => norm(o.text) === words || norm(o.value) === words) || options.find((o) => norm(o.text).includes(words));
        if (!option || field.disabled) return { ok: false, why: 'option' };
        field.focus();
        field.value = option.value;
        field.dispatchEvent(new Event('input', { bubbles: true }));
        field.dispatchEvent(new Event('change', { bubbles: true }));
        mark(field);
        return { ok: true };
      }
      if (!open.includes(field)) return { ok: false, why: 'field' };
    } else if (want) {
      const { one, many } = pick(open, labelOf);
      if (many) return { ok: false, why: 'many', seen: many };
      field = one;
    } else field = open.includes(document.activeElement) ? document.activeElement : open[0];
    if (!field) return { ok: false, why: want ? 'field' : 'nofield' };
    field.focus();
    // The page may change the field, show a login or move to another page as it takes the focus: looked at again before typing.
    if (moved()) return gone();
    if (turnOf() || isSecret(field) || isCard(field) || !field.isConnected) return { ok: false, turn: turnOf() || 'a login' };
    if (field.isContentEditable) field.textContent = request.text;
    else {
      // The element's own setter, so pages that track their fields see the change as typing.
      const kind = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
      Object.getOwnPropertyDescriptor(kind.prototype, 'value').set.call(field, request.text);
    }
    field.dispatchEvent(new Event('input', { bubbles: true }));
    field.dispatchEvent(new Event('change', { bubbles: true }));
    if (request.submit) {
      // And again before sending: what the page did with the typing may have made it the person's turn or moved it, and a
      // form that pays by its button (a saved card shows no field) or by a frame is sent by the person.
      if (moved()) return gone();
      const now = turnOf();
      if (now) return { ok: false, turn: now };
      const form = formOf(field);
      if (form && paysForm(form)) return { ok: false, turn: 'a payment' };
      if (form) form.requestSubmit ? form.requestSubmit() : form.submit();
      else field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', code: 'Enter', bubbles: true }));
    }
    mark(field);
    return { ok: true };
  }
  return { ok: false, why: 'unknown' };
};
