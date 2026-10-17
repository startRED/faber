import DOMPurify from 'dompurify';
import { marked, type Token } from 'marked';
import { createRenderEffect } from 'solid-js';

// Model output is untrusted: rendered as Markdown, then stripped of anything that could run. Every piece of HTML that reaches
// the page passes through DOMPurify here, and nowhere else.
const OPTIONS = { async: false, gfm: true } as const;

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
}
const shown = new WeakMap<HTMLElement, Shown>();

/** The top-level blocks of a text: one entry per paragraph, list, code block, quote or table. A block of raw HTML can open in
 * one block and close in another, so a text that has one is a single block, sanitised whole. */
function blocksOf(text: string): { raw: string; token?: Token }[] {
  const tokens = marked.lexer(text, OPTIONS).filter((token) => token.type !== 'space');
  if (tokens.some((token) => token.type === 'html')) return [{ raw: text }];
  return tokens.map((token) => ({ raw: token.raw, token }));
}

/**
 * Draws `text` into `root` block by block. A block whose source did not change keeps its nodes, so while an answer streams
 * only the open block at the end is parsed, sanitised and built again, and the reader's selection and a code block's scroll
 * stay where they are.
 */
export function renderMarkdown(root: HTMLElement, text: string): void {
  const blocks = blocksOf(text);
  const before = shown.get(root) ?? { raws: [], nodes: [] };
  let same = 0;
  while (same < blocks.length && same < before.raws.length && before.raws[same] === blocks[same]!.raw) same++;
  for (const stale of before.nodes.slice(same)) for (const node of stale) node.remove();
  const raws = before.raws.slice(0, same);
  const nodes = before.nodes.slice(0, same);
  for (const block of blocks.slice(same)) {
    const template = document.createElement('template');
    template.innerHTML = tidy(block.token ? marked.parser([block.token], OPTIONS) : marked.parse(block.raw, OPTIONS));
    raws.push(block.raw);
    nodes.push([...template.content.childNodes]);
    root.append(template.content);
  }
  shown.set(root, { raws, nodes });
}

/** An answer's Markdown. The element it makes holds the blocks; `text` is read on its own, so a change elsewhere in the
 * message never draws it again. */
export function Markdown(props: { text: string }) {
  // The element exists before the first draw: a render effect runs as soon as it is made.
  const root = (<div />) as HTMLDivElement;
  createRenderEffect(() => renderMarkdown(root, props.text));
  return root;
}
