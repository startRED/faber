import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import catalog from '../../engine/src/tools.json';
import { api, type AppEvent, type AppState, type ToolInventoryList, type ToolRow, type Vendor } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { Catalog, forgetCatalogView } from '../../ui/Catalog.tsx';
import { catalogName, LANGUAGES, lang, t, useLanguage } from '../../ui/i18n.ts';
import type { Key } from '../../ui/locales/en.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 25));

check('catalog: every shipped item resolves to its shared name in every language', () => {
  const before = lang();
  try {
    for (const locale of LANGUAGES) {
      useLanguage(locale.code);
      for (const row of [...catalog.skills, ...catalog.servers]) {
        const key = `catalog.name.${row.id}` as Key;
        ok(Object.hasOwn(locale.messages, key), `${locale.code}: ${row.id} has a hand-written name`);
        const name = locale.messages[key];
        ok(typeof name === 'string' && name.trim().length > 0);
        equal(catalogName(row.id), name, `${locale.code}: names resolve without a storage-id fallback`);
      }
      ok(!Object.keys(locale.messages).some((key) => key.startsWith('kit.name.')), 'the work log has no second list of names');
    }
  } finally { useLanguage(before); }
});
const project = { id: 'catalog-project', name: 'Fixture project', path: 'C:/fixture/project' };
const sample = (): ToolRow[] => [
  { id: 'systematic-debugging', job: 'debugging', tier: catalog.skills.find((row) => row.id === 'systematic-debugging')!.tier as ToolRow['tier'], license: 'MIT', repo: 'fixture/debugging', home: 'https://github.com/fixture/debugging', version: 'abc123', installed: false, server: false },
  { id: 'ponytail', job: 'lean', tier: catalog.skills.find((row) => row.id === 'ponytail')!.tier as ToolRow['tier'], license: 'MIT', repo: 'fixture/simple', home: 'https://github.com/fixture/simple', version: 'def456', installed: true, enabled: false, server: false },
  { id: 'chrome-devtools', job: 'measure', license: 'Apache-2.0', repo: 'fixture/browser', home: 'https://github.com/fixture/browser', version: '1.0.0', installed: false, server: true, nodeAvailable: true },
];
const sources: ToolInventoryList[] = [{ vendor: 'claude', rows: [
  { name: 'ponytail', kind: 'skill', source: 'catalog', location: 'https://github.com/fixture/simple', enabled: false, catalogId: 'ponytail' },
  { name: 'personal-skill', kind: 'skill', source: 'user', location: 'C:/person/.claude/skills/personal-skill' },
  { name: 'team-plugin', kind: 'plugin', source: 'project', location: 'C:/person/.claude/plugins/installed_plugins.json' },
], issues: [{ source: 'user', kind: 'plugin', reason: 'unsupported' }] }];
type Options = { keepView?: boolean; rows?: ToolRow[]; inventory?: ToolInventoryList[]; vendors?: Vendor[]; failList?: boolean; failInventory?: boolean; failInstall?: boolean; failToggle?: boolean; failRemove?: boolean; failRestore?: boolean; failReplace?: boolean };
const cards = (root: Element) => [...root.querySelectorAll<HTMLButtonElement>('[data-catalog-card]')];
function button(root: Element, key: Key) {
  const found = [...root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent?.trim() === t(key));
  ok(found, `button ${key}`); return found;
}
const tap = async (element: HTMLElement) => { element.click(); await tick(); };
function search(root: Element, value: string) {
  const input = root.querySelector<HTMLInputElement>('input[type=search]')!;
  input.value = value; input.dispatchEvent(new Event('input', { bubbles: true }));
}
function keyboard(key: string, extra: KeyboardEventInit = {}) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...extra }));
}
async function panel(options: Options = {}) {
  const before = { ...api }, language = lang(), store = structuredClone(options.rows ?? sample());
  const removed = new Map<string, ToolRow>();
  let listener: ((event: AppEvent) => void) | undefined;
  const calls = { list: 0, inventory: 0, install: [] as string[], remove: [] as string[], enabled: [] as boolean[], links: [] as string[], prepare: [] as string[], replace: [] as unknown[] };
  api.toolsClearRemoved = async () => {};
  api.on = async (receive) => { listener = receive; return () => {}; };
  api.toolsList = async () => { calls.list++; if (options.failList) throw 'error.tools.install'; return structuredClone(store); };
  api.toolsInventory = async () => { calls.inventory++; if (options.failInventory) throw 'error.tools.install'; return structuredClone(options.inventory ?? []); };
  api.toolsInstall = async (id) => { calls.install.push(id); if (options.failInstall) throw 'error.tools.install'; Object.assign(store.find((r) => r.id === id)!, { installed: true, hasCopy: true, enabled: store.find((r) => r.id === id)!.enabled !== false, status: undefined }); };
  api.toolsPrepareReplacement = async (id) => { calls.prepare.push(id); return { plan: { id, revision: calls.prepare.length }, from: 'old-pin', to: 'new-pin', enabled: store.find((r) => r.id === id)!.enabled !== false }; };
  api.toolsReplace = async (plan) => { calls.replace.push(plan); if (options.failReplace) throw 'error.tools.install'; const id = (plan as { id: string }).id; Object.assign(store.find((r) => r.id === id)!, { installed: true, hasCopy: true, status: undefined }); };
  api.toolsRemove = async (id) => { calls.remove.push(id); if (options.failRemove) throw 'error.tools.remove'; const row = store.find((r) => r.id === id)!; removed.set(id, structuredClone(row)); row.installed = false; };
  api.toolsRestore = async (id) => { Object.assign(store.find((r) => r.id === id)!, removed.get(id)); if (options.failRestore) throw 'error.tools.restore'; };
  api.toolsSetEnabled = async (id, enabled) => { calls.enabled.push(enabled); if (options.failToggle) throw 'error.tools.toggle'; store.find((r) => r.id === id)!.enabled = enabled; };
  api.openLink = async (url) => { calls.links.push(url); };
  api.mcpServers = async () => [];
  api.inputSeen = async () => {};
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:720px;height:600px;display:flex;flex-direction:column';
  root.dataset.motion = 'reduced';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  if (!options.keepView) forgetCatalogView();
  const dispose = render(() => createComponent(Catalog, { projects: [project], vendors: options.vendors ?? ['claude'] }), root);
  await tick();
  return { root, calls, store, publish: () => listener?.({ type: 'catalog' }), close: () => { dispose(); root.remove(); Object.assign(api, before); useLanguage(language); } };
}

