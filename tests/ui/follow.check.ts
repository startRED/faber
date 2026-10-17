import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Thread } from '../../ui/Thread.tsx';
import { api, type Follow, type Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

// R200: the pull request a reply follows is one quiet line under it, and a message Faber sent by itself is a quiet line in
// place of the person's bubble.
const question: Message = { id: 'q', role: 'user', text: 'Open a pull request for the fix', at: 0 };
const follow = (extra: Partial<Follow>): Follow => ({ repo: 'acme/app', number: 93, url: 'https://github.com/acme/app/pull/93', state: 'pending', since: 0, ...extra });
const answer = (id: string, extra: Partial<Message>): Message => ({ id, role: 'assistant', text: 'Opened it.', at: 1, status: 'done', vendor: 'claude', model: 'claude-sonnet-5-5', ...extra });

function mount(messages: Message[]) {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Thread, { messages, from: 0, conversationId: 'follow-check', options: [], onBranch: () => {} }), root);
  return { root, close: () => { dispose(); root.remove(); } };
}

check('follow: the followed pull request is one line under its reply, with one way to stop while it is open', async () => {
  const stopped: string[] = [];
  const opened: string[] = [];
  const before = { unfollowPr: api.unfollowPr, openLink: api.openLink };
  api.unfollowPr = async (id: string) => void stopped.push(id);
  api.openLink = async (url: string) => void opened.push(url);
  try {
    const pending = mount([question, answer('a', { follow: follow({}) })]);
    try {
      const line = pending.root.querySelector<HTMLElement>('.follow')!;
      ok(line, 'the reply shows the pull request it follows');
      equal(line.querySelector('a')!.textContent, t('follow.pending', { pr: '#93', checks: '' }));
      ok(!line.classList.contains('need'), 'following is quiet, never amber');
      const stop = line.querySelector<HTMLButtonElement>('button')!;
      equal(stop.textContent, t('follow.stop'));
      stop.click();
      line.querySelector('a')!.click();
      deepEqual([stopped, opened], [['follow-check'], ['https://github.com/acme/app/pull/93']]);
    } finally {
      pending.close();
    }

    const held = mount([question, answer('a', { follow: follow({ state: 'failing', failed: ['gate', 'rust'], held: true }) })]);
    try {
      const line = held.root.querySelector<HTMLElement>('.follow')!;
      ok(line.classList.contains('need'), 'checks that wait for the person are amber');
      equal(line.querySelector('a')!.textContent, t('follow.held', { pr: '#93', checks: 'gate, rust' }));
    } finally {
      held.close();
    }

    for (const [state, key] of [['merged', 'follow.merged'], ['passing', 'follow.passing'], ['stopped', 'follow.stopped']] as const) {
      const ended = mount([question, answer('a', { follow: follow({ state }) })]);
      try {
        equal(ended.root.querySelector('.follow a')!.textContent, t(key, { pr: '#93', checks: '' }));
        equal(ended.root.querySelector('.follow button'), null, `nothing to stop once ${state}`);
      } finally {
        ended.close();
      }
    }
    const quiet = mount([question, answer('a', { follow: follow({ state: 'stopped', problem: 'follow.quiet' }) })]);
    try {
      equal(quiet.root.querySelector('.follow a')!.textContent, t('follow.quiet', { pr: '#93', checks: '' }), 'an early end says why');
    } finally {
      quiet.close();
    }
  } finally {
    Object.assign(api, before);
  }
});

check('follow: a message Faber sent by itself is a quiet line, not the person\'s bubble', () => {
  const sent = 'Checks failed on pull request #93 (acme/app) at commit aaa:\n- gate\nRead why, fix it, push.';
  const wake: Message = { id: 'w', role: 'user', text: sent, at: 2, wake: { why: 'wake.checksFailed', pr: 93, checks: ['gate'] } };
  const view = mount([question, answer('a', {}), wake, answer('b', { status: 'streaming', text: '' })]);
  try {
    ok(!view.root.textContent!.includes('Read why, fix it'), 'the words the agent got are not shown as the person\'s');
    const line = view.root.querySelector<HTMLElement>('.woke')!;
    equal(line.textContent!.trim(), t('wake.checksFailed', { pr: '#93', checks: 'gate' }));
    equal(line.title, sent, 'the words the agent got are one hover away');
  } finally {
    view.close();
  }
});
