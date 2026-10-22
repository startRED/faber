import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type MemoryList } from '../../ui/api.ts';
import { Welcome } from '../../ui/Welcome.tsx';
import { ProjectMemory } from '../../ui/ProjectMemory.tsx';
import { t, useLanguage } from '../../ui/i18n.ts';
import { dirtyCount } from '../../ui/unsent.ts';
import { check, equal, ok } from './check.ts';

const sample = (): MemoryList => ({ incomplete: false, files: [
  { path: 'CLAUDE.md', source: 'claude', revision: 'first', tooLarge: false, entries: [
    { start: 10, end: 32, line: 3, text: '- Run the tests first.' },
  ] },
  { path: 'src/AGENTS.override.md', source: 'codex', revision: 'second', tooLarge: false, entries: [
    { start: 0, end: 19, line: 1, text: 'Use native widgets.' },
  ] },
  { path: '.faber/learning.md', source: 'faber', revision: 'third', tooLarge: false, entries: [] },
] });
const settle = (ms = 30) => new Promise((resolve) => setTimeout(resolve, ms));
const deliberate = () => settle(560);
const button = (root: HTMLElement, key: Parameters<typeof t>[0]) => {
  const found = [...root.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === t(key));
  ok(found, `missing button: ${key}`);
  return found;
};

check('project memory: the project page lists each source and line, edits and deletes only after confirmation, and rereads on refresh', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  let reads = 0;
  let edits = 0;
  let data = sample();
  api.projectMemory = async (id) => { equal(id, 'memory-project'); reads++; return structuredClone(data); };
  api.editProjectMemory = async (id, edit) => {
    equal(id, 'memory-project'); edits++;
    equal(edit.path, 'CLAUDE.md'); equal(edit.revision, 'first'); equal(edit.start, 10); equal(edit.end, 32);
    if (edit.text === null) data.files[0].entries = [];
    else { equal(edit.text, '- Use the test runner.'); data.files[0].entries[0].text = edit.text; }
  };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(Welcome, {
    project: { id: 'memory-project', name: 'Memory project', path: 'E:/memory-project' },
    projects: [], recent: [], running: [], onOpen: () => {}, onSuggest: () => {}, children: undefined,
  }), root);
  try {
    equal(reads, 0, 'a closed fold reads nothing');
    button(root, 'memory.title').click(); await settle();
    equal(reads, 1);
    ok(root.textContent!.includes('Claude Code · CLAUDE.md:3'));
    ok(root.textContent!.includes('Codex · src/AGENTS.override.md:1'));
    ok(root.textContent!.includes('Faber · .faber/learning.md'));
    ok(root.textContent!.includes(t('memory.nextLaunch')));
    button(root, 'memory.edit').click(); await settle();
    const field = root.querySelector('textarea')!;
    equal(document.activeElement, field);
    field.value = '- Use the test runner.'; field.dispatchEvent(new Event('input', { bubbles: true }));
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    equal(edits, 1); ok(root.textContent!.includes('- Use the test runner.'));
    button(root, 'memory.delete').click(); await settle();
    equal(edits, 1, 'first activation only asks');
    await deliberate();
    button(root, 'memory.confirmDelete').click(); await settle();
    equal(edits, 2); ok(!root.textContent!.includes('- Use the test runner.'));
    ok(root.textContent!.includes('Use native widgets.'), 'the other CLI stays untouched');
    equal(reads, 3);
    button(root, 'memory.refresh').click(); await settle(); equal(reads, 4);
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.title').click(); await settle(); equal(reads, 5, 'reopening rereads');
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});

check('project memory: delete activations 30 ms apart only arm the row; a separate gesture confirms', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  let writes = 0;
  api.projectMemory = async () => sample();
  api.editProjectMemory = async () => { writes++; };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.delete').click(); await settle(30);
    button(root, 'memory.confirmDelete').click(); await settle();
    equal(writes, 0, 'a double-click must not delete');
    ok(button(root, 'memory.confirmDelete'), 'the row stays armed');
    await deliberate();
    button(root, 'memory.confirmDelete').click(); await settle();
    equal(writes, 1, 'a separate deliberate gesture deletes once');
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});

