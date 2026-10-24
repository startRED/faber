// node --test scripts: the first-user review reads the model's answers safely, keeps the reviewer out of the repository
// and writes its report in plain pt-BR.

import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { basename, dirname, join } from 'node:path';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import * as firstUser from './first-user.mjs';
import { TASKS, describe, fallback, firstJson, insideRepo, parseStep, report, stepPrompt, summaryPrompt } from './first-user.mjs';

const controls = [
  { role: 'button', name: 'Continuar', x: 10, y: 10, disabled: false },
  { role: 'input', name: 'Escreva aqui', x: 20, y: 20, disabled: true },
];

test('artifact identities distinguish a missing app, a rebuilt driver and the previously recorded bytes', async () => {
  const folder = mkdtempSync(join(tmpdir(), 'faber-first-user-identity-'));
  try {
    const paths = { driver: join(folder, 'driver'), app: join(folder, 'app') };
    writeFileSync(paths.driver, 'a');
    const before = await firstUser.artifactIdentity(paths);
    assert.equal(before.driver.sha256, 'ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb');
    assert.equal(before.app.sha256, null);
    writeFileSync(paths.driver, 'b');
    writeFileSync(paths.app, 'a');
    const after = await firstUser.artifactIdentity(paths);
    assert.equal(after.driver.sha256, '3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d');
    assert.equal(after.app.sha256, before.driver.sha256);
    assert.notEqual(after.driver.sha256, before.driver.sha256);
    assert.equal(before.app.sha256, null);
    assert.equal(after.driver.path, paths.driver);
    await assert.rejects(firstUser.artifactIdentity({ driver: folder }));
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
});

test('native choices require an observed enabled selector and an exact enabled option', () => {
  const lists = [{ role: 'list', name: 'Modelo', disabled: false, options: [
    { label: 'Claude Haiku', value: 'haiku', disabled: false },
    { label: 'Unavailable', value: 'no', disabled: true },
  ] }];
  const choose = (n, option, snapshot = lists) => parseStep(JSON.stringify({ action: { type: 'select', n, option } }), snapshot).action.type;
  assert.equal(choose(1, 'Claude Haiku'), 'select');
  assert.equal(choose(1, 'Unavailable'), 'stuck');
  assert.equal(choose(1, 'haiku'), 'stuck');
  assert.equal(choose(2, 'Claude Haiku'), 'stuck');
  assert.equal(choose(1, 'Claude Haiku', [{ ...lists[0], disabled: true }]), 'stuck');
  assert.equal(choose(1, 'Claude Haiku', [{ ...lists[0], options: undefined }]), 'stuck');
  const prompt = stepPrompt(TASKS[0], [], { controls: lists }, { width: 720, height: 480 });
  assert.match(prompt, /Claude Haiku/);
  assert.match(prompt, /"type": "select"/);
  assert.match(describe({ type: 'select', n: 1, option: 'Claude Haiku' }), /Claude Haiku/);
});

test('native selection rechecks identity, visibility and current option state and safely quotes labels', () => {
  const label = 'Haiku "quoted" ${notCode}';
  const option = { label, value: 'haiku', disabled: false, closest: () => null };
  const events = [];
  const el = { isConnected: true, tagName: 'SELECT', disabled: false, options: [option],
    getAttribute: () => null, dispatchEvent: (event) => events.push(event.type), contains: (other) => other === el,
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 100, bottom: 30, width: 100, height: 30 }) };
  const context = { window: { __faberFirstUserControls: [el] }, document: { elementFromPoint: () => el }, innerWidth: 720, innerHeight: 480,
    Event: class { constructor(type) { this.type = type; } } };
  const script = firstUser.selectionScript({ n: 1, option: label }, { controls: [{ options: [{ label, value: 'haiku' }] }] });
  assert.equal(runInNewContext(script, context), true);
  assert.equal(el.value, 'haiku');
  assert.deepEqual(events, ['input', 'change']);
  for (const mutate of [() => { el.isConnected = false; }, () => { el.disabled = true; },
    () => { context.document.elementFromPoint = () => null; }, () => { option.disabled = true; },
    () => { option.value = 'other'; }, () => { el.options = [option, option]; },
    () => { option.closest = () => ({ disabled: true }); }]) {
    el.isConnected = true; el.disabled = false; context.document.elementFromPoint = () => el;
    option.disabled = false; option.value = 'haiku'; option.closest = () => null; el.options = [option];
    events.length = 0;
    mutate();
    assert.equal(runInNewContext(script, context), false);
    assert.deepEqual(events, []);
  }
});

