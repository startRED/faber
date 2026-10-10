import { test } from 'node:test';
import assert from 'node:assert/strict';
import { listCodexModels } from '../src/engine/codex/models.ts';
import { fixture, replay } from './support/replay.ts';

test('the ChatGPT model list comes from the account, without hidden models', async () => {
  const proc = replay(fixture('codex', 'models'));
  const models = await listCodexModels({ bin: 'codex', cwd: '.', spawn: () => proc });
  assert.deepEqual(proc.mismatches, []);
  assert.deepEqual(
    models.map((m) => [m.id, m.label, m.isDefault]),
    [
      ['gpt-6.1-sol', 'GPT-6.1-Sol', true],
      ['gpt-6-astra', 'GPT-6-Astra', false],
    ],
  );
});

test('a CLI that never answers gives an empty list instead of hanging', async () => {
  const models = await listCodexModels({ bin: 'codex', cwd: '.', spawn: () => replay([]), timeoutMs: 50 });
  assert.deepEqual(models, []);
});