// Native visual audits reuse this fixture; this module is absent from production builds.
Object.assign(window, { __faberCatalogReplacementPreview: async () => {
  const rows = sample(); rows[1].status = 'error.tools.reinstall';
  const view = await panel({ rows });
  view.root.style.cssText = 'position:fixed;inset:0;display:flex;flex-direction:column;background:var(--bg);z-index:10000';
  await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
  await tap(button(view.root, 'catalog.replace'));
  return true;
} });

check('catalog manual replacement: a quick repair opens details before any installation', async () => {
  const rows = sample();
  rows[1].status = 'error.tools.reinstall';
  const view = await panel({ rows });
  try {
    const tile = view.root.querySelector('[data-catalog-card="ponytail"]')?.parentElement;
    const action = tile?.querySelector<HTMLButtonElement>('.catalog-quick');
    ok(action, 'a damaged held item offers its repair action');
    await tap(action!);
    deepEqual(view.calls.install, [], 'the existing copy cannot be replaced before reviewing and confirming its details');
    ok(view.root.querySelector('.catalog-detail'), 'repair opens the existing item details');
  } finally { view.close(); }
});

check('catalog manual replacement: confirmation, cancellation and failed-plan retry preserve the off choice', async () => {
  const rows = sample(); rows[1].status = 'error.tools.reinstall';
  const options: Options = { rows, failReplace: true }, view = await panel(options);
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    await tap(button(view.root, 'catalog.replace'));
    ok(view.root.querySelector('[data-catalog-replacement]'));
    ok(view.root.textContent?.includes('old-pin') && view.root.textContent?.includes('new-pin'));
    deepEqual(view.calls.install, []); deepEqual(view.calls.replace, []);
    await tap(button(view.root, 'catalog.cancel'));
    equal(view.root.querySelector('[data-catalog-replacement]'), null);
    await tap(button(view.root, 'catalog.replace'));
    await tap(button(view.root, 'catalog.confirmReplaceAction'));
    equal(view.calls.replace.length, 1); ok(view.root.querySelector('[role=alert]'));
    equal(view.store[1].enabled, false);
    options.failReplace = false;
    await tap(button(view.root, 'catalog.retry'));
    equal(view.calls.prepare.length, 3, 'Retry prepares a fresh confirmation, never reuses a failed plan');
    equal(view.calls.replace.length, 1);
    await tap(button(view.root, 'catalog.confirmReplaceAction'));
    equal(view.calls.replace.length, 2); equal(view.store[1].enabled, false);
    equal(view.root.querySelector<HTMLInputElement>('[role=switch]')?.checked, false);
    deepEqual(view.calls.install, []);
  } finally { view.close(); }
});

check('catalog manual replacement: batch installation excludes existing and removed copies', async () => {
  const rows = sample(); rows[1].status = 'error.tools.reinstall'; rows[2].removedEnabled = false;
  const view = await panel({ rows });
  try {
    await tap(view.root.querySelector<HTMLButtonElement>('[data-catalog-pick-shown]')!);
    await tap(view.root.querySelector<HTMLButtonElement>('[data-batch=install]')!);
    deepEqual(view.calls.install, ['systematic-debugging']);
    deepEqual(view.calls.prepare, []); deepEqual(view.calls.replace, []);
    equal(view.store[2].removedEnabled, false);
  } finally { view.close(); }
});

check('catalog manual replacement: an explicitly missing copy installs afresh and keeps its off choice', async () => {
  const rows = sample(); rows[1].hasCopy = false; rows[1].installed = false; rows[1].status = 'error.tools.reinstall';
  const view = await panel({ rows });
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    await tap(button(view.root, 'catalog.install'));
    deepEqual(view.calls.install, ['ponytail']); deepEqual(view.calls.prepare, []);
    equal(view.root.querySelector<HTMLInputElement>('[role=switch]')?.checked, false);
  } finally { view.close(); }
});

check('catalog manual replacement: Escape cancels the question and returns focus to its heading', async () => {
  const rows = sample(); rows[1].status = 'error.tools.reinstall';
  const view = await panel({ rows });
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    await tap(button(view.root, 'catalog.replace'));
    equal(document.activeElement, button(view.root, 'catalog.cancel'));
    keyboard('Escape'); await tick();
    equal(view.root.querySelector('[data-catalog-replacement]'), null);
    equal(document.activeElement, view.root.querySelector('.catalog-detail h1'));
    deepEqual(view.calls.replace, []);
  } finally { view.close(); }
});

check('catalog manual replacement: leaving details discards a late preparation', async () => {
  const rows = sample(); rows[1].status = 'error.tools.reinstall';
  const view = await panel({ rows });
  try {
    let finish!: (value: Awaited<ReturnType<typeof api.toolsPrepareReplacement>>) => void;
    api.toolsPrepareReplacement = () => new Promise((resolve) => { finish = resolve; });
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    await tap(button(view.root, 'catalog.replace')); await tap(button(view.root, 'catalog.back'));
    finish({ plan: { id: 'ponytail' }, from: 'old', to: 'new', enabled: false }); await tick();
    equal(view.root.querySelector('[data-catalog-replacement]'), null);
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    equal(view.root.querySelector('[data-catalog-replacement]'), null);
    deepEqual(view.calls.replace, []);
  } finally { view.close(); }
});

