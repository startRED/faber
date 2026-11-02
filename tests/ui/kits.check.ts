import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import { api, type AppEvent, type Message, type ToolRow, type Vendor } from '../../ui/api.ts';
import { Catalog, forgetCatalogView } from '../../ui/Catalog.tsx';
import { catalogName, lang, t, useLanguage } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

check('kits: Chat and Projects show session choices and their reason inside the work log for either AI', () => {
  const before = lang();
  try {
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language);
      for (const area of ['chat', 'project']) for (const vendor of ['claude', 'codex'] as Vendor[]) {
        const root = document.createElement('div');
        root.style.width = '720px';
        document.body.append(root);
        const [message, setMessage] = createSignal<Message>({
          id: 'kit', role: 'assistant', at: 0, status: 'streaming', text: 'Working.', vendor,
          toolKit: { ids: ['design-taste-frontend'], jobs: ['web'], method: 'code' },
        });
        const dispose = render(() => createComponent(Thread, {
          get messages() { return [message()]; }, from: 0, conversationId: area, options: [], onBranch: () => {},
        }), root);
        try {
          const fold = root.querySelector<HTMLDetailsElement>('details.kit')!;
          ok(fold && !fold.open, 'the kit line starts folded even without tool calls');
          equal(fold.querySelector('summary')?.textContent, t('kit.details'));
          const line = () => root.querySelector('p.kit');
          equal(line()?.closest('details'), fold, 'the line stays in the kit disclosure');
          equal(line()?.textContent, t('kit.line', { tools: t('catalog.name.design-taste-frontend'), reason: t('kit.code') }));
          fold.open = true;
          setMessage({ ...message(), text: 'Working. More words.', tools: [{ id: 'read', name: 'Read', input: { file_path: 'a.ts' }, ok: true }] });
          equal(root.querySelectorAll('.steps.one .step').length, 1, 'the tool step remains visible beside the kit');
          equal(line()?.textContent, t('kit.line', { tools: t('catalog.name.design-taste-frontend'), reason: t('kit.code') }), 'streaming does not change the session choice');
          for (const method of ['catalog', 'code'] as const) {
            setMessage({ ...message(), toolKit: { ids: ['design-taste-frontend'], jobs: ['web'], method, suggested: 'systematic-debugging' } });
            equal(line()?.textContent, `${t('kit.line', { tools: t('catalog.name.design-taste-frontend'), reason: t(`kit.${method}`) })} ${t('kit.suggest', { tool: t('catalog.name.systematic-debugging') })}`);
            ok(!line()?.textContent?.includes('design-taste-frontend'), 'selected skills use localized display names too');
            ok(!line()?.textContent?.includes('systematic-debugging'), 'the hint uses the localized display name, never its storage id');
            equal(line()?.querySelector('button, input'), null, 'a missing tool is a suggestion, never an installation or setting');
          }
          setMessage({ ...message(), status: 'done', toolKit: { ids: [], jobs: [], method: 'code' } });
          equal(line(), null, 'an empty saved report adds no catalog line to tool steps');
          equal(root.querySelectorAll('.steps.one .step').length, 1, 'an empty report does not hide real tool steps');
          setMessage({ ...message(), tools: [] });
          equal(root.querySelector('details.kit'), null, 'an empty saved report does not create a kit fold');
          setMessage({ ...message(), toolKit: { ids: [], jobs: ['documents'], method: 'code', suggested: 'systematic-debugging' } });
          equal(line()?.textContent, `${t('kit.line', { tools: t('kit.none'), reason: t('kit.code') })} ${t('kit.suggest', { tool: t('catalog.name.systematic-debugging') })}`);
          equal(root.querySelectorAll('details.kit').length, 1, 'a suggestion alone still has a quiet technical fold');
          setMessage({ ...message(), tools: [], toolKit: { ids: [], jobs: [], method: 'code', error: 'error.tools.snapshot' } });
          equal(line()?.textContent, t('error.tools.snapshot'), 'a failed snapshot reports the engine key quietly in every language');
          ok(line()?.classList.contains('pace'), 'snapshot failure uses the existing quiet work-log line');
          equal(root.querySelectorAll('details.kit').length, 1, 'preparation failure alone is inspectable');
          setMessage({ ...message(), toolKit: undefined, tools: [] });
          equal(root.querySelector('details.kit'), null, 'older replies do not gain an empty kit fold');
        } finally { dispose(); root.remove(); }
      }
    }
  } finally { useLanguage(before); }
});

