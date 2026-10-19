import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { dirtyCount, keep, kept, release } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 30));

check('unsent: a failed storage write keeps the box reported as dirty until a write succeeds or the box is emptied', () => {
  const before = dirtyCount();
  const write = Storage.prototype.setItem;
  try {
    Storage.prototype.setItem = () => { throw new Error('full'); };
    keep('check-fails', 'text that cannot be saved');
    equal(dirtyCount(), before + 1, 'the text lives only in memory, so the install must wait');
    keep('check-fails', 'still cannot be saved');
    equal(dirtyCount(), before + 1, 'one box counts once');
    Storage.prototype.setItem = write;
    keep('check-fails', 'saved now');
    equal(dirtyCount(), before, 'a write that succeeds lifts it');
    equal(kept('check-fails'), 'saved now');
    Storage.prototype.setItem = () => { throw new Error('full'); };
    keep('check-fails', 'lost again');
    equal(dirtyCount(), before + 1);
    keep('check-fails', '');
    equal(dirtyCount(), before, 'an emptied box has nothing to lose');
    keep('check-fails', 'again');
    release('check-fails');
    equal(dirtyCount(), before, 'a box that is gone no longer counts');
  } finally {
    Storage.prototype.setItem = write;
    keep('check-fails', '');
  }
});

check('unsent: text a failed write could not save is restored over older stored text, and goes after the next good write', () => {
  const write = Storage.prototype.setItem;
  try {
    keep('check-memory', 'old text');
    Storage.prototype.setItem = () => { throw new Error('full'); };
    keep('check-memory', 'newer text');
    equal(kept('check-memory'), 'newer text', 'a box built again reads the newer text, not the stale stored one');
    release('check-memory');
    equal(kept('check-memory'), 'newer text', 'gone from the screen, the text still comes back');
    Storage.prototype.setItem = write;
    keep('check-memory', 'saved');
    equal(kept('check-memory'), 'saved', 'the next good write replaces it');
    Storage.prototype.setItem = () => { throw new Error('full'); };
    keep('check-memory', 'unsaved');
    keep('check-memory', '');
    Storage.prototype.setItem = write;
    equal(kept('check-memory'), '', 'an emptied box forgets it');
  } finally {
    Storage.prototype.setItem = write;
    keep('check-memory', '');
  }
});

check('thread: an edit box keeps its own draft when the conversation changes and comes back', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const mine = (text: string): Message => ({ id: 'm1', role: 'user', at: 0, status: 'done', text });
  const [conversation, setConversation] = createSignal('check-conv-a');
  const dispose = render(() => createComponent(Thread, {
    get conversationId() { return conversation(); },
    get messages() { return [mine(conversation() === 'check-conv-a' ? 'hello A' : 'hello B')]; },
    from: 0, options: [], onBranch: () => {},
  }), root);
  const type = async (text: string) => {
    root.querySelector<HTMLButtonElement>('button.edit')!.click();
    await settle();
    const field = root.querySelector('textarea')!;
    field.value = text;
    field.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
  };
  try {
    await type('A edited');
    equal(kept('edit.check-conv-a.m1'), 'A edited');
    setConversation('check-conv-b');
    await settle();
    equal(kept('edit.check-conv-a.m1'), 'A edited', "another conversation's message, same id, does not touch it");
    equal(root.querySelector('textarea'), null, 'B has no edit open');
    await type('B edited');
    equal(kept('edit.check-conv-b.m1'), 'B edited');
    setConversation('check-conv-a');
    await settle();
    equal(root.querySelector('textarea')!.value, 'A edited', "A's draft is back");
    equal(kept('edit.check-conv-a.m1'), 'A edited');
    equal(kept('edit.check-conv-b.m1'), 'B edited', "B's is untouched");
  } finally {
    dispose();
    root.remove();
    keep('edit.check-conv-a.m1', '');
    keep('edit.check-conv-b.m1', '');
  }
});

