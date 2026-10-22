import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type SiteTabs, type SiteTabStep } from '../../ui/api.ts';
import { closeMenu, menu } from '../../ui/contextmenu.ts';
import { siteKeyOf } from '../../ui/keys.ts';
import { Running } from '../../ui/Running.tsx';
import { goesTo, openedSite, placeOf, showSite } from '../../ui/Site.tsx';
import { check, deepEqual, equal, frames, ok } from './check.ts';

// Tabs and the bar of Faber's browser (task 030; docs/product-map.md, "Page in Faber's browser").

/** Leaves the tabs, history and events of the browser quiet for a check that is about something else; answers the undo. */
export function quietTabs(): () => void {
  const saved = { siteTabs: api.siteTabs, siteHistory: api.siteHistory, onSiteTabs: api.onSiteTabs, onSiteBusy: api.onSiteBusy, onSiteKey: api.onSiteKey, onSiteLoading: api.onSiteLoading };
  api.siteTabs = async () => null;
  api.siteHistory = async () => [false, false];
  api.onSiteTabs = async () => () => {};
  api.onSiteBusy = async () => () => {};
  api.onSiteKey = async () => () => {};
  api.onSiteLoading = async () => () => {};
  return () => Object.assign(api, saved);
}

check('page in the browser: words in the address field search, an address opens, and what is no web page goes nowhere', () => {
  const search = (words: string) => `https://www.google.com/search?q=${encodeURIComponent(words)}`;
  equal(goesTo('example.com'), 'https://example.com/');
  equal(goesTo('localhost:5173'), 'http://localhost:5173/');
  equal(goesTo('localhost'), 'http://localhost/');
  equal(goesTo('how to center a div'), search('how to center a div'), 'words with spaces');
  equal(goesTo('solidjs'), search('solidjs'), 'one word with no dot is a search, not https://solidjs/');
  equal(goesTo('c++ & rust?'), search('c++ & rust?'), 'what a search holds is encoded');
  for (const nowhere of ['', '  ', 'javascript:alert(1)', 'file:///C:/secret', 'about:blank']) equal(goesTo(nowhere), undefined, nowhere);
  equal(placeOf('https://www.example.org/a'), 'example.org');
  equal(placeOf('http://localhost:3000/'), ':3000', 'an app on this computer is told by its port');
  equal(placeOf('http://127.0.0.1:8080/x'), ':8080');
});

check('page in the browser: the keys of the head and bar, and none that types a character or belongs to the sidebar', () => {
  const press = (key: string, more: KeyboardEventInit = {}) => siteKeyOf(new KeyboardEvent('keydown', { key, ...more }));
  equal(press('t', { ctrlKey: true }), 'new');
  equal(press('T', { ctrlKey: true, shiftKey: true }), 'reopen');
  equal(press('w', { ctrlKey: true }), 'close');
  equal(press('Tab', { ctrlKey: true }), 'next');
  equal(press('Tab', { ctrlKey: true, shiftKey: true }), 'previous');
  equal(press('l', { ctrlKey: true }), 'address');
  equal(press('r', { ctrlKey: true }), 'reload');
  equal(press('ArrowLeft', { altKey: true }), 'back');
  equal(press('ArrowRight', { altKey: true }), 'forward');
  equal(press('1', { ctrlKey: true }), undefined, 'Ctrl and a digit are the sidebar’s outside the page');
  equal(press('t', { ctrlKey: true, altKey: true }), undefined, 'AltGr types a character');
  equal(press('ArrowLeft'), undefined, 'an arrow alone moves the caret');
});

