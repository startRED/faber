import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type Conversation } from '../../ui/api.ts';
import { agentWide, showAgent } from '../../ui/AgentChat.tsx';
import { showSite } from '../../ui/Site.tsx';
import { App } from '../../ui/App.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { ASIDE_USUAL, autoRun, dockable, fit, KINDS, limitsOf, paneUsual, RUN, savedWidths, SIDE, STAGE_MIN } from '../../ui/layout.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// Arranging the shell by hand: the edges of the sidebar and the running column, focus mode (Ctrl+B) and a file over the window.

const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));

check('layout: a dragged width keeps the stage 440 px wide, and what storage holds is repaired', () => {
  deepEqual(limitsOf('side', 1280, 248, true), SIDE);
  equal(limitsOf('side', 1100, 288, true).max, 1100 - RUN.min - STAGE_MIN, 'at 1,100 px the sidebar leaves the docked column and the stage their minimums');
  equal(limitsOf('side', 1100, 288, false).max, SIDE.max, 'with the column over the stage it has its whole range');
  // The docked column gives up room before the stage does; over the stage it may take all of its range.
  equal(limitsOf('run', 1280, 248, true).max, 1280 - 248 - STAGE_MIN, 'a docked column may take everything but the stage minimum');
  equal(limitsOf('run', 1920, 288, true).max, 1920 - 288 - STAGE_MIN, 'far past the usual 560 px on a wide window');
  equal(limitsOf('run', 1100, 360, true).max, 1100 - 360 - STAGE_MIN);
  equal(limitsOf('run', 1000, 360, true).max, RUN.min, 'never under its own minimum');
  equal(limitsOf('run', 1000, 360, false).max, 1000, 'over the stage it may take the whole window, no more');
  equal(fit(5000, SIDE), 420);
  equal(fit(10, SIDE), 200);
  equal(fit(250.6, SIDE), 251);
  deepEqual(savedWidths(null), { side: 288, panes: {} });
  deepEqual(savedWidths({ side: 'wide', panes: { list: NaN, browser: 'x' } }), { side: 288, panes: {} });
  deepEqual(savedWidths({ side: 9999, panes: { list: 1, browser: 5000, nonsense: 400 } }), { side: 420, panes: { list: 300, browser: 4000 } });
  deepEqual(savedWidths({ side: 300, panes: { aside: 400 } }), { side: 300, panes: { aside: 400 } });
  // The one width of earlier versions becomes the width of every kind, once; a stored object of kinds wins over it.
  const old = Object.fromEntries(KINDS.map((k) => [k, 400]));
  deepEqual(savedWidths({ side: 300, run: 400 }), { side: 300, panes: old });
  deepEqual(savedWidths({ side: 300, run: 400, panes: { browser: 700 } }), { side: 300, panes: { browser: 700 } });
});

check('layout: every kind of view has its own usual width, and the column docks by room', () => {
  equal(paneUsual('browser', 1920, 288), Math.round(0.6 * (1920 - 288)), 'a page opens at 60% of the work area');
  for (const kind of ['changes', 'visual', 'file'] as const) equal(paneUsual(kind, 1600, 288), paneUsual('browser', 1600, 288), kind + ' opens as wide as a page');
  equal(paneUsual('aside', 1920, 288), ASIDE_USUAL);
  equal(ASIDE_USUAL, 420);
  equal(paneUsual('list', 1920, 288), 560, 'the running list keeps the room beside the reading column');
  equal(paneUsual('list', 1920, 288), autoRun(1920, 288));
  ok(paneUsual('browser', 1920, 288) > paneUsual('list', 1920, 288) + 300, 'the browser and the running list no longer share a width');
  ok(!dockable(1000, 288) && dockable(1100, 288), 'the column docks whenever the stage keeps 440 px beside it');
  ok(dockable(1040, 288) && !dockable(1020, 288), 'not at the old fixed 1,100 px');
  ok(!dockable(899, 200), 'under 900 px the sidebar is an overlay and the column covers the stage');
  ok(dockable(1050, 420), 'a wide stored sidebar gives way instead of keeping the column from docking');
});

