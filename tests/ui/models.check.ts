import { modelOptions } from '../../ui/models.ts';
import { check, deepEqual, ok } from './check.ts';

const gpt = (models: Parameters<typeof modelOptions>[1]) =>
  modelOptions(['codex'], models).filter((o) => o.choice.vendor === 'codex').map((o) => o.label);

check('models: every GPT model is listed by its own name, with no generic ChatGPT entry', () => {
  const models = [
    { id: 'gpt-6.1-sol', label: 'GPT-6.1-Sol', isDefault: true },
    { id: 'gpt-6-luna', label: 'GPT-6-Luna', isDefault: false },
  ];
  deepEqual(gpt(models), ['GPT-6.1 Sol', 'GPT-6 Luna']);
  ok(modelOptions(['codex'], models).every((o) => o.choice.vendor === 'auto' || o.choice.model));
});

check('models: before the account lists its models, Auto stands in for them', () => {
  deepEqual(gpt([]), []);
  deepEqual(modelOptions(['codex'], []).map((o) => o.choice.vendor), ['auto']);
});
