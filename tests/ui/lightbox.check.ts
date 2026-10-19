import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Lightbox, Thumb } from '../../ui/Lightbox.tsx';
import { t } from '../../ui/i18n.ts';
import { check, equal, ok } from './check.ts';

const PIXEL = 'data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7';

check('lightbox: a thumbnail opens full size, arrows and keys step through the pictures, Esc closes', () => {
  const root = document.createElement('div');
  root.dataset.motion = 'none';
  document.body.append(root);
  const dispose = render(
    () => [
      (() => {
        const gallery = document.createElement('div');
        gallery.dataset.gallery = '';
        render(() => ['one', 'two', 'three'].map((alt) => createComponent(Thumb, { src: `${PIXEL}#${alt}`, alt })), gallery);
        return gallery;
      })(),
      createComponent(Lightbox, {}),
    ],
    root,
  );
  try {
    const shown = () => root.querySelector<HTMLImageElement>('.lightbox > img')?.alt;
    const button = (name: string) => root.querySelector<HTMLButtonElement>(`.lightbox button[aria-label="${t(name as 'image.next')}"]`)!;
    const key = (k: string) => window.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
    equal(shown(), undefined, 'nothing is open before a click');
    root.querySelectorAll<HTMLButtonElement>('.zoom')[1].click();
    equal(shown(), 'two', 'the clicked picture opens');
    equal(root.querySelector('.lb-count')?.textContent, t('image.count', { n: 2, total: 3 }));
    key('ArrowRight');
    equal(shown(), 'three', 'the right arrow key goes on');
    ok(button('image.next').disabled, 'the last picture has no next');
    key('ArrowRight');
    equal(shown(), 'three', 'the last picture stays');
    button('image.previous').click();
    equal(shown(), 'two', 'the arrow on the side goes back');
    key('ArrowLeft');
    ok(button('image.previous').disabled, 'the first picture has no previous');
    key('Escape');
    equal(shown(), undefined, 'Esc closes it');
  } finally {
    dispose();
    root.remove();
  }
});
