import { render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { type AppEvent, api } from './api.ts';
import { App } from './App.tsx';
import { ContextMenuHost } from './ContextMenu.tsx';
import { installContextMenu } from './contextmenu.ts';
import { applyLook } from './appearance.ts';
import { createInbox } from './inbox.ts';
import { store } from './running.ts';

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') {
  await import('../tests/ui/crash.ts');
  await import('../tests/ui/index.ts');
}

// Events are listened for first, and only then is the snapshot asked for. The running list carries a revision and the
// store keeps the highest one, so a change that lands between the two is never lost and an old snapshot never overwrites
// a newer event. Every other event waits in the inbox until the app is drawn and takes them in order, so the end of a
// turn that lands before then still reaches the conversation.
const inbox = createInbox<AppEvent>();
await api.on((e) => {
  if (e.type === 'running') store.offer({ revision: e.revision, items: e.items });
  else inbox.push(e);
});
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

// Right-click is Faber's own (ui/contextmenu.ts): the browser's menu and its reload and print shortcuts never reach the person.
installContextMenu();

render(
  () => (
    <>
      <App initial={state} update={update} inbox={inbox} />
      <ContextMenuHost />
    </>
  ),
  document.getElementById('root')!,
);
// The static first frame (ui/index.html) has done its job: the real screen is in the page now.
document.getElementById('first')?.remove();

// The window starts hidden: it shows once the first frame is painted, so there is never an empty one. The timer covers a
// page whose frames wait for a window that is not shown; the shell's own failsafe covers a page that never gets here.
let told = false;
const painted = () => {
  if (told) return;
  told = true;
  void api.screenReady().catch(() => {});
};
requestAnimationFrame(() => requestAnimationFrame(painted));
setTimeout(painted, 150);
