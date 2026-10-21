import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type AppEvent, type McpServerList } from '../../ui/api.ts';
import { Catalog } from '../../ui/Catalog.tsx';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 20));
const reported: McpServerList[] = [
  { vendor: 'claude', readAt: 1_700_000_000, rows: [
    { name: 'faber-chrome-devtools', source: 'catalog', status: 'connected', enabled: true },
    { name: 'project-reader', source: 'project', status: 'failed' },
    { name: 'personal-reader', source: 'user', status: 'needs-auth' },
  ] },
  { vendor: 'codex', readAt: 1_700_000_001, rows: [
    { name: 'faber-chrome-devtools', source: 'catalog', status: 'disabled', enabled: false },
    { name: 'project-reader', source: 'project', status: 'starting' },
    { name: 'personal-reader', source: 'user', status: 'future-status' },
  ] },
];

async function panel(read: (id: string) => Promise<McpServerList[]>) {
  const before = { ...api };
  const projects = [{ id: 'first', name: 'First project', path: 'C:/first' }, { id: 'second', name: 'Second project', path: 'C:/second' }];
  api.toolsList = async () => [{ id: 'chrome-devtools', job: 'measure', license: 'Apache-2.0', repo: 'ChromeDevTools/chrome-devtools-mcp', home: '', installed: true, enabled: true, nodeAvailable: true, server: true }];
  api.toolsInventory = async () => [];
  api.toolsSetEnabled = async () => {};
  api.toolsClearRemoved = async () => {};
  let listener: ((event: AppEvent) => void) | undefined;
  api.on = async (receive) => { listener = receive; return () => {}; };
  api.mcpServers = async (id) => structuredClone(await read(id));
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => createComponent(Catalog, { projects, vendors: ['claude', 'codex'] }), root);
  await tick();
  const button = (text: string) => {
    const found = [...root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent?.trim() === text);
    ok(found, `button ${text}`);
    return found;
  };
  return {
    root,
    publish: () => listener?.({ type: 'catalog' }),
    open: async () => { root.querySelector<HTMLButtonElement>('[data-catalog-card="chrome-devtools"]')!.click(); await tick(); },
    back: async () => { button(t('catalog.back')).click(); await tick(); button(t('catalog.all')).click(); await tick(); },
    installed: async () => {
      button(t('catalog.installed')).click(); await tick();
      const picker = root.querySelector<HTMLSelectElement>('.catalog-project select')!;
      picker.value = 'first'; picker.dispatchEvent(new Event('change', { bubbles: true })); await tick();
    },
    close: () => { dispose(); root.remove(); Object.assign(api, before); },
  };
}

check('mcp-servers: a catch-up read of an unchanged catalog after an action keeps the focused switch', async () => {
  const view = await panel(async () => reported);
  try {
    await view.open();
    const global = [...view.root.querySelectorAll<HTMLInputElement>('[role="switch"]')].find((el) => !el.closest('[data-mcp-vendor]'));
    ok(global, 'the catalog switch');
    global.click(); await tick(); global.click(); await tick();
    const toggle = view.root.querySelector<HTMLInputElement>('[data-mcp-vendor] [role="switch"]')!;
    toggle.focus();
    // The engine answers an unchanged catalog in its own shape: status null where the screen left it out.
    api.toolsList = async () => [{ id: 'chrome-devtools', job: 'measure', license: 'Apache-2.0', repo: 'ChromeDevTools/chrome-devtools-mcp', home: '', installed: true, enabled: true, status: null, nodeAvailable: true, server: true }];
    view.publish(); await tick();
    equal(view.root.querySelector('[data-mcp-vendor] [role="switch"]'), toggle, 'the switch stays mounted');
    equal(document.activeElement, toggle, 'the catch-up read keeps focus');
  } finally { view.close(); }
});

check('mcp-servers: saving changes only the field and keeps every switch and keyboard focus', async () => {
  const view = await panel(async () => reported);
  let finish: () => void = () => {};
  const saved: boolean[] = [];
  api.setMcpServer = async (_id, _name, enabled) => {
    saved.push(enabled);
    await new Promise<void>((resolve) => { finish = resolve; });
  };
  try {
    await view.open();
    equal(view.root.querySelector('[data-mcp-row="project-reader"]'), null, 'detail lists only this catalog server');
    equal(view.root.querySelector('[data-mcp-row="personal-reader"]'), null);
    const switches = [...view.root.querySelectorAll<HTMLInputElement>('[data-mcp-vendor] [role="switch"]')];
    const toggle = switches[0];
    toggle.focus();
    toggle.click();
    await tick();
    equal(document.activeElement, toggle, 'saving must not disable the focused switch');
    finish();
    await tick();
    equal(document.activeElement, toggle, 'a successful save must keep keyboard focus');
    for (const [index, current] of [...view.root.querySelectorAll('[data-mcp-vendor] [role="switch"]')].entries()) equal(current, switches[index], 'the switches stay mounted');
    equal(toggle.checked, false);
    equal(switches[1].checked, false, 'the same catalog choice updates for both agents');
    toggle.click();
    finish();
    await tick();
    deepEqual(saved, [false, true]);
    equal(toggle.checked, true);
    equal(document.activeElement, toggle);
  } finally { view.close(); }
});

