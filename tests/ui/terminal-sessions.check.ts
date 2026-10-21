import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import type { CliSession, Project } from '../../ui/api.ts';
import { t, useLanguage } from '../../ui/i18n.ts';
import { Welcome } from '../../ui/Welcome.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

// Parity F5: the project home lists the sessions started in a terminal in the project's folder, only when asked, and a row
// opens one as a conversation.

const project: Project = { id: 'p1', name: 'app', path: '/work/app' } as Project;
const session = (id: string, title: string): CliSession => ({ vendor: 'codex', id, title, updatedAt: Date.now() });
const settle = () => new Promise((done) => setTimeout(done, 0));

function home(terminal?: { list: (projectId: string) => Promise<CliSession[]>; onPick: (projectId: string, s: CliSession) => void }, inProject = true) {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(
    () =>
      createComponent(Welcome, {
        recent: [],
        running: [],
        projects: [project],
        project: inProject ? project : undefined,
        onSuggest: () => {},
        onOpen: () => {},
        terminal,
        get children() {
          return document.createElement('div');
        },
      }),
    root,
  );
  return { root, done: () => (dispose(), root.remove()) };
}

check('terminal sessions: asked for only on a click, then a row opens the session', async () => {
  useLanguage('en');
  let asked = 0;
  let answer!: (list: CliSession[]) => void;
  const picked: CliSession[] = [];
  const list = (projectId: string) => (asked++, equal(projectId, 'p1'), new Promise<CliSession[]>((done) => (answer = done)));
  const { root, done } = home({ list, onPick: (projectId, s) => (equal(projectId, 'p1'), picked.push(s)) });
  try {
    equal(asked, 0, 'nothing starts the CLI before the person asks');
    const open = root.querySelector<HTMLButtonElement>('.from-terminal')!;
    ok(open, 'one quiet button');
    equal(open.textContent, t('s1.terminal'));
    equal(open.title, t('s1.terminal.hint'));
    open.click();
    equal(asked, 1);
    const section = root.querySelector<HTMLElement>('.resume.terminal')!;
    equal(section.getAttribute('aria-busy'), 'true');
    equal(section.querySelector('[role=status]')?.textContent, t('aside.looking'));
    answer([session('t1', 'Fix the login form'), session('t2', '')]);
    await settle();
    equal(section.getAttribute('aria-busy'), null);
    equal(section.querySelector('h2')?.textContent, t('s1.terminal'));
    const rows = [...section.querySelectorAll<HTMLButtonElement>('.rrow')];
    deepEqual(rows.map((r) => r.querySelector('.t')?.textContent), ['Fix the login form', t('run.untitled')]);
    ok(rows[0]!.querySelector('.s')?.textContent?.startsWith('Codex · '), 'the CLI and when');
    rows[0]!.click();
    deepEqual(picked.map((s) => s.id), ['t1']);
  } finally {
    done();
  }
});

check('terminal sessions: none found says so, a failure reads as none, and Chat or no Codex shows no button', async () => {
  useLanguage('en');
  for (const list of [() => Promise.resolve([]), () => Promise.reject(new Error('gone'))]) {
    const { root, done } = home({ list, onPick: () => {} });
    try {
      root.querySelector<HTMLButtonElement>('.from-terminal')!.click();
      await settle();
      equal(root.querySelector('.resume.terminal [role=status]')?.textContent, t('s1.terminal.none'));
      equal(root.querySelectorAll('.resume.terminal .rrow').length, 0);
    } finally {
      done();
    }
  }
  for (const [terminal, inProject] of [[undefined, true], [{ list: () => Promise.resolve([]), onPick: () => {} }, false]] as const) {
    const { root, done } = home(terminal, inProject);
    try {
      equal(root.querySelector('.from-terminal'), null);
    } finally {
      done();
    }
  }
});
