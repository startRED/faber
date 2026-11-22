import DOMPurify from 'dompurify';
import { Lexer, type Links, marked, type Token, type Tokens } from 'marked';
import { createContext, createRenderEffect, onCleanup, useContext, type Accessor } from 'solid-js';
import { render } from 'solid-js/web';
import { api } from './api.ts';
import { fileRenderer, showFile } from './fileref.ts';
import { iconMarkup } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { openViewer } from './Viewer.tsx';
import { localMedia, ReplyGallery, ReplyMedia, ReplyMediaContext, type MediaRef, type MediaSession } from './Media.tsx';
import { isWebPicture, ReplyWebMedia, ReplyWebPictures, replyAddress, webReply, type WebReply } from './ReplyWebMedia.tsx';

// Model output is untrusted: rendered as Markdown, then stripped of anything that could run. Every piece of HTML that reaches
// the page passes through DOMPurify here, and nowhere else.
const OPTIONS = { async: false, gfm: true, renderer: fileRenderer } as const;

// The last two words of a plain heading go in `em`, which the look draws in its quieter tone.
const quiet = (h3: string, text: string) => {
  const words = text.split(' ');
  return words.length < 3 ? h3 : `<h3>${words.slice(0, -2).join(' ')} <em>${words.slice(-2).join(' ')}</em></h3>`;
};
const tidy = (markup: string) => DOMPurify.sanitize(markup, {
  FORBID_ATTR: ['data-reply-web-off', 'data-reply-picture-list'],
}).replace(/<h3>([^<]*)<\/h3>/g, quiet);

// Short texts that are drawn again and again (a narration, a report) are parsed once. Small and bounded: this is not a cache
// of the thread.
const parsed = new Map<string, string>();
const LIMIT = 64;

const copiedAt = new WeakMap<Element, ReturnType<typeof setTimeout>>();
const codeButtons = new WeakSet<Element>();
const mediaSlots = new WeakMap<Element, { key: string; dispose: () => void }>();

function disposeMedia(node: Node): void {
  if (!(node instanceof Element)) return;
  const mounted = mediaSlots.get(node);
  if (mounted) {
    mediaSlots.delete(node);
    mounted.dispose();
    return;
  }
  for (const child of node.children) disposeMedia(child);
}

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

/** A regular click on a reply link opens the person's browser; a file the answer names opens in the running column; a
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
type Block = {
  raw: string;
  tokens?: Token[];
  media?: MediaRef[];
  web?: WebReply;
  webPictures?: WebReply[];
  parts?: Block[];
  pictureList?: { ordered: boolean; start: number };
};

function paragraphMedia(token: Token, folder: string): MediaRef[] | undefined {
  if (token.type !== 'paragraph') return;
  const refs: MediaRef[] = [];
  for (const part of (token as Tokens.Paragraph).tokens) {
    if (part.type === 'br' || (part.type === 'text' && !part.text.trim())) continue;
    const ref = part.type === 'image' || part.type === 'link' ? localMedia(part.href, part.text, folder)
      : part.type === 'codespan' || (part.type === 'text' && !part.tokens && !/\s/.test(part.text.trim())) ? localMedia(part.text, '', folder)
      : undefined;
    if (!ref) return;
    refs.push(ref);
  }
  if (refs.length === 1 || (refs.length > 1 && refs.every((ref) => ref.kind === 'picture'))) return refs;
}

function paragraphWeb(token: Token) {
  if (token.type !== 'paragraph') return;
  const [part, second] = (token as Tokens.Paragraph).tokens;
  if (second || !part) return;
  if (part.type === 'link' || part.type === 'image') return webReply(part.href, part.text, part.type === 'image');
  if (part.type === 'text') return webReply(part.text.trim(), '', false);
}

function appendWebBlock(blocks: Block[], block: Block) {
  const previous = blocks.at(-1);
  const pictures = (part?: Block) => part?.pictureList ? undefined
    : part?.webPictures ?? (part?.web && isWebPicture(part.web) ? [part.web] : undefined);
  const before = pictures(previous);
  const next = pictures(block);
  if (previous && before && next) {
    previous.raw += '\n\n' + block.raw;
    previous.webPictures = [...before, ...next];
    previous.web = undefined;
  } else {
    blocks.push(block);
  }
}

/** The model may use explicit Markdown line breaks between standalone file links. */
function lineMedia(token: Token, folder: string): Block[] | undefined {
  if (token.type !== 'paragraph') return;
  const paragraph = token as Tokens.Paragraph;
  const inline = paragraph.tokens;
  if (inline.some((part) => part.type === 'br')) {
    const lines: Token[][] = [[]];
    for (const part of inline) {
      if (part.type === 'br') lines.push([]);
      else lines.at(-1)!.push(part);
    }
    const parts = lines.map((tokens) => {
      const line = { ...paragraph, tokens };
      return { raw: '', media: paragraphMedia(line, folder), web: paragraphWeb(line) };
    });
    if (parts.every((part) => part.media || part.web)) {
      const combined: Block[] = [];
      for (const part of parts) appendWebBlock(combined, part);
      return combined;
    }
  }
  // A bare media address on the line after a caption is still a standalone reference.
  const last = inline.at(-1);
  const before = inline.at(-2);
  if (last?.type === 'link' && last.raw === last.href && before?.type === 'text' && before.raw.endsWith('\n')) {
    const web = paragraphWeb({ ...paragraph, tokens: [last] });
    if (web) return [{ raw: '', tokens: [{ ...paragraph, tokens: inline.slice(0, -1) }] }, { raw: '', web }];
  }
}

