import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type ToolRow, type Vendor } from '../../ui/api.ts';
import { Catalog } from '../../ui/Catalog.tsx';
import { LANGUAGES, catalogName, lang, t, useLanguage } from '../../ui/i18n.ts';
import type { Key } from '../../ui/locales/en.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 25));
const tap = async (element: HTMLElement) => { element.click(); await tick(); };
const cards = (root: Element) => [...root.querySelectorAll<HTMLButtonElement>('[data-catalog-card]')];
function button(root: Element, key: Key) {
  const found = [...root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent?.trim() === t(key));
  ok(found, `button ${key}`); return found;
}
const fixture = (): ToolRow => ({
  id: 'fixture-plugin', job: 'code', repo: 'fixture/plugin', home: 'https://github.com/fixture/plugin',
  license: 'MIT', version: 'fixture-pin', installed: false, server: false, requiresNode: true,
  nodeAvailable: true, claudePluginsAvailable: false,
  pluginParts: [
    { id: 'review', kind: 'skill' }, { id: 'inspect', kind: 'command' },
    { id: 'reviewer', kind: 'agent' }, { id: 'reader', kind: 'server' },
  ],
});
async function panel(row = fixture(), options: { plugin?: boolean; vendors?: Vendor[] } = {}) {
  const before = { ...api }, language = lang();
  const skill = { ...row, id: 'fixture-skill', pluginParts: undefined, requiresNode: false };
  const rows = options.plugin === false ? [skill] : [row, skill];
  let removed: ToolRow | undefined;
  let failInstall = false;
  const calls: string[] = [];
  api.on = async () => () => {};
  api.toolsClearRemoved = async () => {};
  api.toolsList = async () => structuredClone(rows);
  api.toolsInventory = async () => [{ vendor: 'claude', issues: [], rows: row.installed ? [{
    name: row.id, kind: 'plugin', source: 'catalog', location: row.home, catalogId: row.id, enabled: row.enabled,
  }] : [] }];
  api.toolsInstall = async (id) => {
    calls.push(`install:${id}`);
    if (failInstall) throw 'error.tools.install';
    Object.assign(row, { installed: true, enabled: true });
  };
  api.toolsSetEnabled = async (id, enabled) => { calls.push(`toggle:${id}:${enabled}`); row.enabled = enabled; };
  api.toolsRemove = async (id) => { calls.push(`remove:${id}`); removed = structuredClone(row); row.installed = false; };
  api.toolsRestore = async (id) => { calls.push(`restore:${id}`); Object.assign(row, removed); };
  api.mcpServers = async () => [];
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:720px;height:600px;overflow:auto';
  root.dataset.motion = 'reduced';
  document.body.append(root);
  const dispose = render(() => createComponent(Catalog, { projects: [{ id: 'fixture-project', name: 'Fixture project', path: 'C:/fixture/project' }], vendors: options.vendors ?? ['claude', 'codex'] }), root);
  await tick();
  return { root, row, calls, fail: (next: boolean) => { failInstall = next; }, close: () => { dispose(); root.remove(); Object.assign(api, before); useLanguage(language); } };
}

check('plugins: one catalog entry owns its parts, install, off, remove and Undo', async () => {
  const view = await panel();
  try {
    equal(cards(view.root).length, 2);
    await tap(button(view.root, 'catalog.skills'));
    equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, 'fixture-skill');
    await tap(button(view.root, 'catalog.plugins'));
    equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, view.row.id);
    equal(button(view.root, 'catalog.plugins').getAttribute('aria-pressed'), 'true');
    for (const kind of ['skill', 'command', 'agent', 'server'] as const) ok(cards(view.root)[0].textContent?.includes(t(`catalog.parts.${kind}`, { n: 1 })));
    ok(cards(view.root)[0].textContent?.includes(t('catalog.pluginCodexCommands')));
    ok(cards(view.root)[0].textContent?.includes(t('catalog.pluginClaudeUnavailable')));
    await tap(cards(view.root)[0]);
    deepEqual([...view.root.querySelectorAll<HTMLElement>('[data-plugin-part]')].map((part) => part.dataset.pluginPart), ['review', 'inspect', 'reviewer', 'reader']);
    for (const key of ['catalog.accessSkill', 'catalog.accessCommand', 'catalog.accessAgent', 'catalog.accessPluginServer', 'catalog.pluginProjectsOnly', 'catalog.pluginCodexCommands', 'catalog.pluginClaudeUnavailable'] as const) ok(view.root.textContent?.includes(t(key)), key);
    await tap(button(view.root, 'catalog.install'));
    equal(view.row.installed, true);
    await tap(button(view.root, 'catalog.back'));
    await tap(button(view.root, 'catalog.installed'));
    equal(cards(view.root).length, 1);
    equal(view.root.querySelectorAll('[data-inventory-source=catalog]').length, 1, 'parts do not become extra installed rows');
    await tap(view.root.querySelector<HTMLButtonElement>('[data-inventory-source=catalog] button')!);
    const toggle = view.root.querySelector<HTMLInputElement>('[role=switch]')!;
    toggle.focus(); await tap(toggle);
    equal(view.row.enabled, false);
    equal(view.root.querySelector('[role=switch]'), toggle, 'the whole-plugin switch keeps its native input');
    equal(document.activeElement, toggle);
    await tap(button(view.root, 'catalog.remove'));
    await tap(button(view.root, 'catalog.remove'));
    equal(view.row.installed, false);
    await tap(button(view.root, 'catalog.undo'));
    equal(view.row.installed, true); equal(view.row.enabled, false);
    equal(view.root.querySelector<HTMLInputElement>('[role=switch]')?.checked, false);
    deepEqual(view.calls, ['install:fixture-plugin', 'toggle:fixture-plugin:false', 'remove:fixture-plugin', 'restore:fixture-plugin']);
  } finally { view.close(); }
});

