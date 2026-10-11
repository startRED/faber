import { test } from 'node:test';
import assert from 'node:assert/strict';
import { modelOptions } from '../ui/models.ts';

const chatgpt = (models: Parameters<typeof modelOptions>[1]) =>
  modelOptions(['codex'], models).filter((o) => o.choice.vendor === 'codex').map((o) => o.label);

test('ChatGPT names the recommended model it runs and lists it only once', () => {
  const models = [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol', isDefault: true },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna', isDefault: false },
  ];
  assert.deepEqual(chatgpt(models), ['ChatGPT · GPT-6.1 Sol', 'GPT-6 Luna']);
});

test('before the account lists its models, ChatGPT shows no model it cannot name', () => {
  assert.deepEqual(chatgpt([]), ['ChatGPT']);
});
