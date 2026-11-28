import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type Message, type RewindPreview, type RewindResult, type VisualPiece } from '../../ui/api.ts';
import { Composer } from '../../ui/Composer.tsx';
import { ContextMenuHost } from '../../ui/ContextMenu.tsx';
import { installContextMenu } from '../../ui/contextmenu.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { pieceResult } from '../../ui/piece-expression.ts';
import { Thread } from '../../ui/Thread.tsx';
import { dirtyCount } from '../../ui/unsent.ts';
import { PieceReply, PieceView } from '../../ui/VisualPiece.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
async function until(test: () => unknown) {
  const deadline = performance.now() + 4000;
  while (!test() && performance.now() < deadline) await settle();
  ok(test(), 'the piece updated before the deadline');
}

const samples: VisualPiece['data'][] = [
  { type: 'number', title: 'At a glance', items: [{ label: 'Frame rate', value: '144 FPS' }] },
  { type: 'table', title: 'Compare', columns: ['Card', 'Memory'], rows: [['A', '16 GB'], ['B', '12 GB']] },
  { type: 'chart', title: 'Frame rates', items: [{ label: 'Morning', value: 144 }, { label: 'Evening', value: 90 }] },
  { type: 'calculation', title: 'Budget', label: 'Hours', min: 1, max: 10, step: 1, value: 2, expression: 'x * 5' },
  { type: 'checklist', title: 'Prepare', items: ['Save the file', 'Close the window'] },
  { type: 'tabs', title: 'Details', items: [{ label: 'First', text: 'First detail' }, { label: 'Second', text: 'Second detail' }] },
  { type: 'steps', title: 'Install', items: ['Download', 'Open'] },
  { type: 'form', title: 'Choose', options: ['Fast', 'Quiet'] },
  { type: 'notice', title: 'Heads up', text: 'Your work stays saved.' },
  { type: 'timeline', title: 'History', items: [{ label: 'Today', text: 'Started' }, { label: 'Tomorrow', text: 'Finish' }] },
];
const piece = (data: VisualPiece['data'], block = 0): VisualPiece => ({
  messageId: 'reply', block, start: 0, end: 1, data,
  value: data.type === 'calculation' ? data.value : data.type === 'checklist' ? data.items.map(() => false) : ['tabs', 'steps', 'form'].includes(data.type) ? 0 : null,
});
const maximal: VisualPiece['data'][] = [
  { type: 'table', title: 'Maximum table', columns: Array.from({ length: 6 }, (_, i) => `Column ${i + 1}`), rows: Array.from({ length: 12 }, (_, row) => Array.from({ length: 6 }, (_, column) => `Row ${row + 1}, column ${column + 1}`)) },
  { type: 'chart', title: 'Maximum chart', items: Array.from({ length: 12 }, (_, i) => ({ label: `Measurement ${i + 1}`, value: i - 6 })) },
];
function mount(data: VisualPiece, width = 672, conversationId = 'piece-test') {
  const host = document.createElement('div');
  host.className = 'md';
  host.style.cssText = `position:fixed;left:24px;top:0;width:${width}px;max-height:780px;overflow-y:auto`;
  for (const [key, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${key}`, value);
  host.dataset.motion = 'reduced';
  document.body.append(host);
  const dispose = render(() => createComponent(PieceView, { piece: data, conversationId }), host);
  return { host, close: () => { dispose(); host.remove(); } };
}

check('visual pieces: all ten pieces are native reply content with semantic data and no frame or authored HTML', () => {
  for (const data of samples) {
    const view = mount(piece(data));
    try {
      const root = view.host.querySelector<HTMLElement>(`.visual-piece[data-piece="${data.type}"]`)!;
      ok(root, `${data.type} is drawn by Faber`);
      ok(root.textContent!.includes(data.title));
      equal(root.querySelector('iframe, script, object, embed'), null);
      if (data.type === 'number') ok(root.textContent!.includes('144 FPS'));
      if (data.type === 'table') {
        equal(root.querySelectorAll('thead th').length, 2);
        equal(root.querySelectorAll('tbody tr').length, 2);
        ok(root.querySelector('table')!.textContent!.includes('16 GB'));
      }
      if (data.type === 'chart') {
        ok(root.querySelector('svg'), 'Faber draws the chart in SVG');
        ok(root.textContent!.includes('Morning') && root.textContent!.includes('144'), 'chart labels and values remain readable without colour');
        equal(root.querySelector('svg')!.getAttribute('aria-hidden'), 'true', 'the duplicated drawing does not interrupt reading');
        ok(root.querySelector('.piece-data table')!.textContent!.includes('Morning'), 'the same data is available as a semantic table');
      }
      if (data.type === 'calculation') equal(root.querySelector('output')!.textContent!.trim(), `${t('piece.result')}: 10`);
      if (data.type === 'checklist') equal(root.querySelectorAll('input[type="checkbox"]').length, 2);
      if (data.type === 'tabs') equal(root.querySelectorAll('[role="tab"]').length, 2);
      if (data.type === 'steps') ok(root.textContent!.includes('Download'));
      if (data.type === 'form') equal(root.querySelectorAll('input[type="radio"]').length, 2);
      if (data.type === 'notice') ok(root.textContent!.includes(data.text));
      if (data.type === 'timeline') ok(root.textContent!.includes('Today') && root.textContent!.includes('Finish'));
      ok(root.querySelectorAll('*').length <= 180, `${data.type} has bounded DOM`);
    } finally { view.close(); }
  }
  const safe = mount(piece({ type: 'notice', title: '<img src=x onerror=alert(1)>', text: '<script>bad()</script>' }));
  try {
    equal(safe.host.querySelector('img,script'), null, 'authored strings never become HTML');
    ok(safe.host.textContent!.includes('<script>bad()</script>'));
  } finally { safe.close(); }
});

check('visual pieces: calculations accept bounded arithmetic and never execute names, calls or non-finite results', () => {
  equal(pieceResult('2 + x * (3 - 1)', 4), 10);
  equal(pieceResult('-(x + .5) / 2', 3.5), -2);
  for (const expression of ['alert(1)', 'x.constructor', 'globalThis', 'x; 1', '2 ** 3', '1 / 0', '1000000 * 1000000 * 2', '('.repeat(20) + 'x' + ')'.repeat(20), '1+'.repeat(64) + '1']) {
    equal(pieceResult(expression, 1), undefined, `unsafe or unbounded expression stays inert: ${expression}`);
  }
  equal(pieceResult('x', Infinity), undefined);
  equal(pieceResult('x', 1_000_001), undefined);
});

check('visual pieces: checklist, calculation, tabs, steps and options save to the engine and restore after remount', async () => {
  const before = { ...api };
  const saved = new Map<number, VisualPiece['value']>();
  api.savePieceValue = async (id, messageId, block, value) => {
    equal(id, 'piece-test'); equal(messageId, 'reply');
    saved.set(block, structuredClone(value));
  };
  try {
    for (const [block, data] of samples.entries()) {
      if (!['checklist', 'calculation', 'tabs', 'steps', 'form'].includes(data.type)) continue;
      const first = mount(piece(data, block));
      let expected: string | boolean;
      try {
        if (data.type === 'calculation') {
          const input = first.host.querySelector<HTMLInputElement>('input[type="range"]')!;
          ok(input.labels?.length || input.getAttribute('aria-label'), 'the slider is named');
          equal(input.min, '1'); equal(input.max, '10'); equal(input.step, '1');
          ok(input.tabIndex >= 0, 'the native slider participates in keyboard navigation');
          input.value = '4'; input.dispatchEvent(new Event('input', { bubbles: true }));
          input.dispatchEvent(new Event('change', { bubbles: true }));
          await until(() => first.host.querySelector('output')?.textContent?.trim() === `${t('piece.result')}: 20`);
          expected = '4';
        } else if (data.type === 'checklist' || data.type === 'form' || data.type === 'steps') {
          const inputs = first.host.querySelectorAll<HTMLInputElement>('input');
          const input = inputs[data.type === 'checklist' ? 0 : 1]!;
          ok(input.labels?.length || input.getAttribute('aria-label'), 'the native choice is named');
          ok(input.tabIndex >= 0, 'the native choice participates in keyboard navigation');
          input.click();
          expected = true;
        } else {
          const buttons = first.host.querySelectorAll<HTMLButtonElement>('[role="tab"]');
          ok(buttons.length >= 2, 'tabs can advance by native buttons');
          buttons[1]!.click();
          expected = 'Second detail';
        }
        await until(() => saved.has(block));
      } finally { first.close(); }
      const second = mount({ ...piece(data, block), value: saved.get(block)! });
      try {
        if (data.type === 'calculation') {
          equal(second.host.querySelector<HTMLInputElement>('input[type="range"]')!.value, expected as string);
          equal(second.host.querySelector('output')!.textContent!.trim(), `${t('piece.result')}: 20`);
        } else if (data.type === 'checklist' || data.type === 'form' || data.type === 'steps') {
          equal(second.host.querySelectorAll<HTMLInputElement>('input')[data.type === 'checklist' ? 0 : 1]!.checked, expected as boolean);
        } else if (data.type === 'tabs') {
          equal(second.host.querySelector('[role="tab"][aria-selected="true"]')!.textContent, 'Second');
          ok(second.host.querySelector('[role="tabpanel"]')!.textContent!.includes(expected as string));
        }
      } finally { second.close(); }
    }
  } finally { Object.assign(api, before); }
});

check('visual pieces: tab arrows, Home and End keep one selected, named panel without motion', async () => {
  const before = { ...api };
  api.savePieceValue = async () => {};
  const view = mount(piece(samples.find((sample) => sample.type === 'tabs')!));
  try {
    const tabs = [...view.host.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
    const send = (index: number, key: string) => tabs[index]!.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    equal(tabs[0]!.tabIndex, 0); equal(tabs[1]!.tabIndex, -1);
    for (const [from, key, to] of [[0, 'ArrowRight', 1], [1, 'ArrowRight', 0], [0, 'End', 1], [1, 'Home', 0], [0, 'ArrowLeft', 1]] as const) {
      send(from, key); await settle();
      equal(view.host.querySelectorAll('[role="tab"][aria-selected="true"]').length, 1);
      equal(tabs[to]!.getAttribute('aria-selected'), 'true');
      equal(tabs[to]!.tabIndex, 0);
      const panel = view.host.querySelector('[role="tabpanel"]')!;
      equal(panel.getAttribute('aria-labelledby'), tabs[to]!.id);
      equal(tabs[to]!.getAttribute('aria-controls'), panel.id);
    }
  } finally { view.close(); Object.assign(api, before); }
});

check('visual pieces: fast edits coalesce into one latest saved value and a failed save keeps a retry', async () => {
  const before = { ...api };
  const writes: VisualPiece['value'][] = [];
  let release!: () => void;
  const pending = new Promise<void>((resolve) => { release = resolve; });
  api.savePieceValue = async (_id, _messageId, _block, value) => {
    writes.push(structuredClone(value));
    if (writes.length === 1) await pending;
  };
  const view = mount(piece(samples.find((sample) => sample.type === 'checklist')!));
  try {
    const inputs = view.host.querySelectorAll<HTMLInputElement>('input');
    inputs[0]!.click(); inputs[1]!.click(); inputs[0]!.click();
    equal(writes.length, 1, 'an in-flight write does not build a request for every change');
    release();
    await until(() => writes.length === 2);
    deepEqual(writes, [[true, false], [false, true]], 'the next write uses only the newest complete value');
    await settle();
    api.savePieceValue = async () => { throw new Error('test write failure'); };
    inputs[0]!.click();
    await until(() => view.host.querySelector('[role="alert"]'));
    equal(inputs[0]!.checked, true, 'a failed save does not erase the person’s edit');
    let retried: VisualPiece['value'];
    api.savePieceValue = async (_id, _messageId, _block, value) => { retried = value; };
    view.host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click();
    await until(() => !view.host.querySelector('[role="alert"]'));
    deepEqual(retried!, [true, true], 'retry writes the edited value');
  } finally { release(); view.close(); Object.assign(api, before); }
});

check('visual pieces: an options form fills only its own composer and never sends a message', async () => {
  const before = { ...api };
  api.savePieceValue = async () => {};
  const view = mount(piece(samples.find((sample) => sample.type === 'form')!));
  const composer = document.createElement('div'); view.host.append(composer);
  let sent = 0;
  const dispose = render(() => createComponent(Composer, {
    draftKey: 'piece-test', vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => { sent++; },
  }), composer);
  try {
    const field = composer.querySelector('textarea')!;
    document.dispatchEvent(new CustomEvent('faber:piece-fill', { detail: { conversationId: 'other-conversation', text: 'Wrong conversation' } }));
    await settle(); equal(field.value, '', 'a piece in another conversation cannot fill this draft');
    field.value = 'An earlier thought'; field.dispatchEvent(new Event('input', { bubbles: true }));
    view.host.querySelectorAll<HTMLInputElement>('input[type="radio"]')[1]!.click();
    await settle();
    const fill = [...view.host.querySelectorAll<HTMLButtonElement>('.visual-piece button')].find((button) => button.textContent === t('piece.fill'))!;
    ok(fill); fill.click();
    await until(() => field.value.includes('Quiet'));
    ok(field.value.includes('An earlier thought'), 'filling preserves the existing draft');
    ok(field.value.includes('Choose'), 'the draft retains the question the chosen option answers');
    equal(sent, 0, 'only the person can send the filled draft');
  } finally {
    dispose(); view.close(); Object.assign(api, before);
    localStorage.removeItem('faber.unsent.piece-test');
  }
});

check('visual pieces: switching conversations clears discovered forms before a long plain reply can fill its composer', async () => {
  const before = { ...api };
  const form = samples.find((sample) => sample.type === 'form')!;
  const source = '```faber-piece\n' + JSON.stringify(form) + '\n```';
  const plain = 'Conversation B contains only ordinary text. '.repeat(12);
  const [conversation, setConversation] = createSignal({ id: 'piece-switch-form-a', text: source });
  let reads = 0;
  api.visualPieces = async (id, messageId) => {
    equal(id, 'piece-switch-form-a'); equal(messageId, 'same-reply-id');
    reads++;
    return [{ ...piece(form), messageId, end: source.length }];
  };
  const host = document.createElement('div'); document.body.append(host);
  const reply = document.createElement('div'); host.append(reply);
  const composer = document.createElement('div'); host.append(composer);
  const disposeReply = render(() => createComponent(PieceReply, {
    get text() { return conversation().text; }, get conversationId() { return conversation().id; },
    messageId: 'same-reply-id', offset: 0, answers: [],
  }), reply);
  let sent = 0;
  const disposeComposer = render(() => createComponent(Composer, {
    get draftKey() { return conversation().id; }, vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
    onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {}, onSend: () => { sent++; },
  }), composer);
  try {
    await until(() => reply.querySelector('.piece-slot'));
    reply.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => reply.querySelector('.piece-fill'));
    const oldFill = reply.querySelector<HTMLButtonElement>('.piece-fill')!;
    setConversation({ id: 'piece-switch-form-b', text: plain });
    await settle();
    // Try every remaining fill control: an obsolete A form must never acquire B's conversation identity.
    for (const button of reply.querySelectorAll<HTMLButtonElement>('.piece-fill')) button.click();
    oldFill.click();
    await settle();
    equal(composer.querySelector<HTMLTextAreaElement>('textarea')!.value, '', 'A cannot fill B\'s draft after the switch');
    equal(reply.querySelector('.piece-slot'), null, 'the disabled discovery resource cannot retain A\'s form');
    ok(reply.textContent!.includes(plain.trim()), 'the entire long plain reply remains readable');
    equal(reads, 1, 'a plain reply does not request discovery');
    equal(sent, 0);
  } finally {
    disposeComposer(); disposeReply(); host.remove(); Object.assign(api, before);
    localStorage.removeItem('faber.unsent.piece-switch-form-a');
    localStorage.removeItem('faber.unsent.piece-switch-form-b');
  }
});

check('visual pieces: discovery stays bound to its conversation and message when requests finish out of order', async () => {
  const before = { ...api };
  const a: VisualPiece['data'] = { type: 'form', title: 'Only conversation A', options: ['A option'] };
  const b: VisualPiece['data'] = { type: 'notice', title: 'Only conversation B', text: 'The current reply' };
  const fence = (data: VisualPiece['data']) => '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const first = fence(a);
  const second = fence(b);
  const [conversation, setConversation] = createSignal({ id: 'piece-discovery-a', text: first });
  const waiting: { id: string; finish: (pieces: VisualPiece[]) => void }[] = [];
  api.visualPieces = (id, messageId) => {
    equal(messageId, 'same-reply-id');
    return new Promise((finish) => { waiting.push({ id, finish }); });
  };
  const host = document.createElement('div'); document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, {
    get text() { return conversation().text; }, get conversationId() { return conversation().id; },
    messageId: 'same-reply-id', offset: 0, answers: [],
  }), host);
  try {
    await until(() => waiting.length === 1);
    waiting[0]!.finish([{ ...piece(a), messageId: 'same-reply-id', end: first.length }]);
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('.piece-fill'));
    // Refresh A while keeping the original form mounted, then leave before its request completes.
    setConversation({ id: 'piece-discovery-a', text: first + '\n\n```text\nMore\n```' });
    await until(() => waiting.length === 2);
    setConversation({ id: 'piece-discovery-b', text: second });
    await until(() => waiting.length === 3);
    equal(waiting[2]!.id, 'piece-discovery-b');
    equal(host.querySelector('.piece-slot'), null, 'B never renders A while B\'s discovery is pending');
    waiting[2]!.finish([
      { ...piece(a), messageId: 'different-reply-id', end: second.length },
      { ...piece(b), messageId: 'same-reply-id', end: second.length },
    ]);
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('[data-piece="notice"]'));
    waiting[1]!.finish([{ ...piece(a), messageId: 'same-reply-id', end: first.length }]);
    await settle();
    equal(host.querySelectorAll('.piece-slot').length, 1, 'a result from another message is rejected');
    ok(host.textContent!.includes(b.title), 'B keeps its own current piece');
    ok(!host.textContent!.includes(a.title), 'late A discovery cannot replace B even with the same message ID');
    equal(host.querySelector('.piece-fill'), null);
  } finally {
    for (const request of waiting) request.finish([]);
    dispose(); host.remove(); Object.assign(api, before);
  }
});

check('visual pieces: failed choices and retry warnings survive unmount and clear only after a successful save', async () => {
  const before = { ...api };
  const data = piece(samples.find((sample) => sample.type === 'checklist')!);
  const id = 'piece-recover-failed';
  const dirtyBefore = dirtyCount();
  api.draftDirty = async () => {};
  api.savePieceValue = async () => { throw new Error('write failed'); };
  let view = mount(data, 672, id);
  try {
    view.host.querySelector<HTMLInputElement>('input')!.click();
    await until(() => view.host.querySelector('[role="alert"]'));
    view.close();
    ok(dirtyCount() > dirtyBefore, 'the shell still protects a failed edit after its component is gone');
    const other = mount(data, 672, 'piece-recover-unrelated');
    try {
      equal(other.host.querySelector<HTMLInputElement>('input')!.checked, false, 'pending values are conversation-specific');
      equal(other.host.querySelector('[role="alert"]'), null);
    } finally { other.close(); }
    view = mount(data, 672, id);
    await settle();
    equal(view.host.querySelector<HTMLInputElement>('input')!.checked, true, 'stale engine defaults cannot erase the pending choice');
    ok(view.host.querySelector('[role="alert"]'), 'the failed write still offers a retry after returning');
    let saved: VisualPiece['value'];
    api.savePieceValue = async (conversationId, messageId, block, value) => {
      equal(conversationId, id); equal(messageId, data.messageId); equal(block, data.block);
      saved = structuredClone(value);
    };
    view.host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click();
    await until(() => !view.host.querySelector('[role="alert"]'));
    deepEqual(saved!, [true, false], 'retry saves the recovered choice');
    view.close();
    equal(dirtyCount(), dirtyBefore, 'success releases the persistent failed-edit protection');
    // A fresh engine value must win once there is no longer an unsaved edit.
    view = mount(data, 672, id);
    equal(view.host.querySelector<HTMLInputElement>('input')!.checked, false, 'success does not retain an obsolete pending cache');
    equal(view.host.querySelector('[role="alert"]'), null);
  } finally {
    view.close();
    api.savePieceValue = async () => {};
    const recover = mount(data, 672, id);
    recover.host.querySelector<HTMLButtonElement>('[role="alert"] button')?.click();
    await settle(); recover.close(); Object.assign(api, before);
  }
});

check('visual pieces: a write finishing after disposal retains a failure or releases a successfully saved choice', async () => {
  const before = { ...api };
  const data = piece(samples.find((sample) => sample.type === 'checklist')!);
  api.draftDirty = async () => {};
  try {
    for (const fail of [true, false]) {
      const id = `piece-disposed-write-${fail}`;
      let finish!: () => void;
      let server = structuredClone(data.value);
      api.savePieceValue = async (_id, _messageId, _block, value) => {
        await new Promise<void>((resolve) => { finish = resolve; });
        if (fail) throw new Error('late write failed');
        server = structuredClone(value);
      };
      const first = mount(data, 672, id);
      first.host.querySelector<HTMLInputElement>('input')!.click();
      await until(() => finish);
      first.close(); finish(); await settle();
      const returned = mount({ ...data, value: server }, 672, id);
      try {
        equal(returned.host.querySelector<HTMLInputElement>('input')!.checked, true, 'leaving does not lose a choice while its write finishes');
        equal(!!returned.host.querySelector('[role="alert"]'), fail, 'a late failure remains recoverable when returning');
        if (fail) {
          api.savePieceValue = async () => {};
          returned.host.querySelector<HTMLButtonElement>('[role="alert"] button')!.click();
          await until(() => !returned.host.querySelector('[role="alert"]'));
        }
      } finally { returned.close(); }
    }
  } finally { Object.assign(api, before); }
});

check('visual pieces: discarding a recovered failed choice restores the last saved choice and clears its pending journal', async () => {
  const before = { ...api };
  const data = piece(samples.find((sample) => sample.type === 'checklist')!);
  const id = 'piece-discard-recovered';
  const dirtyBefore = dirtyCount();
  let writes = 0;
  let server = structuredClone(data.value);
  api.draftDirty = async () => {};
  api.savePieceValue = async (_id, _messageId, _block, value) => {
    writes++;
    if (writes > 1) throw new Error('the second write failed');
    server = structuredClone(value);
  };
  let view = mount(data, 672, id);
  try {
    view.host.querySelector<HTMLInputElement>('input')!.click();
    await until(() => writes === 1 && dirtyCount() === dirtyBefore);
    view.host.querySelector<HTMLInputElement>('input')!.click();
    await until(() => view.host.querySelector('[role="alert"]'));
    view.close();
    view = mount(data, 672, id);
    equal(view.host.querySelector<HTMLInputElement>('input')!.checked, false, 'the failed newer choice survives leaving');
    const discard = view.host.querySelector<HTMLButtonElement>('.piece-discard');
    ok(discard, 'the recovered failure offers an explicit discard');
    discard.click();
    await until(() => !view.host.querySelector('[role="alert"]'));
    equal(view.host.querySelector<HTMLInputElement>('input')!.checked, true, 'discard restores the last confirmed save, rather than the original default');
    equal(writes, 2, 'discard does not send a new write');
    equal(dirtyCount(), dirtyBefore, 'discard releases failed-edit protection');
    view.close();
    view = mount({ ...data, value: server }, 672, id);
    equal(view.host.querySelector<HTMLInputElement>('input')!.checked, true, 'the engine value remains authoritative after another mount');
    equal(view.host.querySelector('[role="alert"]'), null, 'discarded failures do not return from the local journal');
  } finally {
    view.close();
    const cleanup = mount({ ...data, value: server }, 672, id);
    cleanup.host.querySelector<HTMLButtonElement>('.piece-discard')?.click();
    cleanup.close(); Object.assign(api, before);
  }
});

check('visual pieces: unknown, broken and unfinished fences stay ordinary code and valid pieces keep surrounding references', async () => {
  const before = { ...api };
  let discovered: VisualPiece[] = [];
  api.visualPieces = async () => discovered;
  const fence = (source: string) => `\`\`\`faber-piece\n${source}\n\`\`\``;
  try {
    for (const source of ['{"type":"unknown","title":"Unknown"}', '{broken', '{"type":"chart","title":"Bad","items":[{"label":"x","value":"NaN"}]}']) {
      const host = document.createElement('div'); document.body.append(host);
      const dispose = render(() => createComponent(PieceReply, { text: fence(source), offset: 0, messageId: 'reply', conversationId: 'piece-test', answers: [] }), host);
      try {
        await settle(); equal(host.querySelector('.visual-piece'), null);
        equal(host.querySelector('code')!.textContent!.trim(), source, 'the exact source remains available as code');
      } finally { dispose(); host.remove(); }
    }
    const unfinished = document.createElement('div'); document.body.append(unfinished);
    const closeUnfinished = render(() => createComponent(PieceReply, { text: '```faber-piece\n{"type":"notice","title":"Still writing"', offset: 0, messageId: 'reply', conversationId: 'piece-test', answers: [] }), unfinished);
    try {
      await settle(); equal(unfinished.querySelector('.visual-piece'), null);
      ok(unfinished.querySelector('code')!.textContent!.includes('Still writing'), 'an unfinished fence stays code');
    } finally { closeUnfinished(); unfinished.remove(); }
    const source = JSON.stringify(samples[8]);
    const prefix = 'Before [reference][r].\n\n';
    const text = prefix + fence(source) + '\n\nAfter.\n\n[r]: https://example.com';
    discovered = [{ ...piece(samples[8]!), start: prefix.length, end: prefix.length + fence(source).length }];
    const host = document.createElement('div'); document.body.append(host);
    const dispose = render(() => createComponent(PieceReply, { text, offset: 0, messageId: 'reply', conversationId: 'piece-test', answers: [] }), host);
    try {
      await until(() => host.querySelector('.piece-slot'));
      host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
      await until(() => host.querySelector('.visual-piece'));
      ok(host.textContent!.includes('Before') && host.textContent!.includes('After.'));
      equal(host.querySelector<HTMLAnchorElement>('a')!.href, 'https://example.com/');
      equal(host.querySelector('pre'), null, 'a valid piece does not retain hidden source DOM');
    } finally { dispose(); host.remove(); }
  } finally { Object.assign(api, before); }
});

check('visual pieces: distant pieces release controls and restore the edited value when they return', async () => {
  const before = { ...api };
  const OriginalObserver = window.IntersectionObserver;
  const observers: ControlledObserver[] = [];
  class ControlledObserver implements IntersectionObserver {
    readonly root = null;
    readonly rootMargin = '200px';
    readonly scrollMargin = '0px';
    readonly thresholds = [0];
    readonly targets = new Set<Element>();
    readonly callback: IntersectionObserverCallback;
    constructor(callback: IntersectionObserverCallback) { this.callback = callback; observers.push(this); }
    observe(target: Element) { this.targets.add(target); }
    unobserve(target: Element) { this.targets.delete(target); }
    disconnect() { this.targets.clear(); }
    takeRecords() { return []; }
    show(target: Element, isIntersecting: boolean) { this.callback([{ target, isIntersecting } as IntersectionObserverEntry], this); }
  }
  window.IntersectionObserver = ControlledObserver;
  const data = samples.find((sample) => sample.type === 'checklist')!;
  const text = '```faber-piece\n' + JSON.stringify(data) + '\n```';
  api.visualPieces = async () => [{ ...piece(data), end: text.length }];
  let saved: VisualPiece['value'];
  api.savePieceValue = async (_id, _messageId, _block, value) => { saved = structuredClone(value); };
  const host = document.createElement('div'); document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, { text, offset: 0, messageId: 'reply', conversationId: 'piece-test', answers: [] }), host);
  try {
    await until(() => observers[0]?.targets.size);
    const observer = observers[0]!;
    const target = [...observer.targets][0]!;
    equal(host.querySelector('.visual-piece'), null, 'an offscreen piece keeps only a small placeholder');
    observer.show(target, true);
    await until(() => host.querySelector('input'));
    host.querySelector<HTMLInputElement>('input')!.click();
    await until(() => saved !== undefined);
    await settle();
    host.querySelector<HTMLInputElement>('input')!.blur();
    observer.show(target, false);
    equal(host.querySelector('input'), null, 'offscreen controls are released');
    ok(host.querySelector('.piece-placeholder'), 'the keyboard can still open the distant piece');
    observer.show(target, true);
    await until(() => host.querySelector('input'));
    equal(host.querySelector<HTMLInputElement>('input')!.checked, true, 'scrolling away and back keeps the edited value');
  } finally { dispose(); host.remove(); window.IntersectionObserver = OriginalObserver; Object.assign(api, before); }
});