check('layout: the running column takes the room beside the reading column, 360 to 560 px, and never the thread’s own width', () => {
  equal(autoRun(1920, 248), 560, 'a 1,920 px window gives it the most');
  equal(autoRun(1600, 248), 552);
  equal(autoRun(1440, 248), 392);
  equal(autoRun(1280, 248), 360);
  equal(autoRun(1100, 248), 360, 'a window that is nearly full keeps the least');
  for (const win of [1100, 1280, 1366, 1440, 1600, 1920, 2560]) {
    const run = Math.min(autoRun(win, 248), limitsOf('run', win, 248, true).max);
    ok(win - 248 - run >= 440, `${win} px keeps the stage`);
    if (win >= 1440) ok(win - 248 - run >= 760, `${win} px keeps the thread its 760 px`);
  }
});

const talk: Conversation = {
  id: 'shell-chat', title: 'Shell chat', area: 'chat', sessions: {}, createdAt: 0, updatedAt: 1,
  messages: [{ id: 'q', role: 'user', text: 'Hello', at: 0 }, { id: 'a', role: 'assistant', text: 'Hi.', at: 1, status: 'done', vendor: 'claude' }],
};

/** The whole shell, mounted in a box that fills the window. `motion` is the level the page runs at; `stored` is the layout
 * that an earlier run of the window left in storage. */
async function shell(vendors: AppState['vendors'], motion: 'full' | 'none', run: (root: HTMLElement) => Promise<void> | void, stored?: unknown, convo: Conversation = talk) {
  const originalOpen = api.open;
  const originalClock = api.setClock;
  const place = localStorage.getItem('faber.place');
  const layout = localStorage.getItem('faber.layout');
  const effects = { ambient: false, texture: false, glow: false };
  const initial: AppState = {
    vendors, models: [], quotas: [], projects: [], version: '0.0.0', running: { revision: 0, items: [] },
    conversations: [{ id: convo.id, title: convo.title, area: convo.area, updatedAt: 1 }],
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' } },
    look: {
      vars: themes.css.base, scheme: 'dark', motion, effects, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#f3f0e8', textFont: 'archivo', headingFont: 'narrow', textSize: 14, density: 1, corners: 1, motion, effects },
    },
  };
  api.open = async () => convo;
  api.setClock = async () => {};
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat', id: convo.id }));
  if (stored === undefined) localStorage.removeItem('faber.layout');
  else localStorage.setItem('faber.layout', JSON.stringify(stored));
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;inset:0';
  root.dataset.motion = motion;
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), root);
  try {
    await settle(60);
    await run(root);
  } finally {
    dispose();
    root.remove();
    api.open = originalOpen;
    api.setClock = originalClock;
    for (const [key, value] of [['faber.place', place], ['faber.layout', layout]] as const) {
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    }
  }
}

const pointer = (type: string, x: number) => new PointerEvent(type, { clientX: x, pointerId: 7, isPrimary: true, button: 0, bubbles: true, cancelable: true });
const key = (name: string, extra: KeyboardEventInit = {}) => window.dispatchEvent(new KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true, ...extra }));
const firstTrack = (shellEl: Element) => parseFloat(getComputedStyle(shellEl).gridTemplateColumns.split(' ')[0]!);