check('catalog: reinstall status follows catalog-owned inventory and reported servers without changing external rows', async () => {
  const rows = sample();
  rows[1].status = rows[2].status = 'error.tools.reinstall';
  rows[2].installed = true;
  const view = await panel({ rows, inventory: sources });
  api.mcpServers = async () => [{ vendor: 'claude', rows: [
    { name: 'faber-chrome-devtools', source: 'catalog', enabled: true, status: 'connected' },
    { name: 'person-browser', source: 'user', status: 'connected' },
  ] }];
  try {
    await tap(button(view.root, 'catalog.installed'));
    ok(view.root.querySelector('[data-inventory-source=catalog]')?.textContent?.includes(t('error.tools.reinstall')));
    ok(!view.root.querySelector('[data-inventory-source=user]')?.textContent?.includes(t('error.tools.reinstall')));
    const picker = view.root.querySelector<HTMLSelectElement>('.catalog-project select')!;
    picker.value = project.id; picker.dispatchEvent(new Event('change', { bubbles: true })); await tick();
    const server = view.root.querySelector('[data-mcp-row=faber-chrome-devtools]')!;
    ok(server.textContent?.includes(t('error.tools.reinstall')));
    equal(server.querySelector<HTMLInputElement>('[role=switch]')?.disabled, true);
    equal(server.querySelector<HTMLInputElement>('[role=switch]')?.checked, false);
    ok(!view.root.querySelector('[data-mcp-row=person-browser]')?.textContent?.includes(t('error.tools.reinstall')));
    view.store[1].status = view.store[2].status = undefined;
    view.publish(); await tick();
    ok(!view.root.textContent?.includes(t('error.tools.reinstall')), 'publication refreshes cards, inventory and server hints together');
  } finally { view.close(); }
});

check('catalog: background publication preserves Remove Undo and its previous off choice', async () => {
  const view = await panel();
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === 'ponytail')!);
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
    view.publish(); await tick();
    ok(view.root.querySelector('[role=status]')?.textContent?.includes(t('catalog.removed', { name: catalogName('ponytail') })));
    await tap(button(view.root, 'catalog.undo'));
    equal(view.store[1].installed, true); equal(view.store[1].enabled, false);
    equal(view.root.querySelector<HTMLInputElement>('[role=switch]')?.checked, false);
  } finally { view.close(); }
});

check('catalog: a publication read started before a successful toggle cannot restore its old choice', async () => {
  const rows = sample(); rows[0].installed = true; rows[0].enabled = true;
  const view = await panel({ rows });
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === rows[0].id)!);
    const previous = structuredClone(view.store);
    let publish!: (rows: ToolRow[]) => void;
    api.toolsList = () => new Promise((resolve) => { publish = resolve; });
    view.publish();
    const toggle = view.root.querySelector<HTMLInputElement>('[role=switch]')!;
    toggle.focus(); await tap(toggle);
    equal(view.store[0].enabled, false); equal(toggle.checked, false);
    publish(previous); await tick();
    equal(view.root.querySelector('[role=switch]'), toggle, 'the native input and focus survive');
    equal(document.activeElement, toggle);
    equal(toggle.checked, false, 'the late pre-action snapshot cannot undo a saved choice');
    equal(view.store[0].enabled, false);
  } finally { view.close(); }
});

check('catalog: search covers translated descriptions and jobs without case, and kind filters share one list', async () => {
  const view = await panel();
  try {
    equal(cards(view.root).length, 3);
    ok(view.root.textContent?.includes(t('catalog.goodStart')));
    for (const query of [t('tools.systematic-debugging').toUpperCase(), t('catalog.job.debugging').toUpperCase(), 'FIXTURE/DEBUGGING']) {
      search(view.root, query); equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, 'systematic-debugging');
    }
    search(view.root, ''); await tap(button(view.root, 'catalog.skills')); equal(cards(view.root).length, 2);
    await tap(button(view.root, 'catalog.servers')); equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, 'chrome-devtools');
    await tap(button(view.root, 'catalog.installed')); equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, 'ponytail');
    equal(button(view.root, 'catalog.installed').getAttribute('aria-pressed'), 'true');
    equal(view.calls.list, 1, 'browsing does not reread the store');
  } finally { view.close(); }
});

