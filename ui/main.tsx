import { render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { type AppEvent, api } from './api.ts';
import { App } from './App.tsx';
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
const state = await api.state();
store.offer(state.running);

// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
applyLook(state.look);

render(() => <App initial={state} inbox={inbox} />, document.getElementById('root')!);
