import DOMPurify from 'dompurify';
import { Lexer, type Links, marked, type Token } from 'marked';
import { createContext, createRenderEffect, useContext, type Accessor } from 'solid-js';
import { api } from './api.ts';
import { fileRenderer, showFile } from './fileref.ts';
import { iconMarkup } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { openViewer } from './Viewer.tsx';

// Model output is untrusted: rendered as Markdown, then stripped of anything that could run. Every piece of HTML that reaches
// the page passes through DOMPurify here, and nowhere else.
const OPTIONS = { async: false, gfm: true, renderer: fileRenderer } as const;

// The last two words of a plain heading go in `em`, which the look draws in its quieter tone.
const quiet = (h3: string, text: string) => {
  const words = text.split(' ');
  return words.length < 3 ? h3 : `<h3>${words.slice(0, -2).join(' ')} <em>${words.slice(-2).join(' ')}</em></h3>`;
};
const tidy = (markup: string) => DOMPurify.sanitize(markup).replace(/<h3>([^<]*)<\/h3>/g, quiet);

// Short texts that are drawn again and again (a narration, a report) are parsed once. Small and bounded: this is not a cache
// of the thread.
const parsed = new Map<string, string>();
const LIMIT = 64;

const copiedAt = new WeakMap<Element, ReturnType<typeof setTimeout>>();
const codeButtons = new WeakSet<Element>();

// The copy button of a code block: only that block's words, and the icon turns into a check for a moment (one timer, set by the click).
function copyCode(button: Element) {
  const words = button.closest('.codeblock')?.querySelector('code')?.textContent ?? '';
  void Promise.resolve(navigator.clipboard.writeText(words)).then(
    () => {
      const mark = button.querySelector('.swap');
      clearTimeout(copiedAt.get(button));
      mark?.classList.add('done');
      copiedAt.set(button, setTimeout(() => mark?.classList.remove('done'), 1600));
    },
    () => {},
  );
}

/** Links in answers open in the person's browser, never inside Faber; a file the answer names opens in the running column; a
 * picture opens full screen; a code block copies itself. */