/** A browser pane with tabs, its calls recorded. */
function pane(kept: SiteTabs, answers: Partial<Record<SiteTabStep, SiteTabs>> = {}) {
  const undo = quietTabs();
  const saved = { sitePlace: api.sitePlace, sitePark: api.sitePark, onSite: api.onSite, siteTab: api.siteTab, siteGo: api.siteGo, siteStep: api.siteStep };
  const calls: string[] = [];
  const listeners: { busy?: (b: { conversation: string; tab: string; busy: boolean }) => void; key?: (key: string) => void } = {};
  let closed = 0;
  api.siteTabs = async () => kept;
  api.onSiteBusy = async (listener) => ((listeners.busy = listener), () => {});
  api.onSiteKey = async (listener) => ((listeners.key = listener), () => {});
  api.sitePlace = async () => true;
  api.sitePark = async () => {};
  api.onSite = async () => () => {};
  api.siteStep = async (id, step) => (calls.push(`step ${id} ${step}`), true);
  api.siteGo = async (id, url) => (calls.push(`go ${id} ${url}`), true);
  api.siteTab = async (id, step, tab) => {
    calls.push(`${step} ${tab ?? ''}`.trim());
    const tabs = answers[step] ?? { ...kept, front: tab ?? kept.front };
    return { tabs, tab: tab ?? tabs.front ?? null };
  };
  const front = kept.tabs.find((t) => t.id === kept.front)!;
  showSite({ conversationId: 'tabs', url: front.url, title: front.title ?? '' });
  const root = document.createElement('div');
  root.style.cssText = 'position: fixed; left: 0; top: 0; width: 520px; height: 600px; display: flex;';
  document.body.append(root);
  const dispose = render(
    () =>
      createComponent(Running, {
        items: [],
        projects: [],
        modal: false,
        onOpen: () => {},
        onClose: () => closed++,
        ref: () => {},
        get site() {
          return openedSite();
        },
      }),
    root,
  );
  return {
    root,
    calls,
    listeners,
    closed: () => closed,
    names: () => [...root.querySelectorAll('.site-tab-name')].map((n) => n.textContent),
    tab: (n: number) => root.querySelectorAll<HTMLButtonElement>('.site-tab-go')[n]!,
    done: () => {
      closeMenu();
      showSite(undefined);
      dispose();
      root.remove();
      Object.assign(api, saved);
      undo();
    },
  };
}

const THREE: SiteTabs = {
  tabs: [
    { id: 't1', url: 'https://docs.example.org/guide', title: 'The guide' },
    { id: 't2', url: 'http://localhost:3000/' },
    { id: 't4', url: 'https://example.com/', title: 'Example' },
  ],
  front: 't2',
  agent: 't4',
};

check('page in the browser: tabs show their title or site, open, close by button, middle click and menu, and the last one closes the pane', async () => {
  const blank: SiteTabs = { tabs: [...THREE.tabs, { id: 't5', url: '' }], front: 't5', agent: 't4' };
  const only: SiteTabs = { tabs: [THREE.tabs[1]!], front: 't2' };
  const answers: Partial<Record<SiteTabStep, SiteTabs>> = { new: blank, closeOthers: only };
  const p = pane(THREE, answers);
  try {
    await frames(3);
    deepEqual(p.names(), ['The guide', ':3000', 'Example'], 'a title, else the site, else the port of an app on this computer');
    equal(p.tab(1).getAttribute('aria-current'), 'page', 'the tab in front');
    p.tab(0).click();
    await frames();
    equal(p.calls.at(-1), 'bring t1');
    equal(openedSite()?.url, 'https://docs.example.org/guide', 'its page shows');
    p.tab(2).dispatchEvent(new MouseEvent('auxclick', { button: 1, bubbles: true, cancelable: true }));
    await frames();
    equal(p.calls.at(-1), 'close t4', 'a middle click closes a tab');
    p.tab(0).dispatchEvent(new MouseEvent('contextmenu', { button: 2, bubbles: true, cancelable: true, clientX: 20, clientY: 20 }));
    deepEqual(menu()?.items.map((item) => item.label), ['Open in your own browser', 'Close tab', 'Close other tabs']);
    menu()!.items[2]!.run();
    await frames();
    equal(p.calls.at(-1), 'closeOthers t1');
    p.root.querySelector<HTMLButtonElement>('.site-new')!.click();
    await frames();
    equal(p.calls.at(-1), 'new');
    equal(openedSite()?.url, '', 'a new tab is blank: only the address field');
    equal(p.root.querySelector('.site-address')?.getAttribute('placeholder'), 'Search or type an address', 'with no example in it');
    answers.close = { tabs: [] };
    p.root.querySelectorAll<HTMLButtonElement>('.site-tab-close')[0]!.click();
    await frames();
    equal(p.closed(), 1, 'closing the last tab closes the pane');
  } finally {
    p.done();
  }
});

