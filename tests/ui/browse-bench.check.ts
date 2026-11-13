// What the agent's browser reads cost, and whether its numbers hit the right control, on the bench pages (`bench/browse/`,
// written by `bench/browse/make.mjs`): the read of `host/src/pane/act.js` as the host drives it, with the numbers and the
// hashes of the last read carried between calls (`memo`) like the host does per tab. Tokens are the estimate of
// `scripts/tool-tokens.mjs`, plus `HEADER` for the lines Faber adds to every answer. docs/dev/benchmark.md has the figures.

import { check, deepEqual, equal, ok } from './check.ts';
import act from '../../host/src/pane/act.js';
import shop from '../../bench/browse/shop.html?raw';
import article from '../../bench/browse/article.html?raw';
import signup from '../../bench/browse/signup.html?raw';
import secrets from '../../bench/browse/secrets.html?raw';

const tokens = (text: string) => {
  let total = 0;
  for (const [piece] of text.matchAll(/\p{L}+|\p{N}+|[^\s\p{L}\p{N}]/gu)) total += /^\p{L}/u.test(piece) ? Math.ceil(piece.length / 7) : /^\p{N}/u.test(piece) ? Math.ceil(piece.length / 3) : 1;
  return total;
};
const HEADER = 60;

type Out = { view: string; memo: unknown; diff: boolean };
type Answer = { ok: boolean; why?: string; turn?: string };

async function onBench(html: string, run: (root: HTMLElement) => void | Promise<void>) {
  const root = document.createElement('div');
  root.style.cssText = 'position: fixed; left: 0; top: 0; width: 900px; height: 600px; overflow: auto;';
  const parsed = new DOMParser().parseFromString(html, 'text/html');
  const scripts = [...parsed.body.querySelectorAll('script')].map((s) => s.textContent ?? '');
  parsed.body.querySelectorAll('script').forEach((s) => s.remove());
  root.append(...parsed.body.childNodes);
  document.body.append(root);
  for (const code of scripts) { const s = document.createElement('script'); s.textContent = `{ ${code} }`; root.append(s); }
  try { await run(root); } finally { root.remove(); }
}

/** The agent's side: reads and acts as the host does, keeping each answer's memo for the next call. */
function agent(root: HTMLElement) {
  let memo: unknown = null;
  const spent: number[] = [];
  const read = (controls = false): Out => {
    const out = act({ kind: 'read', view: true, controls, within: root, memo, max: 24001 }) as unknown as Out;
    memo = out.memo;
    spent.push(tokens(out.view) + HEADER);
    return out;
  };
  const click = (ref: number): Answer => act({ kind: 'click', ref, memo }) as Answer;
  const type = (ref: number, text: string): Answer => act({ kind: 'type', ref, text, memo }) as Answer;
  const refOf = (view: string, role: string, name: string, row = '') => {
    const found = view.split('\n').map((l) => new RegExp(`^\\[(\\d+)\\] ${role} "${name}"${row ? ` in "${row}"` : ''}( |$)`).exec(l)).find(Boolean);
    if (!found) throw new Error(`no ${role} "${name}" in the read:\n${view.slice(0, 600)}`);
    return Number(found[1]);
  };
  return { read, click, type, refOf, spent, total: () => spent.reduce((a, b) => a + b, 0) };
}

const words = (el: Element) =>
  [(el as HTMLElement).innerText, (el as HTMLInputElement).value, el.getAttribute('aria-label'), ...[...((el as HTMLInputElement).labels ?? [])].map((l) => l.innerText)].join(' ').replace(/\s+/g, ' ');

check('browser reads: a repeat read of the same page is a fraction of the first, and the first lists every control with a number', () =>
  onBench(shop, (root) => {
    const a = agent(root);
    const first = a.read();
    ok(first.view.startsWith('Controls ('), first.view.slice(0, 80));
    const listed = first.view.split('\n').filter((l) => /^\[\d+\] /.test(l));
    ok(listed.length >= 60, `${listed.length} controls listed`);
    equal(new Set(listed.map((l) => l.match(/^\[(\d+)\]/)![1])).size, listed.length, 'every number is its own');
    ok(first.view.includes('Spring collection') && !first.view.includes('\nAdd to cart\n'), 'the text stays, and a button label is not repeated in it');
    const again = a.read();
    equal(again.view, 'Nothing changed since your last read.');
    ok(again.diff, 'the answer says it is only changes');
    ok(a.spent[1]! <= HEADER + 12, `a read with nothing new is ${a.spent[1]} tokens`);
    const add = a.refOf(first.view, 'button', 'Add to cart', 'Wool scarf');
    equal(a.click(add).ok, true);
    const after = a.read();
    ok(after.diff && after.view.includes('Added Wool scarf to your cart.'), after.view);
    ok(a.spent[2]! * 4 <= a.spent[0]!, `a read after one click is ${a.spent[2]} tokens against ${a.spent[0]} for the first`);
    equal(a.click(add).ok, true, 'the number from the first read still holds');
  }));

