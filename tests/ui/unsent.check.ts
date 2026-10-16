import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import type { Message } from '../../ui/api.ts';
import { dirtyCount, keep, kept, release } from '../../ui/unsent.ts';
import { check, equal } from './check.ts';

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