check('shell: the sidebar edge is a separator that a drag, the arrow keys and a double click move, and its width is kept', async () => {
  useLanguage('en');
  await shell(['claude'], 'none', async (root) => {
    const frame = root.querySelector<HTMLElement>('.shell')!;
    const grip = root.querySelector<HTMLElement>('.grip.side');
    ok(grip, 'the sidebar has an edge to drag');
    equal(grip.getAttribute('role'), 'separator');
    equal(grip.getAttribute('aria-orientation'), 'vertical');
    equal(grip.getAttribute('aria-label'), t('side.resize'));
    equal(grip.tabIndex, 0, 'the keyboard reaches it');
    equal(grip.getAttribute('aria-valuenow'), '288', 'the usual width');
    equal(grip.getAttribute('aria-valuemin'), String(SIDE.min));
    equal(grip.getAttribute('aria-valuemax'), String(SIDE.max));
    equal(firstTrack(frame), 288);

    grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
    equal(firstTrack(frame), 304, 'an arrow key moves it by 16 px');
    grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', shiftKey: true, bubbles: true, cancelable: true }));
    equal(firstTrack(frame), 256, 'with Shift, by 48 px');
    grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true }));
    equal(firstTrack(frame), SIDE.max);
    equal(grip.getAttribute('aria-valuenow'), String(SIDE.max));

    // A drag: the pointer goes 400 px past the limit and the edge stops at it; the width is stored when it is let go.
    grip.dispatchEvent(pointer('pointerdown', SIDE.max));
    ok(document.documentElement.classList.contains('sizing'), 'a drag in progress says so to the page');
    window.dispatchEvent(pointer('pointermove', SIDE.max - 100));
    window.dispatchEvent(pointer('pointerup', SIDE.max - 100));
    ok(!document.documentElement.classList.contains('sizing'));
    equal(firstTrack(frame), SIDE.max - 100, 'the sidebar follows the pointer');
    equal(JSON.parse(localStorage.getItem('faber.layout')!).side, SIDE.max - 100, 'and the width is kept');
    grip.dispatchEvent(pointer('pointerdown', 260));
    window.dispatchEvent(pointer('pointermove', 900));
    window.dispatchEvent(pointer('pointerup', 900));
    equal(firstTrack(frame), SIDE.max, 'the sidebar stops at its limit');

    grip.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    // It glides there (under 300 ms) unless motion is off.
    await settle(450);
    equal(firstTrack(frame), SIDE.fallback, 'a double click puts the usual width back');
    equal(JSON.parse(localStorage.getItem('faber.layout')!).side, SIDE.fallback);
  });
});

check('shell: the running column has an edge of its own, and a stored width is the one that comes back', async () => {
  useLanguage('en');
  // The room the stored 420 px needs beside the stage does not depend on the runner's own window.
  await atWidth(1600, () => shell(['claude'], 'none', async (root) => {
    equal(root.querySelector<HTMLElement>('.grip.side')!.getAttribute('aria-valuenow'), '300', 'the sidebar comes back as it was left');
    root.querySelector<HTMLButtonElement>('.run-btn')!.click();
    await settle(60);
    const column = root.querySelector<HTMLElement>('.runcol');
    ok(column, 'the column is open');
    const grip = column.querySelector<HTMLElement>('.grip.run');
    ok(grip, 'with an edge to drag');
    equal(grip.getAttribute('role'), 'separator');
    equal(grip.getAttribute('aria-label'), t('run.resize'));
    const before = Number(grip.getAttribute('aria-valuenow'));
    equal(before, 420, 'the column comes back as it was left');
    // The column's edge is on its left, so the left arrow makes it wider.
    grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowLeft', bubbles: true, cancelable: true }));
    equal(Number(grip.getAttribute('aria-valuenow')), before + 16);
    equal(JSON.parse(localStorage.getItem('faber.layout')!).panes.list, before + 16);
    ok(column.getBoundingClientRect().width > 0 && column.getBoundingClientRect().width <= before + 16, 'the column is drawn at that width, less the frame');
  }, { side: 300, run: 420 }));
});

/** The window as `px` wide for what `run` does, put back after. The shell reads the width when it mounts and at every resize. */
async function atWidth(px: number, run: () => Promise<void>) {
  const original = Object.getOwnPropertyDescriptor(window, 'innerWidth')!;
  Object.defineProperty(window, 'innerWidth', { configurable: true, value: px });
  window.dispatchEvent(new Event('resize'));
  try {
    await run();
  } finally {
    Object.defineProperty(window, 'innerWidth', original);
    window.dispatchEvent(new Event('resize'));
  }
}

