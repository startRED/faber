import { Show } from 'solid-js';
import type { BuildView } from './api.ts';
import { fromEngine, t } from './i18n.ts';

export function LocalBuild(props: { view: BuildView }) {
  return (
    <div class="build-card" data-testid="local-build">
      <h3>{t('projects.build.title')}</h3>
      <div class="build-slots">
        <div><b>{t('projects.build.current')}:</b> {props.view.current ?? t('projects.value.missing')}</div>
        <div><b>{t('projects.build.next')}:</b> {props.view.next ?? t('projects.value.missing')}</div>
        <div><b>{t('projects.build.lastGood')}:</b> {props.view.lastGood ?? t('projects.value.missing')}</div>
        <Show when={props.view.waiting}>
          <div class="note"><b>{t('projects.build.waiting')}:</b> {fromEngine(props.view.waiting ?? undefined)}</div>
        </Show>
      </div>
    </div>
  );
}