export function openLinks(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const copy = target.closest('.cb-copy');
  if (copy) return copyCode(copy);
  const a = target.closest('a');
  if (!a) {
    if (target instanceof HTMLImageElement && target.closest('.md')) openViewer(target);
    return;
  }
  e.preventDefault();
  if (a.dataset.file) showFile(a.dataset.file);
  else if (/^https?:\/\//.test(a.href)) void api.openLink(a.href);
}

/** The safe HTML of a whole text. */
export function html(text: string): string {
  const known = parsed.get(text);
  if (known !== undefined) return known;
  const made = tidy(marked.parse(text, OPTIONS));
  if (parsed.size >= LIMIT) parsed.clear();
  parsed.set(text, made);
  return made;
}

interface Shown {
  raws: string[];
  nodes: ChildNode[][];
  // The reference link definitions (`[1]: https://…`) the blocks were drawn with: one that appears later changes how an
  // earlier block reads, so a change here draws every block again.
  links: string;
}
const shown = new WeakMap<HTMLElement, Shown>();

/** Enter or Space on a picture of an answer opens it, as a click does (the picture is a button to the keyboard). */
export function pressImage(e: KeyboardEvent) {
  if ((e.key !== 'Enter' && e.key !== ' ') || !(e.target instanceof HTMLImageElement) || !e.target.closest('.md')) return;
  e.preventDefault();
  openViewer(e.target);
}

// A code block gets a bar above it: the language it was written in, when it says one, and a button that copies that block only.
// A picture can be reached and opened with the keyboard.
function dress(root: DocumentFragment) {
  for (const img of root.querySelectorAll('img')) {
    if (img.closest('a')) continue;
    img.tabIndex = 0;
    img.setAttribute('role', 'button');
    img.setAttribute('aria-label', t('viewer.open', { name: img.alt || t('composer.image') }));
  }
  for (const code of root.querySelectorAll('pre > code')) {
    const pre = code.parentElement!;
    const box = document.createElement('div');
    box.className = 'codeblock';
    const bar = document.createElement('div');
    bar.className = 'cb-bar';
    const language = document.createElement('span');
    language.className = 'cb-lang';
    language.textContent = /language-([\w+#.-]+)/.exec(code.className)?.[1] ?? '';
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'iconbtn cb-copy';
    codeButtons.add(button);
    button.title = t('code.copy');
    button.setAttribute('aria-label', t('code.copy'));
    button.innerHTML = `<span class="swap">${iconMarkup('copy')}${iconMarkup('check')}</span>`;
    bar.append(language, button);
    pre.replaceWith(box);
    box.append(bar, pre);
  }
}

/** The top-level blocks of a text: one entry per paragraph, list, code block, quote or table. A block of raw HTML can open in
 * one block and close in another, so a text that has one is a single block, sanitised whole. */
function lex(text: string, references?: Links) {
  const lexer = new Lexer(OPTIONS);
  if (references) lexer.tokens.links = { ...references };
  return lexer.lex(text);
}
function blocksOf(text: string, references?: Links): { blocks: { raw: string; token?: Token }[]; links: Links } {
  const lexed = lex(text, references);
  const tokens = lexed.filter((token) => token.type !== 'space');
  if (tokens.some((token) => token.type === 'html')) return { blocks: [{ raw: text }], links: lexed.links };
  return { blocks: tokens.map((token) => ({ raw: token.raw, token })), links: lexed.links };
}

// Reconcile only sanitised Markdown. Keeping compatible nodes avoids losing selection, focus and code scroll on a delta.
function updateNodes(parent: Node, fresh: ChildNode[], start = 0): void {
  for (const [offset, next] of fresh.entries()) {
    const old = parent.childNodes[start + offset];
    if (!old) { parent.appendChild(next); continue; }
    if (old.nodeType !== next.nodeType || old.nodeName !== next.nodeName) { parent.replaceChild(next, old); continue; }
    if (old instanceof Text && next instanceof Text) {
      if (next.data.startsWith(old.data)) old.appendData(next.data.slice(old.length));
      else if (old.data !== next.data) old.replaceData(0, old.length, next.data);
    } else if (old instanceof Element && next instanceof Element) {
      for (const attribute of [...old.attributes]) if (!next.hasAttribute(attribute.name)) old.removeAttribute(attribute.name);
      for (const attribute of [...next.attributes]) if (old.getAttribute(attribute.name) !== attribute.value) old.setAttribute(attribute.name, attribute.value);
      // The copy button owns its temporary success icon; new text must not reset it.
      if (!codeButtons.has(old) || !codeButtons.has(next)) updateNodes(old, [...next.childNodes]);
    } else if (old.nodeValue !== next.nodeValue) old.nodeValue = next.nodeValue;
  }
  while (parent.childNodes.length > start + fresh.length) parent.lastChild!.remove();
}

/**
 * Draws `text` into `root` block by block. A block whose source did not change keeps its nodes, so while an answer streams
 * only the changing suffix is parsed and sanitised. Compatible nodes in that suffix are updated in place so selection,
 * keyboard focus and a code block's scroll stay where they are.
 */
export const referenceLinks = (text: string): Links => marked.lexer(text, OPTIONS).links;

export function renderMarkdown(root: HTMLElement, text: string, references?: Links): void {
  const { blocks, links: ownLinks } = blocksOf(text, references);
  const links = references ?? ownLinks;
  const linked = JSON.stringify(links);
  const before = shown.get(root) ?? { raws: [], nodes: [], links: linked };
  let same = 0;
  while (before.links === linked && same < blocks.length && same < before.raws.length && before.raws[same] === blocks[same]!.raw) same++;
  const raws = before.raws.slice(0, same);
  const nodes = before.nodes.slice(0, same);
  const fresh: ChildNode[] = [];
  const sizes: number[] = [];
  for (const block of blocks.slice(same)) {
    const template = document.createElement('template');
    const tokens = block.token ? [block.token] : lex(block.raw, references);
    template.innerHTML = tidy(marked.parser(Object.assign(tokens, { links }), OPTIONS));
    raws.push(block.raw);
    dress(template.content);
    sizes.push(template.content.childNodes.length);
    fresh.push(...template.content.childNodes);
  }
  let offset = nodes.reduce((count, block) => count + block.length, 0);
  updateNodes(root, fresh, offset);
  const drawn = [...root.childNodes];
  for (const size of sizes) {
    nodes.push(drawn.slice(offset, offset + size));
    offset += size;
  }
  shown.set(root, { raws, nodes, links: linked });
}

/** An answer's Markdown. The element it makes holds the blocks; `text` is read on its own, so a change elsewhere in the
 * message never draws it again. */
export const MarkdownReferences = createContext<Accessor<Links>>();

export function Markdown(props: { text: string; links?: Links }) {
  // The element exists before the first draw: a render effect runs as soon as it is made.
  const root = (<div />) as HTMLDivElement;
  const references = useContext(MarkdownReferences);
  createRenderEffect(() => renderMarkdown(root, props.text, references?.() ?? props.links));
  return root;
}
