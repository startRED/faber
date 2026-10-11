import { test } from 'node:test';
import assert from 'node:assert/strict';
import { modelOptions } from '../ui/models.ts';

const gpt = (models: Parameters<typeof modelOptions>[1]) =>
  modelOptions(['codex'], models).filter((o) => o.choice.vendor === 'codex').map((o) => o.label);

test('every GPT model is listed by its own name, with no generic ChatGPT entry', () => {
  const models = [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol', isDefault: true },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna', isDefault: false },
  ];
  assert.deepEqual(gpt(models), ['GPT-6.1 Sol', 'GPT-6 Luna']);
  assert.ok(modelOptions(['codex'], models).every((o) => o.choice.vendor === 'auto' || o.choice.model));
});

test('before the account lists its models, Auto stands in for them', () => {
  assert.deepEqual(gpt([]), []);
  assert.deepEqual(modelOptions(['codex'], []).map((o) => o.choice.vendor), ['auto']);
});
