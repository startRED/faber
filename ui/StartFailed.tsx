import { onMount } from 'solid-js';
import './start-failed.css';
import { api } from './api.ts';
import { t } from './i18n.ts';
import { afterStartFailure } from './shell.ts';
import { WindowControls, mac } from './WindowControls.tsx';

/** Where a person reports what went wrong; the page asks for nothing Faber sends. */
const REPORT = 'https://github.com/startRED/faber/issues/new';

/**
 * Faber's background part failed three times while starting, so nothing else on the screen could answer (2026-11-27: on a
 * fresh Windows it could not load and Faber closed without a word). This screen needs no host: starting again, the report
 * page and quitting all go through the shell itself.
 */
export function StartFailed(props: { code: string }) {
  let again: HTMLButtonElement | undefined;
  onMount(() => again?.focus());
  return (
    <div class="start-failed">
      <header class="top" classList={{ mac }} data-tauri-drag-region>
        <WindowControls />
      </header>
      <main class="start-failed-body" aria-labelledby="start-failed-title">
        <h1 id="start-failed-title">{t('startFailed.title')}</h1>
        <p role="alert">{t('startFailed.body')}</p>
        <p class="start-failed-code">{t('startFailed.code', { code: props.code })}</p>
        <div class="start-failed-actions">
          <button type="button" class="btn primary" ref={(el) => (again = el)} onClick={() => afterStartFailure('restart')}>
            {t('startFailed.restart')}
          </button>
          <button type="button" class="btn" onClick={() => void api.openLink(REPORT).catch(() => {})}>{t('startFailed.report')}</button>
          <button type="button" class="btn" onClick={() => afterStartFailure('quit')}>{t('startFailed.quit')}</button>
        </div>
      </main>
    </div>
  );
}
