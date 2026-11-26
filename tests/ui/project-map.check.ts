import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message, type Room } from '../../ui/api.ts';
import { ProjectMapPane, relativeTo, roomOf } from '../../ui/ProjectMap.tsx';
import { t } from '../../ui/i18n.ts';
import { check, equal, frames, ok } from './check.ts';

const picture = (name: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><title>${name}</title><rect width="160" height="100" fill="#39c"/></svg>`)}`;

const ROOMS: Room[] = [
  { name: '', kind: 'screen', at: 'src/pages/index.tsx', recent: [], touched: false },
  {
    name: 'Login', kind: 'screen', at: 'src/pages/Login.tsx', recent: ['src/pages/Login.tsx'], touched: true,
    changedBy: { message: 'q2', answer: 'a2', index: 3 },
    picture: { answer: 'a2', page: { page: 1, before: 'before-a2-1.jpg', after: 'after-a2-1.jpg' } },
  },
  { name: 'components', kind: 'part', at: 'src/components', recent: ['src/components/Button.tsx'], touched: false, changedBy: { message: 'q1', answer: 'a1', index: 1 } },
];

const done = (id: string, tools: Message['tools'] = []): Message => ({ id, role: 'assistant', at: 1, text: 'Done.', status: 'done', tools });

/** Draws the map over a fake host that answers `rooms` and records each read, each picture and each action. */
async function map(messages: () => Message[], rooms: Room[] | 'fail' = ROOMS) {
  const original = { projectRooms: api.projectRooms, sitePicture: api.sitePicture };
  const asked: string[] = [];
  const shown: string[] = [];
  const back: number[] = [];
  const asks: string[] = [];
  Object.assign(api, {
    projectRooms: async (id: string) => { asked.push(id); if (rooms === 'fail') throw new Error('gone'); return rooms; },
    sitePicture: async (id: string, name: string) => { shown.push(`${id}/${name}`); return picture(name); },
  });
  const host = document.createElement('div');
  host.className = 'runcol';
  host.style.cssText = 'position:absolute;left:0;top:0;width:420px';
  document.body.append(host);
  const dispose = render(() => createComponent(ProjectMapPane, {
    conversationId: 'map', get messages() { return messages(); }, folders: ['C:\\work\\app'],
    onGoBack: (index: number) => back.push(index), onAsk: (text: string) => asks.push(text),
    onBack: () => {}, onClose: () => {},
  }), host);
  await frames(2);
  return { host, asked, shown, back, asks, close: () => { dispose(); host.remove(); Object.assign(api, original); } };
}

async function until(found: () => unknown, what: string, tries = 100) {
  for (let i = 0; i < tries; i++) {
    if (found()) return;
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error(`${what} never came`);
}

const room = (host: HTMLElement, at: string) => host.querySelector<HTMLElement>(`.map-room[data-at="${at}"]`)!;

check('project-map: the rooms list, the touched room lit, and one read per answer end', async () => {
  const [messages, setMessages] = createSignal<Message[]>([{ id: 'q2', role: 'user', at: 0, text: 'Fix login.' }, done('a2')]);
  const view = await map(messages);
  try {
    await until(() => view.host.querySelectorAll('.map-room').length === 3, 'the three rooms');
    equal([...view.host.querySelectorAll('.map-room-name')].map((n) => n.textContent).join(','), `${t('map.home')},Login,components`, 'screens then parts, the home screen named');
    equal([...view.host.querySelectorAll('.map-room-kind')].map((n) => n.textContent).join(','), [t('map.screen'), t('map.screen'), t('map.part')].join(','));
    ok(room(view.host, 'src/pages/Login.tsx').hasAttribute('data-lit'), 'the room the last answer changed is lit');
    equal(room(view.host, 'src/pages/Login.tsx').querySelector('.map-room-lit')?.textContent, t('map.lastAnswer'), 'and says so in words');
    equal(view.host.querySelectorAll('.map-room[data-lit]').length, 1, 'only that room');
    equal(view.shown.length, 0, 'no picture loads while its room is closed');
    // A new answer at work: the rooms its edits change light up; streaming text never reads the project again.
    setMessages((list) => [...list, { id: 'q3', role: 'user', at: 2, text: 'Now the button.' },
      { id: 'a3', role: 'assistant', at: 3, text: '', status: 'streaming', tools: [{ id: 'e1', name: 'Edit', input: { file_path: 'C:\\work\\app\\src\\components\\Button.tsx' } }] }]);
    await frames(2);
    ok(room(view.host, 'src/components').hasAttribute('data-lit'), 'the part the answer at work edits is lit');
    equal(room(view.host, 'src/components').querySelector('.map-room-lit')?.textContent, t('map.workingNow'));
    ok(!room(view.host, 'src/pages/Login.tsx').hasAttribute('data-lit'), "the last answer's room goes dark while another works");
    equal(view.asked.length, 1, 'no read while the answer streams');
    setMessages((list) => [...list.slice(0, -1), done('a3')]);
    await until(() => view.asked.length === 2, 'the read once the answer ended');
  } finally { view.close(); }
});

check('project-map: opening a room shows its changes and picture, and both actions work', async () => {
  const view = await map(() => [{ id: 'q2', role: 'user', at: 0, text: 'Fix login.' }, done('a2')]);
  try {
    await until(() => view.host.querySelectorAll('.map-room').length === 3, 'the rooms');
    const login = room(view.host, 'src/pages/Login.tsx');
    const head = login.querySelector<HTMLButtonElement>('.map-room-head')!;
    equal(head.getAttribute('aria-expanded'), 'false');
    head.click();
    await frames(2);
    equal(head.getAttribute('aria-expanded'), 'true', 'the room opens');
    ok(document.getElementById(head.getAttribute('aria-controls')!), 'and names what it opened');
    equal([...login.querySelectorAll('.map-room-recent li')].map((li) => li.textContent).join(','), 'src/pages/Login.tsx', 'its recent changes in plain files');
    await until(() => login.querySelectorAll('.before-after img').length === 2, 'the before and after picture');
    equal(view.shown.join(','), 'map/before-a2-1.jpg,map/after-a2-1.jpg', 'the pictures load only once the room is open');
    login.querySelector<HTMLButtonElement>('[data-map-back]')!.click();
    equal(view.back.join(','), '3', 'go back opens the answer that last changed it');
    login.querySelector<HTMLButtonElement>('[data-map-ask]')!.click();
    equal(view.asks.join('|'), t('map.askText', { name: 'Login', at: 'src/pages/Login.tsx' }), 'ask fills the composer with a question about it');
    // A room no answer changed has no way back, still a question, and no picture.
    const home = room(view.host, 'src/pages/index.tsx');
    home.querySelector<HTMLButtonElement>('.map-room-head')!.click();
    await frames(2);
    equal(home.querySelector('[data-map-back]'), null, 'no way back');
    ok(home.querySelector('[data-map-ask]'), 'a question');
    equal(home.querySelector('.map-room-body .side-meta')?.textContent, t('map.noChanges'));
    equal(login.querySelector('.map-room-body'), null, 'one room open at a time');
  } finally { view.close(); }
});

check('project-map: an empty folder and an unreadable one say so', async () => {
  const empty = await map(() => [], []);
  try {
    await until(() => empty.host.textContent?.includes(t('map.empty')), 'the empty line');
  } finally { empty.close(); }
  const failed = await map(() => [], 'fail');
  try {
    await until(() => failed.host.textContent?.includes(t('map.failed')), 'the failed line');
  } finally { failed.close(); }
});

check('project-map: a changed file finds its room as the engine places it', () => {
  equal(relativeTo('C:\\work\\app', 'c:/work/app/src/a.ts'), 'src/a.ts');
  equal(relativeTo('/home/me/app', '/home/me/app/src/a.ts'), 'src/a.ts');
  equal(relativeTo('/home/me/app', '/home/me/other/a.ts'), undefined);
  equal(relativeTo(undefined, './src/a.ts'), 'src/a.ts');
  const nested: Room[] = [...ROOMS, { name: 'forms', kind: 'part', at: 'src/components/forms', recent: [], touched: false }];
  equal(roomOf(nested, 'src/pages/Login.tsx')?.name, 'Login');
  equal(roomOf(nested, 'src/components/forms/Field.tsx')?.name, 'forms', 'the closest part');
  equal(roomOf(nested, 'src/components/Card.tsx')?.name, 'components');
  equal(roomOf(nested, 'src/componentsX/a.ts'), undefined, 'a folder name is matched whole');
});
