import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type Project } from '../../ui/api.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { ProjectsSection } from '../../ui/ProjectSettings.tsx';
import { Sidebar } from '../../ui/Sidebar.tsx';
import { Welcome } from '../../ui/Welcome.tsx';
import { keep, kept } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const demo: Project = { id: 'review-project', name: 'Review project', path: 'C:/fixture/review' };
const settle = (ms: number = 30) => new Promise((resolve) => setTimeout(resolve, ms));

// A project is an agent chat, never a mission (R160): the only screens of the Projects area are its settings (here) and the
// small "Open a project" screen (tests/ui/recents.check.ts, tests/ui/grammar.check.ts).
// The checks a project declares are confirmed in Settings, Projects.
const declared = ['npm run lint', 'cargo t'];
const proofView = (confirmed: boolean, commands = declared) => ({ commands, declared, source: 'agents' as const, confirmed, changed: false, setup: ['npm ci', 'cargo fetch'] });
const click = async (el: Element | null | undefined) => { ok(el, 'the element to click is on the screen'); (el as HTMLElement).click(); await settle(); };

check('settings projects: refreshing a project keeps its open controls, draft and focus with its identity', async () => {
  const original = api.projectProof;
  api.projectProof = async () => proofView(true);
  useLanguage('en');
  const first: Project = { id: 'before-review', name: 'Before', path: 'C:/fixture/before' };
  const [projects, setProjects] = createSignal([first, { ...demo }]);
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(() => createComponent(ProjectsSection, { get projects() { return projects(); } }), root);
  try {
    await click(root.querySelector('[data-project="review-project"] .pset-head'));
    await click(root.querySelector('[data-project="review-project"] [data-testid="proof-edit"]'));
    const draft = root.querySelector<HTMLTextAreaElement>('[data-testid="proof-draft"]')!;
    draft.value = 'npm run unsent-review';
    draft.dispatchEvent(new Event('input', { bubbles: true }));
    draft.focus();
    setProjects((all) => all.map((p) => ({ ...p, name: `${p.name} refreshed`, separateCopies: false })));
    await settle();
    equal(root.querySelector('[data-testid="proof-draft"]'), draft, 'saving a project keeps the actual open editor');
    equal(document.activeElement, draft, 'refreshing project data retains keyboard focus');
    equal(draft.value, 'npm run unsent-review', 'the unsent edit stays intact');
    equal(root.querySelector<HTMLInputElement>('#pset-review-project-copies-switch')?.checked, false, 'kept controls still receive saved preference changes');
    ok(root.querySelector('[data-project="review-project"] .pset-head')?.textContent?.includes('refreshed'), 'the existing row still receives updated project data');
    setProjects((all) => all.filter((p) => p.id !== first.id));
    await settle();
    equal(root.querySelector('[data-testid="proof-draft"]'), draft, 'removing an earlier project does not move the editor to another record');
    equal(root.querySelector('[data-project="before-review"]'), null, 'removed project is released');
  } finally { dispose(); root.remove(); keep(`proof.${demo.id}`, ''); api.projectProof = original; }
});

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

