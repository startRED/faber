import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { CASES, failure, firstSentence, languageOf, parseClaude, parseCodex, plan, repliesIn, shimTargets, summary, wordsFor } from './persona-eval.mjs';
import { readsIn } from '../ui/language.ts';

const grade = (id, run) => CASES.find((c) => c.id === id).grade({ text: '', helpers: [], ...run });
const pt = 'Os dois resultados estão aqui, e você pode conferir.';
const en = 'Both results are here, and you can check them.';

test('a helper title reads in the screen language, and English titles never read as Portuguese', () => {
  for (const title of ['recorder analysis readiness', 'Explore codebase structure', 'Find auth handlers', 'read docs', 'Sidebar layout']) {
    assert.equal(readsIn(title, 'pt-BR'), false, title);
    assert.equal(readsIn(title, 'en'), true, title);
  }
  for (const title of ['Conferindo a gravação', 'Revisar layout', 'Calcular 17 × 23', 'Ler arquivos', 'Somar os números']) {
    assert.equal(readsIn(title, 'pt-BR'), true, title);
    assert.equal(readsIn(title, 'en'), false, title);
  }
  // English shares a few small words with Portuguese; one of them alone keeps a title English.
  assert.equal(readsIn('No changes needed', 'en'), true);
  for (const title of ['Do security review', 'Do the math', 'No tests', 'Die roll odds']) {
    assert.equal(readsIn(title, 'pt-BR'), false, title);
    assert.equal(readsIn(title, 'de'), false, title);
    assert.equal(readsIn(title, 'en'), true, title);
  }
  assert.equal(readsIn('Analyser le code', 'fr'), true);
  assert.equal(readsIn('Prüfung der Tests', 'de'), true);
  assert.equal(readsIn('', 'en'), false);
  assert.equal(readsIn('  ', 'pt-BR'), false);
});

test('helper titles pass only when every one is a few Portuguese words, and a run with no helper fails', () => {
  assert.equal(grade('titles', { helpers: ['Calcular 17 × 23', 'Somar 1234 + 4321'], text: pt }).pass, true);
  assert.equal(grade('titles', { helpers: ['Calcular 17 × 23', 'Somar 1234 + 4321'], text: en }).pass, false, 'the reply drifted');
  assert.equal(grade('titles', { helpers: ['Calcular 17 × 23', 'Add two numbers'] }).pass, false);
  assert.equal(grade('titles', { helpers: ['Calcular a soma de todos os números que a pessoa mandou agora há pouco'] }).pass, false);
  assert.equal(grade('titles', { helpers: [] }).pass, false);
  assert.equal(grade('english-titles', { helpers: ['Multiply 17 by 23', 'Add 1234 and 4321'], text: en }).pass, true);
  assert.equal(grade('english-titles', { helpers: ['Multiply 17 by 23', 'Add 1234 and 4321'], text: 'Los dos resultados están aquí y los subagentes terminaron.' }).pass, false, 'the reply drifted');
  assert.equal(grade('english-titles', { helpers: ['Calcular 17 × 23'] }).pass, false);
});

test('a reply to a pasted log fails with two of its technical words, whole words only', () => {
  assert.equal(grade('pasted-log', { text: 'Não é grave. Uma verificação automática achou um detalhe de estilo no código; peça para quem cuida dele corrigir.' }).pass, true);
  assert.equal(grade('pasted-log', { text: 'O lint do CI falhou no branch fix/login-timeout.' }).pass, false);
  // One word may stay; "prática" holds "pr" and "commitment" holds "commit", and neither counts.
  assert.equal(grade('pasted-log', { text: 'Na prática, é um commitment pequeno: o merge está parado até alguém corrigir.' }).pass, true);
  // Every time counts, and a slash splits words.
  assert.equal(grade('pasted-log', { text: 'O merge parou. Depois do merge, tudo volta.' }).pass, false);
  assert.equal(grade('pasted-log', { text: 'Falhou o CI/lint.' }).why, 'technical words: CI, lint');
});

