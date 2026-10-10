import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource-variable/geist';
import '@fontsource-variable/geist-mono';
import './styles.css';
import { App } from './App.tsx';

const version = new URLSearchParams(location.search).get('version') ?? 'dev';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App version={version} />
  </StrictMode>,
);
