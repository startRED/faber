import { render } from 'solid-js/web';
import './fonts.css';
import './styles.css';
import { api } from './api.ts';
import { App } from './App.tsx';
import { applyLook } from './appearance.ts';

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');

// The look is on the page before anything is drawn, so the first frame already has the theme's colours.
const state = await api.state();
applyLook(state.look);

render(() => <App initial={state} />, document.getElementById('root')!);