function imageList(token: Token): Block | undefined {
  if (token.type !== 'list') return;
  const list = token as Tokens.List;
  const pictures = list.items.map((item) => {
    const [line, extra] = item.tokens;
    if (item.task || extra || (line?.type !== 'text' && line?.type !== 'paragraph')) return;
    const [image, other] = line.tokens ?? [];
    if (!other && image?.type === 'image') return webReply(image.href, image.text, true);
  });
  if (!pictures.length || !pictures.every((picture): picture is WebReply => !!picture)) return;
  return {
    raw: token.raw, webPictures: pictures,
    pictureList: { ordered: list.ordered, start: typeof list.start === 'number' ? list.start : 1 },
  };
}

function sentencePictures(token: Token, folder: string): Block[] | undefined {
  if (token.type !== 'paragraph') return;
  const paragraph = token as Tokens.Paragraph;
  const pictures: Block[] = [];
  const withoutPictures = (tokens: Token[]): Token[] => tokens.flatMap<Token>((part) => {
    if (part.type === 'image') {
      const media = localMedia(part.href, part.text, folder);
      const web = webReply(part.href, part.text, true);
      if (media?.kind === 'picture' || web) {
        appendWebBlock(pictures, { raw: '', media: media?.kind === 'picture' ? [media] : undefined, web });
        return part.text ? [{ type: 'text', raw: part.text, text: part.text }] : [];
      }
    }
    if (part.type !== 'link' && 'tokens' in part && Array.isArray(part.tokens)) {
      return [{ ...part, tokens: withoutPictures(part.tokens) }];
    }
    return [part];
  });
  const tokens = withoutPictures(paragraph.tokens);
  if (pictures.length) return [{ raw: '', tokens: [{ ...paragraph, tokens }] }, ...pictures];
}

