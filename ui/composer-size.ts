import { reveal } from './motion.ts';

const growing = new WeakMap<HTMLTextAreaElement, Animation>();

/** Use the browser's text layout, including soft wrapping and the textarea's scrollbar width. */
export function caretOnFirstLine(field: HTMLTextAreaElement): boolean {
  const caret = field.selectionStart;
  if (!caret) return true;
  if (field.value.slice(0, caret).includes('\n')) return false;
  const style = getComputedStyle(field);
  const mirror = document.createElement('div');
  for (const property of ['font', 'font-family', 'font-size', 'font-weight', 'font-style', 'font-stretch', 'font-variant',
    'font-kerning', 'font-feature-settings', 'font-variation-settings', 'text-rendering', 'letter-spacing',
    'word-spacing', 'line-height', 'padding', 'text-indent', 'text-transform', 'text-align', 'direction', 'tab-size',
    'white-space', 'overflow-wrap', 'word-break']) {
    mirror.style.setProperty(property, style.getPropertyValue(property));
  }
  Object.assign(mirror.style, { position: 'fixed', visibility: 'hidden', pointerEvents: 'none', left: '-10000px', top: '0',
    boxSizing: 'border-box', width: `${field.clientWidth}px`, border: '0' });
  const text = document.createTextNode(field.value);
  mirror.append(text);
  document.body.append(mirror);
  try {
    const position = document.createRange();
    position.setStart(text, 0);
    position.collapse(true);
    const first = position.getBoundingClientRect().top;
    position.setStart(text, caret);
    position.collapse(true);
    return position.getBoundingClientRect().top === first;
  }
  finally { mirror.remove(); }
}

/**
 * Measure and set the new height in one task, so the collapse to 0 px is never painted. The field is at its new size at
 * once; the card around it opens up to it with a clip reveal, which costs no layout (unlike animating the height).
 */
export function resizeComposer(field: HTMLTextAreaElement | undefined, text: string): void {
  if (!field?.isConnected) return;
  const previous = field.getBoundingClientRect().height;
  const style = getComputedStyle(field);
  field.style.height = '0px';
  const height = Math.min(parseFloat(style.maxHeight), Math.max(parseFloat(style.minHeight), text ? field.scrollHeight : 0));
  field.style.height = `${height}px`;
  growing.get(field)?.cancel();
  const card = field.closest<HTMLElement>('.composer');
  const animation = card && height > previous + 1 ? reveal(card, height - previous) : undefined;
  if (animation) growing.set(field, animation);
  else growing.delete(field);
}