check('catalog: the full curated list pages without duplicates and searches beyond the first page in five languages', async () => {
  const rows: ToolRow[] = [...catalog.skills, ...catalog.servers].map((item, index) => ({
    ...item, tier: 'tier' in item ? item.tier as ToolRow['tier'] : 'more',
    server: 'server' in item, installed: index < 49, home: `https://github.com/${item.repo}`,
  }));
  const view = await panel({ rows });
  try {
    ok(rows.length > 100, 'exercise the shipped catalog rather than a short synthetic list');
    const good = [...view.root.querySelectorAll('.catalog-group')].find((group) => group.querySelector('h2')?.textContent === t('catalog.goodStart'))!;
    const suggestions = cards(good);
    equal(suggestions.length, 10);
    ok(suggestions.every((card) => rows.find((row) => row.id === card.dataset.catalogCard)?.tier === 'recommended'));
    deepEqual(suggestions.map((card) => card.dataset.catalogCard).sort(), [
      'systematic-debugging', 'test-driven-development', 'ponytail', 'design-taste-frontend',
      'improve-codebase-architecture', 'grill-me', 'skill-creator', 'emil-design-eng',
      'break-ui', 'humanizer',
    ].sort(), 'Good start keeps general coding and everyday skills, including the original entry points');
    const seen: string[] = [];
    const pages = Math.ceil(rows.length / 48);
    for (let page = 0; page < pages; page++) {
      const pageCards = cards(view.root);
      equal(pageCards.length, Math.min(48, rows.length - page * 48));
      seen.push(...pageCards.map((card) => card.dataset.catalogCard!));
      equal(button(view.root, 'catalog.previous').disabled, page === 0);
      equal(button(view.root, 'catalog.next').disabled, page === pages - 1);
      if (page < pages - 1) await tap(button(view.root, 'catalog.next'));
    }
    equal(new Set(seen).size, rows.length, 'each item appears on exactly one page');
    deepEqual([...seen].sort(), rows.map((row) => row.id).sort());
    const target = rows.find((row) => row.id === 'flutter-apply-architecture-best-practices')!;
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language);
      for (const query of [target.id.toUpperCase(), catalogName(target.id).toUpperCase(), t(`tools.${target.id}` as Key).toUpperCase()]) {
        search(view.root, query); await tick();
        equal(cards(view.root).length, 1);
        equal(cards(view.root)[0].dataset.catalogCard, target.id, 'search reaches items beyond the first page');
        equal(view.root.querySelector('.catalog-pages'), null, 'search resets pagination');
      }
    }
    search(view.root, ''); await tap(button(view.root, 'catalog.skills'));
    equal(cards(view.root).length, 48);
    await tap(button(view.root, 'catalog.servers'));
    equal(cards(view.root).length, 1); equal(cards(view.root)[0].dataset.catalogCard, 'chrome-devtools');
    await tap(button(view.root, 'catalog.all'));
    search(view.root, target.id); await tick(); await tap(cards(view.root)[0]);
    ok(view.root.textContent?.includes(t(`tools.${target.id}.needs` as Key)), 'prerequisites are readable before installation');
    await tap(button(view.root, 'catalog.back'));
    search(view.root, 'pm-sql-queries'); await tick(); await tap(cards(view.root)[0]);
    ok(view.root.textContent?.includes(t('catalog.cliName', { name: 'sql-queries' })), 'details show the pinned name used by the AI tool');
    await tap(button(view.root, 'catalog.back'));
    search(view.root, '');
    await tap(button(view.root, 'catalog.installed')); await tap(button(view.root, 'catalog.next'));
    equal(cards(view.root).length, 1);
    await tap(cards(view.root)[0]);
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
    await tap(button(view.root, 'catalog.back'));
    equal(cards(view.root).length, 48, 'removing the last item on a page returns to the remaining page');
    equal(view.root.querySelector('.catalog-pages'), null);
    equal(view.calls.list, 1, 'filtering and paging keep the one local snapshot');
    equal(view.calls.inventory, 2, 'only entering Installed and the removal refresh external discovery');
  } finally { view.close(); }
});

check('catalog: return stays reachable after reading a small-window detail', async () => {
  const view = await panel({ vendors: [] });
  try {
    view.root.style.height = '480px';
    // This isolated component must sit above the running test app's caption.
    view.root.style.zIndex = '10000';
    for (const locale of LANGUAGES) {
      useLanguage(locale.code);
      for (const enlarged of [false, true]) {
        view.root.style.setProperty('--fs-sm', enlarged ? '24px' : themes.css.base['fs-sm']);
        view.root.style.setProperty('--fs-ui', enlarged ? '28px' : themes.css.base['fs-ui']);
        await tap(cards(view.root)[0]);
        const surface = view.root.querySelector<HTMLElement>('[data-catalog]')!;
        surface.scrollTop = surface.scrollHeight;
        await tick();
        try { ok(surface.scrollTop > 0, 'the detail was scrolled'); } catch {
          throw new Error(`the detail was scrolled (locale=${locale.code}, enlarged=${enlarged}, motion=${view.root.dataset.motion}, scrollTop=${surface.scrollTop}, clientHeight=${surface.clientHeight}, scrollHeight=${surface.scrollHeight})`);
        }
        const back = button(view.root, 'catalog.back');
        const bounds = back.getBoundingClientRect(), viewport = surface.getBoundingClientRect();
        ok(bounds.top >= viewport.top && bounds.bottom <= viewport.bottom, `${locale.code}: return stays inside the reading viewport`);
        ok(bounds.right <= viewport.right, `${locale.code}: the whole return label fits`);
        const hit = document.elementFromPoint(bounds.left + bounds.width / 2, bounds.top + bounds.height / 2);
        equal(hit?.closest('button'), back, `return is not covered: ${hit?.outerHTML.slice(0, 180)}`);
        await tap(back);
        equal(view.root.querySelector('.catalog-detail'), null);
        equal((document.activeElement as HTMLElement).dataset.catalogCard, 'systematic-debugging');
      }
    }
  } finally { view.close(); }
});

check('catalog: details open their original source in the system browser and install, switch, cancel removal and undo preserve state', async () => {
  const view = await panel();
  try {
    await tap(cards(view.root)[0]);
    ok(view.root.querySelector('.catalog-detail'));
    ok(view.root.textContent?.includes(t('catalog.whenSkill')) && view.root.textContent?.includes(t('catalog.accessSkill')));
    ok(view.root.textContent?.includes('abc123') && view.root.textContent?.includes('MIT'));
    await tap(view.root.querySelector<HTMLButtonElement>('.catalog-detail dl button')!);
    deepEqual(view.calls.links, ['https://github.com/fixture/debugging']);
    await tap(button(view.root, 'catalog.install')); equal(view.store[0].installed, true);
    const toggle = view.root.querySelector<HTMLInputElement>('[role=switch]')!;
    ok(toggle.labels?.length, 'the switch has a screen-reader name');
    toggle.focus(); await tap(toggle); equal(toggle.checked, false); equal(document.activeElement, toggle);
    await tap(toggle); equal(toggle.checked, true); await tap(toggle); equal(toggle.checked, false);
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.cancel')); equal(view.calls.remove.length, 0);
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
    equal(view.store[0].installed, false); ok(view.root.textContent?.includes(t('catalog.removed', { name: catalogName('systematic-debugging') })));
    await tap(button(view.root, 'catalog.undo'));
    equal(view.store[0].installed, true); equal(view.store[0].enabled, false);
    equal(view.root.querySelector<HTMLInputElement>('[role=switch]')?.checked, false, 'undo restores the off choice');
    deepEqual(view.calls.install, ['systematic-debugging']); deepEqual(view.calls.enabled, [false, true, false]);
  } finally { view.close(); }
});

