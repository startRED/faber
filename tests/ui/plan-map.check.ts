import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message, type Plan } from '../../ui/api.ts';
import { DetailsPane } from '../../ui/Details.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { check, equal, ok } from './check.ts';

const picture = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';
const shared = (): Plan => {
  const at = Date.now();
  return { at, steps: [{ text: 'Internal work', state: 'active' }], parts: [
    { id: 'search', area: 'Busca', name: 'Digitar na busca', state: 'done', removed: false, since: at - 9 * 60_000, pictures: ['old', 'new'], evidence: 'Typed pizza, list showed 3 results' },
    { id: 'open', area: 'Busca', name: 'Abrir a conversa', state: 'now', removed: false, since: at - 2 * 60_000, pictures: [], example: '"pizza" → looked at 40, found 3' },
    { id: 'back', area: 'Busca', name: 'Voltar sem perder a busca', state: 'missing', removed: false, since: at, pictures: [] },
  ] };
};

function mount(initial: Plan, running = false, onPlanComparison?: (selection: { before: string; after: string }) => void) {
  const root = document.createElement('div');
  document.body.append(root);
  const [plan, setPlan] = createSignal(initial);
  const messages: Message[] = [
    { id: 'u1', role: 'user', text: 'Improve search', at: 1 },
    { id: 'a1', role: 'assistant', text: 'Search update.', at: 2, ...(running ? { status: 'streaming' } : {}) },
  ];
  const dispose = render(() => createComponent(Thread, {
    messages, area: 'project', conversationId: 'plan-map-test', from: 0, options: [], onBranch: () => {},
    get plan() { return plan(); }, onPlanComparison,
  }), root);
  return { root, setPlan, close: () => { dispose(); root.remove(); } };
}

check('plan-map: the newest reply shows current work, missing work and proof in one click', async () => {
  const original = api.sitePicture;
  const requested: string[] = [];
  api.sitePicture = async (_id, call) => { requested.push(call); return picture; };
  const comparisons: Array<{ before: string; after: string }> = [];
  const view = mount(shared(), true, (comparison) => comparisons.push(comparison));
  try {
    equal(view.root.querySelector('.plan-map-part[data-state="now"] strong')?.textContent, 'Abrir a conversa');
    equal(view.root.querySelector('.plan-map-part[data-state="missing"] strong')?.textContent, 'Voltar sem perder a busca');
    ok(view.root.querySelector('.plan-map-live')?.textContent?.includes('9 min'), 'last time a part started working');
    equal(view.root.querySelector('.plan'), null, 'internal plan is behind technical details');
    view.root.querySelector<HTMLButtonElement>('.plan-map-proof-toggle')!.click();
    equal(view.root.querySelector('.plan-map-proof p')?.textContent, 'Typed pizza, list showed 3 results');
    await Promise.resolve();
    await Promise.resolve();
    equal(view.root.querySelector<HTMLImageElement>('.plan-map-proof img')?.src, picture);
    equal(requested.join(','), 'new', 'proof and thumbnail share the newest picture');
    equal(document.activeElement, view.root.querySelector('.plan-map-proof'), 'the proof is in keyboard focus');
    view.root.querySelector<HTMLButtonElement>('.plan-map-compare')!.click();
    equal(comparisons[0]?.before, 'old');
    equal(comparisons[0]?.after, 'new');
    const plan = shared();
    view.setPlan({ ...plan, parts: plan.parts!.map((part) => ({ ...part, pictures: part.id === 'search' ? ['old', 'old-s0'] : [] })) });
    equal(view.root.querySelector('.plan-map-compare'), null, 'sizes of one capture cannot open a comparison');
    view.setPlan({ ...plan, parts: plan.parts!.map((part) => ({ ...part, before: 'earlier', pictures: part.id === 'search' ? ['new'] : [] })) });
    view.root.querySelector<HTMLButtonElement>('.plan-map-compare')!.click();
    equal(comparisons[1]?.before, 'earlier');
    equal(comparisons[1]?.after, 'new');
  } finally {
    view.close();
    api.sitePicture = original;
  }
});

