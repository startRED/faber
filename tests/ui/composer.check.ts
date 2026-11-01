import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer, type Draft } from '../../ui/Composer.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import type { Choice, Summary } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import titleWords from '../../ui/locales/conversation-titles.json';
import { modelOptions } from '../../ui/models.ts';
import { check, equal, ok } from './check.ts';

check('composer: a localized first-exchange title updates the existing sidebar row without changing the draft', async () => {
  const root = document.createElement('div');
  root.style.width = '720px';
  root.dataset.motion = 'none';
  document.body.append(root);
  const [conversations, setConversations] = createSignal<Summary[]>([
    { id: 'first-exchange-title', title: 'https://example.com/private', area: 'chat', updatedAt: 1_800_000_000_000 },
  ]);
  let fill!: (text: string) => void;
  const closeSidebar = render(() => createComponent(Sidebar, {
    get conversations() { return conversations(); }, projects: [], vendors: [], version: '1',
    place: { area: 'chat' }, running: [], quotas: {}, now: 1_800_000_000_000,
    onNew: () => {}, onArea: () => {}, onProject: () => {}, onNewIn: () => {}, onAddProject: () => {},
    onOpen: () => {}, onRemove: () => {}, onSettings: () => {},
  }), root);
  const composer = document.createElement('div');
  root.append(composer);
  const closeComposer = render(() => createComponent(Composer, {
    vendors: [], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    onReady: (ready) => { fill = ready; },
  }), composer);
  try {
    fill('Keep my next message');
    for (const [language, template] of Object.entries(titleWords)) {
      useLanguage(language);
      const title = template.replace('{host}', 'example.com');
      setConversations((rows) => rows.map((row) => ({ ...row, title })));
      await new Promise((resolve) => setTimeout(resolve, 0));
      const row = root.querySelector('[data-conversation-id="first-exchange-title"]')!;
      ok(row.textContent!.includes(title), `the ${language} title is visible in the same conversation`);
      equal(root.querySelectorAll('[data-conversation-id="first-exchange-title"]').length, 1);
      equal(composer.querySelector('textarea')!.value, 'Keep my next message');
    }
    setConversations((rows) => rows.map((row) => ({ ...row, title: 'Consertar o computador' })));
    await new Promise((resolve) => setTimeout(resolve, 0));
    ok(root.querySelector('[data-conversation-id="first-exchange-title"]')!.textContent!.includes('Consertar o computador'));
  } finally {
    useLanguage('en');
    closeComposer();
    closeSidebar();
    root.remove();
  }
});

check('composer: one model and effort chip preserves the draft and warms the newly chosen model', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:min(720px,100vw)';
  document.body.append(root);
  const [choice, setChoice] = createSignal<Choice>({ vendor: 'claude', model: 'sonnet', effort: 'high' });
  let warmed = 0;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['claude'], options: modelOptions(['claude']), get choice() { return choice(); },
    busy: false, placeholder: '', onChoice: setChoice, onStop: () => {}, onSend: () => {}, onPause: () => {}, onWarm: () => { warmed++; },
  }), root);
  const tick = () => new Promise((resolve) => setTimeout(resolve, 20));
  try {
    await tick();
    const field = root.querySelector('textarea')!;
    field.value = 'Keep this draft'; field.dispatchEvent(new Event('input', { bubbles: true }));
    equal(warmed, 1);
    equal(root.querySelectorAll('.model-chip').length, 1);
    equal(root.querySelectorAll('.effort').length, 0, 'there is no separate effort chip');
    root.querySelector<HTMLButtonElement>('.model-chip')!.click(); await tick();
    const opus = [...root.querySelectorAll<HTMLButtonElement>('.model-option')].find((button) => button.textContent === 'Claude Opus')!;
    opus.click();
    equal(choice().model, 'opus'); equal(choice().effort, 'medium');
    equal(field.value, 'Keep this draft');
    field.value += '!'; field.dispatchEvent(new Event('input', { bubbles: true }));
    equal(warmed, 2, 'the next edit warms the new model');
  } finally { dispose(); root.remove(); }
});

