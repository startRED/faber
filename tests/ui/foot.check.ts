import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { Message, RunningItem } from '../../ui/api.ts';
import { footText, setAgeNow } from '../../ui/foot.ts';
import { t } from '../../ui/i18n.ts';
import { shownRunning, stopOffered } from '../../ui/running.ts';
import { Thread } from '../../ui/Thread.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

const MIN = 60_000;
const START = 1_700_000_000_000;
const reply = (over: Partial<Message> = {}): Message => ({ id: 'r', role: 'assistant', at: START, status: 'streaming', text: 'Done.', ...over });
const row = (over: Partial<RunningItem> = {}): RunningItem => ({ conversationId: 'c1', turnId: 'r', title: 'A', area: 'project', state: 'working', since: START, permissions: [], ...over });

check('foot: a reply says plainly whether the agent is at work or finished, and how long ago it last changed', () => {
  const working = reply({ movedAt: START + 5 * MIN });
  equal(footText(working, true, START + 5 * MIN + 30_000), `${t('foot.working')} · ${t('foot.justNow')}`);
  equal(footText(working, true, START + 8 * MIN), `${t('foot.working')} · ${t('foot.updated', { time: '3 min' })}`, 'a reply that went quiet says for how long');
  // The agent finished but the CLI has not closed its turn: it reads as done, with the work that took 12 minutes.
  const finished = reply({ movedAt: START + 12 * MIN, finishedAt: START + 12 * MIN });
  equal(footText(finished, true, START + 15 * MIN), `${t('foot.done')} · ${t('foot.work', { time: '12 min' })} · ${t('foot.updated', { time: '3 min' })}`);
  equal(footText({ ...finished, status: 'done' }, true, START + 15 * MIN), footText(finished, true, START + 15 * MIN), 'the turn closing changes nothing the person reads');
});

check('foot: an older reply keeps its state and work but shows no age; a short one and an old one say nothing more', () => {
  const finished = reply({ status: 'done', movedAt: START + 12 * MIN, finishedAt: START + 12 * MIN });
  equal(footText(finished, false, START + 15 * MIN), `${t('foot.done')} · ${t('foot.work', { time: '12 min' })}`);
  equal(footText({ ...finished, finishedAt: START + 20_000, movedAt: START + 20_000 }, false, START + 15 * MIN), t('foot.done'), 'a short answer says no work');
  equal(footText(finished, true, START + 26 * 3_600_000), `${t('foot.done')} · ${t('foot.work', { time: '12 min' })}`, 'past a day the sidebar tells the age');
  equal(footText(reply({ status: 'done' }), true, START + MIN), '', 'a reply from before the times were kept says nothing');
  equal(footText(reply({ status: 'error', movedAt: START }), true, START), '', 'an error has its own note');
});

check('foot: in a thread the newest reply reads working, then done the moment the agent finishes, and its age moves with the clock', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>(reply({ at: Date.now(), movedAt: Date.now() }));
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'c1', options: [], onBranch: () => {} }), root);
  const foot = () => root.querySelector<HTMLElement>('.ai .foot')?.textContent;
  try {
    equal(foot(), `${t('foot.working')} · ${t('foot.justNow')}`);
    equal(root.querySelector('.ai .actions'), null, 'no copy or retry while it works: the finished row is not there yet');
    ok(root.querySelector('.ai .cursor'), 'the cursor blinks while it works');
    // The agent says all it will say; the CLI is still closing its turn (the status stays streaming).
    const finishedAt = Date.now();
    setMessage(reply({ at: message().at, movedAt: finishedAt, finishedAt }));
    equal(foot(), `${t('foot.done')} · ${t('foot.justNow')}`);
    equal(message().status, 'streaming');
    equal(root.querySelector('.ai .cursor'), null, 'the cursor stops');
    ok(root.querySelector('.ai .actions .iconbtn'), 'copy and retry are there');
    setAgeNow(finishedAt + 3 * MIN);
    equal(foot(), `${t('foot.done')} · ${t('foot.updated', { time: '3 min' })}`);
    setAgeNow(finishedAt + 4 * MIN);
    equal(foot(), `${t('foot.done')} · ${t('foot.updated', { time: '4 min' })}`);
    // More words after it mean it was not done.
    setMessage(reply({ at: message().at, movedAt: finishedAt + 4 * MIN, finishedAt: undefined }));
    equal(foot(), `${t('foot.working')} · ${t('foot.justNow')}`);
  } finally {
    dispose();
    root.remove();
  }
});

check('running: a finished reply leaves what the screen shows as running and the composer reads Send, while the engine keeps its row', () => {
  const rows = [row({ settled: true }), row({ conversationId: 'c2' })];
  deepEqual(shownRunning(rows).map((i) => i.conversationId), ['c2']);
  equal(rows.length, 2, 'the whole list stays whole for updates');
  equal(stopOffered(true, rows, 'c1'), false, 'finished: Send');
  equal(stopOffered(true, rows, 'c2'), true, 'at work: Stop');
  equal(stopOffered(false, rows, 'c2'), false, 'idle: Send');
});
