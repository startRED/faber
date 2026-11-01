import { modelOptions } from '../../ui/models.ts';
import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { ModelMenu } from '../../ui/ModelMenu.tsx';
import { type Choice } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const gpt = (models: Parameters<typeof modelOptions>[1]) =>
  modelOptions(['codex'], models).filter((o) => o.choice.vendor === 'codex').map((o) => o.label);

check('models: every GPT model is listed by its own name, with no generic ChatGPT entry', () => {
  const models = [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol', isDefault: true },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna', isDefault: false },
  ];
  deepEqual(gpt(models), ['GPT-6.1 Sol', 'GPT-6 Luna']);
  ok(modelOptions(['codex'], models).every((o) => o.choice.vendor === 'auto' || o.choice.model));
});

check('models: before the account lists its models, Auto stands in for them', () => {
  deepEqual(gpt([]), []);
  deepEqual(modelOptions(['codex'], []).map((o) => o.choice.vendor), ['auto']);
});

check('models: current models include Auto profiles and the account default, leaving older ids folded', () => {
  const options = modelOptions(['claude', 'codex'], [
    { id: 'gpt-5', label: 'GPT-5', isDefault: false },
    { id: 'gpt-6.1-sol', label: 'GPT-6.1 Sol', isDefault: false },
    { id: 'gpt-6-luna', label: 'GPT-6 Luna', isDefault: false },
    { id: 'account-model', label: 'Account model', isDefault: true },
  ]);
  deepEqual(options.filter((option) => option.current).map((option) => option.choice.model), ['opus', 'sonnet', 'haiku', 'gpt-6.1-sol', 'gpt-6-luna', 'account-model']);
  equal(options.find((option) => option.choice.model === 'gpt-5')?.current, false);
});

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

check('models: grouped menu folds older models, searches partial names, and keeps model and effort in one chip', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:16px;top:12px;width:min(720px,calc(100vw - 32px))';
  root.dataset.motion = 'none';
  document.body.append(root);
  const options = modelOptions(['claude', 'codex'], [
    { id: 'gpt-5', label: 'GPT-5', isDefault: false },
    { id: 'gpt-6.1-sol', label: 'GPT-6.1 Sol', isDefault: true },
    ...Array.from({ length: 5 }, (_, index) => ({ id: `older-${index}`, label: `Older ${index}`, isDefault: false })),
  ]);
  const [choice, setChoice] = createSignal<Choice>({ vendor: 'auto' });
  const [locked, setLocked] = createSignal(false);
  const dispose = render(() => createComponent(ModelMenu, { options, get choice() { return choice(); }, get disabled() { return locked(); }, onChoice: setChoice }), root);
  const chip = () => root.querySelector<HTMLButtonElement>('.model-chip')!;
  const menu = () => root.querySelector<HTMLDivElement>('.model-menu')!;
  const button = (label: string) => [...menu().querySelectorAll<HTMLButtonElement>('.model-option')].find((item) => item.textContent === label)!;
  try {
    equal(root.querySelectorAll('.chip').length, 1);
    chip().click(); await settle();
    ok(menu().matches(':popover-open'), 'the menu uses the top layer');
    equal(menu().dataset.direction, 'down', 'opens down when there is room');
    ok(menu().getBoundingClientRect().top >= chip().getBoundingClientRect().bottom);
    ok(menu().getBoundingClientRect().right <= innerWidth, 'fits a narrow window');
    equal(menu().querySelector('.model-option')?.textContent, t('model.autoChoose'));
    deepEqual([...menu().querySelectorAll('.model-group')].map((item) => item.textContent), ['Claude', 'Codex']);
    ok(menu().querySelector('.model-group .mk.claude'));
    ok(menu().querySelector('.model-group .mk.codex'));
    const older = menu().querySelector('details')!;
    equal(older.open, false);
    ok(button('GPT-5').getClientRects().length === 0, 'older entries are initially folded');
    equal(menu().querySelector('section[aria-label="Codex"] .model-option')?.textContent, 'GPT-6.1 Sol', 'current comes before older models even if the account listed it last');
    older.querySelector('summary')!.click(); await settle();
    ok(button('GPT-5').getClientRects().length > 0);
    const search = menu().querySelector<HTMLInputElement>('input[type="search"]')!;
    search.value = 'Pt-5'; search.dispatchEvent(new Event('input', { bubbles: true }));
    equal(menu().querySelectorAll('section .model-option').length, 1);
    button('GPT-5').click(); await settle();
    deepEqual(choice(), { vendor: 'codex', model: 'gpt-5', effort: 'medium' });
    equal(root.querySelectorAll('.chip').length, 1);
    ok(chip().textContent?.includes(t('effort.medium')));
    equal(document.activeElement, chip());
    chip().click(); await settle();
    ok(menu().querySelector('details')?.open, 'reopening reveals the selected older model');
    const effort = [...menu().querySelectorAll<HTMLButtonElement>('.model-efforts button')].find((item) => item.textContent === t('effort.high'))!;
    effort.click(); equal(choice().effort, 'high');
    ok(chip().textContent?.includes(t('effort.high')));
    button('GPT-5').click(); await settle();
    equal(choice().effort, 'high', 'picking the same model preserves effort');
    chip().click(); await settle();
    button('Claude Sonnet').click(); await settle();
    equal(choice().effort, 'medium', 'a different model resets effort');
    chip().click(); await settle();
    button(t('model.autoChoose')).click(); await settle();
    deepEqual(choice(), { vendor: 'auto' });
    ok(!chip().textContent?.includes(t('effort.medium')), 'Auto has no manual effort');
    chip().click(); await settle();
    const input = menu().querySelector<HTMLInputElement>('input')!;
    input.value = 'missing'; input.dispatchEvent(new Event('input', { bubbles: true }));
    equal(menu().querySelector('[role="status"]')?.textContent, t('model.noMatches'));
    menu().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    equal(chip().getAttribute('aria-expanded'), 'false');
    equal(document.activeElement, chip());
    chip().click(); await settle(); setLocked(true);
    equal(chip().disabled, true); equal(root.querySelector('.model-menu'), null);
  } finally { dispose(); root.remove(); }
});

check('models: a small menu has no search, supports keyboard navigation and dismisses without changing choice', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:16px;bottom:12px';
  document.body.append(root);
  let picks = 0;
  const dispose = render(() => createComponent(ModelMenu, { options: modelOptions(['claude']), choice: { vendor: 'auto' }, onChoice: () => { picks++; } }), root);
  const chip = root.querySelector<HTMLButtonElement>('.model-chip')!;
  try {
    chip.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })); await settle();
    const menu = root.querySelector<HTMLDivElement>('.model-menu')!;
    equal(menu.dataset.direction, 'up', 'opens above a bottom composer');
    equal(menu.querySelector('input'), null);
    const rows = menu.querySelectorAll('button');
    equal(document.activeElement, rows[0]);
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true })); equal(document.activeElement, rows[1]);
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })); equal(document.activeElement, rows[rows.length - 1]);
    menu.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true })); equal(document.activeElement, rows[0]);
    menu.hidePopover(); await settle();
    equal(chip.getAttribute('aria-expanded'), 'false'); equal(picks, 0);
  } finally { dispose(); root.remove(); }
});
