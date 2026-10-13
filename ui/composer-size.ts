/** Measure without painting a collapsed frame, then let the shared composer transition travel to its new height. */
export function resizeComposer(field: HTMLTextAreaElement | undefined, text: string): void {
  if (!field?.isConnected) return;
  const previous = field.getBoundingClientRect().height;
  const style = getComputedStyle(field);
  field.style.transition = 'none';
  field.style.height = '0px';
  const height = Math.min(parseFloat(style.maxHeight), Math.max(parseFloat(style.minHeight), text ? field.scrollHeight : 0));
  field.style.height = `${previous}px`;
  field.getBoundingClientRect();
  field.style.removeProperty('transition');
  field.style.height = `${height}px`;
}
