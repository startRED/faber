import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type Project } from '../../ui/api.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { ProjectsSection } from '../../ui/ProjectSettings.tsx';
import { kept } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const demo: Project = { id: 'review-project', name: 'Review project', path: 'C:/fixture/review' };
const settle = (ms: number = 30) => new Promise((resolve) => setTimeout(resolve, ms));

// A project is an agent chat, never a mission (R160): the only screens of the Projects area are its settings (here) and the
// small "Open a project" screen (tests/ui/recents.check.ts, tests/ui/grammar.check.ts).
// The checks a project declares are confirmed in Settings, Projects.
const declared = ['npm run lint', 'cargo t'];
const proofView = (confirmed: boolean, commands = declared) => ({ commands, declared, source: 'agents' as const, confirmed, changed: false, setup: ['npm ci', 'cargo fetch'] });
const click = async (el: Element | null | undefined) => { ok(el, 'the element to click is on the screen'); (el as HTMLElement).click(); await settle(); };

// Settings, Projects: one closed entry per project; inside, one short line per option and the rest behind Learn more.
async function withProjectSettings(mock: () => void, run: (root: HTMLElement) => Promise<void>) {
  const originals = { ...api };
  mock();
  useLanguage('en');
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:900px;height:700px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectsSection, { projects: [demo, { id: 'p2', name: 'Second', path: 'C:/fixture/second' }] }), root);
  try { await settle(); await run(root); }
  finally { dispose(); root.remove(); Object.assign(api, originals); }
}

check('settings projects: entries and every Learn more start closed, and closed parts are not in the page', () => {
  let reads = 0;
  return withProjectSettings(() => { api.projectProof = async () => { reads++; return proofView(false); }; }, async (root) => {
    equal(root.querySelectorAll('.pset').length, 2, 'one entry per project');
    equal(root.querySelector('.pset-body'), null, 'every entry starts closed');
    equal(reads, 0, 'a closed entry reads nothing');
    await click(root.querySelector('.pset-head'));
    const body = root.querySelector('.pset-body')!;
    ok(body.querySelector('[data-testid="pset-checks"]')?.textContent?.includes('2 checks read from AGENTS.md · not confirmed yet'), 'the checks in one line');
    ok(body.querySelector('[data-testid="proof-commands"]'), 'unconfirmed checks show their commands: Confirm never acts on a hidden list');
    const marks = [...body.querySelectorAll<HTMLButtonElement>('.st-q')];
    ok(marks.length >= 2, 'every option keeps its long text behind its "?"');
    ok(marks.every((b) => b.getAttribute('aria-expanded') === 'false'), 'all closed');
    equal(body.querySelector('.st-more'), null, 'no long text shows until asked');
    equal([...body.querySelectorAll<HTMLButtonElement>('button.more')].some((b) => b.textContent === 'View commands'), false, 'no View while the list is pending: it is already shown');
  });
});

check('settings projects: the owner edits the checks, one command per line, and confirms the edited list', () => {
  let sent: string[] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => proofView(false);
    api.projectProofConfirm = async (_id, commands) => { sent = commands; return proofView(true, commands); };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = ['cargo fmt --check', '', '  npm test  '].join(String.fromCharCode(10));
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent.join('|'), 'cargo fmt --check|npm test', 'blank lines go and lines are trimmed');
    ok(root.querySelector('[data-testid="pset-checks"]')?.textContent?.includes('Confirmed'), 'the edited list is confirmed');
  });
});

