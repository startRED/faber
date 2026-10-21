import { createComponent, createSignal, Show } from 'solid-js';
import { render } from 'solid-js/web';
import { appliedLook, applyLook } from '../../ui/appearance.ts';
import { en } from '../../ui/locales/en.ts';
import { t } from '../../ui/i18n.ts';
import { grouped, isFirstInstall, NOTES, newestId, notesOf, whatsNew, type Note } from '../../ui/notes.ts';
import { WhatsNew } from '../../ui/WhatsNew.tsx';
import { keepRoot, sala } from './appearance.check.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// "What's new" (ui/notes.ts, ui/WhatsNew.tsx): which entries show, and the one screen that shows them.

// The id only drives the ordering checks; the text is a real English entry of that kind, so the screen always has one to draw.
const entry = (id: number, kind: Note['kind']): Note => ({ id, kind, key: NOTES.find((n) => n.kind === kind)!.key });
const LIST = [entry(1, 'new'), entry(2, 'better'), entry(3, 'fixed'), entry(4, 'new')];

check('notes: the entries are the numbered texts of the English file, oldest first, each id once', () => {
  const ids = NOTES.map((n) => n.id);
  ok(ids.length > 0, 'there is at least one entry');
  deepEqual(ids, [...ids].sort((a, b) => a - b), 'in order');
  equal(new Set(ids).size, ids.length, 'an id is never used twice');
  for (const n of NOTES) ok(n.key in en, `${n.key} is an English text`);
  deepEqual(notesOf(['notes.title', 'notes.2.fixed', 'notes.1.new', 'notes.1.wrong', 'other']).map((n) => n.key), ['notes.1.new', 'notes.2.fixed']);
});

check('notes: after an update only the entries newer than the last one seen show', () => {
  deepEqual(whatsNew(LIST, 2, false).show.map((n) => n.id), [3, 4]);
  deepEqual(whatsNew(LIST, 4, false).show, [], 'the newest was seen: no screen');
  deepEqual(whatsNew(LIST, 9, false).show, [], 'a downgrade shows nothing');
  equal(whatsNew(LIST, 2, false).remember, undefined, 'nothing is stored before the person has seen them');
});

check('notes: a first install shows nothing and starts from the newest entry; an install from before the screen shows the list once', () => {
  deepEqual(whatsNew(LIST, undefined, true), { show: [], remember: 4 });
  deepEqual(whatsNew(LIST, undefined, false).show.map((n) => n.id), [1, 2, 3, 4]);
  deepEqual(whatsNew([], undefined, true), { show: [] });
  equal(newestId(LIST), 4);
  equal(newestId([]), 0);
});

check('notes: the entries are grouped by kind, new first, and a kind with none is left out', () => {
  deepEqual(grouped(LIST).map((g) => [g.kind, g.notes.map((n) => n.id)]), [['new', [1, 4]], ['better', [2]], ['fixed', [3]]]);
  deepEqual(grouped([entry(7, 'fixed')]).map((g) => g.kind), ['fixed']);
});

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

/** Draws the screen on the page for `body`, wearing a look of this motion level, and puts everything back after. */
async function shown(motion: 'full' | 'reduced', onClose: () => void, body: (root: HTMLElement) => Promise<void>) {
  const restore = keepRoot();
  const saved = appliedLook();
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(WhatsNew, { version: '9.9.9', notes: LIST, onClose }), host);
  try {
    await applyLook(sala({ motion }));
    await settle();
    await body(host);
  } finally {
    dispose();
    host.remove();
    if (saved) await applyLook(saved);
    restore();
  }
}