check('browser reads: a long page with few controls repeats as a short diff, and the controls-only read has no text', () =>
  onBench(article, (root) => {
    const a = agent(root);
    const first = a.read();
    equal(a.read().view, 'Nothing changed since your last read.');
    const yes = a.refOf(first.view, 'button', 'Yes');
    root.scrollTop = 0;
    equal(a.click(yes).ok, true);
    const after = a.read();
    ok(after.view.includes('Thanks for the feedback.') && after.diff, after.view);
    ok(a.spent[2]! * 6 <= a.spent[0]!, `${a.spent[2]} against ${a.spent[0]}`);
    const only = agent(root).read(true);
    ok(!only.view.includes('Section 3 explains') && only.view.includes('Introduction'), 'controls only');
    ok(tokens(only.view) * 4 < a.spent[0]!, 'and far smaller');
  }));

check('browser reads: a number only ever reaches the control it was given for, and one that went is refused', () =>
  onBench(shop, (root) => {
    const a = agent(root);
    const first = a.read();
    const hit: Element[] = [];
    root.addEventListener('click', (e) => { hit.push(e.target as Element); e.preventDefault(); }, true);
    let wrong = 0;
    let tried = 0;
    for (const line of first.view.split('\n')) {
      const m = /^\[(\d+)\] (button|link|checkbox|radio) "([^"]*)"(?: in "([^"]*)")?/.exec(line);
      if (!m || / disabled/.test(line)) continue;
      hit.length = 0;
      const r = a.click(Number(m[1]));
      tried++;
      const t = hit[0];
      const good = r.ok && t && words(t).includes(m[3]!) && (!m[4] || (t.closest('article')?.querySelector('h3')?.textContent ?? '').startsWith(m[4]));
      if (!good) wrong++;
    }
    ok(tried >= 50, `${tried} controls clicked by number`);
    equal(wrong, 0, 'no click landed on another control');
    // A control that changed its name is not the one the number was given for.
    const card = root.querySelector('#p3 button')!;
    card.textContent = 'Sold out';
    hit.length = 0;
    const stale = a.click(a.refOf(first.view, 'button', 'Add to cart', 'Wool scarf'));
    deepEqual([stale.ok, stale.why, hit.length], [false, 'ref', 0]);
    const nothing = act({ kind: 'click', ref: 5 }) as Answer;
    deepEqual([nothing.ok, nothing.why], [false, 'noref'], 'a number means nothing before a read');
  }));

check('browser reads: a form flow on the smallest page costs about what whole-page reads did (its text is tiny), by numbers', () =>
  onBench(signup, (root) => {
    const a = agent(root);
    const first = a.read();
    const set = (name: string, text: string) => equal(a.type(a.refOf(first.view, 'textbox', name), text).ok, true, name);
    set('Full name', 'Ana Souza');
    a.read();
    set('Work email', 'ana@example.org');
    a.read();
    equal(a.click(a.refOf(first.view, 'radio', 'Team')).ok, true);
    a.read();
    equal(a.click(a.refOf(first.view, 'checkbox', 'I accept the terms of service')).ok, true);
    const ready = a.read();
    ok(/Create workspace/.test(ready.view) && ready.diff, ready.view);
    equal(a.click(a.refOf(first.view, 'button', 'Create workspace')).ok, true);
    const done = a.read();
    ok(done.view.includes('Workspace created for ana@example.org on the team plan.'), done.view);
    ok(a.total() < (tokens(root.innerText) + HEADER) * a.spent.length * 1.2, `${a.total()} tokens for the flow, against whole-page reads`);
    equal(root.querySelector<HTMLInputElement>('#email')!.value, 'ana@example.org');
  }));

check('browser reads: the same form as one flow costs one first read, a short log and one read of what changed', () =>
  onBench(signup, (root) => {
    const a = agent(root);
    const first = a.read();
    const log: string[] = [];
    const step = (label: string, answer: Answer) => { equal(answer.ok, true, label); log.push(`${log.length + 1}. ${label} - done`); };
    step('type into [name]', a.type(a.refOf(first.view, 'textbox', 'Full name'), 'Ana Souza'));
    step('type into [email]', a.type(a.refOf(first.view, 'textbox', 'Work email'), 'ana@example.org'));
    step('click [team]', a.click(a.refOf(first.view, 'radio', 'Team')));
    step('click [terms]', a.click(a.refOf(first.view, 'checkbox', 'I accept the terms of service')));
    step('click [create]', a.click(a.refOf(first.view, 'button', 'Create workspace')));
    const done = a.read();
    ok(done.view.includes('Workspace created for ana@example.org'), done.view);
    const flow = a.total() + tokens(log.join('\n'));
    ok(flow < 700, `${flow} tokens for the whole flow in one call (${a.spent.length} reads)`);
    equal(a.spent.length, 2, 'two reads: the first one, and the one at the end');
  }));