check('page in the browser: the agent’s tab shows it is busy, and the keys pick, open, close and step tabs', async () => {
  const p = pane(THREE);
  try {
    await frames(3);
    p.listeners.busy?.({ conversation: 'tabs', tab: 't4', busy: true });
    p.listeners.busy?.({ conversation: 'other', tab: 't1', busy: true });
    await frames();
    equal(p.root.querySelectorAll('.site-busy').length, 1, 'only this conversation’s agent, in its own tab');
    equal(p.tab(2).querySelector('.site-busy')?.getAttribute('aria-label'), 'The agent is working in this tab', 'said in words, not by the dot alone');
    p.listeners.busy?.({ conversation: 'tabs', tab: 't4', busy: false });
    await frames();
    equal(p.root.querySelectorAll('.site-busy').length, 0);
    const field = p.root.querySelector<HTMLInputElement>('.site-address')!;
    const press = (key: string, more: KeyboardEventInit = {}) => field.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...more }));
    press('Tab', { ctrlKey: true });
    await frames();
    equal(p.calls.at(-1), 'bring t4', 'Ctrl+Tab: the next tab');
    press('t', { ctrlKey: true });
    await frames();
    equal(p.calls.at(-1), 'new');
    press('ArrowLeft', { altKey: true });
    equal(p.calls.at(-1), 'step tabs back');
    // On the page itself the shell hears the key and names it.
    p.listeners.key?.('tab1');
    await frames();
    equal(p.calls.at(-1), 'bring t1', 'Ctrl+1 on the page: the first tab');
    p.listeners.key?.('tab9');
    await frames();
    equal(p.calls.at(-1), 'bring t4', 'Ctrl+9: the last tab');
    p.listeners.key?.('address');
    equal(document.activeElement, field, 'Ctrl+L: the address field');
  } finally {
    p.done();
  }
});

check('page in the browser: the address field shows the site bright, selects all on focus, and Esc puts it back, then leaves', async () => {
  const p = pane(THREE);
  try {
    await frames(3);
    p.tab(0).click();
    await frames(3);
    equal(p.root.querySelector('.site-host')?.textContent, 'docs.example.org', 'the site in normal text');
    equal(p.root.querySelector('.site-path')?.textContent, '/guide', 'the rest quiet');
    const field = p.root.querySelector<HTMLInputElement>('.site-address')!;
    field.focus();
    // A hidden test window has no focus of its own to give: the field hears it as it would.
    field.dispatchEvent(new FocusEvent('focus'));
    await frames();
    equal(p.root.querySelector('.site-shown'), null, 'focused, the whole address is there to edit');
    deepEqual([field.selectionStart, field.selectionEnd], [0, field.value.length], 'all of it selected');
    field.value = 'faber tabs';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await frames();
    equal(field.value, 'https://docs.example.org/guide', 'the first Esc puts the address back');
    equal(document.activeElement, field);
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    ok(document.activeElement !== field, 'the second leaves the field');
    field.focus();
    field.value = 'faber tabs';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    await frames();
    equal(p.calls.at(-1), 'go tabs https://www.google.com/search?q=faber%20tabs', 'words search, in the tab in front');
  } finally {
    p.done();
  }
});
