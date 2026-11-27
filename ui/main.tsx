import { createEffect, createSignal, Show } from 'solid-js';
import { Portal, render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { type AppEvent, api, failureOf } from './api.ts';
import { App } from './App.tsx';
import { ContextMenuHost } from './ContextMenu.tsx';
import { installContextMenu } from './contextmenu.ts';
import { applyLook } from './appearance.ts';
import { createFrameFeed } from './frame.ts';
import { createInbox } from './inbox.ts';
import { StartFailed } from './StartFailed.tsx';
import { createHold, onQuiet, watchGames } from './quiet.ts';
import { store } from './running.ts';

const checksOnBoot = import.meta.env.MODE === 'test' && new URLSearchParams(location.search).has('checks');

// The window starts hidden and shows once a frame is on screen. The static first frame (ui/index.html) is that frame, so
// the shell hears it now, before anything below waits for the host: a slow engine start no longer leaves the window to
// the shell's failsafe (3.3 s in 41 of the owner's 59 shown starts, 2026-11-25). The timer covers a hidden window whose
// frames wait to be shown. A conversation id the answer carries waits for the real screen, which listens for it.
let ready: Promise<string | null> | undefined;
const firstFrame = () => {
  ready ??= api.screenReady().catch(() => null);
};
requestAnimationFrame(() => requestAnimationFrame(firstFrame));
setTimeout(firstFrame, 150);

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') {
  await import('../tests/ui/crash.ts');
  await import('./waiting.check.ts');
}

// Events are listened for first, and only then is the snapshot asked for. The running list carries a revision and the
// store keeps the highest one, so a change that lands between the two is never lost and an old snapshot never overwrites
// a newer event. Every other event waits in the inbox until the app is drawn and takes them in order, so the end of a
// turn that lands before then still reaches the conversation.
const inbox = createInbox<AppEvent>();
// Set once the shell stopped restarting a host that keeps failing: the start-failure screen covers everything. Only the
// Electron shell sends `hostStopped` (`electron/main.ts`), with the code as `0xC0000135` or `1`; the app never sees it.
const [stopped, setStopped] = createSignal<string>();
type HostStopped = { type: 'hostStopped'; code: string };
const isHostStopped = (e: unknown): e is HostStopped => (e as { type?: unknown } | null)?.type === 'hostStopped';
// Events reach the screen once per frame, together (ui/frame.ts), and while a game is in front (ui/quiet.ts) they wait folded
// into one and land together when the person comes back.
const frame = createFrameFeed<AppEvent>((e) => {
  const raw: unknown = e;
  if (isHostStopped(raw)) setStopped(raw.code);
  else if (e.type === 'running') store.offer({ revision: e.revision, items: e.items });
  else inbox.push(e);
});
const hold = createHold(frame.push);
onQuiet(hold.hold);
await api.on(hold.push);
// The engine reads the quota more often while the window is open and at once when it gains focus (nothing sent to a model).
const attention = (focused: boolean) => void api.quotaAttention(document.visibilityState === 'visible', focused && document.hasFocus()).catch(() => {});
window.addEventListener('focus', () => attention(true));
window.addEventListener('blur', () => attention(false));
document.addEventListener('visibilitychange', () => attention(document.visibilityState === 'visible'));
// A host restarting while the screen starts fails these calls; they are asked again until they answer or the shell says
// it stopped for good (`error.host.stopped`, R459), and only then does the start-failure screen replace the first frame. No
// timer gives up: a first start can be slow (an antivirus reading the new sidecar), and only the shell knows it failed.
async function boot() {
  await api.setClock(-new Date().getTimezoneOffset() * 60);
  const state = await api.state();
  // Read before the first draw: a window that opens during the install at open paints its opening screen at once.
  const update = await api.updateStatus().catch(() => undefined);
  return { state, update };
}
let booted: Awaited<ReturnType<typeof boot>> | undefined;
for (let tries = 1; !booted && stopped() === undefined; tries++) {
  try {
    booted = await boot();
  } catch (failure) {
    const { key, detail } = failureOf(failure);
    if (key === 'error.host.stopped') setStopped(String(detail ?? ''));
    else await new Promise((done) => setTimeout(done, tries < 40 ? 250 : 1000));
  }
}
const state = booted?.state;
const update = booted?.update;
if (state) {
  store.offer(state.running);
  // The look is on the page before anything is drawn, so the first frame already has the theme's colours.
  applyLook(state.look);
}

// The shell is asked whether a game is in front only while the window is shown without the focus.
watchGames(api.gameInFront);

// Right-click is Faber's own (ui/contextmenu.ts): the browser's menu and its reload and print shortcuts never reach the person.
installContextMenu();

render(
  () => (
    <>
      <Show when={state}>{(initial) => !checksOnBoot && <App initial={initial()} update={update} inbox={inbox} />}</Show>
      {/* Over the app when the host stops later, so its drafts stay where they are; the app beneath is inert meanwhile. */}
      <Show when={stopped()}>{(code) => <Portal><StartFailed code={code()} /></Portal>}</Show>
      <ContextMenuHost />
    </>
  ),
  document.getElementById('root')!,
);
createEffect(() => {
  const root = document.getElementById('root');
  if (root) root.inert = stopped() !== undefined;
});
// The static first frame (ui/index.html) has done its job: the real screen is in the page now.
document.getElementById('first')?.remove();

// Checks may move focus: start them only after the app's first mount has placed its own focus.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');

// A screen loaded again had no time to let Faber's browser go: box 0 does so now and makes the shell forget every box, so the
// numbers of this screen start again from 1 whatever the clock does (R161 step 7).
void api.sitePark(0).catch(() => {});

// The real screen is mounted: the conversation the shell asked to show (a notice clicked before the window opened) opens now.
firstFrame();
void ready?.then((id) => {
  if (id) window.dispatchEvent(new CustomEvent('faber:show-conversation', { detail: id }));
});

// Checks own their App fixtures: start after the shared bootstrap, without a second App changing their language or state.
if (import.meta.env.MODE === 'test' && checksOnBoot) {
  const { runChecksOnBoot } = await import('../tests/ui/index.ts');
  await runChecksOnBoot();
}
