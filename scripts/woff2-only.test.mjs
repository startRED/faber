// node --test scripts: fonts ship as woff2 only.

import assert from 'node:assert/strict';
import test from 'node:test';
import { woff2Only, woff2OnlyPlugin } from './woff2-only.mjs';

const plex = "url(./files/ibm-plex-sans-latin-400-normal.woff2) format('woff2'), url(./files/ibm-plex-sans-latin-400-normal.woff) format('woff')";

test('the woff fallback goes and the woff2 face stays', () => {
  assert.equal(woff2Only(plex), "url(./files/ibm-plex-sans-latin-400-normal.woff2) format('woff2')");
});

test('a variable face, which has only woff2, is left alone', () => {
  const variable = "url(../node_modules/@fontsource-variable/archivo/files/archivo-latin-wdth-normal.woff2) format('woff2-variations')";
  assert.equal(woff2Only(variable), variable);
});

test('only the src of a font face is rewritten', () => {
  const plugin = woff2OnlyPlugin();
  const face = { value: plex, parent: { type: 'atrule', name: 'font-face' } };
  const rule = { value: plex, parent: { type: 'rule' } };
  plugin.Declaration.src(face);
  plugin.Declaration.src(rule);
  assert.equal(face.value, "url(./files/ibm-plex-sans-latin-400-normal.woff2) format('woff2')");
  assert.equal(rule.value, plex);
});
