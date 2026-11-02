import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Empty } from '../../ui/Empty.tsx';
import { Scene, SCENE_NAMES } from '../../ui/illustrations/index.tsx';
import { check, equal, ok } from './check.ts';

check('scenes: each drawing is decorative, made of paths and contains no embedded content or hex colours', () => {
  const root = document.createElement('div');
  document.body.append(root);
  try {
    for (const name of SCENE_NAMES) {
      const dispose = render(() => createComponent(Scene, { name }), root);
      const svg = root.querySelector('svg')!;
      equal(svg.getAttribute('aria-hidden'), 'true');
      ok(svg.querySelector('path'), `${name} has a path`);
      ok(!svg.querySelector('image, text, style'), `${name} has no embedded content`);
      ok(!/#[\da-f]{3,8}\b/i.test(svg.outerHTML), `${name} has no hex colour`);
      dispose();
    }
  } finally { root.remove(); }
});

check('scenes: an empty state keeps its drawing inside the ledge and above its title', () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:700px';
  document.body.append(root);
  const dispose = render(() => createComponent(Empty, { title: 'No AI connected', scene: 'noAi' }), root);
  try {
    const ledge = root.querySelector<HTMLElement>('.peek')!.getBoundingClientRect();
    const drawing = root.querySelector<SVGElement>('.peek .scene')!.getBoundingClientRect();
    const title = root.querySelector<HTMLElement>('h1')!.getBoundingClientRect();
    ok(drawing.left >= ledge.left && drawing.right <= ledge.right && drawing.top >= ledge.top && drawing.bottom <= ledge.bottom, 'drawing fits the ledge');
    ok(drawing.bottom <= title.top, 'drawing stays above the title');
  } finally { dispose(); root.remove(); }
});