check('visual pieces: discovering another closed fence preserves the existing control and its locally saved choice', async () => {
  const before = { ...api };
  const checklist = samples.find((sample) => sample.type === 'checklist')!;
  const notice = samples.find((sample) => sample.type === 'notice')!;
  const fence = (data: VisualPiece['data']) => '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const first = fence(checklist);
  const second = fence(notice);
  const [text, setText] = createSignal(first);
  let reads = 0;
  let saves = 0;
  api.visualPieces = async () => {
    reads++;
    // A discovery response can have been read before the latest value was saved.
    const found = [{ ...piece(checklist), end: first.length }];
    if (text() !== first) found.push({ ...piece(notice, 1), start: first.length + 2, end: text().length });
    return found;
  };
  api.savePieceValue = async () => { saves++; };
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:672px';
  document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, {
    get text() { return text(); }, offset: 0, messageId: 'reply', conversationId: 'piece-test', answers: [],
  }), host);
  try {
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('input[type="checkbox"]'));
    const checkbox = host.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    checkbox.click();
    await until(() => saves === 1);
    equal(checkbox.checked, true);
    setText(first + '\n\n' + second);
    await until(() => reads === 2 && host.querySelectorAll('.piece-slot').length === 2);
    host.querySelectorAll('.piece-slot')[1]!.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('[data-piece="notice"]'));
    equal(host.querySelector('input[type="checkbox"]'), checkbox, 'a discovery refresh keeps the existing control mounted');
    equal(checkbox.checked, true, 'stale discovery defaults cannot overwrite the saved local choice');
    equal(saves, 1, 'discovery does not save or reset a choice by itself');
  } finally { dispose(); host.remove(); Object.assign(api, before); }
});