check('projects recovery_320: a moved or missing project folder offers Find the folder and Remove from Faber, and an unclean exit shows one calm home note', async () => {
  useLanguage('en');
  const missing = { id: 'p-moved', name: 'Moved repo', path: 'C:/fixture/moved', state: 'missing' as const } as Project;
  const disconnected = { id: 'p-drive', name: 'USB repo', path: 'E:/fixture/usb', state: 'disconnected' as const } as Project;
  const [unclean, setUnclean] = createSignal<{ at: number; panic: boolean } | undefined>({ at: 1000, panic: true });
  const [active, setActive] = createSignal<Project>(missing);
  const actions: string[] = [];

  const root = document.createElement('div');
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  const dispose = render(
    () => [
      createComponent(Sidebar, {
        conversations: [{ id: 'c1', title: 'Existing work', area: 'project', projectId: missing.id, updatedAt: 1000 }],
        projects: [missing, disconnected],
        vendors: ['claude'],
        version: '0.1.0',
        place: { area: 'project', projectId: missing.id },
        running: [],
        quotas: {},
        now: 1000,
        onNew: () => {},
        onArea: () => {},
        onProject: () => {},
        onNewIn: () => {},
        onAddProject: () => {},
        onOpen: () => {},
        onRemove: () => {},
        onSettings: () => {},
        ...({
          onFindProjectFolder: (id: string) => actions.push(`side-find:${id}`),
          onRemoveProject: (id: string) => actions.push(`side-remove:${id}`),
        } as Record<string, unknown>),
      }),
      createComponent(Welcome, {
        recent: [{ id: 'c1', title: 'Existing work', area: 'project', projectId: missing.id, updatedAt: 1000 }],
        running: [],
        projects: [missing, disconnected],
        get project() {
          return active();
        },
        onSuggest: () => {},
        onOpen: () => {},
        get children() {
          return null;
        },
        get uncleanExit() {
          return unclean();
        },
        onDismissUncleanExit: () => {
          actions.push('dismiss-crash');
          setUnclean(undefined);
        },
        onOpenLogFolder: () => actions.push('open-logs'),
        onFindProjectFolder: (id: string) => actions.push(`home-find:${id}`),
        onRemoveProject: (id: string) => actions.push(`home-remove:${id}`),
      }),
    ],
    root,
  );
  try {
    await settle();
    const sideRow = root.querySelector('[data-project-id="p-moved"] .proj-missing');
    ok(sideRow, 'the sidebar row of a missing project explains that the folder is missing');
    ok(sideRow?.textContent?.includes('C:/fixture/moved'), 'the sidebar names the missing folder');
    await click(sideRow?.querySelector('[data-action="find-folder"]'));
    await click(sideRow?.querySelector('[data-action="remove-project"]'));

    const card = root.querySelector('[data-testid="project-missing"]');
    ok(card, 'the project home shows the missing-folder explanation');
    ok(card?.textContent?.includes('The folder C:/fixture/moved is missing or was moved.'), 'explains the missing folder');
    ok(card?.textContent?.includes('Find the folder'), 'offers Find the folder');
    ok(card?.textContent?.includes('Remove from Faber'), 'offers Remove from Faber');
    await click(card?.querySelector('[data-action="find-folder"]'));
    await click(card?.querySelector('[data-action="remove-project"]'));

    setActive(disconnected);
    await settle();
    ok(
      root.querySelector('[data-testid="project-missing"]')?.textContent?.includes('The drive for E:/fixture/usb is not connected right now.'),
      'a disconnected drive says the drive is not connected right now',
    );

    const crashNote = root.querySelector('[data-testid="crash-note"]');
    ok(crashNote, 'an unclean exit shows one calm home note');
    ok(crashNote?.textContent?.includes('Faber did not finish closing last time. Your conversations and projects are safe.'), 'calm crash note words');
    ok(crashNote?.textContent?.includes(new Date(1000).toLocaleString('en', { dateStyle: 'medium', timeStyle: 'short' })), 'shows the localized time');
    ok(crashNote?.textContent?.includes('A panic was recorded.'), 'says a panic was recorded');
    setUnclean({ at: 1000, panic: false });
    await settle();
    ok(crashNote?.textContent?.includes('No panic was recorded.'), 'says when no panic was recorded');
    ok(crashNote?.textContent?.includes('Open the log folder'), 'offers Open the log folder');
    await click(crashNote?.querySelector('[data-action="open-logs"]'));
    await click(crashNote?.querySelector('[data-action="dismiss-crash"]'));
    equal(root.querySelector('[data-testid="crash-note"]'), null, 'dismissing removes the calm crash note');
    equal(
      actions.join('|'),
      'side-find:p-moved|side-remove:p-moved|home-find:p-moved|home-remove:p-moved|open-logs|dismiss-crash',
      'every recovery action triggers its handler',
    );
  } finally {
    dispose();
    root.remove();
  }
});