const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 20));
const catalog = () => (forgetCatalogView(), createComponent(Catalog, { projects: [], vendors: ['claude'] }));

check('kits: expanded catalog cards, details and work log use the same localized item name', async () => {
  const before = { ...api }, language = lang();
  try {
    api.on = async () => () => {};
    api.toolsClearRemoved = async () => {};
    api.toolsList = async () => [{ id: 'pm-summarize-meeting', job: 'documents', license: 'MIT', repo: '', home: '', installed: true, server: false }];
    for (const locale of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(locale);
      const root = document.createElement('div');
      document.body.append(root);
      const stop = render(() => [catalog(), createComponent(Thread, {
        messages: [{ id: 'named-kit', role: 'assistant', at: 0, status: 'done', text: 'Done.', toolKit: { ids: ['pm-summarize-meeting'], jobs: ['documents'], method: 'code' } }],
        from: 0, conversationId: 'named-kit', options: [], onBranch: () => {},
      })], root);
      try {
        await tick();
        const name = catalogName('pm-summarize-meeting');
        equal(root.querySelector('.catalog-name')?.textContent, name);
        equal(root.querySelector('p.kit')?.textContent, t('kit.line', { tools: name, reason: t('kit.code') }));
        root.querySelector<HTMLButtonElement>('[data-catalog-card]')!.click();
        await tick();
        equal(root.querySelector('.catalog-detail h1')?.textContent, name);
      } finally { stop(); root.remove(); }
    }
  } finally { Object.assign(api, before); useLanguage(language); }
});

check('kits: damaged Catalog items explain reinstalling in every language and preserve Remove until consent', async () => {
  const before = { ...api }, language = lang();
  try {
    api.on = async () => () => {};
    api.toolsClearRemoved = async () => {};
    for (const language of ['en', 'pt-BR', 'es', 'fr', 'de'] as const) {
      useLanguage(language);
      let row: ToolRow = { id: 'systematic-debugging', job: 'debugging', license: 'MIT', repo: '', home: '', installed: true, enabled: true, server: false, status: 'error.tools.reinstall' };
      api.toolsList = async () => [structuredClone(row)];
      let installs = 0;
      let preparations = 0;
      let replacements = 0;
      api.toolsInstall = async () => { installs++; };
      api.toolsPrepareReplacement = async (id) => {
        preparations++;
        return { plan: { id, enabled: row.enabled }, from: 'old-copy', to: 'catalog-copy', enabled: row.enabled !== false };
      };
      api.toolsReplace = async (plan) => {
        replacements++;
        row = { ...row, installed: true, hasCopy: true, status: undefined, enabled: (plan as { enabled: boolean }).enabled };
      };
      const root = document.createElement('div');
      document.body.append(root);
      const dispose = render(catalog, root);
      try {
        await tick();
        ok(root.querySelector('[data-catalog-card]')?.textContent?.includes(t('error.tools.reinstall')), 'cards expose the localized repair status');
        root.querySelector<HTMLButtonElement>('[data-catalog-card]')!.click();
        await tick();
        ok(root.querySelector('.catalog-detail')?.textContent?.includes(t('error.tools.reinstall')));
        const toggle = root.querySelector<HTMLInputElement>('[role=switch]')!;
        equal(toggle.checked, false);
        equal(toggle.disabled, true, 'a damaged copy cannot be enabled');
        ok([...root.querySelectorAll('button')].some((button) => button.textContent === t('catalog.remove')), 'the physical copy remains removable');
        equal(installs, 0, 'nothing installs by itself');
        [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('catalog.replace'))!.click();
        await tick();
        ok(root.querySelector('[data-catalog-replacement]'), 'review shows the replacement details before consent');
        equal(preparations, 1);
        equal(replacements, 0, 'review alone does not replace the copy');
        equal(installs, 0, 'repair does not use the install path');
        [...root.querySelectorAll<HTMLButtonElement>('button')].find((button) => button.textContent === t('catalog.confirmReplaceAction'))!.click();
        await tick();
        equal(replacements, 1, 'one explicit confirmation replaces the damaged copy');
        equal(installs, 0, 'repair never installs alongside the damaged copy');
        equal(root.querySelector<HTMLInputElement>('[role=switch]')!.disabled, false);
        equal(root.querySelector<HTMLInputElement>('[role=switch]')!.checked, true, 'repair restores the saved enabled choice');
        ok(!root.textContent?.includes(t('error.tools.reinstall')));
      } finally { dispose(); root.remove(); }
    }
  } finally { Object.assign(api, before); useLanguage(language); }
});

