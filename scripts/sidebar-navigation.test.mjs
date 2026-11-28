import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { stripTypeScriptTypes } from 'node:module';

const source = readFileSync(new URL('../ui/App.tsx', import.meta.url), 'utf8');
const start = source.indexOf('  const switchArea = (');
const end = source.indexOf('  const addProject = ', start);
assert.ok(start >= 0 && end > start, 'exercise the real area-switch handler');
const body = source.slice(start, end).match(/=> \{([\s\S]*)\n  \};\s*$/)?.[1];
assert.ok(body, 'the area-switch handler must have an executable body');

for (const next of ['chat', 'project']) {
  test(`sidebar: switching to ${next} preserves open navigation and selects the requested area`, () => {
    let overlayOpen = true;
    const calls = [];
    runInNewContext(body, {
      next,
      closeSettings: () => calls.push('settings closed'),
      closeSide: () => { overlayOpen = false; },
      projectId: () => 'fixture-project',
      fresh: (place) => calls.push({ area: place.area, projectId: place.projectId }),
    });
    assert.equal(overlayOpen, true, 'switching areas must leave the open project/chat list reachable');
    assert.deepEqual(calls, ['settings closed', { area: next, projectId: 'fixture-project' }]);
  });
}

test('sidebar: row arrows preserve activation, skip hidden children and respect text editing', () => {
  const sidebar = readFileSync(new URL('../ui/Sidebar.tsx', import.meta.url), 'utf8');
  const start = sidebar.indexOf('<div class="side-scroll"');
  const body = sidebar.slice(start).match(/onKeyDown=\{\(e\) => \{([\s\S]*?)\n      \}\} onMouseLeave=/)?.[1];
  assert.ok(body, 'the sidebar list must receive keyboard navigation events');
  let focused;
  let opened = true;
  let scrolled = 0;
  const parent = { dataset: { projectId: 'p' }, querySelector: () => head };
  class Button {
    constructor(owner, hidden = false) { this.owner = owner; this.hidden = hidden; }
    matches(selector) { return selector === '.item'; }
    closest(selector) { return selector === '.proj' ? this.owner : this.hidden ? {} : null; }
    getClientRects() { return [{}]; }
    getAttribute() { return String(opened); }
    focus() { focused = this; }
    scrollIntoView() { scrolled++; }
  }
  const head = new Button(parent);
  const child = new Button(parent);
  const last = new Button(null);
  const rows = [head, child, last];
  const handle = runInNewContext(stripTypeScriptTypes(`(e) => {${body}}`), {
    HTMLButtonElement: Button,
    toggle: (id, value) => { assert.equal(id, 'p'); opened = value; child.hidden = !value; },
  });
  const press = (target, key, extra = {}) => {
    const e = { target, key, currentTarget: { querySelectorAll: () => rows }, preventDefault() { this.defaultPrevented = true; }, ...extra };
    handle(e);
    return e;
  };
  focused = head;
  assert.ok(press(head, 'ArrowDown').defaultPrevented);
  assert.equal(focused, child);
  press(child, 'ArrowLeft');
  assert.equal(focused, head);
  press(head, 'ArrowLeft');
  assert.equal(opened, false);
  press(head, 'ArrowDown');
  assert.equal(focused, last);
  press(last, 'Home');
  assert.equal(focused, head);
  press(head, 'ArrowRight');
  assert.equal(opened, true);
  assert.equal(focused, head);
  press(head, 'ArrowRight');
  assert.equal(focused, child);
  press(child, 'End');
  assert.equal(focused, last);
  assert.equal(press(last, 'Enter').defaultPrevented, undefined);
  assert.equal(press(last, 'ArrowUp', { altKey: true }).defaultPrevented, undefined);
  assert.equal(press({}, 'Home').defaultPrevented, undefined);
  assert.equal(focused, last);
  assert.ok(scrolled > 0);
});
