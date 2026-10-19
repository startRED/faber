import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { Composer } from '../../ui/Composer.tsx';
import type { Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { helpersAtWork } from '../../ui/worklog.ts';
import { check, equal, ok } from './check.ts';

// On 2026-10-17 a Stop ended four helpers at work with the turn, and their work was lost: with helpers at work, Stop
// says so first and offers to let them work.
check('stop-helpers: Stop with helpers at work says what it ends and offers to keep them working', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:500px';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  document.body.append(root);
  let stops = 0;
  const mount = (helpers: number) =>
    render(() => createComponent(Composer, {
      vendors: ['claude'], options: [], choice: { vendor: 'claude' }, busy: true, helpers, placeholder: 'Write here',
      onChoice: () => {}, onStop: () => { stops++; }, onWarm: () => {}, onPause: () => {}, onSend: () => {},
    }), root);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
  const stop = () => root.querySelector<HTMLButtonElement>('button.stop')!;
  const ask = () => root.querySelector('.stop-ask');

  let dispose = mount(0);
  try {
    await settle();
    stop().click();
    equal(stops, 1, 'with no helper at work, Stop stops at once');
    equal(ask(), null);
    dispose();
    stops = 0;

    dispose = mount(4);
    await settle();
    stop().click();
    await settle();
    equal(stops, 0, 'with helpers at work, the first press only asks');
    ok(ask()?.textContent?.includes(t('composer.stopHelpers', { n: 4 })), ask()?.textContent ?? 'no question');
    const [stopAll, keep] = [...ask()!.querySelectorAll<HTMLButtonElement>('button')];
    equal(keep!.textContent, t('composer.keepWorking'));
    equal(document.activeElement, keep, 'letting them work is the default');
    keep!.click();
    await settle();
    equal(stops, 0, 'keep working stops nothing');
    equal(ask(), null, 'and the question goes');
    equal(document.activeElement, stop(), 'focus comes back to Stop, never to the page');

    stop().click();
    await settle();
    equal(stopAll!.isConnected, false);
    ask()!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    equal(ask(), null, 'Escape keeps them working');
    equal(document.activeElement, stop(), 'and focus comes back the same way');
    stop().click();
    await settle();
    ask()!.querySelector<HTMLButtonElement>('button.stop')!.click();
    await settle();
    equal(stops, 1, 'Stop all is a real stop');
  } finally {
    dispose();
    root.remove();
  }
});

check('stop-helpers: the helpers at work are every open subagent a Stop would end, nested ones and the background ones of earlier replies too', () => {
  const agent = (id: string, ok?: boolean, background = false, parent?: string) => ({ id, name: 'Agent', input: { prompt: id, run_in_background: background }, ok, parent });
  const reply = (status: Message['status'], tools: ReturnType<typeof agent>[]) => ({ id: status!, role: 'assistant', text: '', at: 0, status, tools }) as Message;
  const mine = { id: 'u', role: 'user', text: 'and also this', at: 0 } as Message;
  equal(helpersAtWork([reply('streaming', [agent('a'), agent('b', undefined, true), agent('c', true)])]), 2, 'one running, one in the background');
  equal(helpersAtWork([reply('streaming', [agent('a'), agent('a1', undefined, false, 'a')])]), 2, 'a subagent of a subagent counts on its own');
  equal(helpersAtWork([reply('done', [agent('a', undefined, true), agent('a1', undefined, false, 'a'), agent('b')]), mine, reply('streaming', [])]), 2,
    'a message taken into the turn ended the reply before it, not its background work');
  equal(helpersAtWork([reply('done', [agent('a', undefined, true)]), mine, reply('done', [agent('b', undefined, false, 'a')]), mine, reply('streaming', [])]), 2,
    'a subagent its background parent started later, in a reply of its own, counts too');
  equal(helpersAtWork([reply('stopped',[agent('a', undefined, true)]), mine, reply('streaming', [])]), 0, 'a Stop ended what came before it');
  equal(helpersAtWork(undefined), 0);
});
