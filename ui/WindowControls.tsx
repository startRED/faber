import { currentWindow } from './shell.ts';
import { createSignal, onCleanup, onMount, Show } from 'solid-js';
import { t } from './i18n.ts';

// macOS keeps its own traffic lights over the top bar; Windows and Linux draw these instead of a system title bar.
export const mac = navigator.userAgent.includes('Mac');

// Thin line icons in the style of Windows 11's caption buttons.
const icon = (d: string) => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <path d={d} fill="none" stroke="currentColor" stroke-width="1" />
  </svg>
);

export function WindowControls() {
  const win = currentWindow();
  const [maximized, setMaximized] = createSignal(false);
  onMount(() => {
    const sync = async () => setMaximized(await win.isMaximized());
    void sync();
    const off = win.onResized(() => void sync());
    onCleanup(() => void off.then((stop) => stop()));
  });
  return (
    <Show when={!mac}>
      <div class="caption">
        <button aria-label={t('window.minimize')} title={t('window.minimize')} onClick={() => void win.minimize()}>
          {icon('M0 5.5h10')}
        </button>
        <button
          aria-label={t(maximized() ? 'window.restore' : 'window.maximize')}
          title={t(maximized() ? 'window.restore' : 'window.maximize')}
          onClick={() => void win.toggleMaximize()}
        >
          {maximized() ? icon('M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z') : icon('M.5.5h9v9h-9z')}
        </button>
        <button class="close" aria-label={t('window.close')} title={t('window.close')} onClick={() => void win.close()}>
          {icon('M0 0l10 10M10 0L0 10')}
        </button>
      </div>
    </Show>
  );
}
