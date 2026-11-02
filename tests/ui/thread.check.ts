import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type Message } from '../../ui/api.ts';
import { LANGUAGES, t, templates } from '../../ui/i18n.ts';
import { renderMarkdown } from '../../ui/markdown.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

check('thread: streaming preserves the open paragraph, selection, code scroll and safe link updates', () => {
  const root = document.createElement('div'); root.className = 'md'; document.body.append(root);
  try {
    renderMarkdown(root, 'A readable answer');
    const paragraph = root.firstChild!;
    const words = paragraph.firstChild!;
    const selection = window.getSelection()!;
    const range = document.createRange(); range.setStart(words, 2); range.setEnd(words, 10);
    selection.removeAllRanges(); selection.addRange(range);
    renderMarkdown(root, 'A readable answer grows naturally.');
    equal(root.firstChild, paragraph, 'the live paragraph does not disappear between deltas');
    equal(paragraph.firstChild, words, 'appending words preserves their text node');
    equal(selection.toString(), 'readable', 'new words do not clear the selected words');
    renderMarkdown(root, 'A readable answer grows naturally.\n\n```text\n' + 'wide '.repeat(100));
    const box = root.querySelector<HTMLElement>('.codeblock')!;
    const pre = box.querySelector('pre')!; pre.style.width = '120px'; pre.style.overflow = 'auto';
    pre.scrollLeft = 80;
    const copy = box.querySelector('button')!; copy.focus();
    renderMarkdown(root, 'A readable answer grows naturally.\n\n```text\n' + 'wide '.repeat(101));
    equal(root.querySelector('.codeblock'), box, 'the streaming code block is retained');
    equal(pre.scrollLeft, 80, 'the reader keeps their horizontal position');
    equal(document.activeElement, copy, 'streaming does not discard keyboard focus');
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: https://example.com/one');
    const link = root.querySelector('a')!;
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: https://example.com/two');
    equal(root.querySelector('a'), link);
    equal(link.getAttribute('href'), 'https://example.com/two');
    renderMarkdown(root, 'Read [docs][d]\n\n[d]: javascript:alert(1)');
    equal(root.querySelector('a')?.getAttribute('href'), null, 'removed unsafe attributes never survive reconciliation');
    renderMarkdown(root, '<span class="cb-copy">First answer</span>');
    renderMarkdown(root, '<span class="cb-copy">Corrected answer</span>');
    equal(root.textContent?.trim(), 'Corrected answer', 'answer HTML cannot impersonate a generated copy button');
    renderMarkdown(root, 'Short.');
    equal(root.textContent?.trim(), 'Short.', 'shorter replies release obsolete blocks');
  } finally { window.getSelection()?.removeAllRanges(); root.remove(); }
});