test('the conclusion comes first, and normal use keeps asked-for terms and stays quiet about waiting', () => {
  assert.equal(firstSentence('**Não**, não dá. O limite é menor.'), 'Não, não dá.');
  assert.equal(grade('conclusion-first', { text: 'Não, não dá numa compra só. Faltam R$ 200.' }).pass, true);
  assert.equal(grade('conclusion-first', { text: 'Vamos ver: o limite é R$ 3.000. Então não dá.' }).pass, false);
  assert.equal(grade('asked-for-terms', { text: 'Use `git log -1 --format=%H` para o hash do último commit e `git branch --show-current` para o branch.' }).pass, true);
  assert.equal(grade('asked-for-terms', { text: 'Há um comando para cada coisa.' }).pass, false);
  assert.equal(grade('asked-for-terms', { text: '`git rev-parse HEAD` e `git symbolic-ref --short HEAD`.' }).pass, true);
  assert.equal(grade('conclusion-first', { text: 'Com limite de R$ 3.000, a compra de R$ 3.200 deve ser recusada.' }).pass, true);
  assert.equal(grade('conclusion-first', { text: 'Consegue sim, não se preocupe.' }).pass, false);
  assert.equal(grade('conclusion-first', { text: 'Não se preocupe, você consegue comprar a TV.' }).pass, false);
  assert.equal(grade('conclusion-first', { text: 'Não. Faltam R$ 200.' }).pass, true);
  assert.equal(grade('conclusion-first', { text: 'Numa compra só não dá: faltam R$ 200.' }).pass, true);
  assert.equal(grade('asked-for-terms', { text: '```\ngit rev-parse HEAD\ngit rev-parse --abbrev-ref HEAD\n```' }).pass, true);
  assert.equal(grade('quiet', { text: 'Camberra.' }).pass, true);
  assert.equal(grade('quiet', { text: 'Canberra. Estou aguardando sua próxima pergunta.' }).pass, false);
  assert.equal(grade('quiet', { text: 'Sydney.' }).pass, false);
});

test('Claude Code runs give their text, the titles of the helpers the agent started (not theirs) and the cost', () => {
  const lines = [
    { type: 'assistant', message: { content: [{ type: 'tool_use', name: 'Agent', input: { description: 'Calcular 17 × 23', prompt: '…' } }] } },
    { type: 'assistant', parent_tool_use_id: 'x', message: { content: [{ type: 'tool_use', name: 'Agent', input: { description: 'nested' } }] } },
    { type: 'result', result: 'São 391 e 5555.', total_cost_usd: 0.012, usage: { input_tokens: 10, output_tokens: 5, cache_read_input_tokens: 100 } },
  ];
  const run = parseClaude(['not json', ...lines.map((l) => JSON.stringify(l))].join('\n'));
  assert.deepEqual(run, { text: 'São 391 e 5555.', helpers: ['Calcular 17 × 23'], cost: 0.012, tokens: 115, error: '' });
  assert.equal(parseClaude(JSON.stringify({ type: 'result', is_error: true, subtype: 'error_max_turns' })).error, 'error_max_turns');
});

test('Codex runs give their last message and their tokens', () => {
  const lines = [
    { type: 'item.completed', item: { type: 'agent_message', text: 'Primeiro.' } },
    { type: 'item.completed', item: { type: 'agent_message', text: 'Não, não dá.' } },
    { type: 'turn.completed', usage: { input_tokens: 900, cached_input_tokens: 800, output_tokens: 40 } },
  ];
  assert.deepEqual(parseCodex(lines.map((l) => JSON.stringify(l)).join('\n')), { text: 'Não, não dá.', helpers: [], cost: 0, tokens: 940, error: '' });
});

