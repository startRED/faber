import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

check('thread: ten tool steps and narration fold into one activity row above the final answer', async () => {
  const root = document.createElement('div');
  document.body.append(root);
  const narration = 'Checking the files.\n\nRunning the checks.\n\n';
  const final = 'The requested change is complete.\n\nAll checks passed.';
  const [message, setMessage] = createSignal<Message>({
    id: 'activity-check', role: 'assistant', at: 0, status: 'streaming', text: narration + final,
    textBreaks: ['Checking the files.\n\n'.length, narration.length], activityEnd: narration.length, thinking: 'Consider the simplest change.',
    tools: Array.from({ length: 10 }, (_, i) => ({ id: String(i), name: 'Bash', input: { command: `check ${i}` }, ok: i < 9 ? true : undefined })),
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'activity-check', options: [], onBranch: () => {},
  }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('.activity')!;
    equal(root.querySelectorAll('.activity').length, 1);
    equal(group.open, false);
    ok(root.querySelector('.activity-latest')!.textContent?.includes('check 9'), 'latest running command remains visible');
    setMessage({ ...message(), status: 'done' });
    equal(root.querySelectorAll('.activity').length, 1);
    equal(group.open, false, 'completion stays collapsed');
    equal(root.querySelector('.activity-latest'), null);
    ok(root.querySelector('.ai > .md')!.textContent?.includes(final.split('\n')[0]!));
    ok(!root.querySelector('.ai > .md')!.textContent?.includes('Checking the files'), 'narration is kept out of the final answer');
    group.open = true;
    ok(group.textContent?.includes('Checking the files'));
    equal(group.querySelectorAll('.tools > span').length, 10);
    // Older conversations without recorded boundaries keep their full text.
    setMessage({ ...message(), textBreaks: undefined, activityEnd: undefined });
    ok(root.querySelector('.ai > .md')!.textContent?.includes('Checking the files'));
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: with Auto the model name shows as soon as it is chosen, while the answer streams, and stays the same at the end; its effort shows beside it', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'auto-name', role: 'assistant', at: 0, status: 'streaming', text: '', reason: 'reason.routing' });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'auto-name', options: [], onBranch: () => {},
  }), root);
  const header = () => root.querySelector<HTMLElement>('.model')!;
  const name = () => header().querySelector('b')!.textContent;
  try {
    equal(name(), 'Auto');
    equal(header().querySelector('em')!.textContent, `· ${t('reason.routing')}`, 'while Auto is still choosing it says only that');
    equal(header().children.length, 2, 'the mark with its name, and the reason');
    const choosing = header().getBoundingClientRect().height;
    setMessage({ ...message(), vendor: 'claude', model: 'claude-haiku-5-5', effort: 'medium' });
    equal(name(), 'Haiku 5.5', 'the name shows before the first word');
    equal(header().querySelector('em'), null, 'the reason is said while Auto chooses, and after that it is the tooltip');
    equal(header().textContent, `Haiku 5.5· ${t('answer.effort.medium')}`, 'the header is the mark, the model and its effort');
    ok(header().title.includes(t('reason.routing')), `the reason is in the tooltip: ${header().title}`);
    equal(header().getBoundingClientRect().height, choosing, 'and the header keeps its height');
    setMessage({ ...message(), text: 'Sunlight cont' });
    equal(name(), 'Haiku 5.5', 'and stays while the words arrive');
    setMessage({ ...message(), status: 'done' });
    equal(name(), 'Haiku 5.5', 'and is the same when the answer ends');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: a model the CLI never named reads the same while streaming and at the end, and never as "Codex Codex"', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const [message, setMessage] = createSignal<Message>({ id: 'codex-name', role: 'assistant', at: 0, status: 'streaming', text: 'The', vendor: 'codex' });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'codex-name', options: [], onBranch: () => {},
  }), root);
  const name = () => root.querySelector<HTMLElement>('.model b')!.textContent;
  try {
    equal(name(), 'Codex', 'a model the CLI never named reads as its vendor');
    setMessage({ ...message(), model: 'codex' });
    equal(name(), 'Codex', "the engine's stand-in id is no name");
    setMessage({ ...message(), status: 'done' });
    equal(name(), 'Codex', 'and the end changes nothing');
    setMessage({ ...message(), model: 'gpt-6-luna' });
    equal(name(), 'GPT-6 Luna');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: the last answer and its actions leave at least 48px above either composer height', async () => {
  for (const [width, height] of [[1440, 900], [1000, 700]]) {
    const root = document.createElement('div');
    root.style.cssText = `position:fixed;left:0;top:0;width:${width}px;height:${height}px`;
    root.dataset.motion = 'none';
    for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
    root.innerHTML = '<div class="work" style="height:100%"><div class="thread"><div class="col"></div></div><div class="dock"></div></div>';
    document.body.append(root);
    const messages: Message[] = Array.from({ length: 30 }, (_, i) => ({
      id: String(i), role: i % 2 ? 'assistant' : 'user', text: 'A long thread with enough text to scroll. '.repeat(10),
      at: 0, status: 'done', vendor: 'codex', model: 'gpt-6',
    }));
    let fill!: (text: string) => void;
    const disposeThread = render(() => createComponent(Thread, {
      messages, from: 0, conversationId: 'gap-check', options: [], onBranch: () => {},
    }), root.querySelector('.col')!);
    const disposeComposer = render(() => createComponent(Composer, {
      vendors: ['codex'], options: [], choice: { vendor: 'auto' }, busy: false, placeholder: '',
      onChoice: () => {}, onSend: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onReady: (ready) => { fill = ready; },
    }), root.querySelector('.dock')!);
    try {
      for (const text of ['', 'A very long prompt. '.repeat(500)]) {
        fill(text);
        await new Promise((resolve) => setTimeout(resolve, 40));
        const thread = root.querySelector<HTMLElement>('.thread')!;
        thread.scrollTop = thread.scrollHeight;
        ok(thread.scrollTop > 0, 'the thread must actually overflow');
        const last = root.querySelector('.col')!.lastElementChild!;
        ok(last.querySelector('.actions'), 'the measured answer includes its action row');
        const gap = root.querySelector('.composer')!.getBoundingClientRect().top - last.getBoundingClientRect().bottom;
        ok(gap >= 48 && gap <= 72, `${width}x${height}: gap ${gap}px with ${text ? 'expanded' : 'empty'} composer`);
      }
    } finally {
      disposeThread();
      disposeComposer();
      root.remove();
    }
  }
});

