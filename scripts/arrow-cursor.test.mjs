// node --test scripts: Faber is an app, so buttons, rows and menus show the arrow; only a link in text shows the hand.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// A button drawn as an underlined link inside a sentence counts as a link in text.
const LINKS_IN_TEXT = new Set(['.det-line button']);

test('only a link in text asks for the hand cursor', () => {
  const css = readFileSync(new URL('../ui/styles.css', import.meta.url), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
  const hands = [];
  for (const [, selectors, body] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!/cursor:\s*pointer/.test(body)) continue;
    for (const selector of selectors.split(',').map((one) => one.trim()))
      if (!LINKS_IN_TEXT.has(selector)) hands.push(selector);
  }
  assert.deepEqual(hands, [], 'these controls show the hand instead of the arrow (docs/design/README.md)');
});
