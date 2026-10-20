import { Renderer, type Tokens } from 'marked';
import { createSignal } from 'solid-js';

// A file the agent names (`src/app.ts:42`, or a link to it) becomes a link that opens it in the running column (parity P2):
// `data-file` holds what the agent wrote, and the engine decides whether it is a file inside the conversation's folder.
const LINE_AT = /(?::\d+(?:[:-]\d+)?|#L\d+(?:-L?\d+)?)$/;
const SOURCE = /\.(?:ts|tsx|js|jsx|mjs|cjs|rs|md|json|jsonc|toml|ya?ml|css|scss|html?|py|go|java|kt|swift|c|h|cc|cpp|hpp|cs|rb|php|sh|ps1|sql|txt|xml|svg|vue|svelte|lock|ini|cfg)$/i;
const SCHEME = /^[a-z][a-z0-9+.-]*:/i;

/** Whether a code span reads as a file: a path with folders, a name with a line, or a source file's name (`Thread.tsx`), never
 * a member such as `console.log`, a version or a glob. */
export function namesFile(raw: string): boolean {
  const text = raw.trim();
  if (!text || text.length > 300 || /[\n<>"|?*]/.test(text) || (/^[a-z][a-z0-9+.-]*:\/\//i.test(text) && !/^file:\/\//i.test(text))) return false;
  const path = text.replace(LINE_AT, '');
  const name = path.split(/[\\/]/).pop() ?? '';
  if (!/^[^\s.].*\.[A-Za-z][A-Za-z0-9]{0,7}$/.test(name)) return false;
  const folders = /[\\/]/.test(path);
  if (/\s/.test(path) && !folders) return false;
  return folders || path !== text || SOURCE.test(name);
}

/** Whether a link's address names a file rather than a page: no web or mail scheme, or a drive letter, or `file:`. */
export function linksFile(href: string): boolean {
  if (!href || href.startsWith('#') || href.startsWith('//')) return false;
  if (/^[a-z]:[\\/]/i.test(href) || /^file:/i.test(href)) return true;
  // `Bar.tsx:42` reads like a scheme; `mailto:a@b.c` and `javascript:` are not files.
  return !SCHEME.test(href) || (namesFile(href) && /^[^:]*\.[A-Za-z][A-Za-z0-9]{0,7}:\d/.test(href));
}

const attr = (text: string) => text.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
/** The Markdown renderer of answers: as marked draws, with file links. */
export const fileRenderer = new Renderer();
const renderer = fileRenderer;
const plainLink = renderer.link;
const plainCode = renderer.codespan;
// A code span inside any link stays plain: one link, never one inside another.
let inLink = false;
renderer.link = function (token: Tokens.Link) {
  inLink = true;
  try {
    if (!linksFile(token.href)) return plainLink.call(this, token);
    return `<a class="file-ref" href="#" data-file="${attr(token.href)}">${this.parser.parseInline(token.tokens)}</a>`;
  } finally {
    inLink = false;
  }
};
renderer.codespan = function (token: Tokens.Codespan) {
  const code = plainCode.call(this, token);
  return !inLink && namesFile(token.text) ? `<a class="file-ref" href="#" data-file="${attr(token.text.trim())}">${code}</a>` : code;
};

const [opened, setOpened] = createSignal<{ ref: string }>();
/** The file a reply named that is open in the running column, for the conversation on screen. */
export const openedFile = opened;
/** Opens a file a reply named in the running column, or goes back. A new object each time, so the same file opens again. */
export const showFile = (ref?: string) => setOpened(ref === undefined ? undefined : { ref });
