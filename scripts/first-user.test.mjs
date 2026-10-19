// node --test scripts: the first-user review reads the model's answers safely, keeps the reviewer out of the repository
// and writes its report in plain pt-BR.

import assert from 'node:assert/strict';
import test from 'node:test';
import { join } from 'node:path';
import { TASKS, describe, fallback, firstJson, insideRepo, parseStep, report, stepPrompt, summaryPrompt } from './first-user.mjs';

const controls = [
  { role: 'button', name: 'Continuar', x: 10, y: 10, disabled: false },
  { role: 'input', name: 'Escreva aqui', x: 20, y: 20, disabled: true },
];

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

test('the step prompt holds the task, the screen as words and the numbered controls, never a selector', () => {
  const prompt = stepPrompt(TASKS[0], [], { text: 'Bem-vindo', controls }, { width: 720, height: 480 });
  assert.match(prompt, /Tarefa: Você acabou de instalar/);
  assert.match(prompt, /720x480/);
  assert.match(prompt, /\[1\] button: Continuar/);
  assert.match(prompt, /\[2\] input: Escreva aqui \(desativado\)/);
  assert.doesNotMatch(prompt, /querySelector|\.tsx|class=/);
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
