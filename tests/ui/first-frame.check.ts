import { check, ok } from './check.ts';
import html from '../../ui/index.html?raw';

// R157: the page paints a themed, non-blank first frame before any script runs, so no reveal can show an empty window.
check('first frame: index.html paints the mark on a themed background for both schemes, with no text and no motion', () => {
  const head = html.slice(0, html.indexOf('<script'));
  ok(/<svg[^>]*>\s*<path/.test(head), 'the mark is in the page itself');
  ok(/html, body \{[^}]*background: #[0-9a-f]{6}/.test(head), 'an inline dark background');
  ok(/prefers-color-scheme: light\) \{[^}]*background: #[0-9a-f]{6}/.test(head), 'an inline light background');
  ok(!/animation|transition/.test(head.replace(/<!--[\s\S]*?-->/g, '')), 'no motion');
  ok(html.indexOf('id="first"') < html.indexOf('<script'), 'the frame sits before the script');
  const visible = head.replace(/<!--[\s\S]*?-->/g, '').replace(/<style[\s\S]*?<\/style>/, '').replace(/<title>[\s\S]*?<\/title>/, '').replace(/<[^>]*>/g, '').trim();
  ok(visible === '', 'no text, so no language is needed');
});