check('thread: ten tool steps fold while all words stay visible', async () => {
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
    const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
    equal(head.getAttribute('aria-expanded'), 'false');
    ok(head.textContent!.includes('check 9'), 'the running command remains visible');
    const words = () => [...root.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(words().includes('Checking the files') && words().includes('The requested change is complete.') && words().includes('All checks passed.'), 'all words remain visible');
    setMessage({ ...message(), status: 'done' });
    equal(head.getAttribute('aria-expanded'), 'false');
    head.click();
    equal(root.querySelectorAll('.steps-list .step').length, 10, 'all ten commands are inspectable');
    ok(root.querySelector('.thought'), 'thinking stays visible in its own row');
    setMessage({ ...message(), textBreaks: undefined, activityEnd: undefined });
    ok(words().includes('Checking the files'), 'older replies keep their words');
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
    tools: [{ id: 'c1', name: 'commandExecution', input: `${exe} -Command "${inner}"`, at: 0, ok: true }],
  });
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'worklog', options: [], onBranch: () => {},
  }), root);
  try {
    root.querySelector<HTMLButtonElement>('.steps-head')!.click();
    root.querySelector<HTMLButtonElement>('.thought .step-line')!.click();
    const shown = () => root.textContent ?? '';
    ok(!shown().includes('**') && !shown().includes('commandExecution'));
    const titles = [...root.querySelectorAll('.thought-text p')].map((p) => p.textContent);
    equal(titles.join('|'), 'Minimizing output verbosity|Assessing time budget and test approach|Preparing for final retry');
    const row = root.querySelector<HTMLElement>('.step:not(.thought) .step-line')!;
    equal(row.querySelector('b')!.textContent, 'PowerShell');
    equal(row.querySelector('i')!.textContent, inner);
    equal(row.title, inner);
    equal(shown().split(status).length - 1, 2);
    setMessage({ ...message(), status: 'done' });
    equal(shown().split(status).length - 1, 2, 'completion preserves the words');
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: running steps stay visible alone and in a fold', () => {
  const root = document.createElement('div'); document.body.append(root);
  const running = (id: string, command: string) => ({ id, name: 'Bash', input: { command }, ok: undefined });
  const [message, setMessage] = createSignal<Message>({ id: 'once', role: 'assistant', at: 0, status: 'streaming', text: '', tools: [running('a', 'cargo build')] });
  const dispose = render(() => createComponent(Thread, { get messages() { return [message()]; }, from: 0, conversationId: 'once', options: [], onBranch: () => {} }), root);
  try {
    ok(root.querySelector('.steps.one .step .live'));
    ok(root.querySelector('.step-line')!.textContent!.includes('cargo build'));
    setMessage({ ...message(), tools: [{ ...running('a', 'cargo build'), ok: true }, running('b', 'cargo test')] });
    const head = root.querySelector<HTMLButtonElement>('.steps-head')!;
    ok(head.textContent!.includes('cargo test'));
    head.click();
    equal(root.querySelectorAll('.steps-list .step').length, 2);
    ok(root.querySelector('.steps-list .step[data-state="running"]')!.textContent!.includes('cargo test'));
    equal(root.querySelector('.activity-latest'), null);
  } finally { dispose(); root.remove(); }
});

check('thread: the final answer is never cut, a long command wraps inside a 720 px column, and a command that looks like markup stays text', () => {
  const root = document.createElement('div');
  root.style.width = '720px';
  document.body.append(root);
  const long = `START ${'x'.repeat(200)} END`;
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
    const words = [...root.querySelectorAll('.ai > .md')].map((part) => part.textContent).join('');
    ok(words.includes(narration.trim()) && words.includes(answer), 'all words stay visible');
    root.querySelector<HTMLButtonElement>('.steps-head')!.click();
    const [first, second] = [...root.querySelectorAll<HTMLElement>('.steps-list .step-line')];
    ok(first!.querySelector('i')!.textContent!.includes('START') && first!.querySelector('i')!.textContent!.includes('END'));
    first!.click();
    equal(first!.parentElement!.querySelector('.step-what')!.textContent, long, 'the full command is available');
    ok(first!.getBoundingClientRect().right <= root.getBoundingClientRect().right + 0.5, 'fits the column');
    equal(first!.title, long);
    equal(second!.querySelector('i')!.textContent, markup);
    equal(second!.querySelectorAll('img, b i, i b').length, 0);
  } finally {
    dispose();
    root.remove();
  }
});