check('shell: until it is dragged the running column follows the window, and a double click gives that back', async () => {
  useLanguage('en');
  await atWidth(1920, () =>
    shell(['claude'], 'none', async (root) => {
      const frame = root.querySelector<HTMLElement>('.shell')!;
      root.querySelector<HTMLButtonElement>('.run-btn')!.click();
      await settle(60);
      const grip = root.querySelector<HTMLElement>('.runcol .grip.run')!;
      equal(grip.getAttribute('aria-valuenow'), '560', 'a 1,920 px window gives the column 560 px, not the old 352');
      equal(frame.style.getPropertyValue('--run-w'), '560px');
      equal(localStorage.getItem('faber.layout'), null, 'nothing is stored for a width nobody chose');
      await atWidth(1280, async () => {
        await settle(60);
        equal(grip.getAttribute('aria-valuenow'), '360', 'a smaller window gives it less');
        equal(frame.style.getPropertyValue('--run-w'), '360px');
      });
      await settle(60);
      equal(grip.getAttribute('aria-valuenow'), '560', 'and a bigger one gives it more again');

      grip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
      equal(JSON.parse(localStorage.getItem('faber.layout')!).panes.list, 544, 'a drag is the person’s width and is kept');
      await atWidth(1280, async () => {
        await settle(60);
        equal(grip.getAttribute('aria-valuenow'), '544', 'a chosen width stays whatever the window does');
      });
      grip.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await settle(450);
      equal(grip.getAttribute('aria-valuenow'), '560');
      equal(JSON.parse(localStorage.getItem('faber.layout')!).panes.list, undefined, 'a double click goes back to following the window');
    }),
  );
});

check('shell: a stored width that happens to equal the usual one still goes back to following the window on a double click', async () => {
  useLanguage('en');
  await atWidth(1920, () =>
    shell(['claude'], 'none', async (root) => {
      root.querySelector<HTMLButtonElement>('.run-btn')!.click();
      await settle(60);
      const grip = root.querySelector<HTMLElement>('.runcol .grip.run')!;
      grip.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      await settle(450);
      equal(JSON.parse(localStorage.getItem('faber.layout')!).panes.list, undefined, 'the stored 560 px is let go');
    }, { side: 248, run: 560 }),
  );
});

const agentCall = { id: 'a1', name: 'Agent', input: { description: 'Find file defining load_config', subagent_type: 'Explore', prompt: 'Search the folder.' }, ok: true, output: 'It is config.rs.' };
const withAgent: Conversation = {
  ...talk,
  id: 'shell-agent',
  messages: [talk.messages[0]!, { id: 'a', role: 'assistant', text: 'Done.', at: 1, status: 'done', vendor: 'claude', tools: [agentCall, { id: 'g1', name: 'Grep', input: { pattern: 'load_config' }, parent: 'a1', ok: true }] }],
};

check('shell: an opened subagent can fill the stage as a full thread, with the composer below, and the same button returns it to the column', async () => {
  useLanguage('en');
  await atWidth(1920, () =>
    shell(['claude'], 'none', async (root) => {
      const frame = root.querySelector<HTMLElement>('.shell')!;
      showAgent({ conversationId: withAgent.id, messageId: 'a', callId: 'a1' });
      await settle(80);
      ok(root.querySelector('.shell > .runcol') && frame.classList.contains('docked'), 'the subagent opens beside the stage');
      const button = () => root.querySelector<HTMLButtonElement>('.runcol .agent-wide')!;
      equal(button().getAttribute('aria-label'), t('agent.widen'));
      button().click();
      await settle(60);
      const over = root.querySelector<HTMLElement>('.work > .runcol');
      ok(over && over.classList.contains('wide'), 'now it sits over the work area');
      equal(root.querySelector('.shell > .runcol'), null);
      ok(!frame.classList.contains('docked'), 'and the stage has the whole width');
      equal(over.getBoundingClientRect().width, root.querySelector<HTMLElement>('.work')!.getBoundingClientRect().width, 'as wide as the stage');
      ok(root.querySelector<HTMLElement>('.thread')!.inert, 'the conversation under it cannot be reached');
      ok(root.querySelector('.dock .composer') && !root.querySelector<HTMLElement>('.dock')!.inert, 'the box stays for Tell it');
      equal(root.querySelector('.runcol .grip'), null, 'a full thread has no edge to drag');
      equal(button().getAttribute('aria-label'), t('agent.narrow'));
      equal(document.activeElement, button(), 'focus stays on the button that was pressed');
      button().click();
      await settle(60);
      ok(root.querySelector('.shell > .runcol') && frame.classList.contains('docked'), 'back in the column');
      equal(root.querySelector('.work > .runcol'), null);
      ok(!root.querySelector<HTMLElement>('.thread')!.inert);
      equal(document.activeElement, button());

      // Closing from the full thread leaves nothing wide behind: the next subagent opens in the column.
      button().click();
      await settle(60);
      root.querySelector<HTMLButtonElement>('.runcol .rc-head button:last-child')!.click();
      await settle(60);
      equal(root.querySelector('.runcol'), null);
      ok(!agentWide(), 'closing the column ends the full thread');
      showAgent({ conversationId: withAgent.id, messageId: 'a', callId: 'a1' });
      await settle(80);
      ok(root.querySelector('.shell > .runcol'), 'a subagent opened again is in the column');
      showAgent(undefined);
    }, undefined, withAgent),
  );
});

