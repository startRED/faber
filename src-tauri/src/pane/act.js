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
  const SECRET = /\b(password|passwd|pwd|pass|passcode|passphrase|passwort|senha|contrase[ñn]a|mot de passe|kennwort|pin|otp|one[- ]?time|2fa|verification code)\b/;
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

  const turn = turnOf();
  if (request.kind === 'read') {
    if (request.watch) {
      // A picture is taken after this: what changes in the page until `settle` makes the picture unfit to keep.
      globalThis.faberWatch?.o.disconnect();
      const w = { n: 0, href: location.href, o: new MutationObserver((records) => (w.n += records.length)) };
      // Every root the checks read: the document, its open shadow roots and its own site's frames. A root added later comes
      // with a change in one already watched.
      for (const root of roots()) w.o.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
      globalThis.faberWatch = w;
    }
    return { ok: true, turn };
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
    const things = all('a, button, summary, label, input[type=submit], input[type=button], [role=button], [role=link], [role=tab], [role=menuitem], [role=checkbox], [role=option]').filter(seen);
    const words = (el) => norm(el.innerText || el.value || el.getAttribute('aria-label') || el.title);
    const { one, many } = pick(things, words);
    if (many) return { ok: false, why: 'many', seen: many };
    if (!one) return { ok: false, why: 'click' };
    // What the click reaches: the control a label stands for, or the button something inside it sits in; if that sends a form,
    // the click pays when the form does.
    const target = (one.tagName === 'LABEL' && one.control) || one;
    const sender = [target, target.closest('button')].find((e) => e && isSubmit(e));
    if (commits(one) || commits(target) || (sender?.form && paysForm(sender.form))) return { ok: false, turn: 'a payment' };
    one.scrollIntoView({ block: 'center' });
    if (moved()) return gone();
    one.click();
    mark(one);
    return { ok: true };
  }
  if (request.kind === 'type') {
    if (!want && request.submit) return { ok: false, why: 'submit' };
    const open = all('input, textarea, [contenteditable=""], [contenteditable="true"]')
      .filter(seen)
      .filter((f) => f.isContentEditable || !/^(hidden|password|submit|button|checkbox|radio|file|image|reset|range|color)$/.test(f.type));
    let field;
    if (want) {
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