check('settings projects: Confirm is never pressable while the list or the setup is hidden', () => {
  let proof = proofView(false);
  return withProjectSettings(() => {
    api.projectProof = async () => proof;
    api.projectProofConfirm = async (_id, commands) => { proof = proofView(true, commands); return proof; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    const body = root.querySelector('.pset-body')!;
    ok(body.querySelector('[data-testid="proof-confirm"]'), 'unconfirmed checks offer Confirm');
    ok(body.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run lint'), 'with every command on screen');
    ok(body.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('npm ci'), 'and the setup that runs first');
    await click(body.querySelector('[data-testid="proof-edit"]'));
    ok(body.querySelector('[data-testid="proof-save"]'), 'editing offers Save');
    ok(body.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('cargo fetch'), 'with the setup on screen beside the draft');
    await click([...body.querySelectorAll('button')].find((b) => b.textContent === 'Cancel'));
    await click(body.querySelector('[data-testid="proof-confirm"]'));
    equal(body.querySelector('[data-testid="proof-confirm"]'), null, 'confirmed: no Confirm');
    equal(body.querySelector('[data-testid="proof-commands"]'), null, 'and the calm view folds the list');
    await click([...body.querySelectorAll('button.more')].find((b) => b.textContent === 'View commands'));
    ok(body.querySelector('[data-testid="proof-commands"]'), 'View shows it again');
  });
});

check('settings projects: a refusal swaps the draft for the refreshed list and setup before Save is possible again', () => {
  let current = proofView(false);
  let sent: string[][] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => current;
    api.projectProofConfirm = async (_id, commands, setup, shown) => {
      if (shown.join('|') !== current.declared.join('|')) throw 'reason.proof.declaredChanged';
      sent = [commands, setup, shown];
      return proofView(true, commands);
    };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = 'draft A';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    current = { ...proofView(false, ['npm run evil']), declared: ['npm run evil'], setup: ['npm ci --ignore-scripts=false'] };
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent.length, 0, 'the refused save recorded nothing');
    equal(root.querySelector('[data-testid="proof-save"]'), null, 'Save is gone after a refusal');
    equal(root.querySelector('[data-testid="proof-draft"]'), null, 'and so is the stale draft');
    ok(root.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run evil'), 'the refreshed list is on screen');
    ok(root.querySelector('[data-testid="proof-setup"]')?.textContent?.includes('ignore-scripts'), 'with the refreshed setup');
    await click(root.querySelector('[data-testid="proof-edit"]'));
    equal(root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!.value, 'npm run evil', 'Edit starts again from the refreshed list');
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(sent[2].join('|'), 'npm run evil', 'what is saved is the declaration that was shown');
    equal(sent[1].join('|'), 'npm ci --ignore-scripts=false', 'with the setup that was shown');
  });
});

check('settings projects: a refusal for invalid commands that comes with a moved declaration still drops the draft', () => {
  let current = proofView(false);
  return withProjectSettings(() => {
    api.projectProof = async () => current;
    api.projectProofConfirm = async () => { throw 'reason.proof.command'; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    await click(root.querySelector('[data-testid="proof-edit"]'));
    const box = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    box.value = 'npm test && npm run lint';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await settle();
    current = { ...proofView(false, ['npm run evil']), declared: ['npm run evil'] };
    await click(root.querySelector('[data-testid="proof-save"]'));
    equal(root.querySelector('[data-testid="proof-draft"]'), null, 'the draft is gone, whatever the error was');
    ok(root.querySelector('[data-testid="proof-commands"]')?.textContent?.includes('npm run evil'), 'the new list is on screen');
    equal(kept('proof.' + demo.id), '', 'and the stale draft is not kept for a reopen');
  });
});

check('settings projects: the separate copies switch is on by default and turns off for this project (R201)', () => {
  let saved: unknown[] = [];
  return withProjectSettings(() => {
    api.projectProof = async () => proofView(true);
    api.projectCopies = async (...args) => { saved = args; };
  }, async (root) => {
    await click(root.querySelector('.pset-head'));
    const box = root.querySelector<HTMLInputElement>(`#pset-${demo.id}-copies-switch`)!;
    ok(box.checked, 'on by default');
    box.click();
    await settle();
    equal(JSON.stringify(saved), JSON.stringify([demo.id, false]));
    ok(!box.checked, 'the switch stays off');
  });
});