function blocksOf(text: string, references?: Links, folder?: string): { blocks: Block[]; links: Links } {
  const lexed = lex(text, references);
  const blocks: Block[] = [];
  for (const token of lexed) {
    if (token.type === 'space') continue;
    const media = folder === undefined ? undefined : paragraphMedia(token, folder);
    const previous = blocks.at(-1);
    if (media?.[0]?.kind === 'picture' && previous?.media?.[0]?.kind === 'picture') {
      previous.raw += '\n\n' + token.raw;
      previous.media.push(...media);
      previous.tokens!.push(token);
    } else {
      const web = folder === undefined ? undefined : paragraphWeb(token);
      const list = folder === undefined ? undefined : imageList(token);
      const parts = folder === undefined || media || web ? undefined : lineMedia(token, folder) ?? sentencePictures(token, folder);
      appendWebBlock(blocks, list ?? (parts?.length === 1 && parts[0]?.webPictures
        ? { ...parts[0], raw: token.raw }
        : { raw: token.raw, tokens: [token], media, web, parts }));
    }
  }
  if (lexed.some((token) => token.type === 'html')) {
    return {
      blocks: [{ raw: text, parts: blocks.some((block) => block.media || block.web || block.parts || block.webPictures) ? blocks : undefined }],
      links: lexed.links,
    };
  }
  return { blocks, links: lexed.links };
}

// Reconcile only sanitised Markdown. Keeping compatible nodes avoids losing selection, focus and code scroll on a delta.
function updateNodes(parent: Node, fresh: ChildNode[], start = 0): void {
  for (const [offset, next] of fresh.entries()) {
    const old = parent.childNodes[start + offset];
    if (!old) { parent.appendChild(next); continue; }
    const oldMedia = old instanceof Element ? mediaSlots.get(old) : undefined;
    const nextMedia = next instanceof Element ? mediaSlots.get(next) : undefined;
    if (oldMedia && nextMedia && oldMedia.key === nextMedia.key) {
      disposeMedia(next);
      continue;
    }
    if (old.nodeType !== next.nodeType || old.nodeName !== next.nodeName || oldMedia || nextMedia) {
      parent.replaceChild(next, old);
      disposeMedia(old);
      continue;
    }
    if (old instanceof Text && next instanceof Text) {
      if (next.data.startsWith(old.data)) old.appendData(next.data.slice(old.length));
      else if (old.data !== next.data) old.replaceData(0, old.length, next.data);
    } else if (old instanceof Element && next instanceof Element) {
      for (const name of old.getAttributeNames()) if (!next.hasAttribute(name)) old.removeAttribute(name);
      for (const attribute of next.attributes) if (old.getAttribute(attribute.name) !== attribute.value) old.setAttribute(attribute.name, attribute.value);
      // The copy button owns its temporary success icon; new text must not reset it.
      if (!codeButtons.has(old) || !codeButtons.has(next)) updateNodes(old, [...next.childNodes]);
    } else if (old.nodeValue !== next.nodeValue) old.nodeValue = next.nodeValue;
  }
  while (parent.childNodes.length > start + fresh.length) {
    const removed = parent.lastChild!;
    parent.removeChild(removed);
    disposeMedia(removed);
  }
}

/**
 * Draws `text` into `root` block by block. A block whose source did not change keeps its nodes, so while an answer streams
 * only the changing suffix is parsed and sanitised. Compatible nodes in that suffix are updated in place so selection,
 * keyboard focus and a code block's scroll stay where they are.
 */
export const referenceLinks = (text: string): Links => marked.lexer(text, OPTIONS).links;

/** Preserve the file's Save and Show actions without drawing a second copy of an inline player. */
export function inlineMediaPaths(text: string, folder: string): Set<string> {
  const paths = (block: Block): string[] => block.parts?.flatMap(paths) ?? block.media?.map((ref) => ref.path) ?? [];
  return new Set(blocksOf(text, undefined, folder).blocks.flatMap(paths));
}

function mountMedia(slot: HTMLElement, block: Block, session: MediaSession) {
  if (block.webPictures) return render(() => <ReplyWebPictures pictures={block.webPictures!} list={block.pictureList} session={session}
    mode={session.web?.()?.mode} blocked={session.web?.()?.blocked ?? []}
    canOpen={session.web?.()?.canOpen ?? false}
    open={(url, title) => session.web?.()?.open(url, title) ?? Promise.resolve(false)} />, slot);
  if (block.web) return render(() => <ReplyWebMedia media={block.web!} session={session}
    mode={session.web?.()?.mode} blocked={session.web?.()?.blocked ?? []}
    canOpen={session.web?.()?.canOpen ?? false}
    open={(url, title) => session.web?.()?.open(url, title) ?? Promise.resolve(false)} />, slot);
  const media = block.media!;
  return render(() => media.length === 1 ? <ReplyMedia media={media[0]!} session={session} /> : <ReplyGallery media={media} session={session} />, slot);
}

