import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type FileView } from '../../ui/api.ts';
import { FilePane } from '../../ui/FileView.tsx';
import { useLanguage } from '../../ui/i18n.ts';
import { linksFile, namesFile, openedFile, showFile } from '../../ui/fileref.ts';
import { html, openLinks } from '../../ui/markdown.tsx';
import { check, equal, ok } from './check.ts';

check('file at a line: a code span or link that names a file becomes a link, a member, version or web link does not', () => {
  for (const named of ['src/app.ts:42', 'src/app.ts', 'Thread.tsx', 'engine.rs:12', String.raw`E:\work\app\main.rs`, 'docs/my notes.md', 'a.ts#L4-L9']) {
    ok(namesFile(named), `${named} names a file`);
  }
  for (const not of ['console.log', 'c.messages', 'v1.2', '0.5.0', 'origin/phase-1-base', 'src/*.ts', 'https://example.com/a.ts', 'npm run build', '.env']) {
    ok(!namesFile(not), `${not} does not name a file`);
  }
  ok(linksFile('src/app.ts:42') && linksFile('Bar.tsx:42') && linksFile('file:///E:/a.ts') && linksFile('E:/work/a.ts'), 'a link to a file');
  ok(!linksFile('https://example.com') && !linksFile('mailto:a@b.c') && !linksFile('#top'), 'a page, a mail or an anchor is not');

  const drawn = document.createElement('div');
  drawn.innerHTML = html('See `src/app.ts:42`, [Bar.tsx:7](ui/Bar.tsx:7), [`x.ts`](lib/x.ts), `console.log`, [docs](https://example.com) and [`y.ts:3`](https://example.com/y).');
  const files = [...drawn.querySelectorAll<HTMLAnchorElement>('a.file-ref')].map((a) => a.dataset.file);
  equal(JSON.stringify(files), JSON.stringify(['src/app.ts:42', 'ui/Bar.tsx:7', 'lib/x.ts']));
  equal(drawn.querySelectorAll('a a').length, 0, 'a code span inside a file link is not a second link');
  equal(drawn.querySelector<HTMLAnchorElement>('a:not(.file-ref)')?.getAttribute('href'), 'https://example.com');
  equal(drawn.querySelectorAll('a.file-ref').length, 3, 'a file named inside a web link stays part of that link');
  const hostile = document.createElement('div');
  hostile.innerHTML = html('[x](a.ts"onmouseover="alert(1))');
  equal(hostile.querySelector('[onmouseover]'), null, 'the address stays a value, never markup');
});

check('file at a line: a click on a named file opens it, and the pane marks the line in the middle of what it shows', async () => {
  useLanguage('en');
  const drawn = document.createElement('div');
  drawn.innerHTML = html('Look at `src/app.ts:42`.');
  document.body.append(drawn);
  drawn.addEventListener('click', openLinks);
  const before = api.readFileAt;
  const asked: string[] = [];
  const text = Array.from({ length: 80 }, (_, i) => `line ${i + 1}`).join('\n');
  api.readFileAt = async (id: string, at: string): Promise<FileView> => {
    asked.push(`${id} ${at}`);
    return at === 'gone.ts'
      ? { path: '', first: 1, text: '', total: 0, tooLarge: false, binary: false, error: 'error.file.outside' }
      : { path: 'src/app.ts', line: 42, first: 1, text, total: 80, tooLarge: false, binary: false };
  };
  const host = document.createElement('div');
  host.style.cssText = 'position: fixed; top: 0; left: 0; width: 360px; height: 300px; display: flex; flex-direction: column;';
  document.body.append(host);
  try {
    drawn.querySelector<HTMLAnchorElement>('a.file-ref')!.click();
    equal(openedFile()?.ref, 'src/app.ts:42', 'the click opens what the reply named');
    const dispose = render(() => createComponent(FilePane, { conversationId: 'c1', at: openedFile()!.ref, onBack: () => {}, onClose: () => {} }), host);
    try {
      for (let i = 0; i < 50 && !host.querySelector('.file-line'); i++) await new Promise((resolve) => setTimeout(resolve, 10));
      await new Promise((resolve) => setTimeout(resolve, 30));
      equal(asked.join(), 'c1 src/app.ts:42');
      equal(host.querySelectorAll('.file-line').length, 80);
      const marked = [...host.querySelectorAll<HTMLElement>('.file-line.mark')];
      equal(marked.length, 1);
      equal(marked[0]!.textContent, 'line 42');
      equal(marked[0]!.dataset.n, '42');
      const body = host.querySelector<HTMLElement>('.file-view')!;
      ok(body.scrollTop > 0, 'the marked line is scrolled into view');
      equal(host.querySelector('h2')!.textContent, 'app.ts');
    } finally {
      dispose();
    }
    const refused = render(() => createComponent(FilePane, { conversationId: 'c1', at: 'gone.ts', onBack: () => {}, onClose: () => {} }), host);
    try {
      for (let i = 0; i < 50 && !host.textContent?.includes('only opens'); i++) await new Promise((resolve) => setTimeout(resolve, 10));
      await new Promise((resolve) => setTimeout(resolve, 30));
      ok(host.textContent?.includes('only opens files inside'), host.textContent ?? '');
      equal(host.querySelectorAll('.file-line').length, 0);
    } finally {
      refused();
    }
  } finally {
    api.readFileAt = before;
    showFile(undefined);
    drawn.remove();
    host.remove();
  }
});
