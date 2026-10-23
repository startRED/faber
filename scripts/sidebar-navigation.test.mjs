import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { runInNewContext } from 'node:vm';

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
