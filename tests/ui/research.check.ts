import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { Composer } from '../../ui/Composer.tsx';
import { Thread } from '../../ui/Thread.tsx';
import { api, type Message, type PermissionMode, type Vendor } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { check, deepEqual, equal, ok } from './check.ts';

const settle = () => new Promise((resolve) => setTimeout(resolve, 100));
const waitFor = async (ready: () => boolean, message: string) => {
  const deadline = performance.now() + 2000;
  while (!ready()) {
    ok(performance.now() < deadline, message);
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
};

check('research: slash entry preserves Auto and follows web availability for either AI', async () => {
  for (const vendor of ['auto', 'claude', 'codex'] as const) {
    const root = document.createElement('div');
    document.body.append(root);
    const [enabled, setEnabled] = createSignal(true);
    const sent: string[] = [];
    let picks = 0;
    const dispose = render(() => createComponent(Composer, {
      vendors: ['claude', 'codex'], options: [], choice: { vendor }, busy: false, placeholder: 'Write',
      get canResearch() { return enabled(); },
      onCommands: async () => [], onChoice: () => { picks++; }, onStop: () => {}, onWarm: () => {}, onPause: () => {},
      onSend: (draft) => sent.push(draft.text),
    }), root);
    const field = root.querySelector('textarea')!;
    const type = (text: string) => {
      field.value = text; field.setSelectionRange(text.length, text.length);
      field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    };
    const enter = () => field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
    const entry = () => root.querySelector('.mentions[aria-busy="false"] [role="option"]');
    try {
      // Composer focuses in a microtask; typing before it can lose the synthetic caret in WebKit.
      await waitFor(() => document.activeElement === field, `${vendor}: the composer receives its initial focus`);
      type('/res');
      await waitFor(() => !!entry(), `${vendor}: the research entry finishes loading`);
      equal(entry()?.querySelector('span')?.textContent, '/research', `${vendor}: the slash row is research`);
      equal(entry()?.querySelector('.mention-note')?.textContent, t('composer.research'), `${vendor}: the research row has its label`);
      enter();
      equal(field.value, '/research ');
      equal(sent.length, 0, 'choosing the entry only fills the composer');
      equal(picks, 0, 'Auto remains Auto for quota pacing');
      type('/research Which batteries last longer?'); enter();
      deepEqual(sent, ['/research Which batteries last longer?']);
      type('/res');
      await waitFor(() => !!entry(), `${vendor}: the research entry reopens after sending`);
      setEnabled(false);
      await waitFor(() => !root.querySelector('[role=option]'), `${vendor}: disabling web search removes the research entry`);
      equal(root.querySelector('[role=option]'), null, 'switching web search off closes the research entry');
      setEnabled(true);
      await waitFor(() => !!entry(), `${vendor}: enabling web search restores the research entry`);
      ok(root.querySelector('[role=option]'), 'turning web search on restores the entry');
    } finally { dispose(); root.remove(); }
  }
});

check('research: the shared plan card edits through the composer, approves once and renders ordinary report links', async () => {
  const before = api.approvePlan;
  try {
    for (const vendor of ['claude', 'codex'] as Vendor[]) {
      const root = document.createElement('div');
      root.style.width = '720px';
      document.body.append(root);
      const plan = '1. Compare independent tests.\n2. Check official specifications.';
      const edited = '1. Compare cold-weather tests.\n2. Check official specifications.';
      const card = (id: string, text: string): Message => ({
        id, role: 'assistant', text: vendor === 'codex' ? text : '', at: 1, status: 'done', vendor,
        permissions: [{ requestId: id, tool: 'ExitPlanMode', input: { mode: 'ask', ...(vendor === 'claude' ? { plan: text } : {}) } }],
      });
      const [messages, setMessages] = createSignal<Message[]>([card('plan', plan)]);
      const calls: (PermissionMode | undefined)[] = [];
      api.approvePlan = async (_id, _turn, _request, mode) => {
        calls.push(mode);
        if (mode) setMessages([{ id: 'report', role: 'assistant', at: 2, status: 'done', vendor,
          text: 'Cold affects runtime ([Test lab](https://example.org/tests)).\n\n## Sources\n- [Test lab](https://example.org/tests)' }]);
        else setMessages([]);
      };
      const dispose = render(() => [createComponent(Thread, {
        conversationId: 'research', options: [], from: 0, onBranch: () => {}, onReply: () => {},
        get messages() { return messages(); },
      }), createComponent(Composer, {
        vendors: [vendor], options: [], choice: { vendor }, mode: 'plan', busy: false, placeholder: 'Write',
        onChoice: () => {}, onStop: () => {}, onWarm: () => {}, onPause: () => {},
        onSend: (draft) => { equal(draft.text, 'Compare cold-weather tests instead.'); setMessages([card('edited', edited)]); },
      })], root);
      try {
        equal(root.querySelectorAll('.plan-approval').length, 1);
        equal(root.querySelectorAll('.md li').length, 2, 'the plan appears only once');
        const keep = [...root.querySelectorAll<HTMLButtonElement>('.plan-approval button')].find((b) => b.textContent === t('ask.plan.keep'))!;
        keep.click(); await settle();
        deepEqual(calls, [undefined]);
        const field = root.querySelector('textarea')!;
        field.value = 'Compare cold-weather tests instead.';
        field.dispatchEvent(new InputEvent('input', { bubbles: true }));
        field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }));
        await settle();
        ok(root.textContent?.includes('cold-weather tests'));
        root.querySelector<HTMLButtonElement>('.plan-approval .primary')!.click(); await settle();
        deepEqual(calls, [undefined, 'ask']);
        equal(root.querySelector('.plan-approval'), null);
        const links = [...root.querySelectorAll<HTMLAnchorElement>('.md a')];
        equal(links.length, 2, 'the claim and Sources list use the ordinary Markdown links');
        ok(links.every((a) => a.href === 'https://example.org/tests'));
        equal(root.querySelector('.md h2')?.textContent, 'Sources');
      } finally { dispose(); root.remove(); }
    }
  } finally { api.approvePlan = before; }
});