// The native screen scenario sends trusted Tab/Space keys; dispatching KeyboardEvent cannot toggle a native checkbox.
declare global {
  interface Window {
    __faberMcpKeyboard: {
      mount: () => Promise<void>;
      finishSave: () => void;
      close: () => void;
    };
  }
}
window.__faberMcpKeyboard = {
  mount: async () => {
    const view = await panel(async () => reported);
    view.root.dataset.mcpKeyboard = '';
    let saves = 0;
    api.setMcpServer = async (_id, _name, enabled) => {
      view.root.dataset.saves = String(++saves);
      view.root.dataset.enabled = String(enabled);
      await new Promise<void>((resolve) => { window.__faberMcpKeyboard.finishSave = resolve; });
    };
    window.__faberMcpKeyboard.close = view.close;
    await view.open();
    view.root.querySelector<HTMLSelectElement>('#set-mcp-project-select')!.focus();
  },
  finishSave: () => {},
  close: () => {},
};

check('mcp-servers: Installed reads on demand, shows both agents sources and reported states, and saves only catalog switches', async () => {
  const reads: string[] = [];
  const view = await panel(async (id) => { reads.push(id); return reported; });
  const saved: unknown[] = [];
  api.setMcpServer = async (id, name, enabled) => { saved.push([id, name, enabled]); };
  try {
    equal(reads.length, 0, 'browsing cards reads no project tool list');
    await view.installed();
    deepEqual(reads, ['first']);
    const claude = view.root.querySelector('[data-mcp-vendor="claude"]')!;
    const codex = view.root.querySelector('[data-mcp-vendor="codex"]')!;
    ok(claude.textContent?.includes('Last reported by the agent:'), 'the reading has its time');
    for (const [group, states] of [[claude, ['Connected', 'Failed to connect', 'Sign-in needed']], [codex, ['Off', 'Starting', 'Status not reported']]] as const) {
      for (const source of ['From the catalog', 'From this project', 'Your own setup']) ok(group.textContent?.includes(source), source);
      for (const status of states) ok(group.textContent?.includes(status), status);
      equal(group.querySelectorAll('[role="switch"]').length, 1, 'only a catalog server has a switch');
      equal(group.querySelectorAll('[data-mcp-row="project-reader"] [role="switch"]').length, 0);
      equal(group.querySelectorAll('[data-mcp-row="personal-reader"] [role="switch"]').length, 0);
      ok(group.querySelector('[data-mcp-row="faber-chrome-devtools"]')?.textContent?.includes('Changes apply when the conversation next starts.'));
    }
    const toggle = claude.querySelector<HTMLInputElement>('[role="switch"]')!;
    equal(toggle.checked, true);
    toggle.click();
    await tick();
    deepEqual(saved, [['first', 'faber-chrome-devtools', false]]);
    const changed = view.root.querySelector('[data-mcp-vendor="claude"]')!;
    equal(changed.querySelector<HTMLInputElement>('[role="switch"]')?.checked, false);
    ok(changed.textContent?.includes('Connected'), 'changing a preference never invents a new reported state');
    api.setMcpServer = async () => { throw new Error('save failed'); };
    changed.querySelector<HTMLInputElement>('[role="switch"]')!.click();
    await tick();
    equal(changed.querySelector<HTMLInputElement>('[role="switch"]')?.checked, false, 'a failed save restores the saved choice');
    ok(view.root.querySelector('[role=alert]'), 'a failed save is shown in the catalog');
    await view.open();
    await view.back();
    equal(view.root.querySelector('[data-mcp-servers]'), null, 'closing removes the project list');
    const beforeReopen = reads.length;
    await tick(); equal(reads.length, beforeReopen, 'no background reading');
    await view.open();
    equal(reads.length, beforeReopen + 1, 'reopening reads a fresh snapshot');
  } finally { view.close(); }
});

check('mcp-servers: before an agent report the catalog shows its saved choice and says when the other tools will appear', async () => {
  const view = await panel(async () => [{ vendor: 'claude', rows: [{ name: 'faber-chrome-devtools', source: 'catalog', enabled: true }] }]);
  try {
    await view.open();
    const list = view.root.querySelector('[data-mcp-servers]')!;
    ok(list.textContent?.includes('Other tools appear when the agent next starts.'));
    ok(list.textContent?.includes('Status not reported'));
    ok(!list.textContent?.includes('Last reported by the agent:'));
    equal(list.querySelectorAll('[data-mcp-row]').length, 1);
    equal(list.querySelector<HTMLInputElement>('[role="switch"]')?.checked, true);
  } finally { view.close(); }
});