check('project memory: holding Enter or Space never confirms delete until the key is released', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  let writes = 0;
  api.projectMemory = async () => sample();
  api.editProjectMemory = async () => { writes++; };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    for (const key of ['Enter', ' ']) {
      const remove = button(root, 'memory.delete'); remove.focus();
      const press = (repeat = false) => remove.dispatchEvent(new KeyboardEvent('keydown', { key, repeat, bubbles: true, cancelable: true }));
      press(); remove.click(); await deliberate();
      equal(press(true), false, 'key repeat is stopped');
      remove.click(); await settle();
      equal(writes, key === 'Enter' ? 0 : 1, 'a held key cannot confirm even after 500 ms');
      remove.dispatchEvent(new KeyboardEvent('keyup', { key, bubbles: true, cancelable: true }));
      press(); remove.click(); await settle();
      equal(writes, key === 'Enter' ? 1 : 2, 'a released and newly pressed key confirms');
    }
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});

check('project memory: changed files keep the draft, read errors are recoverable, and German at 720 px wraps sources', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  let failed = true;
  api.projectMemory = async () => { if (failed) throw 'error.memory.read'; return sample(); };
  api.editProjectMemory = async () => { throw 'error.memory.changed'; };
  const root = document.createElement('div'); root.dataset.motion = 'none'; root.style.width = '440px'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    ok(root.textContent!.includes(t('error.memory.read')), 'a read failure is not an empty list');
    failed = false; button(root, 'memory.refresh').click(); await settle();
    button(root, 'memory.edit').click(); await settle();
    const text = root.querySelector('textarea')!.value;
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    equal(root.querySelector('textarea')!.value, text);
    ok(root.textContent!.includes(t('error.memory.changed')));
    button(root, 'memory.cancel').click(); useLanguage('de'); await settle();
    await document.fonts.ready;
    ok(root.scrollWidth <= root.clientWidth + 1, 'no horizontal overflow in a narrow reading column');
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; useLanguage('en'); }
});

check('project memory: an editor blocks unattended updates until saved or cancelled, including a failed save', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory, dirty: api.draftDirty };
  const initial = dirtyCount();
  let blocked = initial > 0;
  let finish!: () => void;
  let fail!: (error: string) => void;
  api.projectMemory = async () => sample();
  api.draftDirty = async (dirty) => { blocked = dirty; };
  api.editProjectMemory = () => new Promise<void>((resolve, reject) => { finish = resolve; fail = reject; });
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  const protectedDraft = () => {
    ok(dirtyCount() > initial, 'the memory editor is registered with the shared draft protection');
    equal(blocked, true, 'the shell must defer an unattended install');
  };
  const released = () => {
    equal(dirtyCount(), initial, 'this editor no longer holds the install');
    equal(blocked, initial > 0);
  };
  try {
    button(root, 'memory.title').click(); await settle(); released();
    button(root, 'memory.edit').click(); await settle();
    const field = root.querySelector('textarea')!;
    field.value = ''; field.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    protectedDraft();
    button(root, 'memory.cancel').click(); await settle(); released();
    button(root, 'memory.edit').click(); await settle();
    const draft = 'Keep this unsaved memory edit.';
    root.querySelector('textarea')!.value = draft;
    root.querySelector('textarea')!.dispatchEvent(new Event('input', { bubbles: true })); await settle();
    protectedDraft();
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    protectedDraft();
    fail('error.memory.changed'); await settle();
    equal(root.querySelector('textarea')!.value, draft, 'a failed save keeps the editor and its words');
    protectedDraft();
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    protectedDraft();
    finish(); await settle();
    equal(root.querySelector('textarea'), null, 'a successful save closes the editor');
    released();
  } finally {
    finish?.(); await settle(); dispose(); root.remove();
    api.projectMemory = before.read; api.editProjectMemory = before.edit; api.draftDirty = before.dirty;
  }
});