test('a seeded project conversation is opt-in and never launches without its fake AI', () => {
  const tasks = firstUser.selectTasks(['--task', 'conversa-projeto']);
  assert.equal(tasks.length, 1);
  assert.match(tasks[0].goal, /In one sentence: why is the sky blue\?/);
  assert.equal(firstUser.selectTasks().some((task) => task.id === 'conversa-projeto'), false);
  assert.deepEqual(firstUser.driverFlags(tasks, 'pt-BR', false), ['--project-demo', '--lang', 'pt-BR']);
  assert.throws(() => firstUser.driverFlags(TASKS, 'en', true), /separate batches/);
  assert.throws(() => firstUser.driverFlags([...TASKS, ...tasks], 'en', false), /one focused task/);
});

test('the project report identifies the seeded replay rather than claiming first-run evidence', () => {
  const [task] = firstUser.selectTasks(['--task', 'conversa-projeto']);
  const text = report({ date: '2026-10-23', agent: 'fixture', size: { width: 720, height: 480 }, lang: 'en',
    findings: [], results: [{ task, steps: [], outcome: 'done' }], shots: 'fixture' });
  assert.match(text, /projeto Demo já preparado/);
  assert.match(text, /resposta gravada/);
  assert.doesNotMatch(text, /primeira abertura sem nenhuma IA/);
});

test('a Chat review uses the recorded AI after the disconnected first-run walk', () => {
  const chat = firstUser.selectTasks(['--task', 'conversa']);
  assert.deepEqual(firstUser.driverFlags(chat, 'en', false), ['--lang', 'en']);
  assert.match(chat[0].goal, /In one sentence: why is the sky blue\?/);
  assert.match(chat[0].goal, /Haiku/);
  const batches = firstUser.reviewBatches(TASKS);
  assert.deepEqual(batches.flat(), TASKS);
  assert.equal(batches.length, 2);
  assert.deepEqual(firstUser.driverFlags(batches[0], 'pt-BR', true), ['--first-run', '--no-ai', '--lang', 'pt-BR', '--build']);
  assert.deepEqual(firstUser.driverFlags(batches[1], 'pt-BR', false), ['--lang', 'pt-BR']);
  const text = report({ date: '2026-10-24', agent: 'fixture', size: { width: 720, height: 480 }, lang: 'en',
    findings: [], results: [{ task: chat[0], steps: [], outcome: 'done' }], shots: 'fixture' });
  assert.match(text, /resposta gravada/);
  assert.doesNotMatch(text, /primeira abertura sem nenhuma IA/);
});

test('a focused newcomer review rejects missing selections before starting the app', () => {
  assert.throws(() => firstUser.selectTasks(['--task']), /missing task/);
  assert.throws(() => firstUser.selectTasks(['--task', '--agent', 'codex']), /missing task/);
});

test('a focused newcomer review selects Catalog and rejects unknown tasks instead of reporting an empty success', () => {
  assert.equal(typeof firstUser.selectTasks, 'function');
  assert.deepEqual(firstUser.selectTasks(), TASKS);
  const selected = firstUser.selectTasks(['--task', 'catalogo']);
  assert.equal(selected.length, 1);
  assert.equal(selected[0].id, 'catalogo');
  assert.throws(() => firstUser.selectTasks(['--task', 'does-not-exist']), /unknown task/);
});

