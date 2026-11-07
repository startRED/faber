import { createRoot } from 'react-dom/client';
import { StillContext } from '../../kit/reduced-motion';
import FaberDemo from './index';

// Narrow the upstream preview loader to this one original scene, including in exports.
export async function mount(name: string, still: boolean) {
  if (name !== 'faber') throw new Error(`Unknown Faber demo: ${name}`);
  createRoot(document.getElementById('root')!).render(
    <div id="example"><StillContext.Provider value={still}><FaberDemo /></StillContext.Provider></div>,
  );
}
