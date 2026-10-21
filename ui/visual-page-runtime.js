// This fixed, hash-authorized loader never shares the screen's script authorization with authored content.
(() => {
  const query = Document.prototype.querySelectorAll.bind(document);
  const create = Document.prototype.createElement.bind(document);
  const get = Element.prototype.getAttribute;
  const attributes = Object.getOwnPropertyDescriptor(Element.prototype, 'attributes').get;
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
  const resourceHandlers = [];
  if (!top) {
    for (const entry of query('script[type="application/faber-events"][data-faber-events]')) {
      const setup = create('script');
      text.set.call(setup, text.get.call(entry));
      append.call(head, setup);
      if (typeof setup.faberAdvance === 'function') resourceHandlers.push(setup.faberAdvance);
      delete setup.faberAdvance;
      remove.call(head, setup);
    }
  }
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
    // External scripts exist only under live permission; the child's first CSP and native URL filter still apply.
    const scripts = Array.from(query('script[type="application/faber-script"]'), (entry) => ({
      type: get.call(entry, 'data-faber-script') || 'text/javascript',
      source: text.get.call(entry),
      url: get.call(entry, 'data-faber-src'),
      // Keep fetching constraints and script configuration, never authorization or loader bookkeeping.
      attributes: Array.from(attributes.call(entry), ({ name, value }) => [name, value])
        .filter(([name]) => /^(?:id|class|title|lang|dir|integrity|crossorigin|fetchpriority|aria-[\w-]+|data-(?!faber-)[\w-]+)$/.test(name)),
      handlers: get.call(entry, 'data-faber-handlers'),
      order: Number(get.call(entry, 'data-faber-order')),
    }));
    const modules = [];
    // Modules are deferred past every classic script, including a later external script still loading.
    const replay = (entry) => {
      const script = create('script');
      set.call(script, 'type', entry.type);
      for (const [name, value] of entry.attributes) set.call(script, name, value);
      // Compile authored callbacks without eval, on the executed element before its request can start.
      if (entry.handlers) {
        const setup = create('script');
        setup.faberTarget = script;
        text.set.call(setup, `((target) => { ${entry.handlers}\n })(document.currentScript.faberTarget);`);
        append.call(head, setup);
        remove.call(head, setup);
        delete setup.faberTarget;
      }
      let external;
      if (entry.url !== null) {
        external = new NativePromise((resolve) => {
          listen.call(script, 'load', resolve, { once: true });
          listen.call(script, 'error', resolve, { once: true });
        });
        set.call(script, 'referrerpolicy', 'no-referrer');
        set.call(script, 'src', entry.url);
      } else text.set.call(script, entry.source);
      if (entry.type === 'module') {
        script.async = false;
      }
      append.call(head, script);
      return { script, external };
    };
    let completed = 0;
    const advance = () => { for (const ready of resourceHandlers) ready(completed); };
    for (const entry of scripts) {
      if (entry.type === 'module') { modules.push(entry); continue; }
      if (entry.url !== null) advance();
      const { script, external } = replay(entry);
      if (external) await external;
      if (entry.order) { completed = entry.order; advance(); }
      remove.call(head, script);
    }
    advance();
    const deferred = modules.map((entry) => replay(entry).script);
    // Inline modules delay the browser's load but do not emit script load events.
    await loaded;
    for (const script of deferred) remove.call(head, script);
    apply(dispatch, document, [new NativeEvent('DOMContentLoaded', { bubbles: true })]);
    apply(dispatch, window, [new NativeEvent('load')]);
  }, { once: true, capture: !top });
})();