check('project memory: a pending save keeps unattended updates blocked after its editor unmounts', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory, dirty: api.draftDirty };
  const initial = dirtyCount();
  let blocked = initial > 0;
  let finish!: () => void;
  api.projectMemory = async () => sample();
  api.draftDirty = async (dirty) => { blocked = dirty; };
  api.editProjectMemory = () => new Promise<void>((resolve) => { finish = resolve; });
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.edit').click(); await settle();
    root.querySelector('form')!.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
    button(root, 'memory.title').click(); await settle();
    equal(root.querySelector('textarea'), null, 'the closed fold unmounted the editor');
    ok(dirtyCount() > initial, 'the pending write owns protection independently of the editor');
    equal(blocked, true, 'the shell must wait for the pending save');
    finish(); await settle();
    equal(dirtyCount(), initial, 'the completed write releases its protection even after unmount');
    equal(blocked, initial > 0);
  } finally {
    finish?.(); await settle(); dispose(); root.remove();
    api.projectMemory = before.read; api.editProjectMemory = before.edit; api.draftDirty = before.dirty;
  }
});

check('project memory: switching projects drops the old draft and delete confirmation even at the same file offset', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  api.projectMemory = async (id) => { const data = sample(); data.files[0].entries[0].text = id; return data; };
  let writes = 0;
  api.editProjectMemory = async () => { writes++; };
  const [project, setProject] = createSignal('first');
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { get projectId() { return project(); } }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.edit').click(); await settle();
    root.querySelector('textarea')!.value = 'private first project draft';
    root.querySelector('textarea')!.dispatchEvent(new Event('input', { bubbles: true }));
    setProject('second'); await settle();
    equal(root.querySelector('textarea'), null, 'no editor follows into another project');
    ok(!root.textContent!.includes('private first project draft'));
    button(root, 'memory.delete').click(); await settle();
    setProject('third'); await settle();
    equal([...root.querySelectorAll('button')].some((b) => b.textContent === t('memory.confirmDelete')), false);
    equal(writes, 0);
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});

check('project memory: a large list mounts fifty entries at a time and closing releases them', async () => {
  const before = api.projectMemory;
  api.projectMemory = async () => {
    const data = sample();
    data.files = [data.files[0]];
    data.files[0].entries = Array.from({ length: 300 }, (_, n) => ({ start: n * 10, end: n * 10 + 10, line: n + 1, text: `Entry ${n}` }));
    return data;
  };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    equal(root.querySelectorAll('.memory-entry').length, 50);
    button(root, 'memory.more').click(); await settle();
    equal(root.querySelectorAll('.memory-entry').length, 100);
    button(root, 'memory.title').click(); await settle();
    equal(root.querySelectorAll('.memory-entry').length, 0);
  } finally { dispose(); root.remove(); api.projectMemory = before; }
});

check('project memory: refresh prevents an editor from borrowing a newer file revision for an older draft', async () => {
  const before = api.projectMemory;
  let reads = 0;
  let finish!: (value: MemoryList) => void;
  api.projectMemory = async () => ++reads === 1 ? sample() : new Promise<MemoryList>((resolve) => { finish = resolve; });
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.refresh').click(); await settle();
    equal(button(root, 'memory.edit').disabled, true);
    equal(button(root, 'memory.delete').disabled, true);
    button(root, 'memory.edit').click(); equal(root.querySelector('textarea'), null);
    const changed = sample(); changed.files[0].revision = 'changed'; changed.files[0].entries[0].text = 'The changed file.';
    finish(changed); await settle();
    button(root, 'memory.edit').click(); await settle();
    equal(root.querySelector('textarea')!.value, 'The changed file.');
  } finally { dispose(); root.remove(); api.projectMemory = before; }
});