check('browser reads: a password or a card is never in the read, and a list takes the option that reads so', () =>
  onBench(secrets, (root) => {
    const a = agent(root);
    const first = a.read();
    ok(!first.view.includes('correct horse'), first.view);
    ok(/Recovery phrase" .*= "\[hidden\]"/.test(first.view), first.view);
    ok(/Display name" = "Ana"/.test(first.view), 'an ordinary field shows its value');
    const secret = a.refOf(first.view, 'textbox', 'Recovery phrase');
    equal(a.type(secret, 'x').ok, false, 'and is never typed in');
    equal(root.querySelector<HTMLInputElement>('#rec')!.value, 'correct horse battery staple');
  }));

check('browser reads: a custom box and a field named by another element are hidden like a password field', () =>
  onBench(secrets, (root) => {
    root.insertAdjacentHTML('beforeend', '<div role="textbox" contenteditable="true" aria-label="Recovery code">hunter2-word</div><span id="lbl">API key</span><input type="text" aria-labelledby="lbl" value="plainvalue99">');
    const view = agent(root).read().view;
    ok(!view.includes('hunter2-word') && !view.includes('plainvalue99'), view);
    ok(/"Recovery code" .*= "\[hidden\]"/.test(view) && /"API key" .*= "\[hidden\]"/.test(view), view);
  }));

check('browser reads: a click waits for a control that is on view, enabled and still, and a flow can check the text', () =>
  onBench(signup, (root) => {
    root.insertAdjacentHTML('beforeend', '<button id="go" style="position: relative">Go ahead</button><button disabled>Stuck</button><p>Order sent</p>');
    const a = agent(root);
    const view = a.read().view;
    const go = a.refOf(view, 'button', 'Go ahead');
    const clicks: Event[] = [];
    root.addEventListener('click', (e) => { clicks.push(e); e.preventDefault(); }, true);
    const probe = act({ kind: 'click', ref: go, memo: null as unknown, probe: true } as never) as unknown as { ok: boolean; why?: string };
    equal(probe.why, 'noref', 'a number needs the memo, as for a click');
    const read = act({ kind: 'click', ref: go, memo: a.read().memo, probe: true } as never) as unknown as { ok: boolean; rect?: number[] };
    ok(read.ok && read.rect?.length === 4 && clicks.length === 0, 'a probe reads the box and does not click');
    // The box it read is not where the control is now: the control moved after the look.
    const moved = act({ kind: 'click', ref: go, memo: a.read().memo, rect: read.rect!.map((v) => v + 10) } as never) as Answer;
    deepEqual([moved.ok, moved.why, clicks.length], [false, 'moving', 0]);
    equal((act({ kind: 'click', ref: go, memo: a.read().memo } as never) as Answer).ok, true);
    equal(clicks.length, 1);
    const stuck = a.refOf(view, 'button', 'Stuck');
    deepEqual((act({ kind: 'click', ref: stuck, memo: a.read().memo } as never) as Answer).why, 'disabled');
    equal((act({ kind: 'has', text: 'order  SENT' }) as Answer).ok, true);
    equal((act({ kind: 'has', text: 'order lost' }) as Answer).ok, false);
  }));

check('browser reads: numbers can be drawn on the controls for a picture and taken off, one control has its box, and a point with a control on it is refused', () =>
  onBench(signup, (root) => {
    root.insertAdjacentHTML('beforeend', '<div id="map" style="position: fixed; left: 500px; top: 300px; width: 120px; height: 80px; background: #cde;"></div>');
    const a = agent(root);
    const first = a.read();
    const mark = 'zz';
    const drawn = act({ kind: 'label', memo: a.read().memo, mark } as never) as unknown as { ok: boolean; count?: number };
    const layer = document.getElementById(`${mark}-labels`);
    ok(drawn.ok && (drawn.count ?? 0) >= 5 && layer && layer.children.length === drawn.count, `${drawn.count} numbers drawn`);
    ok(!!layer!.textContent?.includes(String(a.refOf(first.view, 'button', 'Create workspace'))), 'each is the number of its control');
    act({ kind: 'unlabel', mark } as never);
    equal(document.getElementById(`${mark}-labels`), null, 'and taken off again');
    const create = a.refOf(first.view, 'button', 'Create workspace');
    const box = act({ kind: 'click', ref: create, memo: a.read().memo, box: true } as never) as unknown as { ok: boolean; rect?: number[] };
    ok(box.ok && box.rect!.length === 4 && box.rect![2]! > 0, 'a disabled control still has a box to see');
    const button = root.querySelector('button')!.getBoundingClientRect();
    const onControl = act({ kind: 'at', x: Math.round(button.x + 2), y: Math.round(button.y + 2) } as never) as Answer;
    deepEqual([onControl.ok, onControl.why], [false, 'control']);
    equal((act({ kind: 'at', x: 520, y: 320 } as never) as Answer).ok, true, 'a map has no control');
  }));

check('browser reads: a list takes the option that reads so', () =>
  onBench(shop, (root) => {
    const a = agent(root);
    const first = a.read();
    const sort = a.refOf(first.view, 'combobox', 'Sort by');
    equal(a.type(sort, 'price: high').ok, true);
    equal(root.querySelector<HTMLSelectElement>('select[name=sort]')!.value, 'Price: high to low');
    equal(a.type(sort, 'free shipping').ok, false);
  }));
