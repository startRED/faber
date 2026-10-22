import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from '../../ui/api.ts';
import { Catalog, forgetCatalogView } from '../../ui/Catalog.tsx';
import { catalogName, t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

async function catalog() {
  const clearRemoved = api.toolsClearRemoved;
  api.toolsClearRemoved = async () => {};
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  forgetCatalogView();
  const dispose = render(() => createComponent(Catalog, { projects: [], vendors: [] }), root);
  // The real list came after more than 2 s on a slow macOS runner, and a later check there took 33.9 s (run 39730300698); `tools_list` takes no lock, so the cause is not known.
  for (const start = Date.now(); !root.querySelector('[data-catalog-card]') && Date.now() - start < 45_000; ) await new Promise((resolve) => setTimeout(resolve, 50));
  const filter = (label: string) => {
    const button = [...root.querySelectorAll<HTMLButtonElement>('.catalog-filters button')].find((el) => el.textContent === label);
    ok(button, `catalog filter ${label}`); button.click();
  };
  return { root, filter, close: () => { dispose(); root.remove(); api.toolsClearRemoved = clearRemoved; } };
}

check('skills: the shipped catalog has translated descriptions, licences and an install action for each skill', async () => {
  const view = await catalog();
  try {
    view.filter(t('catalog.skills'));
    const cards = [...view.root.querySelectorAll<HTMLButtonElement>('[data-catalog-card]')];
    ok(cards.length >= 8, `the shipped catalog lists its skills (${cards.length})`);
    for (const card of cards) {
      ok(/(?:MIT|Apache-2\.0|BSD-3-Clause)$/.test(card.querySelector('.catalog-origin')?.textContent ?? ''), `the licence is visible: ${card.textContent}`);
      const description = card.querySelector('.catalog-description')?.textContent ?? '';
      ok(description && description !== card.dataset.catalogCard && !description.includes('tools.'), `translated description: ${description}`);
      ok(card.textContent?.includes(t('catalog.notInstalled')), 'a fresh test profile has nothing installed');
    }
    const debugging = cards.find((card) => card.dataset.catalogCard === 'systematic-debugging');
    ok(debugging); debugging.click();
    equal(view.root.querySelector('.catalog-detail h1')?.textContent, catalogName('systematic-debugging'));
    ok([...view.root.querySelectorAll('button')].some((el) => el.textContent === t('catalog.install')));
    equal(view.root.querySelector('[role=switch]'), null, 'installing and enabling are separate actions');
  } finally { view.close(); }
});

check('skills: the servers filter uses the same catalog and shows the pinned server source and licence', async () => {
  const view = await catalog();
  try {
    view.filter(t('catalog.servers'));
    const shipped = (await api.toolsList()).filter((item) => item.server);
    const cards = [...view.root.querySelectorAll<HTMLButtonElement>('[data-catalog-card]')];
    equal(cards.length, shipped.length);
    ok(cards.length > 0, 'the shipped catalog has a server');
    for (const card of cards) {
      const item = shipped.find((row) => row.id === card.dataset.catalogCard);
      ok(item, 'no skill appears in the servers filter');
      ok(card.querySelector('.catalog-origin')?.textContent?.endsWith(item.license));
    }
    cards.find((card) => card.dataset.catalogCard === 'chrome-devtools')!.click();
    ok(view.root.querySelector('.catalog-detail dl')?.textContent?.includes('Apache-2.0'));
    equal(view.root.querySelector('.catalog-version')?.textContent, shipped.find((item) => item.id === 'chrome-devtools')?.version);
  } finally { view.close(); }
});
