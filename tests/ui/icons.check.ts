import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { ActorMark, STATE_KINDS, StateGlyph, stateKind, ToolGlyph, toolKind, type StateKind, type ToolKind } from '../../ui/icons/kinds.tsx';
import { check, equal, ok } from './check.ts';

// The visual language of docs/design/icons.md: a drawing per kind of step and per state, told apart by shape and not only by colour.
const draw = (view: () => unknown) => {
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(() => view() as never, {}), host);
  return { host, done: () => (dispose(), host.remove()) };
};
const shapeOf = (state: StateKind) => {
  const { host, done } = draw(() => StateGlyph({ state }));
  const d = [...host.querySelectorAll('path')].map((p) => p.getAttribute('d')).join('|');
  done();
  return d;
};

check('icons: every state has its own silhouette, so colour is never the only cue', () => {
  const shapes = STATE_KINDS.map((state) => shapeOf(state));
  equal(new Set(shapes).size, STATE_KINDS.length, 'two states share a drawing');
  for (const [i, shape] of shapes.entries()) ok(shape.length > 0, `the state ${STATE_KINDS[i]} has no drawing`);
});

check('icons: the CLIs\' step names map to a kind, and a name nobody mapped is `other`, never a wrong picture', () => {
  const cases: Record<string, ToolKind> = { Read: 'read', Edit: 'edit', MultiEdit: 'edit', Write: 'write', Bash: 'run', PowerShell: 'run', commandExecution: 'run', Grep: 'search', Glob: 'search', WebSearch: 'web', WebFetch: 'web', Agent: 'subagent', Task: 'subagent', Helper: 'helper', fileChange: 'edit', mcpToolCall: 'mcp', mcp__faber__list: 'mcp', Skill: 'skill', TodoWrite: 'plan', Unheard: 'other' };
  for (const name of ['constructor', 'toString', '__proto__']) equal(toolKind(name), 'other', `${name} is not a step kind`);
  for (const [name, kind] of Object.entries(cases)) equal(toolKind(name), kind, `${name} reads as ${kind}`);
  const kinds = Object.values(cases);
  const { host, done } = draw(() => kinds.map((kind) => ToolGlyph({ kind })));
  equal(host.querySelectorAll('svg.i').length, kinds.length);
  ok([...host.querySelectorAll('svg.i')].every((svg) => svg.querySelector('path')), 'a step kind has no drawing');
  done();
});

check('icons: the engine\'s words for a state map to the state they mean, and an unknown word claims nothing good', () => {
  const cases: Record<string, string> = { running: 'working', background: 'working', waiting: 'waiting', stuck: 'stuck', needsYou: 'waiting', done: 'concluded', passed: 'concluded', unreported: 'ended', unknown: 'ended', interrupted: 'interrupted', stopped: 'stopped', declined: 'stopped', failed: 'failed', error: 'failed', neverHeardOf: 'ended' };
  for (const word of ['constructor', 'toString']) equal(stateKind(word), 'ended', `${word} is not a state`);
  for (const [word, state] of Object.entries(cases)) equal(stateKind(word), state, `${word} reads as ${state}`);
});

check('icons: a state drawing is decoration beside its words, and a named image where nothing else says it', () => {
  const { host, done } = draw(() => [StateGlyph({ state: 'working', small: true }), StateGlyph({ state: 'failed', label: 'Failed' }), ActorMark({ actor: 'faber' }), ActorMark({ actor: 'claude' }), ActorMark({ actor: 'codex' })]);
  const [plain, named] = [...host.querySelectorAll('.st')];
  equal(plain?.getAttribute('aria-hidden'), 'true');
  equal(plain?.getAttribute('role'), null);
  equal(named?.getAttribute('role'), 'img');
  equal(named?.getAttribute('aria-label'), 'Failed');
  ok(host.querySelector('svg.mk-faber'), "Faber's own mark stands for Faber");
  ok(host.querySelector('.mk.claude') && host.querySelector('.mk.codex'), "the vendors' shapes stand for them");
  done();
});

check('icons: the working ring breathes only while motion is on, and keeps its open ring when it does not', () => {
  const { host, done } = draw(() => StateGlyph({ state: 'working' }));
  const ring = host.querySelector<HTMLElement>('.st-working')!;
  ok(getComputedStyle(ring).animationName.includes('pulse'), 'the working ring does not breathe');
  const before = document.documentElement.dataset.motion;
  document.documentElement.dataset.motion = 'none';
  try {
    equal(getComputedStyle(ring).animationName, 'none', 'the working ring breathes with motion off');
  } finally {
    if (before === undefined) delete document.documentElement.dataset.motion;
    else document.documentElement.dataset.motion = before;
  }
  done();
});
