// Offline only: consume standing_context_size_matrix output and reuse Faber's existing token score.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tokens } from '../../../../scripts/tool-tokens.mjs';

const [input, phase] = process.argv.slice(2);
assert(['before', 'after'].includes(phase), 'usage: node measure.mjs <test-log> before|after');
const line = readFileSync(input, 'utf8').split(/\r?\n/).find(line => line.startsWith('CONTEXT_SIZE_JSON='));
assert(line, 'run standing_context_size_matrix with --nocapture');
const rows = JSON.parse(line.slice('CONTEXT_SIZE_JSON='.length));
assert.equal(rows.length, 120);
const size = text => ({ chars: [...text].length, tokens: tokens(text) });
const measured = rows.map(({ blocks, tools, ...row }) => {
  if (row.area === 'chat') for (const key of ['project_details', 'bigger_offer', 'lead', 'start_card']) assert.equal(blocks[key], '');
  const measuredBlocks = Object.fromEntries(Object.entries(blocks).map(([name, text]) => [name, size(text)]));
  return { ...row, blocks: measuredBlocks,
    total: Object.values(measuredBlocks).reduce((sum, block) => ({ chars: sum.chars + block.chars, tokens: sum.tokens + block.tokens }), { chars: 0, tokens: 0 }),
    tools: tools.map(tool => ({ name: tool.name, ...size(JSON.stringify(tool)), description: size(tool.description), schema: size(JSON.stringify(tool.inputSchema)) })) };
});
const root = dirname(fileURLToPath(import.meta.url));
const toolSizes = {};
for (const row of measured) for (const tool of row.tools) toolSizes[`${row.vendor}/${tool.name}`] = tool;
const result = { tools: toolSizes, rows: measured.map(row => ({ ...row, tools: row.tools.map(tool => tool.name) })) };
const variants = readFileSync(input, 'utf8').split(/\r?\n/).find(line => line.startsWith('CONTEXT_VARIANTS_JSON='));
if (variants) result.variants = Object.fromEntries(Object.entries(JSON.parse(variants.slice('CONTEXT_VARIANTS_JSON='.length))).map(([name, text]) => [name, size(text)]));
if (phase === 'after') {
  const baseline = JSON.parse(readFileSync(join(root, 'before.json'), 'utf8'));
  assert.deepEqual(result.tools, baseline.tools, 'tool schemas and descriptions stay unchanged');
  const before = baseline.rows;
  assert.equal(before.length, result.rows.length);
  result.rows.forEach((row, i) => {
    assert.deepEqual(Object.fromEntries(Object.entries(row).filter(([key]) => !['blocks', 'tools', 'total'].includes(key))),
      Object.fromEntries(Object.entries(before[i]).filter(([key]) => !['blocks', 'tools', 'total'].includes(key))));
    for (const [key, value] of Object.entries(row.blocks)) {
      if (JSON.stringify(value) === JSON.stringify(before[i].blocks[key])) continue;
      assert((key === 'skill_index' && row.vendor === 'codex' && row.area === 'project' && row.kits && row.permissions !== 'Isolated')
        || (key === 'tool_definitions' && row.vendor === 'claude' && ['Plan', 'Isolated'].includes(row.permissions) && ['owner', 'member'].includes(row.role)), `unexpected cut: ${i}/${key}`);
      assert(value.chars < before[i].blocks[key].chars && value.tokens < before[i].blocks[key].tokens);
    }
  });
}
writeFileSync(join(root, `${phase}.json`), `${JSON.stringify(result)}\n`);
const columns = ['persona', 'project_details', 'bigger_offer', 'lead', 'group', 'start_card', 'guide_index', 'native_skill_discovery', 'skill_index', 'kit_instructions', 'base', 'tool_definitions'];
const cell = value => `${value.chars}/${value.tokens}`;
const table = [
  `| Vendor / area / role / mode / kits | ${columns.join(' | ')} | Block sum |`,
  `| --- | ${columns.map(() => '---:').join(' | ')} | ---: |`,
  ...measured.map(row => `| ${row.vendor}/${row.area}/${row.role}/${row.permissions}/${row.kits ? 'yes' : 'no'} | ${columns.map(key => cell(row.blocks[key])).join(' | ')} | ${cell(row.total)} |`),
];
writeFileSync(join(root, `${phase}.table.md`), `${table.join('\n')}\n`);
console.log(`${phase}: ${measured.length} rows, characters/estimated tokens; no model calls`);