check('composer: wrapped text grows to 40vh, scrolls inside and shrinks after sending', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  let fill!: (text: string) => void;
  let sent = '';
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'A placeholder that wraps over multiple lines. '.repeat(8),
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
    onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
  }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  try {
    await settle();
    const field = root.querySelector('textarea')!;
    const initial = field.clientHeight;
    equal(initial, 60, 'a wrapped placeholder must not grow an empty field');
    // No newlines: wrapping must grow the field too.
    fill('A long prompt with several words. '.repeat(12));
    await settle();
    ok(field.clientHeight > initial, 'wrapped text grows the field');
    const long = 'A very long prompt. '.repeat(500);
    fill(long);
    await settle();
    ok(field.clientHeight <= innerHeight * 0.4 + 1, 'height stays within 40vh');
    ok(field.scrollHeight > field.clientHeight, 'overflow scrolls inside the field');
    field.scrollTop = field.scrollHeight;
    ok(field.scrollTop > 0, 'the overflow can actually be scrolled');
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, long);
    equal(field.value, '');
    equal(field.clientHeight, initial, 'sending restores the empty height');
    equal(field.scrollTop, 0, 'sending resets the internal scroll');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: typed text survives a reload and is forgotten once sent', async () => {
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  let sent = '';
  const open = () => {
    const root = document.createElement('div');
    root.dataset.motion = 'none';
    document.body.append(root);
    let fill!: (text: string) => void;
    const dispose = render(() => createComponent(Composer, {
      draftKey: 'check-reload', vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
    }), root);
    return { root, fill, close: () => (dispose(), root.remove()) };
  };
  try {
    const first = open();
    await settle();
    first.fill('an unsent thought');
    await settle();
    first.close(); // the window goes away, as it does for an update
    const second = open();
    await settle();
    equal(second.root.querySelector('textarea')!.value, 'an unsent thought', 'the text is back after the reload');
    second.root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle();
    equal(sent, 'an unsent thought');
    second.close();
    const third = open();
    await settle();
    equal(third.root.querySelector('textarea')!.value, '', 'a sent message is not restored');
    third.close();
  } finally {
    localStorage.removeItem('faber.unsent.check-reload');
  }
});

const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

