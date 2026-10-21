import { createSignal } from 'solid-js';
import { t } from './i18n.ts';
import { browserKey } from './keys.ts';

// Right-click in Faber. The browser's own menu (Back, Reload, Print...) never shows. Where a menu helps, it is Faber's: a
// row, a message, selected text or a text field opens `openMenu`; anywhere else nothing opens at all.

export interface MenuItem {
  id: string;
  label: string;
  run: () => void;
  /** Read again at the moment of choosing, so an item that became unavailable while the menu was open does nothing. */
  disabled?: boolean | (() => boolean);
  /** A destructive action: it goes last, in the danger colour. */
  danger?: boolean;
  /** The first activation only arms the item and shows this text instead; the second one runs it. */
  confirm?: string;
}

export interface OpenMenu {
  x: number;
  y: number;
  items: MenuItem[];
  /** Where focus goes back to when the menu closes by itself (Esc, Tab, a chosen item). */
  focus: HTMLElement | null;
  /** Opened from the keyboard: the first item takes focus. */
  keyboard: boolean;
}

const [menu, setMenu] = createSignal<OpenMenu>();
export { menu };

/** Closes the menu; `refocus` gives focus back to where it was. */
export function closeMenu(refocus = false): void {
  const open = menu();
  setMenu(undefined);
  if (refocus && open?.focus?.isConnected) open.focus.focus({ preventScroll: true });
}

/** Opens a menu at the pointer (or at the focused element when the keyboard asked). Call it from a `contextmenu` handler. */
export function openMenu(e: MouseEvent, items: MenuItem[], focus: HTMLElement | null = document.activeElement as HTMLElement | null): void {
  e.preventDefault();
  e.stopPropagation();
  if (items.length === 0) return;
  const keyboard = e.button !== 2;
  let { clientX: x, clientY: y } = e;
  if (keyboard && x === 0 && y === 0 && e.target instanceof Element) {
    const box = e.target.getBoundingClientRect();
    x = box.left + Math.min(24, box.width / 2);
    y = box.bottom;
  }
  setMenu({ x, y, items, focus, keyboard });
}

/** Whether an item cannot be chosen right now. */
export const unavailable = (item: MenuItem): boolean => (typeof item.disabled === 'function' ? item.disabled() : (item.disabled ?? false));

export const copyItem = (text: string): MenuItem => ({ id: 'copy', label: t('menu.copy'), disabled: !text, run: () => void navigator.clipboard.writeText(text) });

const TEXT_TYPES = new Set(['text', 'search', 'email', 'url', 'tel', 'password', 'number', '']);

/** The text box under the pointer, or nothing for a checkbox, a button and the like. */
function fieldOf(target: Element | null): HTMLInputElement | HTMLTextAreaElement | null {
  const field = target?.closest('input, textarea');
  if (field instanceof HTMLTextAreaElement) return field;
  return field instanceof HTMLInputElement && TEXT_TYPES.has(field.type) ? field : null;
}

function editItems(el: HTMLInputElement | HTMLTextAreaElement): MenuItem[] {
  const selected = () => el.value.slice(el.selectionStart ?? 0, el.selectionEnd ?? 0);
  const none = !selected();
  const locked = el.readOnly || el.disabled;
  const secret = el.type === 'password';
  return [
    {
      id: 'cut',
      label: t('menu.cut'),
      disabled: none || locked || secret,
      run: () => {
        void navigator.clipboard.writeText(selected());
        el.focus();
        document.execCommand('delete');
      },
    },
    { id: 'copy', label: t('menu.copy'), disabled: none || secret, run: () => void navigator.clipboard.writeText(selected()) },
    {
      id: 'paste',
      label: t('menu.paste'),
      disabled: locked,
      run: () =>
        void navigator.clipboard.readText().then((text) => {
          el.focus();
          // insertText keeps the field's undo history; the fallback writes the text itself and tells the page.
          if (document.execCommand('insertText', false, text)) return;
          el.setRangeText(text, el.selectionStart ?? el.value.length, el.selectionEnd ?? el.value.length, 'end');
          el.dispatchEvent(new Event('input', { bubbles: true }));
        }, () => {}),
    },
    { id: 'all', label: t('menu.selectAll'), disabled: !el.value, run: () => (el.focus(), el.select()) },
  ];
}

/** The selected text, when the click lands on it. */
function selectionAt(e: MouseEvent): string {
  const selection = getSelection();
  if (!selection || selection.isCollapsed) return '';
  const hit = [...selection.getRangeAt(0).getClientRects()].some((r) => e.clientX >= r.left && e.clientX <= r.right && e.clientY >= r.top && e.clientY <= r.bottom);
  return hit ? selection.toString() : '';
}

let installed = false;
let keyAt = 0;

/** Wires the page once: text fields and selected text get their menu, every other right-click is only stopped (rows and
 * messages open theirs from their own handler, which runs before this one), and the browser's reload, print and save
 * shortcuts do nothing. */
export function installContextMenu(): void {
  if (installed) return;
  installed = true;
  // Capture phase: the menu of a text field or of a selection wins over the menu of the row it sits in.
  window.addEventListener(
    'contextmenu',
    (e) => {
      const target = e.target instanceof Element ? e.target : null;
      if (target?.closest('.cmenu')) return e.preventDefault();
      // A key press gives the page this event too, after the keydown below has opened the menu.
      if (e.isTrusted && e.button !== 2 && performance.now() - keyAt < 500) return (e.preventDefault(), e.stopPropagation());
      const field = fieldOf(target);
      if (field) return openMenu(e, editItems(field), field);
      const text = selectionAt(e);
      if (text) openMenu(e, [copyItem(text)]);
    },
    true,
  );
  window.addEventListener('contextmenu', (e) => e.preventDefault());
  window.addEventListener(
    'keydown',
    (e) => {
      if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
        e.preventDefault();
        keyAt = performance.now();
        (document.activeElement ?? document.body).dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, composed: true }));
      } else if (e.key === 'Escape' && menu()) {
        e.preventDefault();
        e.stopPropagation();
        closeMenu(true);
      } else if (browserKey(e)) {
        e.preventDefault();
      }
    },
    true,
  );
  // Anything else the person does closes an open menu.
  window.addEventListener('pointerdown', (e) => e.target instanceof Element && !e.target.closest('.cmenu') && closeMenu(), true);
  window.addEventListener('blur', () => closeMenu());
  window.addEventListener('resize', () => closeMenu());
  window.addEventListener('scroll', (e) => { if (!(e.target instanceof Element && e.target.closest('.cmenu'))) closeMenu(); }, true);
}
