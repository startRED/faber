import { check, ok } from './check.ts';
import html from '../../ui/index.html?raw';
import css from '../../ui/styles.css?raw';

// R157: the page paints a themed, non-blank first frame before any script runs, so no reveal can show an empty window.
check('first frame: index.html paints the mark, styles.css themes it for both schemes, with no text, no motion and no inline style', () => {
  const head = html.slice(0, html.indexOf('<script'));
  const bare = head.replace(/<!--[\s\S]*?-->/g, '');
  const first = css.slice(css.indexOf('#first {'), css.indexOf('html, body, #root'));
  ok(/<svg[^>]*>\s*<path/.test(head), 'the mark is in the page itself');
  ok(/#first \{[^}]*background: #[0-9a-f]{6}/.test(first), 'a dark background');
  ok(/prefers-color-scheme: light\) \{[^}]*background: #[0-9a-f]{6}/.test(first), 'a light background');
  ok(!/animation|transition/.test(first), 'no motion');
  // The shell adds a hash to the CSP for an inline style, and a CSP with a hash ignores 'unsafe-inline': every style the page adds later is blocked.
  ok(!/<style|style=/.test(bare), 'no inline style in the page');
  ok(html.indexOf('id="first"') < html.indexOf('<script'), 'the frame sits before the script');
  const visible = bare.replace(/<title>[\s\S]*?<\/title>/, '').replace(/<[^>]*>/g, '').trim();
  ok(visible === '', 'no text, so no language is needed');
});