check('plugins: bundled servers share one project picker and keep namespaced choices', async () => {
  const row = fixture(); row.installed = true; row.enabled = true;
  row.pluginParts!.push({ id: 'writer', kind: 'server' });
  const view = await panel(row);
  const chosen: [string, string, boolean][] = [];
  api.mcpServers = async () => [{ vendor: 'claude', rows: [
    { name: 'faber-fixture-plugin--reader', source: 'catalog', enabled: true, status: 'connected' },
    { name: 'faber-fixture-plugin--writer', source: 'catalog', enabled: true, status: 'connected' },
    { name: 'faber-another-server', source: 'catalog', enabled: true, status: 'connected' },
  ] }];
  api.setMcpServer = async (projectId, name, enabled) => { chosen.push([projectId, name, enabled]); };
  try {
    await tap(button(view.root, 'catalog.plugins')); await tap(cards(view.root)[0]);
    equal(view.root.querySelectorAll('#set-mcp-project-select').length, 1);
    equal(view.root.querySelectorAll('[data-mcp-row]').length, 2);
    equal(view.root.querySelector('[data-mcp-row=faber-another-server]'), null);
    await tap(view.root.querySelector<HTMLInputElement>('[data-mcp-row="faber-fixture-plugin--reader"] [role=switch]')!);
    deepEqual(chosen, [['fixture-project', 'faber-fixture-plugin--reader', false]]);
    await tap(view.root.querySelector<HTMLInputElement>('#catalog-enabled-switch')!);
    for (const toggle of view.root.querySelectorAll<HTMLInputElement>('[data-mcp-row] [role=switch]')) {
      equal(toggle.checked, false); equal(toggle.disabled, true);
    }
    equal(chosen.length, 1, 'global Off preserves the per-project server choices');
  } finally { view.close(); }
});

check('plugins: a bundled server needs Node and install failures retain Retry', async () => {
  const row = fixture(); row.nodeAvailable = false;
  const view = await panel(row);
  try {
    await tap(button(view.root, 'catalog.plugins'));
    ok(cards(view.root)[0].textContent?.includes(t('catalog.needsNode')));
    await tap(cards(view.root)[0]);
    equal(button(view.root, 'catalog.install').disabled, true);
    ok(view.root.textContent?.includes(t('catalog.pluginNodeHelp')));
  } finally { view.close(); }
  const retry = await panel();
  try {
    await tap(button(retry.root, 'catalog.plugins')); await tap(cards(retry.root)[0]);
    retry.fail(true); await tap(button(retry.root, 'catalog.install'));
    ok(retry.root.querySelector('[role=alert]'));
    equal(retry.row.installed, false);
    retry.fail(false); await tap(button(retry.root, 'catalog.retry'));
    equal(retry.row.installed, true);
  } finally { retry.close(); }
});

check('plugins: parts and delivery limits are localized in all five languages', async () => {
  const language = lang();
  try {
    for (const locale of LANGUAGES) {
      useLanguage(locale.code);
      const view = await panel();
      try {
        await tap(button(view.root, 'catalog.plugins')); await tap(cards(view.root)[0]);
        for (const kind of ['skill', 'command', 'agent', 'server'] as const) ok(view.root.textContent?.includes(t(`catalog.part.${kind}`)), `${locale.code}: ${kind}`);
        ok(view.root.textContent?.includes(t('catalog.pluginCodexCommands')));
        ok(view.root.textContent?.includes(t('catalog.pluginClaudeUnavailable')));
      } finally { view.close(); }
    }
  } finally { useLanguage(language); }
});

check('plugins: the Plugins filter appears only when the catalog lists a plugin', async () => {
  const none = await panel(fixture(), { plugin: false });
  try {
    ok(![...none.root.querySelectorAll<HTMLButtonElement>('.catalog-filters button')].some((el) => el.textContent?.trim() === t('catalog.plugins')), 'no dead Plugins pill');
  } finally { none.close(); }
  const some = await panel();
  try { button(some.root, 'catalog.plugins'); } finally { some.close(); }
});

check('plugins: the Claude Code limit shows only for commands or subagents and only with Claude connected', async () => {
  const skills = fixture(); skills.pluginParts = [{ id: 'review', kind: 'skill' }];
  const view = await panel(skills);
  try {
    await tap(button(view.root, 'catalog.plugins'));
    ok(!cards(view.root)[0].textContent?.includes(t('catalog.pluginClaudeUnavailable')), 'a skills-only plugin has nothing to lose');
  } finally { view.close(); }
  const codexOnly = await panel(fixture(), { vendors: ['codex'] });
  try {
    await tap(button(codexOnly.root, 'catalog.plugins'));
    ok(!cards(codexOnly.root)[0].textContent?.includes(t('catalog.pluginClaudeUnavailable')), 'no Claude Code, no Claude Code limit');
    ok(cards(codexOnly.root)[0].textContent?.includes(t('catalog.pluginCodexCommands')));
  } finally { codexOnly.close(); }
  const unread = fixture(); unread.claudePluginsAvailable = undefined;
  const probing = await panel(unread);
  try {
    await tap(button(probing.root, 'catalog.plugins'));
    ok(!cards(probing.root)[0].textContent?.includes(t('catalog.pluginClaudeUnavailable')), 'a CLI not read yet says nothing');
  } finally { probing.close(); }
});

check('plugins: a part reaching the work log reads as its plugin name, then the part', () => {
  equal(catalogName('fixture-plugin--careful-reader'), `${catalogName('fixture-plugin')} · careful reader`);
  equal(catalogName('debug'), catalogName('debug'));
  ok(!catalogName('fixture-plugin--reader').includes('--'));
});