check('shell: over the stage, an opened subagent as a full thread is no dialog: the box and the topbar stay in reach', async () => {
  useLanguage('en');
  await atWidth(1000, () =>
    shell(['claude'], 'none', async (root) => {
      showAgent({ conversationId: withAgent.id, messageId: 'a', callId: 'a1' });
      await settle(80);
      equal(root.querySelector('.work > .runcol')!.getAttribute('aria-modal'), 'true', 'beside the thread it is a dialog over the stage');
      root.querySelector<HTMLButtonElement>('.runcol .agent-wide')!.click();
      await settle(60);
      const column = root.querySelector('.work > .runcol.wide')!;
      equal(column.getAttribute('aria-modal'), null, 'as a full thread it is not');
      ok(!root.querySelector<HTMLElement>('.dock')!.inert && !root.querySelector<HTMLElement>('.topbar')!.inert, 'so the box and the topbar can be reached');
      showAgent(undefined);
    }, undefined, withAgent),
  );
});

check('shell: Ctrl+B steps the sidebar and the column aside and brings them back, and a button says how', async () => {
  useLanguage('en');
  await shell(['claude'], 'none', async (root) => {
    const frame = root.querySelector<HTMLElement>('.shell')!;
    const side = root.querySelector<HTMLElement>('.side')!;
    root.querySelector<HTMLButtonElement>('.run-btn')!.click();
    await settle(60);
    ok(root.querySelector('.runcol'), 'the column is open');
    const exit = root.querySelector<HTMLButtonElement>('.focus-exit')!;
    ok(exit.inert, 'the way out of focus mode is shut while the sidebar is there');

    key('b', { ctrlKey: true });
    await settle(60);
    ok(frame.classList.contains('focus'), 'focus mode is on');
    equal(root.querySelector('.runcol'), null, 'the column steps aside');
    equal(firstTrack(frame), 0, 'the sidebar has no room');
    ok(side.inert && side.getAttribute('aria-hidden') === 'true', 'and cannot be reached');
    equal(getComputedStyle(side).opacity, '0');
    ok(!exit.inert, 'the button that brings it back is on');
    equal(exit.getAttribute('aria-label'), t('focus.exit'));
    ok(frame.querySelector<HTMLElement>('.main')!.getBoundingClientRect().width >= innerWidth - 40, 'the stage takes the window');

    key('b', { ctrlKey: true });
    await settle(60);
    ok(!frame.classList.contains('focus'), 'focus mode is off');
    ok(root.querySelector('.runcol'), 'the column that was open is open again');
    equal(firstTrack(frame), 288);
    // Beside the stage the column leaves the sidebar alone; over it (a window under 1,100 px) the column is modal and keeps it shut.
    equal(side.inert, !dockable(innerWidth, 288), 'the sidebar can be reached again unless the column is a dialog over the stage');

    // The button and the run button leave focus mode too.
    key('b', { ctrlKey: true });
    await settle(60);
    exit.click();
    await settle(60);
    ok(!frame.classList.contains('focus'), 'the button brings the sidebar back');
    key('b', { ctrlKey: true });
    await settle(60);
    root.querySelector<HTMLButtonElement>('.run-btn')!.click();
    await settle(60);
    ok(!frame.classList.contains('focus'), 'the run button shows the column, so focus mode ends');

    // Ctrl+Shift+B is somebody else's.
    key('b', { ctrlKey: true, shiftKey: true });
    await settle(30);
    ok(!frame.classList.contains('focus'));
  });
});