test('a reply reads in the language of its own small words, with code left out', () => {
  assert.equal(languageOf('A memória RAM guarda o que está em uso; o armazenamento é o que fica quando você desliga.'), 'pt-BR');
  assert.equal(languageOf('RAM is the memory your computer uses for what is open; storage keeps files when it is off.'), 'en');
  assert.equal(languageOf('Seu disco encheu, não há espaço.\n```\nError: no space left on device, the write failed and it is gone\n```'), 'pt-BR');
  assert.equal(languageOf('Der Speicher ist voll und die Datei wurde nicht geschrieben.'), 'de');
  assert.equal(languageOf('42'), '');
  // Short English replies the small words miss still read as English; one with another language's marks does not.
  assert.equal(repliesIn('Both subagents finished:\n\n- 17 × 23 = **391**\n- 1234 + 4321 = **5555**', 'en'), true);
  assert.equal(repliesIn('Los dos resultados:\n\n- 17 × 23 = **391**', 'en'), false);
  assert.equal(repliesIn('Both subagents finished.', 'pt-BR'), false);
  // A real Codex reply that the first word list could not tell.
  assert.equal(languageOf('O disco do servidor ficou sem espaço ao salvar o PDF. Libere espaço na máquina onde o app roda.'), 'pt-BR');
  assert.equal(languageOf('El disco del servidor se quedó sin espacio y los archivos no se guardaron.'), 'es');
  const screen = (lang, text) => CASES.filter((c) => c.language === lang && c.aim === 'control').every((c) => c.grade({ text, helpers: [] }).pass);
  assert.ok(screen('en', 'A RAM é rápida e some quando você desliga; o armazenamento não.'), 'an English screen, a Portuguese reply');
  assert.ok(screen('pt-BR', 'RAM is fast and is wiped when you turn it off; storage is not.'), 'a Portuguese screen, an English reply');
  assert.equal(screen('pt-BR', 'A RAM é rápida e some quando você desliga.'), false);
});

test('a case with a screen language gets the words for it when they exist, and the old words otherwise', () => {
  const c = CASES.find((x) => x.id === 'screen-pt-pasted-only');
  const now = { claude: 'a', codex: 'b', languages: { 'pt-BR': { claude: 'pt a', codex: 'pt b' } } };
  assert.equal(wordsFor(now, c).codex, 'pt b');
  assert.equal(wordsFor({ claude: 'v8', codex: 'v8' }, c).codex, 'v8');
  assert.equal(wordsFor(now, CASES.find((x) => x.id === 'quiet')).codex, 'b');
});

test('a run fails on a time-out, an error, a non-zero exit or no answer', () => {
  const ok = { text: 'Sim.', error: '' };
  assert.equal(failure({ code: 0, timedOut: false, err: '' }, ok), '');
  assert.equal(failure({ code: null, timedOut: true, err: '' }, ok), 'timed out');
  assert.equal(failure({ code: 1, timedOut: false, err: 'boom\n' }, ok), 'exit 1: boom');
  assert.equal(failure({ code: 0, timedOut: false, err: '' }, { text: '', error: '' }), 'no answer');
  assert.equal(failure({ code: 0, timedOut: false, err: '' }, { text: '', error: 'usage limit' }), 'usage limit');
});

test('an npm shim points to the program it runs', () => {
  const shim = 'IF EXIST "%dp0%\\node.exe" (\r\n) & "%_prog%"  "%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js" %*';
  assert.deepEqual(shimTargets(shim), ['node.exe', 'node_modules\\@openai\\codex\\bin\\codex.js']);
  assert.deepEqual(shimTargets('"%~dp0\\bin\\tool.exe" %*'), ['bin\\tool.exe']);
});

test('the plan runs old and new words for each sample of each case a CLI can run, and the summary counts them', () => {
  const runs = plan({ samples: 2, clis: ['claude', 'codex'], only: ['titles', 'quiet'] });
  assert.equal(runs.length, 2 * 2 + 2 * 2 * 2);
  assert.ok(runs.every((r) => r.cli === 'claude' || r.case === 'quiet'), 'Codex never runs the helper titles case');
  assert.deepEqual(runs.slice(0, 2).map((r) => r.side), ['old', 'new']);
  const lines = summary([
    { case: 'quiet', cli: 'codex', side: 'old', pass: true, tokens: 100 },
    { case: 'quiet', cli: 'codex', side: 'new', pass: false, tokens: 50 },
    { case: 'quiet', cli: 'claude', side: 'new', pass: true, cost: 0.5, tokens: 10 },
  ]);
  assert.match(lines.find((l) => l.startsWith('quiet') && l.includes('codex')), /control\s+1\/1\s+0\/1$/);
  assert.equal(lines.at(-1), 'cost: Claude Code says US$ 0.50 (10 tokens); Codex 150 tokens');
});