test('same-day review runs keep separate screenshots and default reports, including identical configurations', () => {
  assert.equal(typeof firstUser.runPaths, 'function');
  const root = mkdtempSync(join(tmpdir(), 'faber-first-user-paths-'));
  try {
    const target = join(root, 'target');
    const first = firstUser.runPaths(root, target, '2026-10-23');
    writeFileSync(join(first.shots, 'primeira-abertura-1.png'), 'first run');
    const second = firstUser.runPaths(root, target, '2026-10-23');
    assert.notEqual(first.shots, second.shots);
    assert.notEqual(first.out, second.out);
    assert.equal(readFileSync(join(first.shots, 'primeira-abertura-1.png'), 'utf8'), 'first run');
    assert.equal(dirname(first.shots), join(target, 'faber-test', 'first-user'));
    assert.equal(first.out, join(root, 'docs', 'research', `first-user-${basename(first.shots)}.md`));
    const explicit = firstUser.runPaths(root, target, '2026-10-23', 'custom-report.md');
    assert.equal(explicit.out, join(root, 'custom-report.md'));
    assert.notEqual(explicit.shots, second.shots);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the reviewer folder must sit outside the repository', () => {
  const root = join('E:', 'repo');
  assert.equal(insideRepo(root, root), true);
  assert.equal(insideRepo(join(root, 'scripts'), root), true);
  assert.equal(insideRepo(join('E:', 'Temp', 'faber-first-user-1'), root), false);
  assert.equal(insideRepo(join('E:', 'repo-other'), root), false);
});

test('an answer wrapped in prose or a code fence still gives its JSON, and no JSON gives null', () => {
  assert.deepEqual(firstJson('Claro!\n```json\n{"a": {"b": 1}}\n```\nPronto.'), { a: { b: 1 } });
  assert.deepEqual(firstJson('{"a":1} e depois {'), { a: 1 });
  assert.equal(firstJson('sem nada'), null);
});

test('a step outside the allowed actions or a control that is not on screen ends the task as stuck', () => {
  assert.deepEqual(parseStep('{"thought":"vejo um botão","confusion":null,"action":{"type":"click","n":1}}', controls).action, {
    type: 'click',
    n: 1,
  });
  assert.equal(parseStep('{"action":{"type":"click","n":7}}', controls).action.type, 'stuck');
  assert.equal(parseStep('{"action":{"type":"rm -rf"}}', controls).action.type, 'stuck');
  assert.equal(parseStep('não sei', controls).action.type, 'stuck');
});

test('a newcomer can wait for a response without reporting the task complete', () => {
  const step = parseStep('{"thought":"a resposta ainda está chegando","action":{"type":"wait"}}', controls);
  assert.equal(step.action.type, 'wait');
  assert.equal(describe(step.action), 'aguardou');
});

test('the step prompt holds the task, the screen as words and the numbered controls, never a selector', () => {
  const prompt = stepPrompt(TASKS[0], [], { text: 'Bem-vindo', controls }, { width: 720, height: 480 });
  assert.match(prompt, /Tarefa: Você acabou de instalar/);
  assert.match(prompt, /720x480/);
  assert.match(prompt, /\[1\] button: Continuar/);
  assert.match(prompt, /\[2\] input: Escreva aqui \(desativado\)/);
  assert.doesNotMatch(prompt, /querySelector|\.tsx|class=/);
  const english = stepPrompt(TASKS[0], [], { text: 'Welcome', controls }, { width: 1280, height: 800 }, 'en');
  assert.match(english, /idioma da interface é en/);
  assert.doesNotMatch(english, /app está em português/);
});

test('the reviewer must read the screenshot, not body text copied from below the viewport', () => {
  const prompt = stepPrompt(TASKS[0], [], { text: 'OFFSCREEN_SENTINEL_NOT_VISIBLE', controls }, { width: 720, height: 480 });
  assert.doesNotMatch(prompt, /OFFSCREEN_SENTINEL_NOT_VISIBLE/);
  assert.match(prompt, /Leia o texto diretamente na imagem/);
});

test('the report lists findings first, then each task with its steps and screenshots', () => {
  const steps = [{ thought: 'vejo um botão', confusion: 'não sei o que é IA', action: { type: 'click', n: 1, name: 'Continuar' }, shot: 'a-1.png' }];
  const results = [{ task: TASKS[0], steps, outcome: 'stuck' }];
  assert.match(summaryPrompt(results), /confuso: não sei o que é IA \(a-1\.png\)/);
  const text = report({
    date: '2026-10-19',
    agent: 'Haiku',
    size: { width: 720, height: 480 },
    lang: 'pt-BR',
    findings: [{ task: 'primeira-abertura', tried: 'continuar', stuck: 'Não achou onde conectar.', shot: 'a-1.png' }],
    results,
    shots: 'target/x',
  });
  assert.ok(text.indexOf('## O que confundiu') < text.indexOf('## Tarefa por tarefa'));
  assert.match(text, /- \*\*primeira-abertura\*\*: continuar\. Não achou onde conectar\. \(`a-1\.png`\)/);
  assert.match(text, /### primeira-abertura: travou/);
  assert.match(text, /clicou em \[1\] "Continuar"/);
  assert.equal(describe({ type: 'key', key: 'Enter' }), 'apertou Enter');
});

test('a reviewer saying done is reported as its claim, not verified completion', () => {
  const text = report({ date: '2026-10-23', agent: 'fixture', size: { width: 720, height: 480 }, lang: 'pt-BR',
    findings: [], results: [{ task: TASKS[0], steps: [], outcome: 'done' }], shots: 'fixture' });
  assert.match(text, /conclusão declarada pela IA/);
  assert.match(text, /precisa conferir as imagens/);
  assert.doesNotMatch(text, /### primeira-abertura: conseguiu/);
});

test('without a usable summary the report still lists every confusion and every stuck task', () => {
  const steps = [
    { thought: 'a', confusion: null, action: { type: 'click', n: 1 }, shot: 'a-1.png' },
    { thought: 'b', confusion: 'nome estranho', action: { type: 'click', n: 2 }, shot: 'a-2.png' },
    { thought: 'c', confusion: null, action: { type: 'stuck', where: 'sem saída' }, shot: 'a-3.png' },
  ];
  assert.deepEqual(fallback([{ task: TASKS[0], steps, outcome: 'stuck' }]), [
    { task: 'primeira-abertura', tried: 'clicou em [2]', stuck: 'nome estranho', shot: 'a-2.png' },
    { task: 'primeira-abertura', tried: 'travou: sem saída', stuck: 'sem saída', shot: 'a-3.png' },
  ]);
});
