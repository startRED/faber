// Run before authored scripts in opaque visual frames. CSP does not cover DNS hints or every transport.
(() => {
  if (window === window.top) return;

  const apply = Reflect.apply;
  const define = Object.defineProperty;
  const descriptor = Object.getOwnPropertyDescriptor;
  const stringify = String;
  const lower = String.prototype.toLowerCase;
  const trim = String.prototype.trim;
  const nodeType = descriptor(Node.prototype, 'nodeType').get;
  const firstChild = descriptor(Node.prototype, 'firstChild').get;
  const nextSibling = descriptor(Node.prototype, 'nextSibling').get;
  const parentNode = descriptor(Node.prototype, 'parentNode').get;
  const ownerDocument = descriptor(Node.prototype, 'ownerDocument').get;
  const localName = descriptor(Element.prototype, 'localName').get;
  const namespace = descriptor(Element.prototype, 'namespaceURI').get;
  const shadowRoot = descriptor(Element.prototype, 'shadowRoot').get;
  const shadowHost = descriptor(ShadowRoot.prototype, 'host').get;
  const templateContent = descriptor(HTMLTemplateElement.prototype, 'content').get;
  const attrName = descriptor(Attr.prototype, 'localName').get;
  const attrValue = descriptor(Attr.prototype, 'value');
  const innerHTML = descriptor(Element.prototype, 'innerHTML');
  const bodyGetter = descriptor(Document.prototype, 'body').get;
  const rootGetter = descriptor(Document.prototype, 'documentElement').get;
  const nativeCreate = Document.prototype.createElement;
  const nativeRemove = Node.prototype.removeChild;
  const nativeAppend = Node.prototype.appendChild;
  const nativeReplace = Node.prototype.replaceChild;
  const nativeReplaceElementChildren = Element.prototype.replaceChildren;
  const nativeReplaceFragmentChildren = DocumentFragment.prototype.replaceChildren;
  const nativeCreateRange = Document.prototype.createRange;
  const nativeRangeFragment = Range.prototype.createContextualFragment;
  const nativeRangeContents = Range.prototype.selectNodeContents;
  const nativeRangeNode = Range.prototype.selectNode;
  const nativeRangeCollapse = Range.prototype.collapse;
  const nativeRangeInsert = Range.prototype.insertNode;
  const nativeRangeDelete = Range.prototype.deleteContents;
  const nativeSelection = Document.prototype.getSelection;
  const selectionCount = descriptor(Selection.prototype, 'rangeCount').get;
  const selectionRange = Selection.prototype.getRangeAt;
  const nativeGetAttribute = Element.prototype.getAttribute;
  const nativeSetAttribute = Element.prototype.setAttribute;
  const nativeRemoveAttribute = Element.prototype.removeAttribute;
  const nativeCreateText = Document.prototype.createTextNode;
  const nativeParser = DOMParser;
  const nativeParse = DOMParser.prototype.parseFromString;
  const nativeAddListener = EventTarget.prototype.addEventListener;
  const addedNodes = descriptor(MutationRecord.prototype, 'addedNodes').get;
  const nodeListLength = descriptor(NodeList.prototype, 'length').get;
  const nodeListItem = NodeList.prototype.item;
  const NativeMutationObserver = MutationObserver;
  const nativeObserve = MutationObserver.prototype.observe;
  const nativeDisconnect = MutationObserver.prototype.disconnect;
  const nativeTakeRecords = MutationObserver.prototype.takeRecords;
  // A same-origin initial empty child is not a native navigation. Remove it before it can supply a fresh realm.
  const frame = window.parent !== window.top ? window.frameElement : null;
  if (frame) {
    const parent = apply(parentNode, frame, []);
    if (parent) apply(nativeRemove, parent, [frame]);
  }
  const preventDefault = Event.prototype.preventDefault;
  const htmlNamespace = 'http://www.w3.org/1999/xhtml';
  const scriptType = 'application/x-faber-disabled';
  const relLists = new WeakSet();
  const relListHas = WeakSet.prototype.has;
  const relListAdd = WeakSet.prototype.add;
  const shadowRoots = new WeakMap();
  const shadowGet = WeakMap.prototype.get;
  const shadowSet = WeakMap.prototype.set;
  const watched = new WeakSet();
  const watchedHas = WeakSet.prototype.has;
  const watchedAdd = WeakSet.prototype.add;
  let parsing = true;
  const normalize = (value) => apply(lower, apply(trim, stringify(value), []), []);
  const isElement = (node) => apply(nodeType, node, []) === 1;
  const nameOf = (node) => apply(lower, apply(localName, node, []), []);
  const isScript = (node) => isElement(node) && nameOf(node) === 'script';
  const isSpeculation = (value) => normalize(value) === 'speculationrules';
  const deniedName = (name) => name === 'iframe' || name === 'frame' || name === 'object' || name === 'embed' || name === 'noscript';
  const safeRel = (value) => normalize(value) === 'stylesheet' ? 'stylesheet' : '';
  const deniedNode = (node) => isElement(node) && (deniedName(nameOf(node))
    || (nameOf(node) === 'meta' && normalize(apply(nativeGetAttribute, node, ['http-equiv']) ?? '') === 'refresh')
    || (nameOf(node) === 'link' && !safeRel(apply(nativeGetAttribute, node, ['rel']) ?? '')));
  const inertNode = () => apply(nativeCreateText, document, ['']);

  const lockMethod = (prototype, name, make) => {
    const entry = descriptor(prototype, name);
    if (!entry || typeof entry.value !== 'function') return;
    define(prototype, name, { ...entry, value: make(entry.value), writable: false, configurable: false });
  };
  const lockSetter = (prototype, name, make) => {
    const entry = descriptor(prototype, name);
    if (!entry || !entry.set) return;
    define(prototype, name, { ...entry, set: make(entry.set), configurable: false });
  };

  // Only stylesheets may connect: every hint and every combination containing a hint stays inert.
  // Nested browsing contexts must not provide an unguarded realm's DOM methods.
  const clean = (node) => {
    if (isScript(node) && isSpeculation(apply(nativeGetAttribute, node, ['type']) ?? '')) {
      apply(nativeSetAttribute, node, ['type', scriptType]);
    }
    let child = apply(firstChild, node, []);
    while (child) {
      const next = apply(nextSibling, child, []);
      if (deniedNode(child)) apply(nativeRemove, node, [child]);
      else clean(child);
      child = next;
    }
    if (!isElement(node)) return;
    if (nameOf(node) === 'template' && apply(namespace, node, []) === htmlNamespace) {
      apply(nativeRemoveAttribute, node, ['shadowrootmode']);
      apply(nativeRemoveAttribute, node, ['shadowroot']);
      clean(apply(templateContent, node, []));
    }
    const shadow = apply(shadowGet, shadowRoots, [node]) || apply(shadowRoot, node, []);
    if (shadow) { watchShadow(shadow); clean(shadow); }
  };
  const cleanNode = (node) => {
    if (deniedNode(node)) return inertNode();
    clean(node);
    return node;
  };
  // Parser insertions bypass DOM methods; observe them before any authored script can inspect a child realm.
  const cleanParsed = (records) => {
    for (let i = 0; i < records.length; i++) {
      const nodes = apply(addedNodes, records[i], []);
      const count = apply(nodeListLength, nodes, []);
      for (let j = 0; j < count; j++) {
        const node = apply(nodeListItem, nodes, [j]);
        if (deniedNode(node)) {
          const parent = apply(parentNode, node, []);
          if (parent) apply(nativeRemove, parent, [node]);
        } else clean(node);
      }
    }
  };
  const parserGuard = new NativeMutationObserver(cleanParsed);
  const watchShadow = (root) => {
    if (!parsing || apply(watchedHas, watched, [root])) return;
    apply(watchedAdd, watched, [root]);
    apply(nativeObserve, parserGuard, [root, { childList: true, subtree: true }]);
  };
  // Initial empty children may never run initialization scripts. Do not expose their unguarded realm from this one.
  for (const constructor of [window.HTMLIFrameElement, window.HTMLFrameElement]) {
    if (!constructor) continue;
    for (const name of ['contentWindow', 'contentDocument']) {
      const entry = descriptor(constructor.prototype, name);
      if (entry?.get) define(constructor.prototype, name, { ...entry, get() { return null; }, configurable: false });
    }
  }
  const refuseOpen = () => null;
  lockMethod(Window.prototype, 'open', () => refuseOpen);
  define(window, 'open', { value: refuseOpen, writable: false, configurable: false });
  const internals = window.ElementInternals?.prototype;
  const internalsRoot = internals && descriptor(internals, 'shadowRoot');
  if (internalsRoot?.get) define(internals, 'shadowRoot', { ...internalsRoot, configurable: false, get() {
    const root = apply(internalsRoot.get, this, []);
    if (root) { watchShadow(root); clean(root); }
    return root;
  } });
  lockMethod(Element.prototype, 'attachShadow', (native) => function (options) {
    const root = apply(native, this, [options]);
    // Keep the returned reference even for closed roots; document observers cannot see their parser insertions.
    apply(shadowSet, shadowRoots, [this, root]);
    watchShadow(root);
    clean(root);
    return root;
  });
  apply(nativeObserve, parserGuard, [document, { childList: true, subtree: true }]);
  apply(nativeAddListener, window, ['DOMContentLoaded', () => {
    cleanParsed(apply(nativeTakeRecords, parserGuard, []));
    clean(document);
    parsing = false;
    apply(nativeDisconnect, parserGuard, []);
  }, { once: true, capture: true }]);
  const cleanArgument = (value) => {
    // ParentNode and ChildNode APIs accept strings as well as nodes.
    try { apply(nodeType, value, []); } catch { return value; }
    return cleanNode(value);
  };
  const cleanHTML = (value) => {
    const template = apply(nativeCreate, document, ['template']);
    apply(innerHTML.set, template, [value]);
    clean(apply(templateContent, template, []));
    return apply(innerHTML.get, template, []);
  };
  // The last parse must use the real HTML/SVG/MathML context while still disconnected.
  const rangeFragment = (range, value) => {
    const fragment = apply(nativeRangeFragment, range, [cleanHTML(value)]);
    clean(fragment);
    return fragment;
  };
  const rangeFor = (element, outside) => {
    const range = apply(nativeCreateRange, apply(ownerDocument, element, []) || document, []);
    apply(outside ? nativeRangeNode : nativeRangeContents, range, [element]);
    return range;
  };
  const replaceContents = (target, context, value) => {
    const fragment = rangeFragment(rangeFor(context, false), value);
    const replace = apply(nodeType, target, []) === 11 ? nativeReplaceFragmentChildren : nativeReplaceElementChildren;
    apply(replace, target, [fragment]);
  };
  const elementContents = (element) => nameOf(element) === 'template' && apply(namespace, element, []) === htmlNamespace
    ? apply(templateContent, element, []) : element;
  const cleanAttribute = (attribute) => {
    if (apply(lower, apply(attrName, attribute, []), []) === 'type'
      && isSpeculation(apply(attrValue.get, attribute, []))) {
      apply(attrValue.set, attribute, [scriptType]);
    }
    if (normalize(apply(attrName, attribute, [])) === 'rel') {
      apply(attrValue.set, attribute, [safeRel(apply(attrValue.get, attribute, []))]);
    }
    return attribute;
  };

  // These transports bypass the native HTTPS resource filter, including trusted private TLS endpoints.
  for (const name of ['RTCPeerConnection', 'webkitRTCPeerConnection', 'RTCDataChannel', 'webkitRTCDataChannel', 'WebSocket', 'WebSocketStream', 'WebTransport']) {
    const constructor = window[name];
    if (constructor?.prototype) define(constructor.prototype, 'constructor', { value: undefined, writable: false, configurable: false });
    define(window, name, { value: undefined, writable: false, configurable: false });
  }
  // A connected stylesheet cannot turn into a hint through reflected properties or its token list.
  const relList = descriptor(HTMLLinkElement.prototype, 'relList');
  define(HTMLLinkElement.prototype, 'relList', { ...relList, configurable: false, get() {
    const list = apply(relList.get, this, []);
    apply(relListAdd, relLists, [list]);
    return list;
  } });
  lockSetter(HTMLLinkElement.prototype, 'rel', (native) => function (value) {
    return apply(native, this, [safeRel(value)]);
  });
  lockSetter(DOMTokenList.prototype, 'value', (native) => function (value) {
    return apply(native, this, [apply(relListHas, relLists, [this]) ? safeRel(value) : value]);
  });
  for (const name of ['add', 'replace', 'toggle']) {
    lockMethod(DOMTokenList.prototype, name, (native) => function (...values) {
      if (apply(relListHas, relLists, [this])) {
        const count = name === 'toggle' ? 1 : name === 'replace' ? 2 : values.length;
        for (let i = 0; i < count; i++) {
          values[i] = stringify(values[i]);
          if (values[i] !== 'stylesheet') return false;
        }
      }
      return apply(native, this, values);
    });
  }
  lockMethod(Document.prototype, 'createElement', (native) => function (name, options) {
    const text = stringify(name);
    if (deniedName(apply(lower, text, []))) return apply(native, this, ['faber-inert']);
    return apply(native, this, [text, options]);
  });
  lockMethod(Document.prototype, 'createElementNS', (native) => function (ns, name, options) {
    return cleanNode(apply(native, this, [ns, name, options]));
  });
  lockSetter(Element.prototype, 'innerHTML', () => function (value) {
    replaceContents(elementContents(this), this, value);
  });
  lockSetter(Element.prototype, 'outerHTML', (native) => function (value) {
    const parent = apply(parentNode, this, []);
    if (!parent) return;
    // Preserve the native document-parent error without ever parsing authored markup into it.
    if (apply(nodeType, parent, []) === 9) return apply(native, this, ['']);
    const fragment = rangeFragment(rangeFor(this, true), value);
    apply(nativeReplace, parent, [fragment, this]);
  });
  lockSetter(ShadowRoot.prototype, 'innerHTML', () => function (value) {
    replaceContents(this, apply(shadowHost, this, []), value);
  });
  lockMethod(Element.prototype, 'insertAdjacentHTML', (native) => function (position, value) {
    const positionText = stringify(position);
    const where = apply(lower, positionText, []);
    if (where !== 'beforebegin' && where !== 'afterbegin' && where !== 'beforeend' && where !== 'afterend') {
      return apply(native, this, [positionText, '']);
    }
    const outside = where === 'beforebegin' || where === 'afterend';
    const parent = apply(parentNode, this, []);
    if (outside && (!parent || apply(nodeType, parent, []) === 9)) return apply(native, this, [where, '']);
    const range = rangeFor(this, outside);
    apply(nativeRangeCollapse, range, [where === 'beforebegin' || where === 'afterbegin']);
    const fragment = rangeFragment(range, value);
    apply(nativeRangeInsert, range, [fragment]);
  });
  lockMethod(Range.prototype, 'createContextualFragment', () => function (value) {
    return rangeFragment(this, value);
  });
  lockMethod(DOMParser.prototype, 'parseFromString', (native) => function (value, type) {
    const parsed = apply(native, this, [value, type]);
    clean(parsed);
    return parsed;
  });
  for (const prototype of [Element.prototype, ShadowRoot.prototype]) {
    const shadow = prototype === ShadowRoot.prototype;
    for (const name of ['setHTML', 'setHTMLUnsafe']) {
      lockMethod(prototype, name, () => function (value) {
        // Sandbox parsing deliberately omits custom sanitizers and declarative shadow-root options.
        replaceContents(shadow ? this : elementContents(this), shadow ? apply(shadowHost, this, []) : this, value);
      });
    }
  }
  for (const name of ['parseHTML', 'parseHTMLUnsafe']) {
    lockMethod(Document, name, () => function (value) {
      // DOMParser keeps declarative shadow roots inert, including closed roots whose contents cannot be inspected.
      const parsed = apply(nativeParse, new nativeParser(), [cleanHTML(value), 'text/html']);
      clean(parsed);
      return parsed;
    });
  }
  lockMethod(Document.prototype, 'execCommand', (native) => function (command, showUI, value) {
    const text = stringify(command);
    if (normalize(text) !== 'inserthtml') return apply(native, this, [text, showUI, value]);
    const selection = apply(nativeSelection, this, []);
    if (!selection || !apply(selectionCount, selection, [])) return false;
    const range = apply(selectionRange, selection, [0]);
    const fragment = rangeFragment(range, value);
    apply(nativeRangeDelete, range, []);
    apply(nativeRangeInsert, range, [fragment]);
    return true;
  });

  for (const name of ['appendChild', 'insertBefore', 'replaceChild']) {
    lockMethod(Node.prototype, name, (native) => function (node, reference) {
      return apply(native, this, [cleanNode(node), reference]);
    });
  }
  for (const prototype of [Element.prototype, Document.prototype, DocumentFragment.prototype]) {
    for (const name of ['append', 'prepend', 'replaceChildren', 'moveBefore']) {
      lockMethod(prototype, name, (native) => function (...values) {
        const count = name === 'moveBefore' ? 1 : values.length;
        for (let index = 0; index < count; index++) values[index] = cleanArgument(values[index]);
        return apply(native, this, values);
      });
    }
  }
  for (const prototype of [Element.prototype, CharacterData.prototype, DocumentType.prototype]) {
    for (const name of ['before', 'after', 'replaceWith']) {
      lockMethod(prototype, name, (native) => function (...values) {
        for (let index = 0; index < values.length; index++) values[index] = cleanArgument(values[index]);
        return apply(native, this, values);
      });
    }
  }
  for (const name of ['insertNode', 'surroundContents']) {
    lockMethod(Range.prototype, name, (native) => function (node) {
      return apply(native, this, [cleanNode(node)]);
    });
  }
  lockMethod(Element.prototype, 'insertAdjacentElement', (native) => function (position, node) {
    const safe = deniedNode(node) ? apply(nativeCreate, document, ['faber-inert']) : cleanNode(node);
    return apply(native, this, [position, safe]);
  });

  // A connected ordinary script must not become speculation rules through another attribute API.
  lockSetter(HTMLScriptElement.prototype, 'type', (native) => function (value) {
    const text = stringify(value);
    return apply(native, this, [isSpeculation(text) ? scriptType : text]);
  });
  lockMethod(Element.prototype, 'setAttribute', (native) => function (name, value) {
    const attribute = stringify(name);
    const text = stringify(value);
    const safe = nameOf(this) === 'link' && normalize(attribute) === 'rel' ? safeRel(text) : text;
    return apply(native, this, [attribute, isScript(this) && normalize(attribute) === 'type' && isSpeculation(text) ? scriptType : safe]);
  });
  lockMethod(Element.prototype, 'setAttributeNS', (native) => function (ns, name, value) {
    const qualifiedName = stringify(name);
    const text = stringify(value);
    const attribute = normalize(qualifiedName);
    const safe = nameOf(this) === 'link' && attribute === 'rel' ? safeRel(text) : text;
    return apply(native, this, [ns, qualifiedName, isScript(this) && isSpeculation(text) ? scriptType : safe]);
  });
  for (const name of ['setAttributeNode', 'setAttributeNodeNS']) {
    lockMethod(Element.prototype, name, (native) => function (attribute) {
      return apply(native, this, [cleanAttribute(attribute)]);
    });
  }
  for (const name of ['setNamedItem', 'setNamedItemNS']) {
    lockMethod(NamedNodeMap.prototype, name, (native) => function (attribute) {
      return apply(native, this, [cleanAttribute(attribute)]);
    });
  }
  lockSetter(Attr.prototype, 'value', (native) => function (value) {
    const text = stringify(value);
    const name = normalize(apply(attrName, this, []));
    return apply(native, this, [name === 'type' && isSpeculation(text) ? scriptType : name === 'rel' ? safeRel(text) : text]);
  });
  for (const name of ['nodeValue', 'textContent']) {
    lockSetter(Node.prototype, name, (native) => function (value) {
      const typeAttribute = apply(nodeType, this, []) === 2 && normalize(apply(attrName, this, [])) === 'type';
      const relAttribute = apply(nodeType, this, []) === 2 && normalize(apply(attrName, this, [])) === 'rel';
      const text = typeAttribute && value !== null ? stringify(value) : value;
      return apply(native, this, [typeAttribute && isSpeculation(text) ? scriptType : relAttribute ? safeRel(text) : text]);
    });
  }

  // document.open would erase early listeners; writes instead append sanitized, inert markup.
  lockMethod(Document.prototype, 'open', () => function () { return this; });
  for (const name of ['write', 'writeln']) {
    lockMethod(Document.prototype, name, () => function (...values) {
      let source = '';
      for (let index = 0; index < values.length; index++) source += stringify(values[index]);
      const parsed = apply(nativeParse, new nativeParser(), [cleanHTML(source), 'text/html']);
      clean(parsed);
      const root = apply(bodyGetter, this, []) || apply(rootGetter, this, []);
      if (!root) return;
      const content = apply(bodyGetter, parsed, []);
      let child = apply(firstChild, content, []);
      while (child) {
        const next = apply(nextSibling, child, []);
        apply(nativeAppend, root, [child]);
        child = next;
      }
    });
  }

  // Dismissal is the only cross-frame action. Capture listeners run before the authored ones.
  const parent = window.parent;
  const post = parent.postMessage;
  const key = descriptor(KeyboardEvent.prototype, 'key').get;
  const composing = descriptor(KeyboardEvent.prototype, 'isComposing').get;
  const ctrl = descriptor(KeyboardEvent.prototype, 'ctrlKey').get;
  const alt = descriptor(KeyboardEvent.prototype, 'altKey').get;
  const meta = descriptor(KeyboardEvent.prototype, 'metaKey').get;
  apply(nativeAddListener, window, ['keydown', (event) => {
    if (parent !== window.top || apply(key, event, []) !== 'Escape' || apply(composing, event, [])
      || apply(ctrl, event, []) || apply(alt, event, []) || apply(meta, event, [])) return;
    apply(preventDefault, event, []);
    apply(post, parent, [{ type: 'faber-visual-escape' }, '*']);
  }, true]);
  apply(nativeAddListener, window, ['contextmenu', (event) => {
    apply(preventDefault, event, []);
    if (parent === window.top) apply(post, parent, [{ type: 'faber-visual-contextmenu' }, '*']);
  }, true]);
})();
