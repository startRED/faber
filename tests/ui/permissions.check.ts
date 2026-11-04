import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Asks } from '../../ui/Ask.tsx';
import { check, equal, ok } from './check.ts';
import { t } from '../../ui/i18n.ts';
import { api } from '../../ui/api.ts';
import { PermissionRules } from '../../ui/PermissionRules.tsx';
import { deepEqual } from './check.ts';

check('permission cards: PowerShell shows its command and the reason without JSON', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Asks, {
    conversationId: 'card', turnId: 'turn', permissions: [{ requestId: 'r', tool: 'PowerShell', input: { command: 'Get-ChildItem -LiteralPath .', description: 'Read the project folders' } }],
  }), root);
  try {
    equal(root.querySelector('b')?.textContent, t('ask.command'));
    equal(root.querySelector('pre')?.textContent, 'Get-ChildItem -LiteralPath .');
    ok(root.textContent?.includes('Read the project folders'));
  } finally { dispose(); root.remove(); }
});

check('permission cards: a plan is readable markdown with no permanent approval', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Asks, {
    conversationId: 'plan', turnId: 'turn', permissions: [{ requestId: 'r', tool: 'ExitPlanMode', input: { plan: '# My plan\n\n1. Read the files\n2. Change the title' } }],
  }), root);
  try {
    equal(root.querySelector('.md h1')?.textContent, 'My plan');
    equal(root.querySelectorAll('.md li').length, 2);
    ok(![...root.querySelectorAll('button')].some(b => b.textContent === t('ask.always')));
    ok(root.querySelector('textarea'), 'plan feedback has a labelled text field');
  } finally { dispose(); root.remove(); }
});

check('permission cards: edits expose every proposed line without hiding them from assistive technology', () => {
  const root = document.createElement('div');
  document.body.append(root);
  const dispose = render(() => createComponent(Asks, {
    conversationId: 'edit', turnId: 'turn', permissions: [{ requestId: 'r', tool: 'Edit', input: { file_path: 'ui/title.ts', old_string: 'old title', new_string: 'new title' } }],
  }), root);
  try {
    ok(root.querySelector('.permission-diff')?.textContent?.includes('old title'));
    ok(root.querySelector('.permission-diff')?.textContent?.includes('new title'));
    equal(root.querySelector('.permission-diff')?.getAttribute('aria-hidden'), null);
  } finally { dispose(); root.remove(); }
});

check('permission cards: missing plans cannot be approved and rendered plans remove unsafe markup', () => {
  for (const plan of ['', '# Safe\n<script>alert(1)</script>']) {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'ExitPlanMode',input:{plan}}] }), root);
    try {
      equal(root.querySelector<HTMLButtonElement>('.primary')?.disabled, !plan);
      equal(root.querySelector('.permission-plan script'), null);
      ok(root.querySelector<HTMLLabelElement>('label')?.htmlFor === root.querySelector('textarea')?.id);
    } finally { dispose(); root.remove(); }
  }
});

check('permission cards: a native command keeps its reason and folder visible', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'command',input:{command:'git status --short',cwd:'C:/Project with spaces',reason:'Read local changes'}}] }), root);
  try {
    equal(root.querySelector('.permission-body')?.textContent, 'git status --short');
    ok(root.textContent?.includes('Read local changes'));
    ok(root.textContent?.includes('C:/Project with spaces'));
    equal(root.querySelector('textarea'), null, 'the native protocol cannot carry a decline reason');
  } finally { dispose(); root.remove(); }
});

check('permission cards: a connected tool shows its server, tool and inspectable arguments', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'mcp__calendar__create_event',input:{title:'Design review'}}] }), root);
  try {
    equal([...root.querySelectorAll('dd')].map(d=>d.textContent).join('|'), 'calendar|create_event');
    ok(root.querySelector('details pre')?.textContent?.includes('Design review'));
  } finally { dispose(); root.remove(); }
});

check('permission cards: declining a plan sends feedback, prevents duplicates and preserves text after failure', async () => {
  const original = api.answer;
  const calls: unknown[][] = [];
  let reject!: (reason: unknown) => void;
  api.answer = (...args) => { calls.push(args); return new Promise((_, no) => { reject = no; }); };
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'ExitPlanMode',input:{plan:'# Read first'}}] }), root);
  try {
    const field = root.querySelector('textarea')!;
    field.value = 'Please inspect the failing check first.'; field.dispatchEvent(new Event('input', {bubbles:true}));
    const deny = [...root.querySelectorAll('button')].find(b=>b.textContent === t('ask.plan.keep'))!;
    deny.click(); deny.click();
    equal(calls.length, 1);
    deepEqual(calls[0], ['c','t','r',false,false,'Please inspect the failing check first.']);
    reject('error.permission.undelivered');
    await Promise.resolve(); await Promise.resolve();
    equal(field.value, 'Please inspect the failing check first.');
    ok(root.querySelector('[role="alert"]'));
    equal(deny.disabled, false);
  } finally { dispose(); root.remove(); api.answer = original; }
});

check('permission rules: shows only saved results and withdraws one tool, retaining the others', async () => {
  const originals = { ...api };
  let rules = [{conversationId:'c',title:'A long conversation title',tools:['Bash','Write']}];
  api.permissionRules = async () => rules;
  const calls: string[][] = [];
  api.revokePermissionRule = async (id,tool) => { calls.push([id,tool]); rules = [{...rules[0]!,tools:['Write']}]; };
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(PermissionRules, {}), root);
  try {
    await Promise.resolve(); await Promise.resolve();
    equal(root.querySelectorAll('button').length, 2);
    root.querySelector<HTMLButtonElement>('button')!.click();
    await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
    deepEqual(calls, [['c','Bash']]);
    equal(root.querySelectorAll('button').length, 1);
    ok(root.textContent?.includes('Write'));
  } finally { dispose(); root.remove(); Object.assign(api, originals); }
});

check('permission rules: empty, loading and failed reads never pretend a grant was removed', async () => {
  const originals = { ...api };
  for (const fail of [false,true]) {
    api.permissionRules = async () => { if (fail) throw 'error.store.write'; return []; };
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(PermissionRules, {}), root);
    try {
      ok(root.textContent?.includes(t('set.rules.loading')));
      await Promise.resolve(); await Promise.resolve();
      ok(root.textContent?.includes(t(fail ? 'error.store.write' : 'set.rules.empty')));
      equal(root.querySelectorAll('button').length, 0);
    } finally { dispose(); root.remove(); }
  }
  Object.assign(api, originals);
});