function registerMedia(slot: HTMLElement, block: Block, session: MediaSession) {
  mediaSlots.set(slot, { key: JSON.stringify(block.media ?? block.web ?? [block.webPictures, block.pictureList]), dispose: mountMedia(slot, block, session) });
}

export function renderMarkdown(root: HTMLElement, text: string, references?: Links, session?: MediaSession): void {
  const { blocks, links: ownLinks } = blocksOf(text, references, session?.folder);
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
    const markup = (part: Block) => marked.parser(Object.assign(part.tokens ?? lex(part.raw, references), { links }), OPTIONS);
    const marks = new Map<string, Block>();
    const mediaMarkup = (part: Block): string => {
      if (part.parts) return part.parts.map(mediaMarkup).join('');
      if (!part.media && !part.web && !part.webPictures) return markup(part);
      // Keep raw HTML in one sanitising pass. Only renderer-created markers can mount players.
      const key = crypto.getRandomValues(new Uint32Array(4)).join('-');
      marks.set(key, part);
      return `<div data-reply-media="${key}"></div>`;
    };
    const source = block.parts ? mediaMarkup(block) : markup(block);
    template.innerHTML = tidy(source);
    raws.push(block.raw);
    if (session && (block.media || block.web || block.webPictures)) {
      const slot = document.createElement('div');
      registerMedia(slot, block, session);
      template.content.replaceChildren(slot);
    } else {
      if (session) {
        for (const node of template.content.querySelectorAll('video, audio, iframe, source')) node.remove();
        for (const img of template.content.querySelectorAll('img')) {
          img.removeAttribute('srcset');
          const address = img.getAttribute('src')?.trim() ?? '';
          const ref = localMedia(address, img.alt, session.folder);
          const web = replyAddress(address.startsWith('//') ? 'https:' + address : address);
          if (!web && /^(?:https?:)?\/\//i.test(address)) {
            img.replaceWith(document.createTextNode(img.alt));
            continue;
          }
          if (!ref && !web) {
            if (!address.startsWith('data:')) img.removeAttribute('src');
            continue;
          }
          const name = ref?.name ?? (img.alt || web!.href);
          if (img.closest('a')) {
            img.replaceWith(document.createTextNode(name));
            continue;
          }
          const link = document.createElement('a');
          link.href = web?.href ?? '#';
          if (ref) {
            link.className = 'file-ref';
            link.dataset.file = ref.path;
          }
          link.textContent = name;
          img.replaceWith(link);
        }
      }
      dress(template.content);
      if (session && marks.size) {
        for (const slot of template.content.querySelectorAll<HTMLElement>('div[data-reply-media]')) {
          const key = slot.dataset.replyMedia ?? '';
          const refs = marks.get(key);
          if (!refs) continue;
          marks.delete(key);
          slot.removeAttribute('data-reply-media');
          registerMedia(slot, refs, session);
        }
      }
    }
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
  const media = useContext(ReplyMediaContext);
  let session: MediaSession | undefined;
  function disposeBlocks() {
    for (const node of root.children) disposeMedia(node);
    shown.delete(root);
  }
  createRenderEffect(() => {
    const current = media?.();
    if (current !== session) {
      disposeBlocks();
      root.replaceChildren();
      session = current;
    }
    renderMarkdown(root, props.text, references?.() ?? props.links, current);
  });
  root.addEventListener('click', (event) => {
    const link = (event.target as HTMLElement).closest<HTMLAnchorElement>('a[data-file]');
    const ref = link && session && localMedia(link.dataset.file ?? '', link.textContent ?? '', session.folder);
    if (!link || !ref || !session) return;
    event.preventDefault();
    event.stopPropagation();
    void session.open(ref, link);
  });
  onCleanup(disposeBlocks);
  return root;
}