check('catalog: a failed removal of another item preserves the previous Undo item and its on choice', async () => {
  const rows = sample(); rows[0].installed = true; rows[0].enabled = true;
  const options: Options = { rows }, view = await panel(options);
  try {
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === rows[0].id)!);
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
    await tap(button(view.root, 'catalog.back'));
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === rows[1].id)!);
    options.failRemove = true;
    await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
    ok(view.root.querySelector('[role=alert]'));
    equal(view.store[1].installed, true); equal(view.store[1].enabled, false);
    ok(view.root.querySelector('[role=status]')?.textContent?.includes(t('catalog.removed', { name: catalogName(rows[0].id) })));
    await tap(button(view.root, 'catalog.undo'));
    await tap(button(view.root, 'catalog.back'));
    await tap(cards(view.root).find((card) => card.dataset.catalogCard === rows[0].id)!);
    equal(view.store[0].installed, true); equal(view.store[0].enabled, true);
    equal(view.root.querySelector<HTMLInputElement>('#catalog-enabled-switch')?.checked, true, 'the screen and restored store agree');
    equal(view.root.querySelector('[role=status]'), null, 'Undo clears only its own removal');
  } finally { view.close(); }
});

check('catalog: Retry after a failed Undo refreshes the restored on/off state and clears the removal notice', async () => {
  for (const enabled of [true, false]) {
    const rows = sample(); rows[0].installed = true; rows[0].enabled = enabled;
    const options: Options = { rows, failRestore: true }, view = await panel(options);
    try {
      await tap(cards(view.root).find((card) => card.dataset.catalogCard === rows[0].id)!);
      await tap(button(view.root, 'catalog.remove')); await tap(button(view.root, 'catalog.remove'));
      await tap(button(view.root, 'catalog.undo'));
      ok(view.root.querySelector('[role=alert]'));
      ok(view.root.querySelector('[role=status]'), 'a failed Undo keeps its recovery action');
      equal(view.root.querySelector('#catalog-enabled-switch'), null, 'the failed acknowledgement has not changed the screen');
      options.failRestore = false; await tap(button(view.root, 'catalog.retry'));
      equal(view.root.querySelector('[role=alert], [role=status]'), null);
      equal(view.root.querySelector<HTMLInputElement>('#catalog-enabled-switch')?.checked, enabled);
      equal(view.store[0].installed, true); equal(view.store[0].enabled, enabled);
    } finally { view.close(); }
  }
});

check('catalog: shortcuts focus search, details focus their heading, Escape unwinds confirmation then detail and restores the card', async () => {
  const view = await panel();
  try {
    const card = cards(view.root).find((el) => el.dataset.catalogCard === 'ponytail')!;
    card.focus(); keyboard('f', { code: 'KeyF', ctrlKey: true }); await tick();
    equal(document.activeElement, view.root.querySelector('input[type=search]'));
    equal(view.root.querySelector('input')?.getAttribute('aria-keyshortcuts'), null, 'caps and matching both come from the shared shortcut list');
    await tap(card); equal(document.activeElement, view.root.querySelector('.catalog-detail h1'));
    await tap(button(view.root, 'catalog.remove')); keyboard('Escape'); await tick();
    ok(view.root.querySelector('.catalog-detail')); equal(view.root.textContent?.includes(t('catalog.confirmRemove', { name: catalogName('ponytail') })), false);
    keyboard('Escape'); await tick();
    equal(view.root.querySelector('.catalog-detail'), null);
    equal(document.activeElement, view.root.querySelector('[data-catalog-card=ponytail]'));
  } finally { view.close(); }
});

check('catalog: first-run, empty, no matches and missing Node are actionable without an AI', async () => {
  const view = await panel({ rows: [], vendors: [], inventory: [{ vendor: 'claude', rows: [], issues: [] }] });
  try {
    ok(view.root.textContent?.includes(t('catalog.noAi')) && view.root.textContent?.includes(t('catalog.empty')));
    await tap(button(view.root, 'catalog.installed')); ok(view.root.textContent?.includes(t('catalog.emptyInstalled')));
    ok(view.root.textContent?.includes(t('catalog.noneFound')));
    search(view.root, 'nothing-here'); await tick(); ok(view.root.textContent?.includes(t('catalog.noMatch')));
  } finally { view.close(); }
  const rows = sample(); rows[2].nodeAvailable = false;
  const node = await panel({ rows });
  try {
    await tap(cards(node.root).find((el) => el.dataset.catalogCard === 'chrome-devtools')!);
    ok(node.root.textContent?.includes(t('catalog.needsNode')) && node.root.textContent?.includes(t('catalog.nodeHelp')));
    equal(button(node.root, 'catalog.install').disabled, true);
    await tap(button(node.root, 'catalog.getNode')); deepEqual(node.calls.links, ['https://nodejs.org/en/download']);
    equal(node.calls.install.length, 0);
  } finally { node.close(); }
});

check('catalog: failed listing, installation and switching expose retry and keep saved choices', async () => {
  const options: Options = { failList: true, failInstall: true, failToggle: true }, view = await panel(options);
  try {
    ok(view.root.querySelector('[role=alert]')); options.failList = false;
    await tap(button(view.root, 'catalog.retry')); equal(cards(view.root).length, 3);
    await tap(cards(view.root)[0]); await tap(button(view.root, 'catalog.install'));
    equal(view.store[0].installed, false); ok(view.root.textContent?.includes(t('error.tools.install')));
    options.failInstall = false; await tap(button(view.root, 'catalog.retry')); equal(view.store[0].installed, true);
    const toggle = view.root.querySelector<HTMLInputElement>('[role=switch]')!;
    await tap(toggle); equal(toggle.checked, true); equal(view.store[0].enabled, true);
    options.failToggle = false; await tap(button(view.root, 'catalog.retry')); equal(toggle.checked, false);
  } finally { view.close(); }
});

