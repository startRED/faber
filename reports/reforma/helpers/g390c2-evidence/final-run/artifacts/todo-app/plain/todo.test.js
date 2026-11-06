const test = require('node:test');
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

test('tasks persist across runs, can be completed, and renumber after removal', t => {
  const { file, run } = setup(t);
  for (const text of ['buy milk', 'walk dog', 'read book']) {
    const result = run('add', ...text.split(' '));
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Added task/);
  }
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), [
    { text: 'buy milk', done: false },
    { text: 'walk dog', done: false },
    { text: 'read book', done: false },
  ]);
  assert.equal(run('list').stdout, '1. [ ] buy milk\n2. [ ] walk dog\n3. [ ] read book\n');
  assert.equal(run('done', '2').status, 0);
  assert.equal(run('done', '2').status, 0);
  assert.equal(run('list').stdout, '1. [ ] buy milk\n2. [x] walk dog\n3. [ ] read book\n');
  assert.equal(run('remove', '1').status, 0);
  assert.equal(run('list').stdout, '1. [x] walk dog\n2. [ ] read book\n');
  assert.equal(run('remove', '2').status, 0);
  assert.equal(run('remove', '1').status, 0);
  assert.equal(run('list').stdout, '');
  assert.deepEqual(JSON.parse(fs.readFileSync(file, 'utf8')), []);
});

test('missing file lists no tasks without creating a file', t => {
  const { file, run } = setup(t);
  const result = run('list');
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout, '');
  assert.equal(fs.existsSync(file), false);
});

test('invalid commands and arguments fail without changing saved tasks', t => {
  const { file, run } = setup(t);
  assert.equal(run('add', 'keep me').status, 0);
  const original = fs.readFileSync(file, 'utf8');
  const cases = [[], ['unknown'], ['add'], ['add', '   '], ['list', 'extra']];
  for (const command of ['done', 'remove']) {
    for (const args of [[], ['0'], ['-1'], ['1.5'], ['1abc'], ['NaN'], ['2'], ['1e0'], ['9007199254740992'], ['1', 'extra']]) {
      cases.push([command, ...args]);
    }
  }
  for (const args of cases) {
    const result = run(...args);
    assert.equal(result.status, 1, JSON.stringify(args));
    assert.match(result.stderr, /Error: .+/);
    assert.equal(result.stdout, '');
    assert.equal(fs.readFileSync(file, 'utf8'), original);
  }
});

test('invalid storage is reported and never overwritten', t => {
  const { file, run } = setup(t);
  for (const contents of ['{broken', '{}', '[null]', '[{"text":"x","done":"false"}]']) {
    fs.writeFileSync(file, contents);
    const result = run('add', 'new task');
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Error: .+/);
    assert.equal(fs.readFileSync(file, 'utf8'), contents);
  }
});

test('storage read and write failures return clear errors', t => {
  const { file, run } = setup(t);
  fs.mkdirSync(file);
  const readFailure = run('list');
  assert.equal(readFailure.status, 1);
  assert.match(readFailure.stderr, /Error: .+/);
  fs.rmdirSync(file);
  assert.equal(run('add', 'keep me').status, 0);
  const original = fs.readFileSync(file, 'utf8');
  const result = spawnSync(process.execPath, ['-e', `
    const fs = require('node:fs');
    fs.renameSync = () => { throw new Error('simulated write failure'); };
    process.argv = [process.execPath, ${JSON.stringify(script)}, 'add', 'new task'];
    require(${JSON.stringify(script)});
  `], { cwd: path.dirname(file), encoding: 'utf8' });
  assert.ifError(result.error);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Error: simulated write failure/);
  assert.equal(fs.readFileSync(file, 'utf8'), original);
  assert.deepEqual(fs.readdirSync(path.dirname(file)), ['tasks.json']);
});