check('composer: removing a picked file allows picking the same file again', async () => {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent: Draft | undefined;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
  }), root);
  try {
    const input = root.querySelector<HTMLInputElement>('input[type="file"]')!;
    const file = new File(['%PDF-1.4'], 'same.pdf', { type: 'application/pdf' });
    const pick = () => {
      const transfer = new DataTransfer();
      transfer.items.add(file);
      input.files = transfer.files;
      input.dispatchEvent(new Event('change', { bubbles: true }));
      equal(input.value, '', 'reset the native selection so choosing the same file can fire change again');
    };
    const attached = () => root.querySelector<HTMLButtonElement>('.thumb-x');
    const waitForFile = async () => {
      for (let i = 0; i < 100 && !attached(); i++) await new Promise((resolve) => setTimeout(resolve, 20));
      ok(attached(), 'the picked file is attached after its asynchronous read');
    };
    pick();
    await waitForFile();
    attached()!.click();
    equal(attached(), null, 'removing the file clears its card');
    pick();
    await waitForFile();
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    equal(sent?.files.length, 1, 'the reselected file is sent once');
    equal(sent?.files[0]?.name, 'same.pdf');
    equal(sent?.files[0]?.base64, 'JVBERi0xLjQ=', 'resetting selection preserves the file bytes');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: images show thumbnails and files small cards, each with its own x that removes only it', async () => {
  const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
  const root = document.createElement('div');
  // Pointer-hit checks must reach this isolated fixture, not the running app's title bar underneath it.
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px;z-index:1000';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent: Draft | undefined;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
  }), root);
  try {
    await settle();
    equal(root.querySelector('.attach-row'), null, 'nothing attached, no row');
    const form = root.querySelector('form')!;
    const image = (name: string) => new File([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], name, { type: 'image/png' });
    const transfer = new DataTransfer();
    for (const file of [image('one.png'), image('two.png'), new File(['%PDF-1.4'], 'doc.pdf', { type: 'application/pdf' }), new File(['hello notes'], 'notes.txt', { type: 'text/plain' })]) transfer.items.add(file);
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    const thumbs = () => [...root.querySelectorAll<HTMLElement>('.attach-row .thumb')];
    // Reading the dropped files is asynchronous, and slower in WebKit: wait for them instead of a fixed time.
    for (let i = 0; i < 100 && thumbs().length < 4; i++) await settle(20);
    const nameOf = (el: HTMLElement) => el.querySelector('img')?.alt ?? el.querySelector('b')!.textContent;
    equal(thumbs().map(nameOf).join(), 'one.png,two.png,doc.pdf,notes.txt');
    equal(thumbs().filter((el) => el.querySelector('img')).length, 2, 'the pictures are thumbnails');
    equal(thumbs().filter((el) => el.classList.contains('file')).length, 2, 'the others are cards');
    for (const el of thumbs().filter((e) => e.querySelector('img'))) ok(el.getBoundingClientRect().width <= 64, 'a thumbnail is small');
    const field = root.querySelector('textarea')!;
    ok(field.value.includes('hello notes'), 'a small text file goes inside the message');
    const x = (name: string) => root.querySelector<HTMLButtonElement>(`.thumb-x[aria-label="${t('composer.removeOne', { name })}"]`)!;
    equal(root.querySelectorAll('.thumb-x').length, 4, 'each one has its own x');
    for (const enlargedCorners of [false, true]) {
      if (enlargedCorners) {
        root.style.setProperty('--r-xs', '12px');
        root.style.setProperty('--r-md', '19px');
      }
      for (const button of root.querySelectorAll<HTMLButtonElement>('.thumb-x')) {
        const box = button.getBoundingClientRect();
        ok(box.width >= 24 && box.height >= 24, 'each attachment remove target is at least 24 by 24 CSS pixels');
        for (const dx of [-11.9, 11.9]) for (const dy of [-11.9, 11.9]) {
          const hit = document.elementFromPoint(box.x + box.width / 2 + dx, box.y + box.height / 2 + dy);
          ok(hit === button || (hit !== null && button.contains(hit)), `the remove target contains a full 24-pixel square, including its rounded corners (${button.getAttribute('aria-label')}, enlarged=${enlargedCorners}, box=${box.x},${box.y},${box.width},${box.height}, corner=${dx},${dy}, hit=${hit?.tagName}.${hit?.getAttribute('class')}, radius=${getComputedStyle(button).borderRadius})`);
        }
      }
    }
    root.dataset.motion = 'full';
    let pressRules = 0;
    for (const base of ['sala', 'oficina']) {
      root.dataset.base = base;
      // Synthetic events cannot hold :active; inspect the shipped selectors as if each button were pressed.
      const inspect = (rules: CSSRuleList) => { for (const rule of rules) {
        if (rule instanceof CSSStyleRule && rule.selectorText.includes(':active') && rule.style.transform) {
          pressRules++;
          for (const button of root.querySelectorAll('.thumb-x')) {
            if (button.matches(rule.selectorText.replaceAll(':active', ''))) equal(rule.style.transform, 'none', `a pressed removal target stays still in ${base}`);
          }
        }
        if ('cssRules' in rule) inspect((rule as CSSGroupingRule).cssRules);
      } };
      for (const sheet of document.styleSheets) { let rules: CSSRuleList; try { rules = sheet.cssRules; } catch { continue; } inspect(rules); }
    }
    ok(pressRules > 0, 'the shipped press transforms were inspected');
    root.dataset.motion = 'none';
    equal(getComputedStyle(x('two.png')).opacity, '0', 'shown only for the pointer or the keyboard');
    equal(getComputedStyle(x('two.png')).visibility, 'visible', 'so the keyboard still reaches it');
    // A hidden test window never has the focus or a pointer, so the rule that shows it is read.
    const rules = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]).filter((r): r is CSSStyleRule => r instanceof CSSStyleRule);
    ok(rules.some((r) => r.selectorText.includes('.thumb-x') && r.selectorText.includes(':focus-within') && r.style.opacity === '1'), 'and by hover or focus it shows');
    ok(root.querySelector('.attach-all'), 'with several, one quiet "remove all" remains');
    x('two.png').click();
    x('notes.txt').click();
    await settle();
    equal(thumbs().map(nameOf).join(), 'one.png,doc.pdf', 'only the ones pressed went');
    ok(!field.value.includes('hello notes'), 'a text file takes its words out of the message with it');
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await settle(400);
    equal(sent?.images.length, 1, 'one picture is sent');
    equal(sent?.files.map((f) => f.name).join(), 'doc.pdf');
    equal(root.querySelector('.attach-row'), null, 'and the row closes after sending');
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    await settle(150);
    root.querySelector<HTMLButtonElement>('.attach-all')!.click();
    await settle();
    equal(thumbs().length, 0, 'remove all clears the row');
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: a rejected message comes back with its pictures, files and text files, each still with its own x', async () => {
  const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px';
  root.dataset.motion = 'none';
  document.body.append(root);
  let sent: Draft | undefined;
  let restore!: (draft: Draft) => boolean;
  const dispose = render(() => createComponent(Composer, {
    vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
    onRestore: (put) => { restore = put; },
  }), root);
  try {
    await settle();
    const form = root.querySelector('form')!;
    const transfer = new DataTransfer();
    transfer.items.add(new File([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], 'shot.png', { type: 'image/png' }));
    transfer.items.add(new File(['hello notes'], 'notes.txt', { type: 'text/plain' }));
    form.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    // Each state is waited for, not timed: a slow runner reads the dropped files later than a fixed pause.
    const thumbs = async (n: number) => {
      for (const start = Date.now(); root.querySelectorAll('.thumb').length !== n && Date.now() - start < 3000; ) await settle();
    };
    await thumbs(2);
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    await thumbs(0);
    equal(root.querySelectorAll('.thumb').length, 0, 'sent, the box is empty');
    ok(restore(sent!), 'the screen puts the rejected message back');
    await settle();
    const names = [...root.querySelectorAll('.attach-row .thumb')].map((el) => el.querySelector('img')?.alt ?? el.querySelector('b')!.textContent);
    equal(names.join(), 'shot.png,notes.txt', 'with the names it had, the text file too');
    equal(root.querySelectorAll('.thumb-x').length, 2);
  } finally {
    dispose();
    root.remove();
  }
});

