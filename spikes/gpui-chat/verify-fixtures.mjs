import fs from 'node:fs';
import assert from 'node:assert/strict';
const c = JSON.parse(fs.readFileSync('out/thousand.json'));
const at = 1790000000000;
const paragraph = 'Rayleigh scattering sends **short blue wavelengths** in every direction, so the sky looks blue. ';
assert.equal(c.messages.length, 1000);
for (let i = 0; i < 1000; i++) {
  const m = c.messages[i];
  assert.equal(m.id, `5f1c${i.toString(16).padStart(4, '0')}-0000-4000-8000-0000000000${(i % 256).toString(16).padStart(2, '0')}`);
  assert.equal(m.at, at + i);
  assert.equal(m.role, i % 2 ? 'assistant' : 'user');
  if (i % 2) {
    assert.equal(m.text, [paragraph.repeat(6), '- one\n- two', `\`\`\`js\nconsole.log(${i});\n\`\`\``].join('\n\n'));
    assert.equal(m.vendor, 'claude'); assert.equal(m.model, 'claude-haiku-5-5'); assert.equal(m.status, 'done');
    assert.equal('tools' in m, false);
  } else assert.equal(m.text, `Question ${i / 2}: why is the sky blue?`);
}
assert.deepEqual(JSON.parse(fs.readFileSync('out/small.json')).messages, c.messages.slice(0, 2));
console.log('Fixture content verified');
