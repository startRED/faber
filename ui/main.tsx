import { render } from 'solid-js/web';
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './styles.css';
import { App } from './App.tsx';

// Only a test build (`vite build --mode test`) carries the screen's checks; a production build drops this line.
if (import.meta.env.MODE === 'test') await import('../tests/ui/index.ts');

render(() => <App />, document.getElementById('root')!);
