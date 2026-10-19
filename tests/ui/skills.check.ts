import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { SkillsCard } from '../../ui/Skills.tsx';
import { check, equal, ok } from './check.ts';

check('skills: one closed card says how many are on and opens to one switch per skill, each with its words and its licence', async () => {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => createComponent(SkillsCard, {}), root);
  try {
    for (let i = 0; i < 100 && !root.querySelector('.pset-head'); i++) await new Promise((r) => setTimeout(r, 20));
    const head = root.querySelector<HTMLButtonElement>('.pset-head')!;
    equal(head.getAttribute('aria-expanded'), 'false');
    equal(root.querySelector('[role=switch]'), null, 'closed, the card is one row');
    ok(/^0\D+\d+/.test(head.querySelector('.s')?.textContent ?? ''), `it counts what is on: ${head.textContent}`);
    head.click();
    await new Promise((r) => setTimeout(r, 20));
    const switches = [...root.querySelectorAll<HTMLInputElement>('input[role=switch]')];
    ok(switches.length >= 8, `the shipped catalog lists its skills (${switches.length})`);
    ok(switches.every((s) => !s.checked), 'a fresh test profile has nothing installed');
    const rows = [...root.querySelectorAll('.st-opt')];
    for (const row of rows) {
      const hint = row.querySelector('.st-hint')?.textContent ?? '';
      ok(/ · (MIT|Apache-2\.0)$/.test(hint), `the licence closes the line: ${hint}`);
      ok(!/tools\./.test(hint), `no raw key: ${hint}`);
    }
    equal(root.querySelector('[data-opt="set-skill-systematic-debugging"] .st-name')?.textContent?.trim(), 'systematic-debugging?');
  } finally {
    dispose();
    root.remove();
  }
});