check('thread: a Codex work log reads cleanly: titles on their own lines, tidy command rows, the status sentence once', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const status = 'The window control did not connect.';
  const exe = '"C:\\\\WINDOWS\\\\System32\\\\WindowsPowerShell\\\\v1.0\\\\powershell.exe"';
  const inner = "Set-Content -LiteralPath 'a.txt' -Value 'one' -NoNewline";
  const [message, setMessage] = createSignal<Message>({
    id: 'worklog', role: 'assistant', at: 0, status: 'streaming', text: `${status}\n\n${status}\n\nI checked the files instead.`,
    textBreaks: [status.length + 2], activityEnd: status.length + 2,
    thinking: '**Minimizing output verbosity****Assessing time budget and test approach****Preparing for final retry**',
    tools: [{ id: 'c1', name: 'commandExecution', input: `${exe} -Command "${inner}"`, ok: true }],
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'worklog', options: [], onBranch: () => {},
  }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('.activity')!;
    group.open = true;
    const shown = () => root.textContent ?? '';
    ok(!shown().includes('**'), 'no raw bold markers');
    ok(!shown().includes('commandExecution'), 'no internal kind name');
    ok(!shown().includes('\\\\'), 'no doubled backslashes');
    const titles = [...group.querySelectorAll('.activity-thinking p')].map((p) => p.textContent);
    equal(titles.join('|'), 'Minimizing output verbosity|Assessing time budget and test approach|Preparing for final retry');
    const row = group.querySelector<HTMLElement>('.tools > span')!;
    equal(row.querySelector('b')!.textContent, 'PowerShell');
    equal(row.querySelector('i')!.textContent, inner);
    equal(row.title, inner, 'the full command is the tooltip');
    equal(shown().split(status).length - 1, 2, 'while streaming the answer shows the words as the agent said them');
    equal(root.querySelector('.activity-latest'), null, 'no status line under a visible answer');
    setMessage({ ...message(), status: 'done' });
    equal((root.textContent ?? '').split(status).length - 1, 1, 'and the status sentence shows once when done');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: the final answer is never cut, long commands are cut in the middle, and a command that looks like markup stays text', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const long = `START ${'x'.repeat(300)} END`;
  const markup = '<img src=x onerror=alert(1)> **not bold** & <b>raw</b>';
  const narration = 'Check\n\n';
  const answer = 'Checking files.';
  const [message] = createSignal<Message>({
    id: 'worklog-edges', role: 'assistant', at: 0, status: 'done', text: narration + answer, textBreaks: [narration.length], activityEnd: narration.length,
    tools: [
      { id: 'l', name: 'Bash', input: { command: long }, ok: true },
      { id: 'm', name: 'Bash', input: { command: markup }, ok: true },
    ],
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'worklog-edges', options: [], onBranch: () => {},
  }), root);
  try {
    const group = root.querySelector<HTMLDetailsElement>('.activity')!;
    group.open = true;
    equal(root.querySelector('.ai > .md')!.textContent?.trim(), answer, 'a status that only starts the answer does not cut it');
    ok(group.textContent?.includes('Check'), 'and stays in the activity');
    const [first, second] = [...group.querySelectorAll<HTMLElement>('.tools > span')];
    const cut = first!.querySelector('i')!.textContent!;
    ok(cut.includes('…') && cut.length <= 96 && cut.startsWith('START') && cut.endsWith('END'), cut);
    equal(first!.title, long, 'the full command is the tooltip');
    equal(second!.querySelector('i')!.textContent, markup);
    equal(second!.querySelectorAll('img, b i, i b').length, 0, 'no markup is built from a command');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: while an answer streams every paragraph stays in the answer, and the narration folds into the activity only when it ends', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const said = ['Sigo no código.', 'Corrigindo a parte 2.', 'Parte 2 pronta.', 'Atualizando o mapa do produto.'];
  const before = said.join('\n\n');
  const after = `${before}\n\nAll done.`;
  const tools = [{ id: '1', name: 'Bash', input: { command: 'one' }, ok: true }, { id: '2', name: 'Bash', input: { command: 'two' } }];
  const reply = (text: string, end: number, status: Message['status'] = 'streaming'): Message => ({ id: 'streamed', role: 'assistant', at: 0, status, text, activityEnd: end, tools });
  const [message, setMessage] = createSignal<Message>(reply(before, before.length));
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'streamed', options: [], onBranch: () => {},
  }), root);
  const answer = () => root.querySelector('.ai > .md')!.textContent ?? '';
  const folded = () => root.querySelector('.activity-body .md')?.textContent ?? '';
  const everything = (when: string) => {
    for (const paragraph of said) ok(answer().includes(paragraph), `${when}: "${paragraph}" is in the answer, which reads ${JSON.stringify(answer())}`);
    equal(folded(), '', `${when}: nothing is folded away yet`);
  };
  try {
    everything('while a step runs');
    setMessage(reply(after, before.length));
    everything('when words come after the step');
    ok(answer().includes('All done.'), 'and the new words join them');
    setMessage(reply(after, after.length));
    everything('when the next step starts');
    setMessage(reply(after, before.length, 'done'));
    equal(answer().trim(), 'All done.', 'when the answer ends only the final text shows');
    // The narration's last two paragraphs show in the activity; the earlier ones wait under "2 more" (task 018).
    for (const paragraph of said.slice(2)) ok(folded().includes(paragraph), `the narration "${paragraph}" is inside the activity`);
    ok(!folded().includes(said[0]!), 'the earliest paragraph waits under more');
    const more = root.querySelector<HTMLButtonElement>('.activity-body .more-narration')!;
    equal(more.textContent!.trim(), t('narration.more', { n: 2 }));
    more.click();
    for (const paragraph of said) ok(folded().includes(paragraph), `the narration "${paragraph}" is inside the activity`);
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: a held-back command says what it would have done, and Got it on the latest reply takes it off the needs-you list', async () => {
  const original = api.parkedSeen;
  const calls: [string, string][] = [];
  api.parkedSeen = async (id, turn) => void calls.push([id, turn]);
  const root = document.createElement('div');
  document.body.append(root);
  const held = (id: string, over: Partial<Message> = {}): Message => ({
    id, role: 'assistant', at: 1, status: 'done', text: 'Done.',
    parked: [{ toolUseId: `t-${id}`, tool: 'Bash', kind: 'command', label: '', does: 'delete', at: 1 }], ...over,
  });
  const [messages, setMessages] = createSignal<Message[]>([held('older'), held('latest')]);
  const dispose = render(() => createComponent(Thread, {
    get messages() { return messages(); }, from: 0, conversationId: 'held', options: [], onBranch: () => {},
  }), root);
  try {
    const cards = () => [...root.querySelectorAll('.ask.parked')];
    equal(cards().length, 2);
    ok(cards()[0]!.textContent?.includes(t('parked.does.delete')), 'the kind of command, never its words');
    equal(cards()[0]!.querySelector('button'), null, 'an older reply is only a record');
    const got = cards()[1]!.querySelector('button')!;
    equal(got.textContent, t('parked.seen'));
    got.click();
    await Promise.resolve();
    deepEqual(calls, [['held', 'latest']]);
    setMessages([held('older'), held('latest', { parkedSeen: true })]);
    equal(cards()[1]!.querySelector('button'), null, 'once read, the note stays without the button');
    setMessages([held('latest', { parked: [{ toolUseId: 't', tool: 'Bash', kind: 'command', label: '', at: 1 }] })]);
    ok(cards()[0]!.textContent?.includes(t('parked.command')), 'a command with no known kind keeps the plain words');
  } finally {
    dispose(); root.remove(); api.parkedSeen = original;
  }
});
