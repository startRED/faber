import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { measure, table, tokens } from './tool-tokens.mjs';

test('a longer text never costs fewer tokens than a part of it', () => {
  assert.equal(tokens(''), 0);
  assert.ok(tokens('Hands one piece of work to a helper.') < tokens('Hands one piece of work to a helper, then waits.'));
  assert.equal(tokens('a, b'), 3);
  assert.equal(tokens('12345'), 2);
});

test('the table shows what each cut saved and the total', () => {
  const tool = (description, schema) => ({ mcp: [{ name: 'helper', description, inputSchema: { type: schema } }] });
  const persona = { claude: 'Be brief.', codex: 'Be brief.' };
  const before = measure(tool('Hands a long piece of work to a helper agent.', 'object'), persona);
  const after = measure(tool('Hands work to a helper.', 'object'), persona);
  const out = table(before, after);
  assert.match(out, /helper description\s+\d+\s+\d+\s+\d+%/);
  assert.match(out, /all tools/);
});