check('composer: unsent attachments survive remount with names bytes and removable inline text', async () => {
  const key = 'check-durable-attachments';
  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
  let sent: Draft | undefined;
  const open = () => {
    const root = document.createElement('div');
    root.dataset.motion = 'none';
    document.body.append(root);
    const dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft; },
    }), root);
    return { root, close: () => { dispose(); root.remove(); } };
  };
  const waitFor = async (root: HTMLElement, n: number) => {
    for (const start = Date.now(); root.querySelectorAll('.thumb').length !== n && Date.now() - start < 3000;) await settle();
    equal(root.querySelectorAll('.thumb').length, n, 'all attachments are restored');
  };
  let current = open();
  try {
    const transfer = new DataTransfer();
    transfer.items.add(new File([Uint8Array.from(atob(PNG), (c) => c.charCodeAt(0))], 'durable.png', { type: 'image/png' }));
    transfer.items.add(new File(['%PDF-1.4'], 'durable.pdf', { type: 'application/pdf' }));
    transfer.items.add(new File(['durable note'], 'durable.txt', { type: 'text/plain' }));
    current.root.querySelector('form')!.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    await waitFor(current.root, 3);
    current.close();
    current = open();
    await waitFor(current.root, 3);
    equal([...current.root.querySelectorAll('.thumb')].map((el) => el.querySelector('img')?.alt ?? el.querySelector('b')!.textContent).join(), 'durable.png,durable.pdf,durable.txt');
    current.root.querySelector<HTMLButtonElement>(`.thumb-x[aria-label="${t('composer.removeOne', { name: 'durable.txt' })}"]`)!.click();
    ok(!current.root.querySelector('textarea')!.value.includes('durable note'), 'restored text file retains its removal block');
    current.root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    equal(sent?.images[0]?.base64, PNG, 'image bytes are intact');
    equal(sent?.files[0]?.base64, 'JVBERi0xLjQ=', 'binary bytes are intact');
    current.close();
    current = open();
    await settle();
    equal(current.root.querySelectorAll('.thumb').length, 0, 'sending never restores old attachments');
  } finally {
    current.close();
    localStorage.removeItem('faber.unsent.' + key);
  }
});

