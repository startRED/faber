const FOCUSABLE = 'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])';

/** Tab and Shift+Tab stay inside an open modal dialog: past its last stop they go back to its first, and the other way. */
export function keepFocusIn(e: KeyboardEvent, dialog: HTMLElement | undefined | null) {
  if (!dialog) return;
  const stops = [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(
    (el) => !(el as HTMLButtonElement).disabled && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden',
  );
  const first = stops[0];
  const last = stops.at(-1);
  if (!first || !last) return;
  const at = document.activeElement;
  const inside = !!at && dialog.contains(at);
  if (e.shiftKey ? !inside || at === first : !inside || at === last) {
    e.preventDefault();
    (e.shiftKey ? last : first).focus();
  }
}
