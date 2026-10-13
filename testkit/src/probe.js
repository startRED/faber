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
    for (let node = el; node && node.nodeType === 1; node = node.parentElement) opacity *= parseFloat(getComputedStyle(node).opacity);
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

  window.__sx = { clock, start, finish, stateOf };
})();
