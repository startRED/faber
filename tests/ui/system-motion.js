// Runs in the checks page before any script of the app (src-tauri/src/main.rs, `initialization_script`). CI's machines ask
// Windows for reduced motion, WebView2 has no switch that turns that off (Chromium's only one forces it on, and
// `--blink-settings` stops the app from starting), so the checks page reports "no preference" to every script, the ones that
// read it while the app loads included. system-motion.ts hides it from the stylesheet. Only the checks page does this: the
// scenarios drive the real page and set the preference through the DevTools port (`system_motion`).
(() => {
  const real = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches');
  Object.defineProperty(MediaQueryList.prototype, 'matches', {
    ...real,
    get() {
      return this.media === '(prefers-reduced-motion: reduce)' ? false : real.get.call(this);
    },
  });
})();

// A window the system holds back (the hidden one a test build runs in, on macOS) can pause animation frames, and then every
// callback waiting for one, the app's own included, never runs. On the checks page a frame callback runs at the next frame
// or 250 ms later, whichever comes first, so a paused page still gets through its checks. The scenarios drive the real page
// and are not touched.
(() => {
  const request = window.requestAnimationFrame.bind(window);
  const cancel = window.cancelAnimationFrame.bind(window);
  const waiting = new Map();
  window.requestAnimationFrame = (callback) => {
    let done = false;
    const run = (time) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      waiting.delete(id);
      callback(time);
    };
    const id = request(run);
    const timer = setTimeout(() => run(performance.now()), 250);
    waiting.set(id, () => { done = true; clearTimeout(timer); });
    return id;
  };
  window.cancelAnimationFrame = (id) => {
    waiting.get(id)?.();
    waiting.delete(id);
    cancel(id);
  };
})();

// macOS WebKit can hang the app's main thread for good in a scrolling handshake (RemoteLayerTreeEventDispatcher::
// mainThreadDisplayDidRefresh), taken while a smooth-scroll animation runs in the hidden test window. On the checks page a
// smooth scroll runs instant: the screen still asks for 'smooth' (a check that stubs the method sees that), only the real
// browser call changes. The scenarios drive the real page and are not touched.
(() => {
  const instant = (options) => (options && typeof options === 'object' && options.behavior === 'smooth' ? { ...options, behavior: 'auto' } : options);
  for (const [target, names] of [[Element.prototype, ['scrollTo', 'scrollBy', 'scrollIntoView']], [window, ['scrollTo', 'scrollBy']]]) {
    for (const name of names) {
      const real = target[name];
      target[name] = function (first, ...rest) {
        return real.call(this, instant(first), ...rest);
      };
    }
  }
})();
