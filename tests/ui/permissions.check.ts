import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { Asks } from '../../ui/Ask.tsx';
import { check, equal, ok } from './check.ts';
import { t } from '../../ui/i18n.ts';

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

check('permission cards: network destination is visible without a command', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'command',input:{networkApprovalContext:{host:'upload.example.com',protocol:'https'}}}] }), root);
  try { ok(root.querySelector('.permission-body')?.textContent?.includes('upload.example.com')); } finally { dispose(); root.remove(); }
});

check('permission cards: command arguments retain their boundaries', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'command',input:{command:['rm','-rf','/tmp/keep me','','a"b']}}] }), root);
  try { equal(root.querySelector('.permission-body')?.textContent, 'rm -rf "/tmp/keep me" "" "a\\"b"'); } finally { dispose(); root.remove(); }
});

check('permission cards: connected-tool arguments are visible and bounded', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'mcp__database__query',input:{sql:'DROP TABLE customers;'}}] }), root);
  try { ok(root.querySelector('.permission-body')?.textContent?.includes('DROP TABLE customers;')); } finally { dispose(); root.remove(); }
});

check('permission cards: long tool arguments have a visible continuation marker', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'mcp__database__query',input:{sql:'x'.repeat(1200)+'TAIL'}}] }), root);
  try { ok(root.querySelector('.permission-body')?.textContent?.includes(t('ask.more'))); ok(!root.querySelector('.permission-body')?.textContent?.includes('TAIL')); ok(root.querySelector('details pre')?.textContent?.includes('TAIL')); } finally { dispose(); root.remove(); }
});

check('permission cards: controls are visible in the preview and request details', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'PowerShell',input:{command:'report\u202etxt.exe',reason:'why\u2066here'}}] }), root);
  try { ok(root.querySelector('.permission-body')?.textContent?.includes('<U+202E>')); ok(root.querySelector('.permission-raw pre')?.textContent?.includes('<U+202E>')); ok(root.querySelector('.permission-reason')?.textContent?.includes('<U+2066>')); } finally { dispose(); root.remove(); }
});

check('permission cards: a multi-line command keeps its line breaks and tabs', () => {
  const root = document.createElement('div'); document.body.append(root);
  const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{requestId:'r',tool:'Bash',input:{command:'cd x\n\tls'}}] }), root);
  try { equal(root.querySelector('.permission-body')?.textContent, 'cd x\n\tls'); } finally { dispose(); root.remove(); }
});

check('permission cards: the network host shows beside a command, and invisible or look-alike characters cannot hide', () => {
  const show = (permission: { tool: string; input: unknown }) => {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{ requestId:'r', ...permission }] }), root);
    const body = root.querySelector('.permission-body')?.textContent ?? '';
    dispose(); root.remove();
    return body;
  };
  ok(show({ tool:'command', input:{ command:['curl','https://alias.example'], networkApprovalContext:{ host:'upload.example.com', protocol:'https' } } }).includes('https://upload.example.com'));
  equal(show({ tool:'command', input:{ command:['rm','-rf',"'notes",'/home/a','backup\''] } }), `rm -rf "'notes" /home/a "backup'"`);
  ok(show({ tool:'command', input:{ command:['./git\u200b','status'] } }).includes('<U+200B>'));
  ok(show({ tool:'command', input:{ command:['rm','\u200f'] } }).includes('<U+200F>'));
  ok(show({ tool:'WebFetch', input:{ url:'https://\u0430pple.com/login' } }).includes('xn--'));
});

check('permission cards: a network host is shown in ASCII too, file changes name their files, and any other tool shows its target', () => {
  const show = (permission: { tool: string; input: unknown }) => {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{ requestId:'r', ...permission }] }), root);
    const body = [...root.querySelectorAll('.permission-body')].map((el) => el.textContent).join('|');
    dispose(); root.remove();
    return body;
  };
  ok(show({ tool:'command', input:{ networkApprovalContext:{ host:'\u0430pple.com', protocol:'https' } } }).includes('xn--'));
  ok(show({ tool:'command', input:{ command:['./git\u034f','status'] } }).includes('<U+034F>'));
  const files = show({ tool:'fileChange', input:{ changes:{ 'one.txt':{ add:{ content:'first' } }, 'two.txt':{ add:{ content:'second' } } } } });
  ok(files.includes('one.txt') && files.includes('two.txt'));
  ok(show({ tool:'Read', input:{ file_path:'C:/outside/private.txt' } }).includes('C:/outside/private.txt'));
});

check('permission cards: a native deletion reads as removed lines and default-ignorable characters are escaped', () => {
  const show = (permission: { tool: string; input: unknown }, selector: string) => {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{ requestId:'r', ...permission }] }), root);
    const body = root.querySelector(selector)?.textContent ?? '';
    dispose(); root.remove();
    return body;
  };
  ok(show({ tool:'fileChange', input:{ changes:{ 'important.txt':{ delete:{ content:'keep this' } } } } }, '.permission-diff').includes('− keep this'));
  ok(show({ tool:'command', input:{ command:['./git\u{e0100}','status'] } }, '.permission-body').includes('<U+E0100>'));
  ok(show({ tool:'command', input:{ command:['./git\u180f','status'] } }, '.permission-body').includes('<U+180F>'));
});

check('permission cards: an address with look-alike letters shows its ASCII host whatever its slashes', () => {
  const show = (url: string) => {
    const root = document.createElement('div'); document.body.append(root);
    const dispose = render(() => createComponent(Asks, { conversationId:'c', turnId:'t', permissions:[{ requestId:'r', tool:'WebFetch', input:{ url } }] }), root);
    const body = root.querySelector('.permission-body')?.textContent ?? '';
    dispose(); root.remove();
    return body;
  };
  ok(show('https:///\u0430pple.com/login').includes('xn--'));
  ok(show('https:/\u0430pple.com/login').includes('xn--'));
  equal(show('https://apple.com/login'), 'https://apple.com/login');
});