check('composer: a text only draft still sends when IndexedDB is unavailable', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const open = IDBFactory.prototype.open;
  let sent = '';
  let fill!: (text: string) => void;
  let dispose = () => {};
  try {
    IDBFactory.prototype.open = () => { throw new Error('storage unavailable'); };
    dispose = render(() => createComponent(Composer, {
      draftKey: 'check-no-attachment-storage', vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onSend: (draft) => { sent = draft.text; }, onReady: (ready) => { fill = ready; },
    }), root);
    fill('text is independent of binary storage');
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    equal(sent, 'text is independent of binary storage');
  } finally {
    IDBFactory.prototype.open = open;
    dispose(); root.remove();
    localStorage.removeItem('faber.unsent.check-no-attachment-storage');
  }
});

check('composer: slow attachment restoration preserves new typing and typing never rewrites bytes', async () => {
  const { keep, keepAttachments, forgetDraft } = await import('../../ui/unsent.ts');
  const key = 'check-slow-attachment-load';
  let restore!: (draft: Draft) => boolean;
  const root = document.createElement('div');
  document.body.append(root);
  const read = FileReader.prototype.readAsDataURL;
  const open = IDBFactory.prototype.open;
  const reads: (() => void)[] = [];
  let opens = 0;
  let dispose = () => {};
  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
  try {
    keep(key, '');
    ok(await keepAttachments(key, { images: [{ name: 'slow.png', mediaType: 'image/png', base64: PNG }], files: [], inlined: [] }));
    FileReader.prototype.readAsDataURL = function (blob) { reads.push(() => read.call(this, blob)); };
    IDBFactory.prototype.open = function (...args: Parameters<IDBFactory['open']>) { opens++; return open.apply(this, args); };
    dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {}, onRestore: (put) => { restore = put; },
    }), root);
    for (const start = Date.now(); !reads.length && Date.now() - start < 3000;) await settle();
    ok(reads.length, 'the stored image read is held in flight');
    equal(restore({ text: '', images: [{ mediaType: 'image/png', base64: PNG }], files: [] }), false, 'an image-only saved draft is not empty while its bytes load');
    const field = root.querySelector('textarea')!;
    field.value = 'new words typed during recovery';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    reads.splice(0).forEach((resume) => resume());
    for (const start = Date.now(); !root.querySelector('.thumb') && Date.now() - start < 3000;) await settle();
    equal(root.querySelector('img')?.alt, 'slow.png');
    equal(field.value, 'new words typed during recovery');
    const before = opens;
    for (let i = 0; i < 10; i++) {
      field.value += 'x';
      field.dispatchEvent(new Event('input', { bubbles: true }));
    }
    await settle();
    equal(opens, before, 'typing stores text only, with no IndexedDB reads or writes');
  } finally {
    FileReader.prototype.readAsDataURL = read;
    IDBFactory.prototype.open = open;
    reads.splice(0).forEach((resume) => resume());
    dispose(); root.remove();
    await forgetDraft(key);
  }
});

check('composer: a failed attachment load preserves its saved record and offers explicit discard', async () => {
  const { keep, keepAttachments, keptAttachments, forgetDraft } = await import('../../ui/unsent.ts');
  const key = 'check-failed-attachment-load';
  const root = document.createElement('div');
  document.body.append(root);
  const open = IDBFactory.prototype.open;
  let dispose = () => {};
  let sent = false;
  try {
    keep(key, 'keep my words');
    ok(await keepAttachments(key, { images: [], files: [{ name: 'stored.dat', base64: 'AAE=' }], inlined: [] }));
    IDBFactory.prototype.open = () => { throw new Error('storage unavailable'); };
    dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => { sent = true; },
    }), root);
    await new Promise((resolve) => setTimeout(resolve, 30));
    ok(root.textContent?.includes(t('composer.draftLoadFailed')), 'the failed read is announced beside the field');
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    equal(sent, false, 'sending cannot silently discard unread attachments');
    IDBFactory.prototype.open = open;
    equal((await keptAttachments(key))?.files[0]?.name, 'stored.dat', 'a failed load never writes an empty replacement');
    root.querySelector<HTMLButtonElement>('.hint button')!.click();
    equal(root.querySelector('textarea')!.value, 'keep my words');
    ok(await forgetDraft(key));
    equal(await keptAttachments(key), undefined, 'only the explicit discard removes the saved attachments');
  } finally {
    IDBFactory.prototype.open = open;
    dispose(); root.remove();
    await forgetDraft(key);
  }
});