check('plan-map: folded areas and removed parts can be opened in a narrow conversation', () => {
  const plan = shared();
  const finished = { ...plan.parts![0]!, pictures: [] };
  const view = mount({ ...plan, parts: [
    ...Array.from({ length: 7 }, (_, n) => ({ ...finished, id: `a${n}`, area: `Finished ${n}` })),
    ...plan.parts!.slice(1),
    { ...finished, id: 'removed', area: 'Busca', removed: true, why: 'The request changed' },
  ] });
  view.root.style.width = '350px';
  try {
    equal(view.root.querySelector('.plan-map-panel h3')?.textContent, 'Inside Busca');
    equal(view.root.querySelectorAll('.plan-map-areas .plan-map-area').length, 2);
    equal(getComputedStyle(view.root.querySelector('.plan-map-blocks')!).flexDirection, 'column');
    ok(view.root.scrollWidth <= view.root.clientWidth, 'the map fits the available width');
    equal(view.root.querySelector('.plan-map-part[data-state="removed"]'), null);
    view.root.querySelector<HTMLButtonElement>('.plan-map-more')!.click();
    ok(view.root.querySelector('.plan-map-part[data-state="removed"]')?.textContent?.includes('The request changed'));
    view.root.querySelector<HTMLButtonElement>('.plan-map-area-pile')!.click();
    view.root.querySelector<HTMLButtonElement>('.plan-map-folded button')!.click();
    equal(view.root.querySelector('.plan-map-panel h3')?.textContent, 'Inside Finished 0');
  } finally { view.close(); }
});

check('plan-map: the internal plan can still be read in Technical details', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(DetailsPane, {
    conversationId: 'plan-map-details-test', messages: [], background: [], plan: shared(),
    changes: { id: 'plan-map-details-test', files: [], loading: false },
    onBack: () => {}, onClose: () => {},
  }), root);
  try {
    ok(root.querySelector('.plan li')?.textContent?.includes('Internal work'));
    equal(root.querySelector('.plan-head'), null, 'opening Technical details is enough to show the steps');
  } finally { dispose(); root.remove(); }
});

check('plan-map: the person still sees a risk notice with no technical steps', () => {
  const view = mount({ ...shared(), steps: [], size: 'risky', raisedBecause: 'publish' }, true);
  try {
    ok([...view.root.querySelectorAll('[role="status"]')].some((node) => node.textContent?.includes('publishes or changes shared history')));
    ok(view.root.querySelector('.plan-map'));
  } finally { view.close(); }
});

check('plan-map: a parts-only update shows why working search came back', () => {
  const plan = shared();
  const view = mount(plan, true);
  try {
    view.setPlan({ ...plan, steps: [], parts: plan.parts!.map((part) => part.id === 'search'
      ? { ...part, state: 'redo', why: 'Total was wrong going from 1 to 2' }
      : part) });
    ok(view.root.querySelector('.plan-map-part[data-state="redo"]')?.textContent?.includes('Worked · being redone'));
    ok(view.root.querySelector('.plan-map-hanging')?.textContent?.includes('Total was wrong going from 1 to 2'));
  } finally { view.close(); }
});

check('plan-map: a reopened parts-only conversation still shows the map when all parts work', () => {
  const plan = shared();
  const view = mount({ ...plan, steps: [], parts: plan.parts!.map((part) => ({ ...part, state: 'done', pictures: [], evidence: 'Search works' })) });
  try {
    equal(view.root.querySelector('.plan-map-summary')?.textContent, '3 of 3 parts working');
    equal(view.root.querySelector('.plan-map-areas'), null, 'one area has no strip');
    equal(view.root.querySelector('.plan-map-live'), null, 'a reopened idle reply has no running clock');
    view.root.querySelector<HTMLButtonElement>('.plan-map-pile button')!.click();
    equal(view.root.querySelectorAll('.plan-map-list .plan-map-part').length, 2, 'see the two parts folded behind the latest');
  } finally { view.close(); }
});
