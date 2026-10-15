import { reveal } from './motion.ts';

const growing = new WeakMap<HTMLTextAreaElement, Animation>();

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