check('project memory: a decision shows its author, an AI one reads "not reviewed" until Keep, and an unmarked one claims nothing', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  const asked: Array<{ start: number; text: string | null; keep?: boolean }> = [];
  const data: MemoryList = { incomplete: false, files: [
    { path: '.faber/decisions.md', source: 'faber', revision: 'decisions', tooLarge: false, entries: [
      { start: 0, end: 30, line: 1, text: '- Use SQLite. [by AI]', body: '- Use SQLite.', author: 'ai' },
      { start: 31, end: 60, line: 2, text: '- Keep the export. [by person]', body: '- Keep the export.', author: 'person' },
      { start: 61, end: 80, line: 3, text: '- Ship on Fridays' },
    ] },
  ] };
  api.projectMemory = async () => structuredClone(data);
  api.editProjectMemory = async (_id, edit) => {
    asked.push({ start: edit.start, text: edit.text, keep: edit.keep });
    Object.assign(data.files[0].entries[0], { author: 'ai', reviewed: true });
  };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    const flags = () => [...root.querySelectorAll('.memory-flag')].map((f) => f.textContent);
    equal(JSON.stringify(flags()), JSON.stringify([t('memory.author.ai'), t('memory.author.person')]), 'the unmarked decision claims no author');
    ok(root.textContent!.includes('- Use SQLite.') && !root.textContent!.includes('[by AI]'), 'the mark is a flag, not part of the words');
    equal([...root.querySelectorAll('button')].filter((b) => b.textContent === t('memory.keep')).length, 1, 'only the AI decision offers Keep');
    button(root, 'memory.keep').click(); await settle();
    equal(JSON.stringify(asked), JSON.stringify([{ start: 0, text: null, keep: true }]));
    equal(JSON.stringify(flags()), JSON.stringify([t('memory.author.aiKept'), t('memory.author.person')]));
    equal([...root.querySelectorAll('button')].some((b) => b.textContent === t('memory.keep')), false, 'nothing left to keep');
    equal(document.activeElement?.textContent, t('memory.edit'), 'focus moves to the kept row, not to the page');
    equal(root.querySelector('[role="status"].sr-only')!.textContent, t('memory.kept'), 'a screen reader hears the decision was kept');
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});

check('project memory: Keep leaves a long list as long as it was', async () => {
  const before = { read: api.projectMemory, edit: api.editProjectMemory };
  const data: MemoryList = { incomplete: false, files: [
    { path: '.faber/decisions.md', source: 'faber', revision: 'many', tooLarge: false, entries: Array.from({ length: 60 }, (_, i) => (
      { start: i * 10, end: i * 10 + 9, line: i + 1, text: `- Choice ${i}. [by AI]`, body: `- Choice ${i}.`, author: 'ai' as const })) },
  ] };
  api.projectMemory = async () => structuredClone(data);
  api.editProjectMemory = async (_id, edit) => { Object.assign(data.files[0].entries[edit.start / 10], { reviewed: true }); };
  const root = document.createElement('div'); root.dataset.motion = 'none'; document.body.append(root);
  const dispose = render(() => createComponent(ProjectMemory, { projectId: 'memory-project' }), root);
  try {
    button(root, 'memory.title').click(); await settle();
    button(root, 'memory.more').click(); await settle();
    equal(root.querySelectorAll('.memory-entry').length, 60);
    [...root.querySelectorAll<HTMLButtonElement>('button')].filter((b) => b.textContent === t('memory.keep'))[55].click(); await settle();
    equal(root.querySelectorAll('.memory-entry').length, 60, 'the list is not folded back to 50 rows');
    equal(root.querySelectorAll('.memory-flag.ask').length, 59);
    equal(document.activeElement?.closest('.memory-entry')?.textContent?.includes('Choice 55.'), true, 'focus stays on the row that was kept');
  } finally { dispose(); root.remove(); api.projectMemory = before.read; api.editProjectMemory = before.edit; }
});