check('catalog: installed source inventory is read-only for personal and project items and a failed read has retry', async () => {
  const options: Options = { inventory: sources, failInventory: true }, view = await panel(options);
  try {
    equal(view.calls.inventory, 0, 'the inventory starts unmounted');
    await tap(button(view.root, 'catalog.installed')); ok(view.root.querySelector('[data-catalog-inventory] [role=alert]'));
    options.failInventory = false; await tap(button(view.root, 'catalog.retry'));
    for (const source of ['catalog', 'user', 'project']) {
      const row = view.root.querySelector(`[data-inventory-source=${source}]`)!;
      ok(row.textContent?.includes(t(`set.mcp.source.${source}` as Key)));
      equal(row.querySelectorAll('input, [role=switch]').length, 0);
      equal(row.querySelectorAll('button').length, source === 'catalog' ? 1 : 0, 'external installs cannot be removed or switched');
    }
    equal(view.root.querySelector('[data-inventory-source=catalog] button')?.textContent, catalogName('ponytail'));
    ok(view.root.textContent?.includes('personal-skill') && view.root.textContent?.includes('team-plugin'));
    ok(view.root.querySelector('[data-inventory-issue]')?.textContent?.includes(t('catalog.issue.unsupported')));
    const picker = view.root.querySelector<HTMLSelectElement>('.catalog-project select')!;
    picker.value = project.id; picker.dispatchEvent(new Event('change', { bubbles: true })); await tick();
    equal(picker.value, project.id); ok(view.calls.inventory >= 3);
    await tap(view.root.querySelector<HTMLButtonElement>('[data-inventory-source=catalog] button')!);
    equal(view.root.querySelector('.catalog-detail h1')?.textContent, catalogName('ponytail'));
  } finally { view.close(); }
});

check('catalog: German reduced-motion cards fit 720px, never travel on hover, paginate at 48 and leave no hidden DOM or reads', async () => {
  const rows = Array.from({ length: 55 }, (_, i) => ({ ...sample()[0], id: `fixture-${i}-${'long-name'.repeat(6)}` }));
  const view = await panel({ rows });
  try {
    useLanguage('de'); await tick(); equal(cards(view.root).length, 48);
    const outer = view.root.getBoundingClientRect();
    for (const item of view.root.querySelectorAll<HTMLElement>('.catalog-search, .catalog-card, .catalog-filters')) {
      const box = item.getBoundingClientRect(); ok(box.left >= outer.left - 1 && box.right <= outer.right + 1, `${item.className} stays inside 720px`);
    }
    equal(getComputedStyle(cards(view.root)[0]).transform, 'none');
    equal(getComputedStyle(cards(view.root)[0]).transitionDuration, '0s');
    let hoverRules = 0;
    const inspect = (rules: CSSRuleList) => { for (const rule of rules) {
      if (rule instanceof CSSStyleRule && rule.selectorText.includes('.catalog-card:hover')) { hoverRules++; ok(!rule.style.transform || rule.style.transform === 'none', 'hover does not translate or scale a card'); }
      if ('cssRules' in rule) inspect((rule as CSSGroupingRule).cssRules);
    } };
    for (const sheet of document.styleSheets) { let rules: CSSRuleList; try { rules = sheet.cssRules; } catch { continue; } inspect(rules); }
    ok(hoverRules > 0, 'the hover rule is actually inspected');
    await tap(button(view.root, 'catalog.next')); equal(cards(view.root).length, 7);
    await tap(button(view.root, 'catalog.previous')); equal(cards(view.root).length, 48);
    equal(view.calls.list, 1);
  } finally { view.close(); }
  const reads = view.calls.list; await tick(); equal(view.root.querySelector('[data-catalog]'), null); equal(view.calls.list, reads);
});

check('catalog: full-motion cards never inherit a moving press in either button look', async () => {
  const view = await panel();
  try {
    view.root.dataset.motion = 'full';
    let pressRules = 0;
    for (const base of ['sala', 'oficina']) {
      view.root.dataset.base = base;
      const card = cards(view.root)[0];
      // Synthetic events cannot hold :active; match each transform selector as if this card were pressed.
      const inspect = (rules: CSSRuleList) => { for (const rule of rules) {
        if (rule instanceof CSSStyleRule && rule.selectorText.includes(':active') && rule.style.transform) {
          pressRules++;
          if (card.matches(rule.selectorText.replaceAll(':active', ''))) equal(rule.style.transform, 'none', `a pressed card stays still in ${base}`);
        }
        if ('cssRules' in rule) inspect((rule as CSSGroupingRule).cssRules);
      } };
      for (const sheet of document.styleSheets) { let rules: CSSRuleList; try { rules = sheet.cssRules; } catch { continue; } inspect(rules); }
    }
    ok(pressRules > 0, 'the shipped press transforms were inspected');
  } finally { view.close(); }
});

check('catalog: installed inventories also cap each AI at 48 rows per page', async () => {
  const inventory: ToolInventoryList[] = [{ vendor: 'codex', issues: [], rows: Array.from({ length: 51 }, (_, i) => ({ name: `own-${i}`, kind: 'skill', source: 'user', location: `C:/person/.agents/skills/own-${i}` })) }];
  const view = await panel({ inventory });
  try {
    await tap(button(view.root, 'catalog.installed')); equal(view.root.querySelectorAll('[data-inventory-source]').length, 48);
    await tap(button(view.root.querySelector('[data-catalog-inventory]')!, 'catalog.next'));
    equal(view.root.querySelectorAll('[data-inventory-source]').length, 3); equal(view.calls.inventory, 1);
  } finally { view.close(); }
});