for (const grouped of [false, true]) check(`visual pieces: rewind and Redo refresh a retained ${grouped ? 'grouped automatic-run' : 'ordinary-reply'} checklist before its next edit`, async () => {
  const before = { ...api };
  installContextMenu();
  const data = samples.find((sample) => sample.type === 'checklist')!;
  const source = '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const conversationId = `piece-rewind-screen-${grouped ? 'run' : 'reply'}`;
  const messages: Message[] = [
    ...(grouped ? [
      { id: 'initial-user', role: 'user', text: 'Prepare everything', at: 1 },
      { id: 'initial-reply', role: 'assistant', text: 'Starting the preparation.', at: 2, status: 'done' },
    ] as Message[] : []),
    { id: 'earlier-user', role: 'user', text: 'Prepare first', at: 3, ...(grouped ? { wake: { why: 'wake.keepGoing' } } : {}) },
    { id: 'earlier-reply', role: 'assistant', text: source, at: 4, status: 'done' },
    ...(grouped ? [
      { id: 'automatic-user', role: 'user', text: 'Keep preparing', at: 5, wake: { why: 'wake.keepGoing' } },
      { id: 'automatic-reply', role: 'assistant', text: 'The automatic work continued.', at: 6, status: 'done' },
    ] as Message[] : []),
    { id: 'later-user', role: 'user', text: 'Continue afterwards', at: 7 },
    { id: 'later-reply', role: 'assistant', text: 'The later answer.', at: 8, status: 'done' },
  ];
  const retainedLength = messages.length - 2;
  const [shown, setShown] = createSignal(messages);
  let retained = messages;
  let server: VisualPiece['value'] = [false, false];
  let snapshot: VisualPiece['value'] = structuredClone(server);
  let restored = false;
  let redone = false;
  let reads = 0;
  const writes: VisualPiece['value'][] = [];
  const preview: RewindPreview = {
    files: [], replacements: 0, recreations: 0, removals: 0, conflicts: 0, messages: 2,
    coverage: { saved: 0, skipped: 0, unavailable: false, incomplete: false }, running: [],
    recovery: false, redoExpiresAt: Date.now() + 86_400_000, notice: 'note.rewind.savedMessages',
  };
  const result: RewindResult = { id: 'piece-redo', restored: [], kept: [], failed: [], partial: false, recovery: false, notice: 'note.rewind.savedMessages' };
  api.draftDirty = async () => {};
  api.visualPieces = async (id, messageId) => {
    equal(id, conversationId); equal(messageId, 'earlier-reply'); reads++;
    return [{ ...piece(data), messageId, end: source.length, value: structuredClone(server) }];
  };
  api.savePieceValue = async (_id, messageId, _block, value) => {
    equal(messageId, 'earlier-reply'); writes.push(structuredClone(value)); server = structuredClone(value);
  };
  api.rewindPoints = async () => [{ id: 'piece-point', messageId: 'later-user', turnId: 'later-turn', at: 3, sealed: true, cursor: { entry: null, turn: null }, coverage: preview.coverage }];
  api.rewindPreview = async () => preview;
  api.rewindRedoPreview = async () => preview;
  api.rewindRedos = async () => restored && !redone ? [{ id: 'piece-redo', messageId: 'later-user', mode: 'conversation', expiresAt: Date.now() + 86_400_000 }] : [];
  api.rewindRestore = async (request) => {
    equal(request.messageId, 'later-user'); equal(request.mode, 'conversation');
    snapshot = structuredClone(server); retained = messages.slice(0, retainedLength); restored = true;
    return result;
  };
  api.rewindRedo = async () => { server = structuredClone(snapshot); retained = messages; redone = true; return result; };
  const host = document.createElement('div');
  host.dataset.motion = 'none';
  host.style.cssText = 'position:fixed;inset:0;width:720px;height:780px;overflow:auto';
  for (const [name, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${name}`, value);
  document.body.append(host);
  const dispose = render(() => [createComponent(Thread, {
    get messages() { return shown(); }, from: 0, conversationId, options: [], onBranch: () => {},
    onRewind: async () => { setShown([...retained]); },
  }), createComponent(ContextMenuHost, {})], host);
  const dialogButton = (label: string) => [...document.querySelectorAll<HTMLButtonElement>('.rewind-confirm button')].find((button) => button.textContent === label);
  const openMenuEntry = async (mine: HTMLElement, label: string) => {
    mine.focus(); mine.dispatchEvent(new KeyboardEvent('keydown', { key: 'ContextMenu', bubbles: true, cancelable: true }));
    await until(() => [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].some((entry) => entry.textContent === label));
    [...document.querySelectorAll<HTMLButtonElement>('.cmenu:not(.leaving) [role=menuitem]')].find((entry) => entry.textContent === label)!.click();
    await until(() => document.querySelector<HTMLDialogElement>('.rewind-confirm')?.open);
  };
  const closeDialog = async () => {
    await until(() => dialogButton(t('rewind.cancel')) && !dialogButton(t('rewind.cancel'))!.disabled);
    dialogButton(t('rewind.cancel'))!.click();
    await until(() => !document.querySelector('.rewind-confirm'));
  };
  try {
    if (grouped) {
      await until(() => host.querySelector('.run-block'));
      const run = host.querySelector<HTMLDetailsElement>('.run-block')!;
      run.open = true; run.dispatchEvent(new Event('toggle'));
      await until(() => host.querySelector('.run-earlier .piece-slot'));
    }
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('input[type=checkbox]'));
    const checkbox = host.querySelector<HTMLInputElement>('input[type=checkbox]')!;
    checkbox.click();
    await until(() => writes.length === 1); await settle();
    deepEqual(server, [true, false]);
    if (grouped) ok(checkbox.closest('.run-earlier'), 'the checklist exercises Thread\'s expanded earlier-turn rendering path');
    await openMenuEntry(host.querySelector<HTMLElement>(`.mine[data-message-index="${retainedLength}"]`)!, t('rewind.title'));
    document.querySelector<HTMLInputElement>('.rewind-confirm input[value=conversation]')!.click();
    await until(() => dialogButton(t('rewind.go')) && !dialogButton(t('rewind.go'))!.disabled);
    dialogButton(t('rewind.go'))!.click();
    await until(() => shown().length === retainedLength && reads >= 2);
    await closeDialog();
    equal(host.querySelector('input[type=checkbox]'), checkbox, 'rewind keeps the earlier checklist mounted');
    checkbox.click();
    await until(() => writes.length === 2); await settle();
    deepEqual(server, [false, false], 'the earlier choice can change after rewinding');
    await openMenuEntry(host.querySelector<HTMLElement>('.mine')!, t('rewind.redo'));
    await until(() => dialogButton(t('rewind.redo')) && !dialogButton(t('rewind.redo'))!.disabled);
    dialogButton(t('rewind.redo'))!.click();
    await until(() => redone && reads >= 3 && checkbox.checked);
    await closeDialog();
    equal(host.querySelector('input[type=checkbox]'), checkbox, 'Redo refreshes the same mounted control despite unchanged source text');
    const second = host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')[1]!;
    second.click();
    await until(() => writes.length === 3);
    deepEqual(writes[2], [true, true], 'the next edit includes the restored first choice instead of overwriting it');
  } finally { dispose(); host.remove(); Object.assign(api, before); }
});

check('visual pieces: restoration refresh preserves a failed edit and discard uses the refreshed confirmed value', async () => {
  const before = { ...api };
  const data = samples.find((sample) => sample.type === 'checklist')!;
  const text = '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const [revision, setRevision] = createSignal(0);
  let server: VisualPiece['value'] = [false, false];
  let reads = 0;
  api.draftDirty = async () => {};
  api.savePieceValue = async () => { throw new Error('write failed before restoration'); };
  api.visualPieces = async () => { reads++; return [{ ...piece(data), end: text.length, value: structuredClone(server) }]; };
  const host = document.createElement('div'); host.className = 'md'; document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, {
    text, offset: 0, messageId: 'reply', conversationId: 'piece-restoration-pending', answers: [],
    get revision() { return revision(); },
  }), host);
  try {
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('input[type=checkbox]'));
    const first = host.querySelector<HTMLInputElement>('input[type=checkbox]')!;
    first.click();
    await until(() => host.querySelector('[role=alert]'));
    server = [false, true]; setRevision(1);
    await until(() => reads === 2); await settle();
    equal(host.querySelector('input[type=checkbox]'), first, 'restoration keeps the failed edit mounted');
    equal(first.checked, true, 'refresh cannot erase an unsaved choice');
    equal(host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')[1]!.checked, false);
    ok(host.querySelector('[role=alert]'), 'the failed-save warning remains available');
    host.querySelector<HTMLButtonElement>('.piece-discard')!.click();
    await until(() => !host.querySelector('[role=alert]'));
    equal(first.checked, false);
    equal(host.querySelectorAll<HTMLInputElement>('input[type=checkbox]')[1]!.checked, true, 'explicit discard returns to the latest restored engine value');
  } finally {
    host.querySelector<HTMLButtonElement>('.piece-discard')?.click();
    dispose(); host.remove(); Object.assign(api, before);
  }
});

check('visual pieces: a pending choice retried after restoration survives releasing and remounting its controls', async () => {
  const before = { ...api };
  const data = samples.find((sample) => sample.type === 'checklist')!;
  const text = '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const conversationId = 'piece-restoration-retry-remount';
  const [revision, setRevision] = createSignal(0);
  let server: VisualPiece['value'] = [false, false];
  let reads = 0;
  const writes: VisualPiece['value'][] = [];
  api.draftDirty = async () => {};
  api.savePieceValue = async () => { throw new Error('write failed before restoration'); };
  api.visualPieces = async () => { reads++; return [{ ...piece(data), end: text.length, value: structuredClone(server) }]; };
  const host = document.createElement('div'); host.className = 'md';
  host.style.cssText = 'position:fixed;left:0;top:0;width:672px'; document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, {
    text, offset: 0, messageId: 'reply', conversationId, answers: [], get revision() { return revision(); },
  }), host);
  try {
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('input[type=checkbox]'));
    host.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();
    await until(() => host.querySelector('[role=alert]'));
    setRevision(1);
    await until(() => reads === 2); await settle();
    equal(host.querySelector<HTMLInputElement>('input[type=checkbox]')!.checked, true, 'restoration preserves the failed newer choice');
    api.savePieceValue = async (_id, _messageId, _block, value) => { writes.push(structuredClone(value)); server = structuredClone(value); };
    [...host.querySelectorAll<HTMLButtonElement>('[role=alert] button')].find((button) => button.textContent === t('piece.retry'))!.click();
    await until(() => !host.querySelector('[role=alert]'));
    deepEqual(writes, [[true, false]], 'retry confirms the preserved pending choice');
    (document.activeElement as HTMLElement | null)?.blur();
    host.style.top = '10000px';
    await until(() => host.querySelector('.piece-placeholder') && !host.querySelector('input[type=checkbox]'));
    host.style.top = '0px';
    await until(() => host.querySelector('input[type=checkbox]'));
    const returned = host.querySelectorAll<HTMLInputElement>('input[type=checkbox]');
    equal(returned[0]!.checked, true, 'a successful retry remains checked when nearby controls are mounted again');
    returned[1]!.click();
    await until(() => writes.length === 2);
    deepEqual(writes[1], [true, true], 'the next edit retains the successfully retried choice after remount');
  } finally {
    host.querySelector<HTMLButtonElement>('.piece-discard')?.click();
    dispose(); host.remove();
    const cleanup = mount({ ...piece(data), value: server }, 672, conversationId);
    cleanup.host.querySelector<HTMLButtonElement>('.piece-discard')?.click(); cleanup.close();
    Object.assign(api, before);
  }
});

check('visual pieces: restoration refreshes a failed choice while its returning reply is still offscreen', async () => {
  const before = { ...api };
  const data = samples.find((sample) => sample.type === 'checklist')!;
  const text = '```faber-piece\n' + JSON.stringify(data) + '\n```';
  const conversationId = 'piece-restoration-offscreen-pending';
  const [revision, setRevision] = createSignal(0);
  let server: VisualPiece['value'] = [false, false];
  let reads = 0;
  api.draftDirty = async () => {};
  api.savePieceValue = async () => { throw new Error('write failed before leaving'); };
  api.visualPieces = async () => { reads++; return [{ ...piece(data), end: text.length, value: structuredClone(server) }]; };
  const first = mount(piece(data), 672, conversationId);
  try {
    first.host.querySelector<HTMLInputElement>('input[type=checkbox]')!.click();
    await until(() => first.host.querySelector('[role=alert]'));
  } finally { first.close(); }
  const host = document.createElement('div'); host.className = 'md';
  host.style.cssText = 'position:fixed;left:0;top:10000px;width:672px';
  document.body.append(host);
  const dispose = render(() => createComponent(PieceReply, {
    text, offset: 0, messageId: 'reply', conversationId, answers: [], get revision() { return revision(); },
  }), host);
  try {
    await until(() => host.querySelector('.piece-placeholder'));
    equal(host.querySelector('.visual-piece'), null, 'returning offscreen keeps the failed choice in its journal without mounting controls');
    server = [false, true]; setRevision(1);
    await until(() => reads === 2); await settle();
    equal(host.querySelector('.visual-piece'), null, 'the restored confirmed value arrives before the piece mounts');
    host.querySelector<HTMLButtonElement>('.piece-placeholder')!.click();
    await until(() => host.querySelector('[role=alert]'));
    const inputs = host.querySelectorAll<HTMLInputElement>('input[type=checkbox]');
    equal(inputs[0]!.checked, true, 'the first mount after restoration preserves the failed pending choice');
    equal(inputs[1]!.checked, false);
    host.querySelector<HTMLButtonElement>('.piece-discard')!.click();
    await until(() => !host.querySelector('[role=alert]'));
    equal(inputs[0]!.checked, false);
    equal(inputs[1]!.checked, true, 'discard uses the confirmed value refreshed while controls were absent');
  } finally {
    host.querySelector<HTMLButtonElement>('.piece-discard')?.click();
    dispose(); host.remove();
    const cleanup = mount({ ...piece(data), value: server }, 672, conversationId);
    cleanup.host.querySelector<HTMLButtonElement>('.piece-discard')?.click(); cleanup.close();
    Object.assign(api, before);
  }
});

check('visual pieces: a big number uses the theme sizes inside a real Markdown reply at 720 px', async () => {
  const before = { ...api };
  const data = samples.find((sample) => sample.type === 'number')!;
  const prefix = 'The result at a glance.\n\n';
  const text = prefix + '```faber-piece\n' + JSON.stringify(data) + '\n```';
  api.visualPieces = async () => [{ ...piece(data), messageId: 'number-reply', start: prefix.length, end: text.length }];
  api.rewindRedos = async () => [];
  const host = document.createElement('div');
  host.dataset.motion = 'reduced';
  host.style.cssText = 'position:fixed;inset:0;width:720px;height:640px;overflow:auto';
  for (const [name, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${name}`, value);
  document.body.append(host);
  const dispose = render(() => createComponent(Thread, {
    messages: [{ id: 'number-reply', role: 'assistant', text, at: 1, status: 'done' }], from: 0,
    conversationId: 'piece-number-real-reply', options: [], onBranch: () => {},
  }), host);
  try {
    await until(() => host.querySelector('.piece-slot'));
    host.querySelector<HTMLButtonElement>('.piece-placeholder')?.click();
    await until(() => host.querySelector('.piece-numbers dd'));
    const number = host.querySelector<HTMLElement>('.piece-numbers dd')!;
    const heading = host.querySelector<HTMLElement>('.visual-piece h3')!;
    const prose = [...host.querySelectorAll<HTMLElement>('.md p')].find((paragraph) => paragraph.textContent === prefix.trim());
    ok(heading, 'the piece title is present in the real reply');
    ok(prose, 'the introductory paragraph remains present inside the Markdown renderer');
    ok(number.closest('.md'), 'the check exercises the actual reply Markdown ancestor');
    equal(getComputedStyle(number).fontSize, themes.css.base['fs-h2'], 'the big number uses the existing large theme size');
    equal(getComputedStyle(heading).fontSize, themes.css.base['fs-lead'], 'the title uses an existing heading size');
    ok(parseFloat(getComputedStyle(number).fontSize) > parseFloat(getComputedStyle(prose!).fontSize), 'the value is visibly larger than ordinary reply text');
    ok(host.scrollWidth <= host.clientWidth + 1, 'the real reply remains within the 720 px window');
  } finally { dispose(); host.remove(); Object.assign(api, before); }
});

check('visual pieces: German content at 720 px wraps and reduced or no motion leaves bounded static DOM', () => {
  useLanguage('de');
  for (const motion of ['reduced', 'none']) {
    for (const data of [...samples, ...maximal]) {
      const view = mount(piece({ ...data, title: 'AusführlicheVergleichsübersicht'.repeat(8) }));
      view.host.dataset.motion = motion;
      try {
        const root = view.host.querySelector<HTMLElement>('.visual-piece')!;
        ok(root.getBoundingClientRect().width <= 672, `${data.type} fits the reply column of a 720 px window`);
        ok(view.host.scrollWidth <= view.host.clientWidth + 1, `${data.type} does not widen the window`);
        for (const el of [root, ...root.querySelectorAll<HTMLElement>('*')]) {
          const style = getComputedStyle(el);
          equal(style.animationName, 'none', `${data.type} has no reading animation`);
          equal(style.offsetPath, 'none', `${data.type} has no motion path`);
        }
        ok(root.querySelectorAll('*').length <= 180, 'a bounded piece adds at most 180 elements, even at the maximum table and chart size');
        const nodes = document.createTreeWalker(root, NodeFilter.SHOW_ALL);
        let count = 1;
        while (nodes.nextNode()) count++;
        ok(count <= 512, 'text and renderer markers also stay bounded to 512 DOM nodes per piece');
      } finally { view.close(); }
    }
  }
});
