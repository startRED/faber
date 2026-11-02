import { render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { type AppEvent, api } from './api.ts';
import { App } from './App.tsx';
import { ContextMenuHost } from './ContextMenu.tsx';
import { installContextMenu } from './contextmenu.ts';
import { applyLook } from './appearance.ts';
import { createFrameFeed } from './frame.ts';
import { createInbox } from './inbox.ts';
import { createHold, onQuiet, watchGames } from './quiet.ts';
import { store } from './running.ts';

const checksOnBoot = import.meta.env.MODE === 'test' && new URLSearchParams(location.search).has('checks');

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') {
  await import('../tests/ui/crash.ts');
}

// Events are listened for first, and only then is the snapshot asked for. The running list carries a revision and the
// store keeps the highest one, so a change that lands between the two is never lost and an old snapshot never overwrites
// a newer event. Every other event waits in the inbox until the app is drawn and takes them in order, so the end of a
// turn that lands before then still reaches the conversation.
const inbox = createInbox<AppEvent>();
// Events reach the screen once per frame, together (ui/frame.ts), and while a game is in front (ui/quiet.ts) they wait folded
// into one and land together when the person comes back.
const frame = createFrameFeed<AppEvent>((e) => {
  if (e.type === 'running') store.offer({ revision: e.revision, items: e.items });
  else inbox.push(e);
});
const hold = createHold(frame.push);
onQuiet(hold.hold);
await api.on(hold.push);
await api.setClock(-new Date().getTimezoneOffset() * 60);
// The engine reads the quota more often while the window is open and at once when it gains focus (nothing sent to a model).
const attention = (focused: boolean) => void api.quotaAttention(document.visibilityState === 'visible', focused && document.hasFocus()).catch(() => {});
window.addEventListener('focus', () => attention(true));
window.addEventListener('blur', () => attention(false));
document.addEventListener('visibilitychange', () => attention(document.visibilityState === 'visible'));
const state = await api.state();
// Read before the first draw: a window that opens during the install at open paints its opening screen at once.
const update = await api.updateStatus().catch(() => undefined);
store.offer(state.running);

// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
applyLook(state.look);

// The shell is asked whether a game is in front only while the window is shown without the focus.
watchGames(api.gameInFront);

// Right-click is Faber's own (ui/contextmenu.ts): the browser's menu and its reload and print shortcuts never reach the person.
installContextMenu();

render(
  () => (
    <>
      {!checksOnBoot && <App initial={state} update={update} inbox={inbox} />}
      <ContextMenuHost />
    </>
  ),
  document.getElementById('root')!,
);
// The static first frame (ui/index.html) has done its job: the real screen is in the page now.
document.getElementById('first')?.remove();

// Checks may move focus: start them only after the app's first mount has placed its own focus.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');

// A screen loaded again had no time to let Faber's browser go: box 0 does so now and makes the shell forget every box, so the
// numbers of this screen start again from 1 whatever the clock does (R161 step 7).
void api.sitePark(0).catch(() => {});

// The window starts hidden: it shows once the first frame is painted, so there is never an empty one. The timer covers a
// page whose frames wait for a window that is not shown; the shell's own failsafe covers a page that never gets here.
let told = false;
const painted = () => {
  if (told) return;
  told = true;
  void api.screenReady().then((id) => { if (id) window.dispatchEvent(new CustomEvent('faber:show-conversation', { detail: id })); }).catch(() => {});
};
requestAnimationFrame(() => requestAnimationFrame(painted));
setTimeout(painted, 150);

// Checks own their App fixtures: start after the shared bootstrap, without a second App changing their language or state.
if (import.meta.env.MODE === 'test' && checksOnBoot) {
  const { runChecksOnBoot } = await import('../tests/ui/index.ts');
  await runChecksOnBoot();
}