check('mcp-servers: changing projects ignores an older project list that arrives later', async () => {
  let finishFirst: (list: McpServerList[]) => void = () => {};
  const view = await panel((id) => id === 'first' ? new Promise((resolve) => { finishFirst = resolve; }) : Promise.resolve([
    { vendor: 'codex', rows: [{ name: 'second-project-server', source: 'project', status: 'notStarted' }] },
  ]));
  try {
    await view.installed();
    const picker = view.root.querySelector<HTMLSelectElement>('.catalog-project select')!;
    picker.value = 'second';
    picker.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    ok(view.root.querySelector('[data-mcp-row="second-project-server"]'));
    finishFirst(reported);
    await tick();
    ok(view.root.querySelector('[data-mcp-row="second-project-server"]'));
    equal(view.root.querySelector('[data-mcp-row="personal-reader"]'), null, 'the first project cannot replace the second project');
  } finally { view.close(); }
});

check('mcp-servers: global off disables project controls in detail and Installed without changing the project preference', async () => {
  const view = await panel(async () => reported);
  const saved: boolean[] = [];
  api.setMcpServer = async (_id, _name, enabled) => { saved.push(enabled); };
  try {
    await view.open();
    const global = view.root.querySelector<HTMLInputElement>('#catalog-enabled-switch')!;
    global.click(); await tick();
    const project = () => view.root.querySelector<HTMLInputElement>('#set-mcp-claude-0-switch')!;
    equal(project().disabled, true); equal(project().checked, false);
    project().click(); await tick(); deepEqual(saved, []);
    global.click(); await tick();
    equal(project().disabled, false); equal(project().checked, true, 'global availability preserves the saved project choice');
    global.click(); await tick();
    await view.back(); await view.installed();
    equal(project().disabled, true); equal(project().checked, false);
    ok(view.root.querySelector('[data-mcp-row="faber-chrome-devtools"]')?.textContent?.includes(t('catalog.off')));
    deepEqual(saved, [], 'availability never writes the per-project preference');
  } finally { view.close(); }
});

check('mcp-servers: a failed snapshot is visible and Retry refreshes the same detail or Installed list', async () => {
  for (const installed of [false, true]) {
    let failed = true, reads = 0;
    const view = await panel(async () => { reads++; if (failed) throw 'error.tools.install'; return reported; });
    try {
      if (installed) await view.installed(); else await view.open();
      ok(view.root.querySelector('[role=alert]'));
      equal(reads, 1);
      failed = false;
      const retry = [...view.root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent === t('catalog.retry'));
      ok(retry); retry.click(); await tick();
      equal(reads, 2, 'Retry rereads MCP rather than only the file inventory');
      ok(view.root.querySelector('[data-mcp-row="faber-chrome-devtools"]'));
      equal(view.root.querySelector('[role=alert]'), null, 'a successful retry clears the failure');
    } finally { view.close(); }
  }
});

check('mcp-servers: removal disables a reported catalog server in Installed and Undo restores its saved project choice', async () => {
  const view = await panel(async () => reported);
  const saved: boolean[] = [];
  api.toolsRemove = async () => {};
  api.toolsRestore = async () => {};
  api.setMcpServer = async (_id, _name, enabled) => { saved.push(enabled); };
  const button = (key: 'catalog.remove' | 'catalog.undo') => {
    const found = [...view.root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent?.trim() === t(key));
    ok(found); return found;
  };
  const toggle = (vendor: string) => view.root.querySelector<HTMLInputElement>(`[data-mcp-vendor="${vendor}"] [role=switch]`)!;
  try {
    await view.open();
    button('catalog.remove').click(); await tick();
    button('catalog.remove').click(); await tick();
    await view.back(); await view.installed();
    for (const vendor of ['claude', 'codex']) {
      equal(toggle(vendor).disabled, true); equal(toggle(vendor).checked, false);
      toggle(vendor).click(); await tick();
    }
    ok(view.root.querySelector('[data-mcp-vendor="claude"] [data-mcp-row="faber-chrome-devtools"]')?.textContent?.includes(t('set.mcp.status.connected')), 'removal preserves the last reported state');
    deepEqual(saved, [], 'removal never writes a project preference');
    button('catalog.undo').click(); await tick();
    equal(toggle('claude').disabled, false); equal(toggle('claude').checked, true);
    equal(toggle('codex').disabled, false); equal(toggle('codex').checked, false, 'Undo preserves each saved choice');
    deepEqual(saved, []);
  } finally { view.close(); }
});