check('thread: words remain visible in order between steps while streaming and after completion', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const said = ['Sigo no código.', 'Corrigindo a parte 2.', 'Parte 2 pronta.', 'Atualizando o mapa do produto.'];
  const before = said.join('\n\n') + '\n\n';
  const tools = [{ id: '1', name: 'Bash', input: { command: 'one' }, ok: true, at: before.length }];
  const reply = (text: string, status: Message['status'] = 'streaming'): Message => ({ id: 'streamed', role: 'assistant', at: 0, status, text, tools });
  const [message, setMessage] = createSignal<Message>(reply(before + 'All done.'));
  const dispose = render(() => createComponent(Thread, {
    get messages() { return [message()]; }, from: 0, conversationId: 'streamed', options: [], onBranch: () => {},
  }), root);
  try {
    for (const status of ['streaming', 'done'] as const) {
      setMessage(reply(before + 'All done.', status));
      const parts = [...root.querySelectorAll('.ai > .md, .ai > .steps')];
      deepEqual(parts.map((part) => part.classList.contains('md') ? 'words' : 'steps'), ['words', 'steps', 'words']);
      const words = parts.map((part) => part.textContent ?? '').join(' ');
      const phrases = [...said, 'All done.'];
      for (let i = 0; i < phrases.length; i++) {
        ok(words.includes(phrases[i]!), phrases[i]);
        if (i) ok(words.indexOf(phrases[i - 1]!) < words.indexOf(phrases[i]!), 'the words keep their order');
      }
    }
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

check('thread: a reply Faber was closed under says so, offers Continue on the newest one, and the person’s own Stop stays "Stopped."', async () => {
  const user = { id: 'u', role: 'user', text: 'go', at: 1 } as Message;
  const reply = (closed: boolean) => ({ id: 'reply', role: 'assistant', text: 'Partial', at: 2, status: 'stopped', vendor: 'claude', closed }) as Message;
  const before = api.continueClosed;
  const asked: string[][] = [];
  api.continueClosed = async (id, turn) => { asked.push([id, turn]); };
  try {
    for (const area of ['chat', 'project'] as const) {
      // Attached, so the click reaches Solid's delegated handler.
      const host = document.createElement('div');
      document.body.append(host);
      const dispose = render(() => createComponent(Thread, { conversationId: 'c', area, messages: [user, reply(true)], from: 0, options: [], onBranch: () => {} }), host);
      try {
        ok(host.textContent!.includes(t('answer.closed')), area + ': the close is said');
        ok(!host.textContent!.includes(t('answer.stopped')), area + ': in place of "Stopped."');
        equal(host.textContent!.includes(t('answer.closedFiles')), area === 'project', area + ': the files line only in a project');
        const button = [...host.querySelectorAll('button')].find((b) => b.textContent === t('answer.closedContinue'));
        ok(button, area + ': the newest such reply offers Continue');
        button!.click();
      } finally { dispose(); host.remove(); }
    }
    deepEqual(asked, [['c', 'reply'], ['c', 'reply']], 'Continue names the conversation and the cut reply');
    // An older cut reply says it, without the button; the person's own Stop says "Stopped." as before.
    const later = [{ id: 'u2', role: 'user', text: 'more', at: 3 } as Message, { ...reply(false), id: 'reply2', at: 4 }];
    const host = document.createElement('div');
    const dispose = render(() => createComponent(Thread, { conversationId: 'c', messages: [user, reply(true), ...later], from: 0, options: [], onBranch: () => {} }), host);
    try {
      ok(host.textContent!.includes(t('answer.closed')));
      ok(![...host.querySelectorAll('button')].some((b) => b.textContent === t('answer.closedContinue')), 'no Continue once the reply is not the newest');
      ok(host.textContent!.includes(t('answer.stopped')), 'the person’s Stop is still said');
    } finally { dispose(); }
  } finally { api.continueClosed = before; }
});

check('thread: the answer area speaks plain words, never git, cache or CLI words', () => {
  // The engineering words each language used for these texts before (the locale audit of 2026-10-29).
  // Whole words only: "programa" holds "rama" and "cliquez" holds "cli".
  const words = (list: string) => new RegExp(`(^|[^\\p{L}])(${list})($|[^\\p{L}])`, 'iu');
  const jargon: Record<string, RegExp> = {
    en: words('branch|cache|CLI|session|assistant'),
    'pt-BR': words('galho|cache|CLI|sessão|assistente'),
    es: words('rama|caché|CLI|sesión|asistente'),
    fr: words('branche|cache|CLI|session|assistant'),
    de: words('Abzweig\\p{L}*|Cache|CLI|Sitzung|Assistent'),
  };
  ok(jargon.es!.test('Crear rama desde aquí') && !jargon.es!.test('el programa de la IA'), 'a word inside another word is not jargon');
  const keys = ['answer.branchHere', 'mine.sendBranch', 'answer.cacheWarm', 'answer.terminalHint', 'kit.details', 'kit.line', 'error.driverStopped', 'error.sessionLost', 'note.historyResent', 'ask.question.label', 'projects.chat.lead'] as const;
  for (const { code } of LANGUAGES) {
    for (const key of keys) {
      const text = templates(key, code).other;
      ok(!jargon[code]!.test(text), `${code} ${key} reads plainly: ${text}`);
    }
  }
});