check('kits: Catalog rows stay visible during preparation and publication refreshes status without polling', async () => {
  const before = { ...api };
  const root = document.createElement('div');
  document.body.append(root);
  let listener: ((event: AppEvent) => void) | undefined;
  let registered!: (stop: () => void) => void;
  let removed = 0;
  let reads = 0;
  let current: ToolRow[] = [{ id: 'systematic-debugging', job: 'debugging', license: 'MIT', repo: '', home: '', installed: true, server: false }];
  api.toolsClearRemoved = async () => {};
  api.toolsList = async () => { reads++; return current; };
  api.on = (receive) => { listener = receive; return new Promise((resolve) => { registered = resolve; }); };
  const dispose = render(catalog, root);
  try {
    await tick();
    const card = root.querySelector<HTMLButtonElement>('[data-catalog-card]')!;
    ok(card, 'cached inventory renders while preparation and listener registration are pending');
    card.click();
    await tick();
    const toggle = root.querySelector<HTMLInputElement>('[role=switch]')!;
    toggle.focus();
    current = [{ ...current[0], status: 'error.tools.reinstall' }];
    registered(() => { removed++; });
    await tick();
    ok(root.textContent?.includes(t('error.tools.reinstall')), 'registration closes a publication missed after the initial read');
    equal(root.querySelector<HTMLInputElement>('[role=switch]')!.checked, false);
    equal(root.querySelector('.catalog-empty'), null, 'a refresh never hides the current detail behind loading');
    current = [{ ...current[0], status: undefined }];
    listener!({ type: 'catalog' });
    await tick();
    equal(root.querySelector<HTMLInputElement>('[role=switch]')!.checked, true);
    ok(!root.textContent?.includes(t('error.tools.reinstall')));
    const afterPublish = reads;
    await tick(); equal(reads, afterPublish, 'the mounted Catalog does not poll');
    dispose(); equal(removed, 1, 'closing Catalog releases its listener');
    listener!({ type: 'catalog' });
    await tick(); equal(reads, afterPublish, 'late events cannot refetch a closed Catalog');
  } finally { dispose(); root.remove(); Object.assign(api, before); }
});

check('kits: closing before Catalog listener registration releases the late subscription', async () => {
  const before = { ...api };
  const root = document.createElement('div');
  let registered!: (stop: () => void) => void;
  let removed = 0;
  let reads = 0;
  api.toolsClearRemoved = async () => {};
  api.toolsList = async () => { reads++; return []; };
  api.on = () => new Promise((resolve) => { registered = resolve; });
  const dispose = render(catalog, root);
  try {
    await tick(); dispose();
    const initial = reads;
    registered(() => { removed++; });
    await Promise.resolve(); equal(removed, 1);
    equal(reads, initial, 'a late subscription never starts another read after closure');
  } finally { dispose(); Object.assign(api, before); }
});