check('thread: opening the edit box on a prompt and cancelling leaves a streaming answer exactly as it was', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const paragraphs = ['Sigo no código.', 'Corrigindo a parte 2.', 'Parte 2 pronta.', 'Atualizando o mapa do produto.'];
  const tool = (id: string, done: boolean) => ({ id, name: 'Bash', input: { command: `step ${id}` }, ok: done ? true : undefined });
  const user: Message = { id: 'p1', role: 'user', at: 0, status: 'done', text: 'Fix part 2' };
  // The engine marks where the newest step starts, so while it runs everything said so far reads as the answer.
  const answer = (tick: number): Message => {
    const text = paragraphs.join('\n\n');
    return { id: 'a1', role: 'assistant', at: 0, status: 'streaming', text, activityEnd: text.length, cacheWarmUntil: tick, tools: [tool('1', true), tool('2', true), tool('3', false)] };
  };
  const [message, setMessage] = createSignal<Message>(answer(0));
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [user, message()]; }, from: 0, conversationId: 'check-edit-stream', options: [], onBranch: () => {},
  }), root);
  const shown = () => root.querySelector('.ai > .md')!.textContent ?? '';
  const everything = (when: string) => {
    for (const paragraph of paragraphs) ok(shown().includes(paragraph), `${when}: "${paragraph}" is shown, the answer reads ${JSON.stringify(shown())}`);
  };
  try {
    await settle();
    everything('before the edit');
    root.querySelector<HTMLButtonElement>('button.edit')!.click();
    await settle();
    ok(root.querySelector('textarea'), 'the edit box is open');
    setMessage(answer(1));
    await settle();
    everything('while the edit box is open');
    root.querySelector<HTMLButtonElement>('.you.editing button[type="button"]')!.click();
    await settle();
    equal(root.querySelector('textarea'), null, 'the edit box is closed');
    everything('right after cancelling');
    setMessage(answer(2));
    await settle();
    everything('on the next tick of the stream');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: in a scrolled thread, cancelling an edit leaves every paragraph of the streaming answer drawn', async () => {
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:600px';
  host.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) host.style.setProperty(`--${key}`, value);
  host.innerHTML = '<div class="work" style="height:100%"><div class="thread"><div class="col"></div></div></div>';
  document.body.append(host);
  const paragraphs = Array.from({ length: 4 }, (_, i) => `Paragraph ${i + 1}. ${'It says a good deal about the work in progress. '.repeat(6)}`);
  const user: Message = { id: 'p1', role: 'user', at: 0, status: 'done', text: 'Fix part 2' };
  const [message, setMessage] = createSignal<Message>({ id: 'a1', role: 'assistant', at: 0, status: 'streaming', text: '' });
  const say = (said: string[], tick: number): Message => {
    const text = said.join('\n\n');
    return { id: 'a1', role: 'assistant', at: 0, status: 'streaming', text, activityEnd: text.length, cacheWarmUntil: tick, tools: [{ id: '1', name: 'Bash', input: { command: 'x' }, ok: true }, { id: '2', name: 'Bash', input: { command: 'y' } }] };
  };
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [user, message()]; }, from: 0, conversationId: 'check-edit-layout', options: [], onBranch: () => {},
  }), host.querySelector('.col')!);
  const thread = host.querySelector<HTMLElement>('.thread')!;
  const pin = () => { thread.scrollTop = thread.scrollHeight; };
  const drawn = (when: string) => {
    const blocks = [...host.querySelectorAll<HTMLElement>('.ai > .md > div > p')];
    equal(blocks.length, paragraphs.length, `${when}: every paragraph is in the answer`);
    for (const block of blocks) {
      ok(block.getBoundingClientRect().height > 0, `${when}: a paragraph has no height`);
      ok(block.checkVisibility({ contentVisibilityAuto: true }), `${when}: a paragraph is skipped by the browser`);
    }
  };
  try {
    setMessage(say(paragraphs, 0));
    await settle();
    pin();
    await settle();
    drawn('before the edit');
    host.querySelector<HTMLButtonElement>('button.edit')!.click();
    await settle();
    pin();
    setMessage(say(paragraphs, 1));
    await settle();
    drawn('while the edit box is open');
    host.querySelector<HTMLButtonElement>('.you.editing button[type="button"]')!.click();
    await settle();
    drawn('right after cancelling');
    pin();
    await settle();
    drawn('after the thread follows the tail');
  } finally {
    dispose();
    host.remove();
  }
});