check('catalog: the sidebar opens a browse-only tab and Chat unmounts it without starting a tool', async () => {
  const view = await panel(), before = { ...api }, place = localStorage.getItem('faber.place');
  const initial: AppState = { vendors: ['claude'], models: [], quotas: [], projects: [], conversations: [], version: '0.0.0', running: { revision: 0, items: [] },
    settings: { setup: 'done', tutorialDone: true, language: 'en' }, look: { vars: {}, scheme: 'dark', motion: 'none', effects: { ambient: false, texture: false, glow: false }, checks: [], notices: [],
      appearance: { base: 'sala', ground: 'sala', accent: '#ffffff', textFont: 'archivo', textSize: 14, density: 1, corners: 1, motion: 'none', effects: { ambient: false, texture: false, glow: false } } } };
  initial.settings.appearance = initial.look.appearance;
  api.state = async () => initial; api.setClock = async () => {}; api.recents = async () => ({ folders: [], environments: [] });
  api.notesSeen = async () => Number.MAX_SAFE_INTEGER; api.saveNotesSeen = async () => {}; api.inputSeen = async () => {};
  api.updateStatus = async () => ({ phase: 'none' }); api.onUpdate = async () => () => {};
  let started = 0; api.warm = async () => { started++; }; api.send = async () => { started++; return 'unused'; };
  api.commands = async () => []; api.promptHistory = async () => [];
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
  const host = document.createElement('div'); host.style.cssText = 'position:fixed;inset:0'; document.body.append(host);
  const dispose = render(() => createComponent(App, { initial, inbox: { take: () => {} } }), host);
  try {
    await tick(); ok(host.querySelector('.composer textarea'), 'Chat starts with its composer');
    const tab = host.querySelector<HTMLButtonElement>('[data-catalog-tab]')!;
    const tabsTop = host.querySelector('.areas')!.getBoundingClientRect().top;
    equal(tab.textContent?.trim(), t('area.catalog')); await tap(tab);
    ok(host.querySelector('[data-screen=catalog]')); equal(tab.getAttribute('aria-pressed'), 'true');
    const stage = host.querySelector('.main')!.getBoundingClientRect(), catalog = host.querySelector('[data-catalog]')!.getBoundingClientRect();
    ok(catalog.height >= stage.height - 60, 'Catalog fills the main stage, including when the wide screen has no topbar');
    equal(host.querySelector('.composer, .thread'), null);
    equal(host.querySelector('.areas')!.getBoundingClientRect().top, tabsTop, 'the navigation keeps its place when Catalog opens');
    ok(host.querySelector('.newchat'), 'New conversation remains available');
    const reads = view.calls.list;
    await tap(button(host.querySelector('.areas')!, 'area.chat'));
    equal(host.querySelector('[data-catalog]'), null); equal(view.calls.list, reads); equal(started, 0);
    for (let attempt = 0; attempt < 2; attempt++) {
      await tap(host.querySelector<HTMLButtonElement>('[data-catalog-tab]')!);
      equal(host.querySelector('#running'), null, 'Catalog hides an already open running column too');
      keyboard('k', { ctrlKey: true }); await tick();
      const input = host.querySelector<HTMLInputElement>('.pal input')!;
      ok(input, 'Ctrl+K opens commands from Catalog');
      input.value = t('run.toggle'); input.dispatchEvent(new Event('input', { bubbles: true }));
      const action = [...host.querySelectorAll<HTMLElement>('.pal [role=option]')].find((el) => el.querySelector('.pal-t')?.textContent === t('run.toggle'));
      ok(action, 'the running action is available'); await tap(action);
      equal(host.querySelector('[data-catalog]'), null, 'the action returns to work');
      ok(host.querySelector('[data-screen="chat.new"] .composer textarea'), 'the last work area returns');
      equal(host.querySelector('.run-btn')?.getAttribute('aria-expanded'), 'true', 'the running column opens even if it was already open behind Catalog');
      ok(host.querySelector('#running')?.getBoundingClientRect().width, 'the column is visible');
    }
    equal(started, 0, 'navigation starts no tool');
  } finally { dispose(); host.remove(); Object.assign(api, before); view.close(); if (place === null) localStorage.removeItem('faber.place'); else localStorage.setItem('faber.place', place); }
});

check('catalog: Installed search includes external skills, linked entries, plugin records and reported servers before showing empty states', async () => {
  const external: ToolInventoryList[] = [{ vendor: 'claude', issues: [], rows: [
    { name: 'personal-skill', kind: 'skill', source: 'user', location: 'C:/person/skills/personal-skill', linked: true },
    { name: 'project-plugin', kind: 'plugin', source: 'project', location: 'pluginRecord' },
  ] }];
  const view = await panel({ rows: [], inventory: external });
  api.mcpServers = async () => [{ vendor: 'claude', rows: [{ name: 'project-reader', source: 'project', status: 'connected' }] }];
  try {
    await tap(button(view.root, 'catalog.installed'));
    equal(view.root.textContent?.includes(t('catalog.emptyInstalled')), false, 'personal installs prevent a false empty state');
    search(view.root, 'PERSONAL-SKILL'); await tick();
    equal(view.root.querySelectorAll('[data-inventory-source]').length, 1);
    ok(view.root.querySelector('[data-inventory-linked]')?.textContent?.includes(t('catalog.linked')));
    equal(view.root.textContent?.includes(t('catalog.noMatch')), false);
    search(view.root, t('catalog.pluginRecord')); await tick();
    equal(view.root.querySelectorAll('[data-inventory-source]').length, 1);
    ok(view.root.querySelector('[data-inventory-source]')?.textContent?.includes('project-plugin'));
    equal(view.root.querySelector('.catalog-location')?.textContent, t('catalog.pluginRecord'));
    const picker = view.root.querySelector<HTMLSelectElement>('.catalog-project select')!;
    picker.value = project.id; picker.dispatchEvent(new Event('change', { bubbles: true }));
    search(view.root, 'project-reader'); await tick();
    ok(view.root.querySelector('[data-mcp-row=project-reader]'));
    equal(view.root.textContent?.includes(t('catalog.noMatch')), false, 'reported external servers also count as a match');
    search(view.root, 'no-external-match'); await tick();
    equal(view.root.querySelectorAll('[data-inventory-source], [data-mcp-row]').length, 0);
    ok(view.root.textContent?.includes(t('catalog.noMatch')));
  } finally { view.close(); }
});