check('shell: focus mode moves with the motion level: the sidebar fades and slides out, and does nothing at all when motion is off', async () => {
  useLanguage('en');
  for (const level of ['full', 'none'] as const) {
    await shell(['claude'], level, async (root) => {
      const side = root.querySelector<HTMLElement>('.side')!;
      key('b', { ctrlKey: true });
      const style = getComputedStyle(side);
      const names = style.transitionProperty.split(', ');
      if (level === 'full' && !matchMedia('(prefers-reduced-motion: reduce)').matches) {
        ok(names.includes('opacity') && names.includes('transform'), `the sidebar moves by opacity and transform, not layout: ${names}`);
        ok(parseFloat(style.transitionDuration) > 0 && parseFloat(style.transitionDuration) <= 0.3, `in under 300 ms: ${style.transitionDuration}`);
        ok(!names.includes('all') && !names.includes('width'));
      } else equal(parseFloat(style.transitionDuration), 0, 'no transition');
    });
  }
});

const dragged = (type: string, ...names: string[]) => {
  const hand = new DataTransfer();
  for (const name of names) hand.items.add(new File(['hello'], name, { type: 'text/plain' }));
  return new DragEvent(type, { dataTransfer: hand, bubbles: true, cancelable: true });
};

check('shell: a file over the window says "Drop to attach" anywhere, and dropping it attaches it through the composer', async () => {
  useLanguage('en');
  await shell(['claude'], 'none', async (root) => {
    equal(document.querySelector('.dropveil'), null, 'nothing to say while no file is over the window');
    // A drag of plain text is not a file.
    const text = new DataTransfer();
    text.setData('text/plain', 'words');
    root.querySelector('.topbar')!.dispatchEvent(new DragEvent('dragenter', { dataTransfer: text, bubbles: true, cancelable: true }));
    equal(document.querySelector('.dropveil'), null);

    // Over the top bar, far from the composer.
    root.querySelector('.topbar')!.dispatchEvent(dragged('dragenter', 'notes.txt'));
    const veil = document.querySelector<HTMLElement>('.dropveil');
    ok(veil, 'the window shows it');
    equal(veil.textContent!.trim(), t('drop.attach'));
    equal(getComputedStyle(veil).pointerEvents, 'none', 'and takes no click');
    ok(veil.getBoundingClientRect().width >= innerWidth - 1 && veil.getBoundingClientRect().height >= innerHeight - 1, 'over the whole window');
    const over = dragged('dragover', 'notes.txt');
    root.querySelector('.topbar')!.dispatchEvent(over);
    ok(over.defaultPrevented, 'the window accepts the drop');

    root.querySelector('.topbar')!.dispatchEvent(dragged('drop', 'notes.txt'));
    await settle(320);
    equal(document.querySelector('.dropveil'), null, 'the notice goes once the file is let go');
    ok([...root.querySelectorAll('.composer .attach-row [title]')].some((el) => el.getAttribute('title')!.includes('notes.txt')), 'the composer holds the file');

    // The pointer leaving the window ends the notice with nothing attached.
    root.querySelector('.topbar')!.dispatchEvent(dragged('dragenter', 'other.txt'));
    ok(document.querySelector('.dropveil'));
    root.querySelector('.topbar')!.dispatchEvent(new DragEvent('dragleave', { dataTransfer: dragged('x', 'other.txt').dataTransfer, bubbles: true, relatedTarget: null }));
    await settle(320);
    equal(document.querySelector('.dropveil'), null);
    ok(![...root.querySelectorAll('.composer .chip')].some((chip) => chip.textContent!.includes('other.txt')));
  });
});

