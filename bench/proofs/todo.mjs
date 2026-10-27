// Proof of "todo-app": node todo.mjs <final folder> <pristine folder> <answer file>
// Runs the agent's own tests, then the commands the request names, in a copy of the folder.
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const [folder] = process.argv.slice(2);
const checks = [];
const check = (name, ok, detail = '') => checks.push({ name, ok: Boolean(ok), detail });

const copy = mkdtempSync(join(tmpdir(), 'faber-bench-todo-'));
cpSync(folder, copy, { recursive: true, filter: (src) => !/[\\/](\.git|node_modules)([\\/]|$)/.test(src) });
rmSync(join(copy, 'tasks.json'), { force: true });

const run = (args, options = {}) =>
  spawnSync(process.execPath, args, { cwd: copy, encoding: 'utf8', timeout: 60_000, windowsHide: true, ...options });
const lines = (r) => r.stdout.split(/\r?\n/).map((l) => l.trim()).filter(Boolean);

const tests = run(['--test']);
const passed = Number(/^(?:#|ℹ) pass (\d+)/m.exec(tests.stdout)?.[1] ?? 0);
check('the tests it wrote pass with node --test', tests.status === 0 && passed >= 1, `exit ${tests.status}, ${passed} passing`);
rmSync(join(copy, 'tasks.json'), { force: true });

check('todo.js exists', existsSync(join(copy, 'todo.js')));
check('list on no tasks exits 0', run(['todo.js', 'list']).status === 0);
check('add succeeds twice', run(['todo.js', 'add', 'buy milk']).status === 0 && run(['todo.js', 'add', 'call mum']).status === 0);
let list = lines(run(['todo.js', 'list']));
check('list numbers the tasks from 1 with an empty box', /^1\.\s*\[ \]\s*buy milk$/.test(list[0] ?? '') && /^2\.\s*\[ \]\s*call mum$/.test(list[1] ?? ''), list.join(' | '));
check('done marks the first task', run(['todo.js', 'done', '1']).status === 0 && /^1\.\s*\[x\]\s*buy milk$/i.test(lines(run(['todo.js', 'list']))[0] ?? ''));
check('remove deletes the first task and the rest shifts down', run(['todo.js', 'remove', '1']).status === 0 && /^1\.\s*\[ \]\s*call mum$/.test(lines(run(['todo.js', 'list']))[0] ?? ''));
list = lines(run(['todo.js', 'list']));
check('exactly one task is left', list.length === 1, list.join(' | '));
let saved = '';
try { saved = readFileSync(join(copy, 'tasks.json'), 'utf8'); JSON.parse(saved); } catch { saved = ''; }
check('tasks.json is valid JSON and holds the task', saved.includes('call mum'), saved.slice(0, 120));
const bad = [['done', '99'], ['done', '0'], ['remove', 'x'], ['remove', '0'], ['add'], ['frobnicate']].map((a) => run(['todo.js', ...a]));
check('a bad number, a missing text and an unknown command exit non-zero with a message', bad.every((r) => r.status !== 0 && (r.stdout + r.stderr).trim().length > 0), bad.map((r) => r.status).join(','));
check('a README exists', ['README.md', 'readme.md', 'Readme.md'].some((n) => existsSync(join(copy, n)) && /usage/i.test(readFileSync(join(copy, n), 'utf8'))));

rmSync(copy, { recursive: true, force: true });
console.log(JSON.stringify({ pass: checks.every((c) => c.ok), checks }));