check('composer: late file reads after remount preserve both additions and update the current box', async () => {
  const { forgetDraft, keptAttachments } = await import('../../ui/unsent.ts');
  const key = 'check-late-two-attachments';
  const read = FileReader.prototype.readAsDataURL;
  const reads: (() => void)[] = [];
  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
  const open = () => {
    const root = document.createElement('div');
    root.dataset.motion = 'none'; document.body.append(root);
    const dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    }), root);
    return { root, close: () => { dispose(); root.remove(); } };
  };
  let current = open();
  try {
    FileReader.prototype.readAsDataURL = function (blob) {
      const result = (blob as File).name === 'late.png' ? 'data:image/png;base64,' + PNG : 'data:application/pdf;base64,JVBERi0xLjQ=';
      reads.push(() => {
        Object.defineProperty(this, 'result', { value: result, configurable: true });
        this.onload?.call(this, new ProgressEvent('load') as ProgressEvent<FileReader>);
      });
    };
    for (const file of [new File(['image'], 'late.png', { type: 'image/png' }), new File(['pdf'], 'late.pdf', { type: 'application/pdf' })]) {
      const transfer = new DataTransfer(); transfer.items.add(file);
      current.root.querySelector('form')!.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    }
    for (const start = Date.now(); reads.length !== 2 && Date.now() - start < 3000;) await settle();
    equal(reads.length, 2, 'both selected files are still reading');
    current.close();
    current = open();
    const field = current.root.querySelector('textarea')!;
    field.value = 'words typed after returning';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    FileReader.prototype.readAsDataURL = read;
    reads.splice(0).forEach((resume) => resume());
    for (const start = Date.now(); current.root.querySelectorAll('.thumb').length !== 2 && Date.now() - start < 3000;) await settle();
    const stored = await keptAttachments(key);
    equal((stored?.images.length ?? 0) + (stored?.files.length ?? 0), 2, 'two late read-modify-writes cannot overwrite one another');
    equal(current.root.querySelectorAll('.thumb').length, 2, 'the current composer sees attachments saved by its old instance');
    equal(field.value, 'words typed after returning');
    field.value += ' plus another word';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    equal(current.root.querySelectorAll('.thumb').length, 2, 'later typing cannot erase an invisible attachment');
  } finally {
    FileReader.prototype.readAsDataURL = read;
    reads.splice(0).forEach((resume) => resume());
    current.close(); await forgetDraft(key);
  }
});

check('composer: sending after remount cancels a file still being read by the old box', async () => {
  const { forgetDraft, keptAttachments } = await import('../../ui/unsent.ts');
  const key = 'check-send-before-old-file';
  const read = FileReader.prototype.readAsDataURL;
  let resume: (() => void) | undefined;
  let sent = '';
  const open = () => {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: (draft) => { sent = draft.text; },
    }), root);
    return { root, close: () => { dispose(); root.remove(); } };
  };
  let current = open();
  try {
    FileReader.prototype.readAsDataURL = function () {
      resume = () => {
        Object.defineProperty(this, 'result', { value: 'data:application/pdf;base64,JVBERi0xLjQ=', configurable: true });
        this.onload?.call(this, new ProgressEvent('load') as ProgressEvent<FileReader>);
      };
    };
    const transfer = new DataTransfer(); transfer.items.add(new File(['pdf'], 'late.pdf', { type: 'application/pdf' }));
    current.root.querySelector('form')!.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    for (const start = Date.now(); !resume && Date.now() - start < 3000;) await new Promise((resolve) => setTimeout(resolve, 20));
    ok(resume, 'the file read is pending');
    current.close(); current = open();
    const field = current.root.querySelector('textarea')!;
    field.value = 'send these words'; field.dispatchEvent(new Event('input', { bubbles: true }));
    current.root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
    equal(sent, 'send these words');
    FileReader.prototype.readAsDataURL = read;
    resume(); resume = undefined;
    await new Promise((resolve) => setTimeout(resolve, 30));
    equal(await keptAttachments(key), undefined, 'the previous box cannot restore a file after the new one sent');
    equal(current.root.querySelectorAll('.thumb').length, 0);
    equal(field.value, '');
  } finally {
    FileReader.prototype.readAsDataURL = read;
    resume?.(); current.close(); await forgetDraft(key);
  }
});