check('shell: with no composer on screen, a file over the window shows no notice', async () => {
  useLanguage('en');
  await shell([], 'none', async (root) => {
    equal(root.querySelector('.composer'), null);
    root.querySelector('.topbar')!.dispatchEvent(dragged('dragenter', 'notes.txt'));
    equal(document.querySelector('.dropveil'), null);
  });
});

check('screen: it reads as an app, with no system scrollbar and no stray selection of its chrome', () => {
  // In WebView2 a scrollbar-width or scrollbar-color anywhere brings back the system bar with its arrows and turns off the
  // screen's own thin one (ui/styles.css), so no rule of the screen may set either.
  const set: string[] = [];
  const walk = (rules: CSSRuleList) => {
    for (const rule of rules) {
      if (rule instanceof CSSStyleRule && (rule.style.getPropertyValue('scrollbar-width') || rule.style.getPropertyValue('scrollbar-color'))) set.push(rule.selectorText);
      if ('cssRules' in rule) walk((rule as CSSGroupingRule).cssRules);
    }
  };
  for (const sheet of document.styleSheets) {
    try {
      walk(sheet.cssRules);
    } catch {
      // a sheet of another origin cannot be read, and the screen has none
    }
  }
  deepEqual(set, [], 'rules that set scrollbar-width or scrollbar-color');
  // WebView2 names the property with and without its old prefix, depending on its version.
  const selects = (style: CSSStyleDeclaration) => style.getPropertyValue('user-select') || style.getPropertyValue('-webkit-user-select');
  const html = getComputedStyle(document.documentElement);
  equal(selects(html), 'none', 'the chrome is not selected by a stray drag');
  equal(html.cursor, 'default', 'and keeps the arrow');
  const answer = document.createElement('div');
  answer.className = 'md';
  answer.textContent = 'An answer';
  document.body.append(answer);
  const button = document.createElement('button');
  document.body.append(button);
  try {
    equal(selects(getComputedStyle(answer)), 'text', 'an answer is selectable');
    equal(getComputedStyle(button).cursor, 'default', 'a button shows the arrow, never the hand');
  } finally {
    answer.remove();
    button.remove();
  }
});

/** Faber's browser, with the native box stubbed: the page is blank, which is what the column shows for a person's own address. */
async function withBrowser(run: () => Promise<void>) {
  const [siteSize, sitePlace, sitePark, onSite] = [api.siteSize, api.sitePlace, api.sitePark, api.onSite];
  api.siteSize = async () => ({ now: null, sizes: [] });
  api.sitePlace = async () => true;
  api.sitePark = async () => {};
  api.onSite = async () => () => {};
  try {
    await run();
  } finally {
    showSite(undefined);
    Object.assign(api, { siteSize, sitePlace, sitePark, onSite });
  }
}
const openBrowser = () => showSite({ conversationId: talk.id, url: '', title: '' });

check('shell: the browser opens at 60% of the work area and the running list at its own width, and a drag on one leaves the other alone', async () => {
  useLanguage('en');
  await withBrowser(() =>
    atWidth(1920, () =>
      shell(['claude'], 'none', async (root) => {
        const frame = root.querySelector<HTMLElement>('.shell')!;
        const grip = () => root.querySelector<HTMLElement>('.runcol .grip.run')!;
        openBrowser();
        await settle(80);
        equal(grip().getAttribute('aria-valuenow'), String(Math.round(0.6 * (1920 - 288))), 'the browser opens at about 60%');
        equal(frame.style.getPropertyValue('--run-w'), '979px');
        ok(!frame.classList.contains('folded'), 'a 1,920 px window keeps the sidebar beside it');
        showSite(undefined);
        await settle(80);
        equal(grip().getAttribute('aria-valuenow'), '560', 'the running list keeps its own width');
        equal(frame.style.getPropertyValue('--run-w'), '560px');

        // A drag made on the list is the list's only.
        grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
        let stored = JSON.parse(localStorage.getItem('faber.layout')!);
        equal(stored.panes.list, 544);
        equal(stored.panes.browser, undefined);
        openBrowser();
        await settle(80);
        equal(grip().getAttribute('aria-valuenow'), '979', 'the browser is untouched');
        // ... and a drag on the browser is the browser's only.
        grip().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
        stored = JSON.parse(localStorage.getItem('faber.layout')!);
        equal(stored.panes.browser, 963);
        equal(stored.panes.list, 544);
        showSite(undefined);
        await settle(80);
        equal(grip().getAttribute('aria-valuenow'), '544', 'the list is where it was left');
        // A double click on the browser's grip gives its usual width back, and only its own.
        openBrowser();
        await settle(80);
        grip().dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        await settle(450);
        equal(grip().getAttribute('aria-valuenow'), '979');
        stored = JSON.parse(localStorage.getItem('faber.layout')!);
        equal(stored.panes.browser, undefined);
        equal(stored.panes.list, 544);
      }),
    ),
  );
});

