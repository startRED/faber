import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const script = fileURLToPath(new URL('../../bench/proofs/todo.mjs', import.meta.url));

function proof(acceptZeroFor) {
  const folder = mkdtempSync(join(tmpdir(), 'faber-todo-proof-'));
  try {
    writeFileSync(join(folder, 'todo.js'), `
      const fs = require('node:fs');
      const tasks = fs.existsSync('tasks.json') ? JSON.parse(fs.readFileSync('tasks.json', 'utf8')) : [];
      const [command, argument] = process.argv.slice(2);
      const save = () => fs.writeFileSync('tasks.json', JSON.stringify(tasks));
      const fail = () => { console.error('Invalid command or argument'); process.exitCode = 1; };
      if (command === 'add' && argument) { tasks.push({ text: argument, done: false }); save(); console.log('Added'); }
      else if (command === 'list') tasks.forEach((task, i) => console.log((i + 1) + '. [' + (task.done ? 'x' : ' ') + '] ' + task.text));
      else if (command === 'done' || command === 'remove') {
        const number = Number(argument);
        if (command === ${JSON.stringify(acceptZeroFor)} && number === 0) console.log('Accepted zero');
        else if (!Number.isInteger(number) || number < 1 || number > tasks.length) fail();
        else { if (command === 'done') tasks[number - 1].done = true; else tasks.splice(number - 1, 1); save(); console.log('Updated'); }
      } else fail();
    `);
    writeFileSync(join(folder, 'todo.test.cjs'), `
      const test = require('node:test'), assert = require('node:assert/strict');
      const { spawnSync } = require('node:child_process');
      test('listing tasks succeeds', () => assert.equal(spawnSync(process.execPath, ['todo.js', 'list'], { cwd: __dirname, windowsHide: true }).status, 0));
    `);
    writeFileSync(join(folder, 'README.md'), '# Usage\nnode todo.js add milk\n');
    const env = { ...process.env };
    delete env.NODE_TEST_CONTEXT;
    const result = spawnSync(process.execPath, [script, folder], { cwd: folder, env, encoding: 'utf8', windowsHide: true, timeout: 30000 });
    assert.equal(result.status, 0, result.stderr);
    return JSON.parse(result.stdout);
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}

test('todo proof accepts a tool that rejects zero for both numbered commands', () => {
  const result = proof(null);
  assert.equal(result.pass, true, JSON.stringify(result.checks));
});

for (const command of ['done', 'remove']) {
  test(`todo proof rejects a tool that accepts zero for ${command}`, () => {
    const result = proof(command);
    assert.equal(result.pass, false);
    assert.ok(result.checks.some(check => !check.ok && check.name.includes('bad number')));
  });
}
