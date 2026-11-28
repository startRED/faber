// This fixed, hash-authorized loader never shares the screen's script authorization with authored content.
(() => {
  const query = Document.prototype.querySelectorAll.bind(document);
  const create = Document.prototype.createElement.bind(document);
  const get = Element.prototype.getAttribute;
  const set = Element.prototype.setAttribute;
  const append = Node.prototype.appendChild;
  const remove = Node.prototype.removeChild;
  const text = Object.getOwnPropertyDescriptor(Node.prototype, 'textContent');
  const head = Object.getOwnPropertyDescriptor(Document.prototype, 'head').get.call(document);
  const listen = EventTarget.prototype.addEventListener;
  const unlisten = EventTarget.prototype.removeEventListener;
  const dispatch = EventTarget.prototype.dispatchEvent;
  const stop = Event.prototype.stopImmediatePropagation;
  const target = Object.getOwnPropertyDescriptor(Event.prototype, 'target').get;
  const NativeEvent = Event;
  const NativePromise = Promise;
  const apply = Reflect.apply;
  const top = window === window.top;
  // Resource load events must still reach scripts and images; only the page's own load waits for its scripts.
  const loaded = !top && new NativePromise((resolve) => {
    const onLoad = (event) => {
      const at = apply(target, event, []);
      if (!event.isTrusted || (at !== document && at !== window)) return;
      apply(stop, event, []);
      apply(unlisten, window, ['load', onLoad, true]);
      resolve();
    };
    apply(listen, window, ['load', onLoad, true]);
  });
  listen.call(top ? document : window, 'DOMContentLoaded', async (event) => {
    if (top) {
      // Vite leaves its entries inert; only this hash-authorized loader starts the screen's modules.
      for (const entry of query('script[type="application/faber-module"]')) {
        const script = create('script');
        set.call(script, 'type', 'module');
        const source = get.call(entry, 'src');
        if (source !== null) {
          const url = new URL(source, location.href);
          if (url.origin !== location.origin) continue;
          set.call(script, 'src', url.href);
        } else text.set.call(script, text.get.call(entry));
        append.call(head, script);
      }
      return;
    }
    // The authored listeners do not exist yet. Deliver their lifecycle only after replaying the page's scripts.
    apply(stop, event, []);
    // Only inline code is replayed; every external source is absent, and the child's first CSP still applies.
    const scripts = Array.from(query('script[type="application/faber-script"]'), (entry) => ({
      type: get.call(entry, 'data-faber-script') || 'text/javascript',
      source: text.get.call(entry),
    }));
    const modules = [];
    for (const entry of scripts) {
      const script = create('script');
      set.call(script, 'type', entry.type);
      text.set.call(script, entry.source);
      if (entry.type === 'module') {
        script.async = false;
        modules.push(script);
      }
      append.call(head, script);
      if (entry.type !== 'module') remove.call(head, script);
    }
    // Inline modules delay the browser's load but do not emit script load events.
    await loaded;
    for (const script of modules) remove.call(head, script);
    apply(dispatch, document, [new NativeEvent('DOMContentLoaded', { bubbles: true })]);
    apply(dispatch, window, [new NativeEvent('load')]);
  }, { once: true, capture: !top });
})();