check('shell: a width kept for the browser survives a restart, and the single width of earlier versions is every view’s', async () => {
  useLanguage('en');
  await withBrowser(() =>
    atWidth(1920, async () => {
      await shell(['claude'], 'none', async (root) => {
        openBrowser();
        await settle(80);
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), '800');
        showSite(undefined);
        await settle(80);
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), '560', 'the list keeps following the window');
      }, { side: 288, panes: { browser: 800 } });
      await shell(['claude'], 'none', async (root) => {
        openBrowser();
        await settle(80);
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), '450', 'the old width, once, for the browser');
        showSite(undefined);
        await settle(80);
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), '450', 'and for the list');
      }, { side: 288, run: 450 });
    }),
  );
});

check('shell: a wide view that would squeeze the stage folds the sidebar away until it closes, and nothing is stored', async () => {
  useLanguage('en');
  await withBrowser(() =>
    atWidth(1440, () =>
      shell(['claude'], 'none', async (root) => {
        const frame = root.querySelector<HTMLElement>('.shell')!;
        const side = root.querySelector<HTMLElement>('.side')!;
        equal(firstTrack(frame), 288);
        openBrowser();
        await settle(100);
        ok(frame.classList.contains('folded'), 'the sidebar folds away');
        ok(side.inert && side.getAttribute('aria-hidden') === 'true', 'and cannot be reached while it is');
        equal(firstTrack(frame), 0, 'and takes no room');
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), String(Math.round(0.6 * 1440)), '60% of the whole window now');
        equal(localStorage.getItem('faber.layout'), null, 'the fold is not stored');
        showSite(undefined);
        await settle(100);
        ok(!frame.classList.contains('folded'), 'the sidebar comes back with the list');
        ok(!side.inert);
        equal(firstTrack(frame), 288);
        equal(root.querySelector<HTMLElement>('.runcol .grip.run')!.getAttribute('aria-valuenow'), '360', 'the list at its own width');
      }),
    ),
  );
});

check('shell: the column docks by room, not at a fixed 1,100 px', async () => {
  useLanguage('en');
  for (const [win, docked] of [[1060, true], [1000, false]] as const) {
    await atWidth(win, () =>
      shell(['claude'], 'none', async (root) => {
        root.querySelector<HTMLButtonElement>('.run-btn')!.click();
        await settle(80);
        equal(root.querySelector('.shell')!.classList.contains('docked'), docked, win + ' px');
      }),
    );
  }
});

check('shell: while the sidebar edge is dragged the stage follows the pointer, before the width is stored', async () => {
  useLanguage('en');
  await shell(['claude'], 'none', async (root) => {
    const frame = root.querySelector<HTMLElement>('.shell')!;
    const grip = root.querySelector<HTMLElement>('.grip.side')!;
    grip.dispatchEvent(pointer('pointerdown', 288));
    window.dispatchEvent(pointer('pointermove', 340));
    await settle(60);
    equal(firstTrack(frame), 340, 'the grid track moves with the drag, not after it');
    window.dispatchEvent(pointer('pointerup', 340));
    equal(firstTrack(frame), 340);
  });
});
