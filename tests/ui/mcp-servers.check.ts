import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type McpServerList } from '../../ui/api.ts';
import { SkillsCard } from '../../ui/Skills.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 20));
const reported: McpServerList[] = [
  { vendor: 'claude', readAt: 1_700_000_000, rows: [
    { name: 'chrome-devtools', source: 'catalog', status: 'connected', enabled: true },
    { name: 'project-reader', source: 'project', status: 'failed' },
    { name: 'personal-reader', source: 'user', status: 'needs-auth' },
  ] },
  { vendor: 'codex', readAt: 1_700_000_001, rows: [
    { name: 'chrome-devtools', source: 'catalog', status: 'disabled', enabled: false },
    { name: 'project-reader', source: 'project', status: 'starting' },
    { name: 'personal-reader', source: 'user', status: 'future-status' },
  ] },
];

async function panel(read: (id: string) => Promise<McpServerList[]>) {
  const before = { ...api };
  const initial = await before.state();
  let stateReads = 0;
  api.state = async () => {
    stateReads++;
    return { ...initial, projects: [{ id: 'first', name: 'First project', path: 'C:/first' }, { id: 'second', name: 'Second project', path: 'C:/second' }] };
  };
  api.toolsList = async () => [{ id: 'chrome-devtools', job: '', license: 'Apache-2.0', repo: 'ChromeDevTools/chrome-devtools-mcp', home: '', installed: true, server: true }];
  api.mcpServers = async (id) => structuredClone(await read(id));
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const errors: unknown[] = [];
  const dispose = render(() => createComponent(SkillsCard, { servers: true, onError: (error) => errors.push(error) }), root);
  await tick();
  return {
    root, errors, stateReads: () => stateReads,
    head: () => root.querySelector<HTMLButtonElement>('.pset-head')!,
    close: () => { dispose(); root.remove(); Object.assign(api, before); },
  };
}

check('mcp-servers: saving changes only the field and keeps every switch and keyboard focus', async () => {
  const view = await panel(async () => reported);
  let finish: () => void = () => {};
  const saved: boolean[] = [];
  api.setMcpServer = async (_id, _name, enabled) => {
    saved.push(enabled);
    await new Promise<void>((resolve) => { finish = resolve; });
  };
  try {
    view.head().click();
    await tick();
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
    view.head().click();
    await tick();
    view.root.querySelector<HTMLSelectElement>('#set-mcp-project-select')!.focus();
  },
  finishSave: () => {},
  close: () => {},
};

check('mcp-servers: the catalog reads only while open, shows both agents sources and reported states, and saves only catalog switches', async () => {
  const reads: string[] = [];
  const view = await panel(async (id) => { reads.push(id); return reported; });
  const saved: unknown[] = [];
  api.setMcpServer = async (id, name, enabled) => { saved.push([id, name, enabled]); };
  try {
    equal(reads.length, 0, 'a closed catalog reads no project tool list');
    equal(view.stateReads(), 0, 'a closed catalog reads no project picker');
    view.head().click();
    await tick();
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
      ok(group.querySelector('[data-mcp-row="chrome-devtools"]')?.textContent?.includes('Changes apply when the conversation next starts.'));
    }
    const toggle = claude.querySelector<HTMLInputElement>('[role="switch"]')!;
    equal(toggle.checked, true);
    toggle.click();
    await tick();
    deepEqual(saved, [['first', 'chrome-devtools', false]]);
    const changed = view.root.querySelector('[data-mcp-vendor="claude"]')!;
    equal(changed.querySelector<HTMLInputElement>('[role="switch"]')?.checked, false);
    ok(changed.textContent?.includes('Connected'), 'changing a preference never invents a new reported state');
    api.setMcpServer = async () => { throw new Error('save failed'); };
    changed.querySelector<HTMLInputElement>('[role="switch"]')!.click();
    await tick();
    equal(changed.querySelector<HTMLInputElement>('[role="switch"]')?.checked, false, 'a failed save restores the saved choice');
    equal(view.errors.length, 1, 'a save error reaches the Settings error handler');
    view.head().click();
    await tick();
    equal(view.root.querySelector('[data-mcp-servers]'), null, 'closing removes the project list');
    equal(reads.length, 1, 'no background reading');
    view.head().click();
    await tick();
    equal(reads.length, 2, 'reopening reads a fresh snapshot');
  } finally { view.close(); }
});

check('mcp-servers: before an agent report the catalog shows its saved choice and says when the other tools will appear', async () => {
  const view = await panel(async () => [{ vendor: 'claude', rows: [{ name: 'chrome-devtools', source: 'catalog', enabled: true }] }]);
  try {
    view.head().click();
    await tick();
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
    view.head().click();
    await tick();
    const picker = view.root.querySelector<HTMLSelectElement>('#set-mcp-project-select')!;
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
