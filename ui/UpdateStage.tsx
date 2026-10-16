import { onMount, Show } from 'solid-js';
import type { UpdateStatus } from './api.ts';
import { t } from './i18n.ts';
import { Mark } from './Mark.tsx';

/** The words for where the install is: the download adds how far it is, a whole percent or, with no size, the megabytes. */
export function updateText(status: UpdateStatus): string {
  switch (status.phase) {
    case 'checking':
      return t('update.checking');
    case 'downloading':
      if (status.percent !== undefined) return t('update.downloadingPercent', { percent: status.percent });
      if (status.mb !== undefined) return t('update.downloadingSize', { mb: status.mb });
      return t('update.downloading');
    case 'installing':
      return t('update.installing');
    default:
      return '';
  }
}

/** The sidebar's calm line while an update downloads or installs. Looking for one says nothing: it is over in a moment. */
export function UpdateLine(props: { status: UpdateStatus }) {
  return (
    <Show when={props.status.phase === 'downloading' || props.status.phase === 'installing'}>
      <div class="upd" role="status">
        {updateText(props.status)}
      </div>
    </Show>
  );
}

/**
 * The opening screen while Faber looks for an update, downloads it and installs it, so the window is never blank or
 * looks stuck: the mark, the words for where it is and, when the size is known, a thin bar. It is drawn at once, with no
 * entrance, because it is the first thing the person sees. Below the title bar, so the window's buttons stay in reach.
 */
export function UpdateStage(props: { status: UpdateStatus }) {
  onMount(() => {
    // Nothing behind can take typing: an update at open waits for the person who types, and no one is meant to here.
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
  });
  const percent = () => (props.status.phase === 'downloading' ? props.status.percent : undefined);
  return (
    <div class="ustage" data-surface="update">
      <div class="ustage-col">
        <Mark size={36} />
        <p class="ustage-text num" role="status">
          {updateText(props.status)}
        </p>
        <Show when={percent() !== undefined}>
          <div class="ubar" role="progressbar" aria-label={t('update.downloading')} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent()}>
            <span style={{ transform: `scaleX(${(percent() ?? 0) / 100})` }} />
          </div>
        </Show>
      </div>
    </div>
  );
}
