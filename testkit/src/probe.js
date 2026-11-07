// The UX sentinel's probe: injected into the page of a hidden test build over the DevTools port (like locator.js). It
// only measures and returns numbers; every judgement is made in testkit/src/sentinel.rs, where it is unit tested.
(() => {
  if (window.__sx) return;

  // The clock: a 1 s transition on a hidden element. Returns the frame times and whether `transitionend` came.
  const clock = () => new Promise((resolve) => {
    const el = document.createElement('div');
    el.style.cssText = 'position:fixed;left:0;top:0;width:1px;height:1px;pointer-events:none;transition:transform 1s linear';
    document.body.append(el);
    let ended = false;
    el.addEventListener('transitionend', () => { ended = true; });
    getComputedStyle(el).transform;
    el.style.transform = 'translateX(10px)';
    const frames = [];
    const began = performance.now();
    // Real time, not the frame's own timestamp: on a desktop no monitor shows, frames can come with stale timestamps.
    const tick = () => {
      const now = performance.now();
      frames.push(now);
      if (now - began < 1100 && !ended) requestAnimationFrame(tick);
      else { const debug = el.getAnimations().map((x) => `${x.constructor.name} ${x.playState} ${x.currentTime}`); el.remove(); resolve({ frames, ended, debug, span: now - began, first: frames[0] - began }); }
    };
    requestAnimationFrame(tick);
  });

  // Every place a moving part can be: the page, and the shadow roots where exit copies are kept while they leave.
  const roots = () => [document, ...[...document.querySelectorAll('.motion-exit')].map((layer) => layer.shadowRoot).filter(Boolean)];
  const effective = (el) => {
    let opacity = 1;
    // Past the top of a shadow root the next element is its host.
    for (let node = el; node && node.nodeType === 1; node = node.parentElement ?? node.getRootNode().host) opacity *= parseFloat(getComputedStyle(node).opacity);
    return opacity;
  };
  // [present, x, y, width, height, opacity] of the most visible element of this surface, the original or an exit copy.
  const stateOf = (name) => {
    let best = null;
    for (const root of roots()) {
      for (const el of root.querySelectorAll(`[data-surface="${name}"]`)) {
        const r = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        const shown = r.width > 0 && r.height > 0 && style.visibility !== 'hidden' && style.display !== 'none';
        const state = [shown ? 1 : 0, r.x, r.y, r.width, r.height, shown ? effective(el) : 0];
        if (!best || (state[0] && state[5] > best[5])) best = state;
      }
    }
    return best ?? [0, 0, 0, 0, 0, 0];
  };

  const skip = new Set(['offset', 'easing', 'composite', 'computedOffset']);
  let run = null;

  // Starts sampling these surfaces on every frame, and noting each animation that starts.
  const start = (names) => {
    const seen = new Set(document.getAnimations());
    run = { names, samples: Object.fromEntries(names.map((n) => [n, []])), animations: [], began: performance.now(), stop: false };
    const current = run;
    // The state before anything happens is taken now, not on the next frame, which the action may already have changed.
    for (const name of names) current.samples[name].push([0, ...stateOf(name)]);
    const frame = () => {
      const now = performance.now();
      for (const name of names) current.samples[name].push([Math.max(1, Math.round(now - current.began)), ...stateOf(name)]);
      for (const animation of document.getAnimations()) {
        if (seen.has(animation)) continue;
        seen.add(animation);
        const timing = animation.effect.getTiming();
        const target = animation.effect.target;
        current.animations.push({
          target: target ? `${target.tagName.toLowerCase()}${target.className && typeof target.className === 'string' ? '.' + target.className.trim().split(/\s+/).join('.') : ''}` : '?',
          props: [...new Set(animation.effect.getKeyframes().flatMap((k) => Object.keys(k)).filter((k) => !skip.has(k)))],
          ms: Number(timing.duration) + Number(timing.delay || 0),
          infinite: timing.iterations === Infinity,
        });
      }
      if (!current.stop) requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  };

  // Waits for the surfaces to rest (no change for `quiet` ms, and at least `least` ms since now), then stops and returns.
  const finish = (least = 400, quiet = 300, most = 4000) => new Promise((resolve) => {
    const current = run;
    const began = performance.now();
    const last = () => Math.max(0, ...current.names.map((n) => {
      const rows = current.samples[n];
      for (let i = rows.length - 1; i > 0; i--) if (rows[i].slice(1).some((v, k) => Math.abs(v - rows[i - 1][k + 1]) > 0.01)) return rows[i][0];
      return 0;
    }));
    const poll = () => {
      const now = performance.now() - current.began;
      const waited = performance.now() - began;
      if ((waited >= least && now - last() >= quiet) || waited >= most) {
        current.stop = true;
        run = null;
        resolve({ samples: current.samples, animations: current.animations });
      } else setTimeout(poll, 50);
    };
    poll();
  });

  // Composition (V1 to V3). The art is the look's effects; `data-sx-art` marks the art a planted twin brings.
  const ART = '.ambient, canvas.amb-hole, [data-sx-art]';
  const describe = (el) => el.tagName.toLowerCase() + (typeof el.className === 'string' && el.className.trim() ? '.' + el.className.trim().split(/\s+/)[0] : '');
  const inView = (r) => r.width > 0 && r.height > 0 && r.right > 0 && r.bottom > 0 && r.left < innerWidth && r.top < innerHeight;
  // Every text a person can read now: its element, its words and the boxes of its letters, line by line.
  const shown = () => {
    const out = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode;
      const el = node.parentElement;
      const text = node.nodeValue.replace(/\s+/g, ' ').trim();
      if (!el || !text || el.closest(`script, style, ${ART}`) || !el.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) || effective(el) < 0.1) continue;
      // Text clipped to a pixel (the screen-reader-only pattern) is not read by the eye.
      const box = el.getBoundingClientRect();
      if (box.width < 2 || box.height < 2) continue;
      const range = document.createRange();
      range.selectNodeContents(node);
      const rects = [...range.getClientRects()].filter((r) => inView(r) && r.width >= 2 && r.height >= 2);
      if (rects.length) out.push({ el, text, rects });
    }
    return out;
  };
  const texts = () => shown().map(({ el, text }) => [describe(el), text]);
  const headings = () => [...new Set(shown().filter(({ el }) => el.closest('h1, h2, h3')).map(({ text }) => text))];
  const hasArt = () => [...document.querySelectorAll(ART)].some((el) => inView(el.getBoundingClientRect()) && effective(el) > 0);

  // Takes the letters away (`text`), and the art too (`text art`), while the screen is photographed; '' puts both back.
  // Nothing transitions meanwhile, so each photo shows a state, not a step between two. The letters go by their fill only:
  // `color` stays, so a surface or an icon drawn in `currentColor` is the same in both photos.
  let hiding = null;
  const hide = (what) => {
    hiding?.remove();
    hiding = null;
    if (!what) return true;
    hiding = document.createElement('style');
    hiding.textContent = '*, *::before, *::after { transition: none !important; } '
      + '*, *::before, *::after, ::placeholder { -webkit-text-fill-color: transparent !important; text-shadow: none !important; caret-color: transparent !important; text-decoration-color: transparent !important; }'
      + (what.includes('art') ? ` :is(${ART}) { opacity: 0 !important; }` : '');
    document.head.append(hiding);
    return true;
  };

  // The light the art adds under each text: the brightest channel of the difference between the two photos, at the
  // 99th percentile of the letters' pixels (a grain of dust does not count, a lit band does).
  const light = async (withArt, without) => {
    const pixels = async (data) => {
      const img = new Image();
      img.src = `data:image/png;base64,${data}`;
      await img.decode();
      const canvas = document.createElement('canvas');
      [canvas.width, canvas.height] = [img.naturalWidth, img.naturalHeight];
      const g = canvas.getContext('2d', { willReadFrequently: true });
      g.drawImage(img, 0, 0);
      return g.getImageData(0, 0, canvas.width, canvas.height);
    };
    const [a, b] = [await pixels(withArt), await pixels(without)];
    const scale = a.width / innerWidth;
    const out = [];
    for (const { el, text, rects } of shown()) {
      const values = [];
      for (const r of rects) {
        const [x0, x1] = [Math.max(0, Math.floor(r.left * scale)), Math.min(a.width, Math.ceil(r.right * scale))];
        const [y0, y1] = [Math.max(0, Math.floor(r.top * scale)), Math.min(a.height, Math.ceil(r.bottom * scale))];
        for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++) {
          const i = (y * a.width + x) * 4;
          values.push(Math.max(Math.abs(a.data[i] - b.data[i]), Math.abs(a.data[i + 1] - b.data[i + 1]), Math.abs(a.data[i + 2] - b.data[i + 2])));
        }
      }
      if (!values.length) continue;
      values.sort((p, q) => p - q);
      out.push({ element: describe(el), text: text.slice(0, 80), light: values[Math.floor(0.99 * (values.length - 1))] });
    }
    return out;
  };

  // Each grid that sets two columns side by side, at least 240 px wide and 120 px tall each (a row of a label and its
  // switch is not two columns), and where the content of each ends. A frame of the window (it runs to the window's
  // bottom edge, as the shell of sidebar and stage does) or a column that scrolls ends where the window does, so neither
  // counts.
  const columns = () => {
    const flows = (el) => [...el.children].filter((k) => {
      const s = getComputedStyle(k);
      return s.position !== 'absolute' && s.position !== 'fixed' && k.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true }) && k.getBoundingClientRect().height > 0;
    });
    // The column itself is stretched by the grid; its content is not. One child that fills it is looked into.
    const end = (el) => {
      const kids = flows(el);
      if (!kids.length) return el.getBoundingClientRect().bottom;
      if (kids.length === 1 && Math.abs(kids[0].getBoundingClientRect().bottom - el.getBoundingClientRect().bottom) < 1) return end(kids[0]);
      return Math.max(...kids.map((k) => k.getBoundingClientRect().bottom));
    };
    const out = [];
    for (const grid of document.body.querySelectorAll('*')) {
      const frame = grid.getBoundingClientRect();
      if (!/grid/.test(getComputedStyle(grid).display) || (frame.bottom >= innerHeight - 2 && frame.height >= 0.8 * innerHeight)) continue;
      if (!grid.checkVisibility({ checkOpacity: true, checkVisibilityCSS: true })) continue;
      const kids = flows(grid);
      if (kids.length !== 2) continue;
      const [l, r] = kids.map((k) => k.getBoundingClientRect());
      const beside = (l.right <= r.left + 1 || r.right <= l.left + 1) && Math.min(l.bottom, r.bottom) > Math.max(l.top, r.top);
      const big = [l, r].every((box) => box.width >= 240 && box.height >= 120);
      const scrolls = kids.some((k) => /auto|scroll/.test(getComputedStyle(k).overflowY) && k.scrollHeight > k.clientHeight + 1);
      if (!beside || !big || scrolls || !inView(grid.getBoundingClientRect())) continue;
      const [left, right] = l.left <= r.left ? [end(kids[0]), end(kids[1])] : [end(kids[1]), end(kids[0])];
      out.push({ grid: describe(grid), left, right });
    }
    return out;
  };

  window.__sx = { clock, start, finish, stateOf, texts, headings, hasArt, hide, light, columns };
})();