check('notes: the screen shows the version and the entries by kind, and focus starts on Continue', () =>
  shown('full', () => {}, async (host) => {
    const screen = host.querySelector<HTMLElement>('.notes')!;
    ok(screen, 'the screen is on the page');
    equal(screen.getAttribute('aria-modal'), 'true');
    ok(screen.textContent?.includes('9.9.9'), 'the version');
    deepEqual([...screen.querySelectorAll('h2')].map((h) => h.textContent), [t('notes.kind.new'), t('notes.kind.better'), t('notes.kind.fixed')]);
    equal(screen.querySelectorAll('li').length, LIST.length);
    equal(document.activeElement, host.querySelector('button'), 'Continue has the focus');
    equal(host.querySelector('button')!.textContent, t('notes.continue'));
  }));

check('notes: Continue closes it once', () => {
  let closed = 0;
  return shown('full', () => closed++, async (host) => {
    host.querySelector('button')!.click();
    equal(closed, 1);
  });
});

check('notes: Esc closes it too, and the shell does not see the key', () => {
  let closed = 0;
  let leaked = 0;
  const spy = () => leaked++;
  return shown('full', () => closed++, async () => {
    // A listener on the document stands for the shell's own shortcuts: the capture on the window must keep the key from it.
    document.addEventListener('keydown', spy);
    try {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    } finally {
      document.removeEventListener('keydown', spy);
    }
    ok(closed >= 1, 'Esc closed it');
    equal(leaked, 0, 'the key stopped at the screen');
  });
});

/** The screen as the app mounts it: shown while `open`, gone from the page when it is not. */
async function mounted(close: (host: HTMLElement) => void) {
  const host = document.createElement('div');
  document.body.append(host);
  const [open, setOpen] = createSignal(true);
  const dispose = render(
    // `render` runs its callback once, so the condition lives in `Show`, as in the app.
    () =>
      createComponent(Show, {
        keyed: false as never,
        get when() {
          return open();
        },
        get children() {
          return createComponent(WhatsNew, { version: '9.9.9', notes: LIST, onClose: () => setOpen(false) });
        },
      }),
    host,
  );
  try {
    await settle();
    ok(document.querySelector('.notes'), 'drawn');
    close(host);
    await settle();
    equal(document.querySelector('.notes'), null, 'nothing of the screen is left on the page');
    equal(host.querySelector('button'), null, 'not even its button');
  } finally {
    dispose();
    host.remove();
  }
}

check('notes: after Continue the whole screen is gone from the page', () => mounted((host) => host.querySelector('button')!.click()));

check('notes: after Esc the whole screen is gone from the page', () =>
  mounted(() => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }))));

check('notes: only an install with no data at all is a first install', () => {
  ok(isFirstInstall({}, []), 'no settings, no conversation');
  ok(isFirstInstall(undefined, []));
  equal(isFirstInstall({ language: 'en' }, []), false, 'a saved choice means the app was used');
  equal(isFirstInstall({}, [{ id: 'c1' }]), false, 'so does a conversation');
  // The install that stayed on 0.3.14: it finished the first run long ago but never saved an appearance with a base.
  const old = { setup: 'done', tutorialDone: true, userName: 'Erick' };
  equal(isFirstInstall(old, [{ id: 'c1' }]), false);
  equal(isFirstInstall(old, []), false);
  const due = whatsNew(LIST, undefined, isFirstInstall(old, [{ id: 'c1' }]));
  deepEqual(due.show.map((n) => n.id), [1, 2, 3, 4], 'an existing install without a seen entry is shown the entries');
  deepEqual(whatsNew(LIST, undefined, isFirstInstall({}, [])), { show: [], remember: 4 });
});

check('notes: it arrives with a gentle entrance, and with none at all under reduced motion', async () => {
  await shown('full', () => {}, async (host) => {
    equal(getComputedStyle(host.querySelector('.notes')!).animationName, 'arrive', 'the screen eases in');
    equal(getComputedStyle(host.querySelector('li')!).animationName, 'arrive', 'each line arrives');
  });
  await shown('reduced', () => {}, async (host) => {
    for (const el of host.querySelectorAll<HTMLElement>('.notes, h2, li')) {
      equal(getComputedStyle(el).animationName, 'none', 'no animation under reduced motion');
    }
  });
});