check('composer: cleaning edited inline metadata on reopen preserves a newly selected file still being read', async () => {
  const { keep, keepAttachments, keptAttachments, forgetDraft } = await import('../../ui/unsent.ts');
  const key = 'check-inline-cleanup-pending-file';
  const read = FileReader.prototype.readAsDataURL;
  const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
  let resume: (() => void) | undefined;
  const open = () => {
    const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
    const dispose = render(() => createComponent(Composer, {
      draftKey: key, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: 'Ask',
      onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    }), root);
    return { root, close: () => { dispose(); root.remove(); } };
  };
  let current: ReturnType<typeof open> | undefined;
  try {
    const block = '\n\nprivate contents of the old text file';
    keep(key, 'Original words' + block);
    ok(await keepAttachments(key, { images: [], files: [], inlined: [{ name: 'old.txt', block }] }));
    current = open();
    for (const start = Date.now(); !current.root.querySelector('.thumb') && Date.now() - start < 3000;) await settle();
    equal(current.root.querySelector('.thumb b')?.textContent, 'old.txt', 'the original inline file has loaded');
    FileReader.prototype.readAsDataURL = function () {
      resume = () => {
        Object.defineProperty(this, 'result', { value: 'data:application/pdf;base64,JVBERi0xLjQ=', configurable: true });
        this.onload?.call(this, new ProgressEvent('load') as ProgressEvent<FileReader>);
      };
    };
    const transfer = new DataTransfer(); transfer.items.add(new File(['%PDF-1.4'], 'new.pdf', { type: 'application/pdf' }));
    current.root.querySelector('form')!.dispatchEvent(new DragEvent('drop', { dataTransfer: transfer, bubbles: true, cancelable: true }));
    for (const start = Date.now(); !resume && Date.now() - start < 3000;) await settle();
    ok(resume, 'the newly selected binary file is still being read');
    const field = current.root.querySelector('textarea')!;
    field.value = 'Edited words without the old private file';
    field.dispatchEvent(new Event('input', { bubbles: true }));
    current.close(); current = open();
    // Reopening prunes the obsolete metadata and commits its deletion while the new file's reader is held.
    for (const start = Date.now(); localStorage.getItem('faber.unsent-attachments.' + key) !== null && Date.now() - start < 3000;) await settle();
    equal(localStorage.getItem('faber.unsent-attachments.' + key), null, 'the obsolete metadata was durably removed');
    equal(current.root.querySelectorAll('.thumb').length, 0);
    FileReader.prototype.readAsDataURL = read;
    resume(); resume = undefined;
    for (const start = Date.now(); !current.root.querySelector('.thumb') && Date.now() - start < 3000;) await settle();
    equal(current.root.querySelector('.thumb b')?.textContent, 'new.pdf', 'cleanup must not cancel the separate file selected before navigation');
    const saved = await keptAttachments(key);
    equal(saved?.files[0]?.name, 'new.pdf');
    equal(saved?.files[0]?.base64, 'JVBERi0xLjQ=', 'the new binary bytes are retained');
    equal(saved?.inlined.length, 0, 'the obsolete private metadata stays removed');
    equal(current.root.querySelector('textarea')!.value, 'Edited words without the old private file');
  } finally {
    FileReader.prototype.readAsDataURL = read;
    resume?.(); current?.close(); await forgetDraft(key);
  }
});
