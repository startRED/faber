import { render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { api } from './api.ts';
import { App } from './App.tsx';
import { applyLook } from './appearance.ts';
import { store } from './running.ts';

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');

// The running list is listened for first, and only then is the snapshot asked for. Both carry a revision and the store
// keeps the highest one, so a change that lands between the two is never lost and an old snapshot never overwrites a
// newer event.
await api.on((e) => {
  if (e.type === 'running') store.offer({ revision: e.revision, items: e.items });
});
const state = await api.state();
store.offer(state.running);

// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
applyLook(state.look);

render(() => <App initial={state} />, document.getElementById('root')!);