check('catalog: cards act in place and a selection installs, switches and removes several items at once', async () => {
  const view = await panel();
  const pick = (id: string) => view.root.querySelector<HTMLInputElement>(`[data-catalog-card="${id}"] ~ .catalog-pick`)!;
  const batch = (action: string) => view.root.querySelector<HTMLButtonElement>(`[data-batch="${action}"]`);
  try {
    await tap(view.root.querySelector<HTMLButtonElement>(`[data-catalog-card="ponytail"] ~ .catalog-site`)!);
    deepEqual(view.calls.links, ['https://github.com/fixture/simple'], 'the card opens its source page without opening the detail');
    equal(view.root.querySelector('.catalog-detail'), null);
    const quick = view.root.querySelector<HTMLInputElement>('[data-catalog-card="ponytail"] ~ [role=switch]')!;
    equal(quick.checked, false); await tap(quick);
    deepEqual(view.calls.enabled, [true]); equal(view.store[1].enabled, true, 'the card switch saves the same choice as the detail');
    equal(view.root.querySelector('[data-catalog-batch]'), null, 'no bar before anything is selected');
    await tap(view.root.querySelector<HTMLButtonElement>('[data-catalog-pick-shown]')!);
    ok(view.root.querySelector('[data-catalog-batch]')?.textContent?.includes(t('catalog.picked', { n: 3 })));
    equal(batch('on'), null, 'an action that fits nothing selected is not offered');
    await tap(batch('install')!);
    deepEqual(view.calls.install, ['systematic-debugging', 'chrome-devtools']);
    ok(view.store.every((row) => row.installed), 'every selected item that was missing is installed');
    ok(view.root.querySelector('[data-catalog-batch]')?.textContent?.includes(t('catalog.picked', { n: 1 })), 'done items leave the selection; the one Install did not fit stays');
    pick('chrome-devtools').click(); await tick();
    await tap(batch('off')!);
    deepEqual(view.calls.enabled, [true, false, false]); ok(view.store.filter((row) => row.id !== 'systematic-debugging').every((row) => row.enabled === false));
    pick('ponytail').click(); pick('systematic-debugging').click(); await tick();
    await tap(batch('remove')!); equal(view.calls.remove.length, 0, 'removing several asks first');
    ok(view.root.textContent?.includes(t('catalog.confirmRemoveMany', { n: 2 })));
    keyboard('Escape'); await tick(); ok(batch('remove'), 'Escape leaves the question and keeps the selection');
    keyboard('Escape'); await tick(); equal(view.root.querySelector('[data-catalog-batch]'), null, 'a second Escape clears the selection');
    pick('ponytail').click(); pick('systematic-debugging').click(); await tick();
    await tap(batch('remove')!); await tap(batch('confirm-remove')!);
    deepEqual(view.calls.remove, ['systematic-debugging', 'ponytail']);
    equal(view.root.querySelector('[role=status]')?.textContent?.includes(t('catalog.undo')) ?? false, false, 'a group removal offers no single-item Undo');
  } finally { view.close(); }
  const failing = await panel({ failInstall: true });
  try {
    failing.root.querySelector<HTMLInputElement>('[data-catalog-card="chrome-devtools"] ~ .catalog-pick')!.click(); await tick();
    await tap(failing.root.querySelector<HTMLButtonElement>('[data-batch="install"]')!);
    ok(failing.root.querySelector('[role=alert]')?.textContent?.includes(t('catalog.batchFailed', { n: 1 })));
    ok(failing.root.querySelector('[data-catalog-batch] [role=alert]'), 'the failure shows in the bar beside the selection');
    failing.calls.install.length = 0;
    await tap(button(failing.root.querySelector('[data-catalog-batch]')!, 'catalog.retry'));
    deepEqual(failing.calls.install, ['chrome-devtools'], 'Try again repeats the same action on the failed item');
  } finally { failing.close(); }
});

check('catalog: the filter and the search are still there when the Catalog is opened again', async () => {
  const first = await panel();
  try {
    // Earlier checks leave a card selected, which is kept like the rest: go back to the list first.
    const back = [...first.root.querySelectorAll<HTMLButtonElement>('button')].find((el) => el.textContent?.trim() === t('catalog.back'));
    if (back) await tap(back);
    await tap(button(first.root, 'catalog.skills'));
    search(first.root, 'pony');
  } finally { first.close(); }
  const again = await panel({ keepView: true });
  try {
    equal(button(again.root, 'catalog.skills').getAttribute('aria-pressed'), 'true', 'the filter comes back');
    equal(again.root.querySelector<HTMLInputElement>('.catalog-search input')!.value, 'pony', 'the search comes back');
  } finally {
    await tap(button(again.root, 'catalog.all'));
    search(again.root, '');
    again.close();
  }
});

check('catalog_remove: an agent removal is shown with the existing Undo when Catalog opens', async () => {
  const rows = sample();
  rows[0].removedEnabled = false;
  const view = await panel({ rows });
  try {
    ok(view.root.querySelector('[role=status]')?.textContent?.includes(t('catalog.removed', { name: catalogName(rows[0].id) })));
    ok(button(view.root, 'catalog.undo'));
  } finally { view.close(); }
});

check('catalog_remove: Undo keeps the next agent removal available without reopening Catalog', async () => {
  const rows = sample();
  rows[0].removedEnabled = false;
  rows[1].installed = false; rows[1].removedEnabled = true;
  const view = await panel({ rows });
  const restored: string[] = [];
  api.toolsRestore = async (id) => {
    restored.push(id);
    const row = view.store.find((row) => row.id === id)!;
    row.installed = true; row.enabled = row.removedEnabled ?? undefined;
    delete row.removedEnabled;
  };
  try {
    await tap(button(view.root, 'catalog.undo'));
    ok(view.root.querySelector('[role=status]')?.textContent?.includes(t('catalog.removed', { name: catalogName(rows[1].id) })), 'the next pending removal remains reachable');
    await tap(button(view.root, 'catalog.undo'));
    deepEqual(restored, [rows[0].id, rows[1].id]);
    equal(view.store[0].enabled, false); equal(view.store[1].enabled, true);
    ok(!view.root.querySelector('[role=status]'), 'all restored receipts leave no Undo notice');
  } finally { view.close(); }
});
