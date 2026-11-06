const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const script = path.join(__dirname, 'todo.js');
function setup(t) {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'todo-test-'));
  t.after(() => fs.rmSync(cwd, { recursive: true, force: true }));
  return {
    file: path.join(cwd, 'tasks.json'),
    run(...args) {
      const result = spawnSync(process.execPath, [script, ...args], { cwd, encoding: 'utf8' });
      assert.ifError(result.error);
      return result;
    },
  };
}

test('missing file lists no tasks and does not create a file', t => {
  const { file, run } = setup(t);
  const result = run('list');
  assert.equal(result.status, 0);
  assert.equal(result.stdout, '');
  assert.equal(fs.existsSync(file), false);
});

test('commands persist across runs and removal shifts task numbers', t => {
  const { file, run } = setup(t);
  for (const args of [['add', 'buy', 'milk'], ['add', 'walk dog'], ['add', 'read book']]) {
    const result = run(...args);
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Added task/);
  }
  assert.equal(run('list').stdout, '1. [ ] buy milk\n2. [ ] walk dog\n3. [ ] read book\n');
  assert.equal(run('done', '2').status, 0);
  assert.equal(run('done', '2').status, 0);
  assert.equal(run('list').stdout, '1. [ ] buy milk\n2. [x] walk dog\n3. [ ] read book\n');
  assert.equal(run('remove', '1').status, 0);
  assert.equal(run('list').stdout, '1. [x] walk dog\n2. [ ] read book\n');
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), [
    { text: 'walk dog', done: true }, { text: 'read book', done: false },
  ]);
  assert.equal(run('remove', '2').status, 0);
  assert.equal(run('remove', '1').status, 0);
  assert.equal(run('list').stdout, '');
});

test('bad commands and arguments fail clearly without changing saved data', t => {
  const { file, run } = setup(t);
  assert.equal(run('add', 'keep me').status, 0);
  const before = fs.readFileSync(file, 'utf8');
  const invalid = [[], ['unknown'], ['add'], ['add', '  '], ['add', 'two\nlines'], ['list', 'extra']];
  for (const command of ['done', 'remove']) {
    for (const args of [[], ['0'], ['-1'], ['1.5'], ['abc'], ['1abc'], ['2'], ['1', '2'], ['9007199254740993']]) {
      invalid.push([command, ...args]);
    }
  }
  for (const args of invalid) {
    const result = run(...args);
    assert.equal(result.status, 1, JSON.stringify(args));
    assert.match(result.stderr, /Error: .+/);
    assert.equal(result.stdout, '');
    assert.equal(fs.readFileSync(file, 'utf8'), before);
  }
});

test('corrupt or invalid saved data is reported and preserved', t => {
  const { file, run } = setup(t);
  for (const content of ['{broken', '{}', '[null]', '[{"text":"task","done":"false"}]']) {
    fs.writeFileSync(file, content);
    for (const args of [['list'], ['add', 'new task']]) {
      const result = run(...args);
      assert.equal(result.status, 1);
      assert.match(result.stderr, /tasks\.json/);
      assert.equal(fs.readFileSync(file, 'utf8'), content);
    }
  }
});

test('storage errors exit unsuccessfully without a success confirmation', t => {
  const { file, run } = setup(t);
  fs.mkdirSync(file);
  const result = run('add', 'new task');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Cannot read tasks\.json/);
  assert.equal(result.stdout, '');
});
