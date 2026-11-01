import { Show } from 'solid-js';
import type { AppState, Project } from './api.ts';
import { lang, t } from './i18n.ts';
import { openMenu } from './contextmenu.ts';

export function confirmProjectRemoval(e: MouseEvent, run: () => void) {
  openMenu(e, [{ id: 'remove', label: t('projects.removeFromFaber'), danger: true, confirm: t('menu.removeConfirm'), run }]);
}

export function CrashNote(props: { exit?: AppState['uncleanExit']; onDismiss?: () => void; onOpenLogs?: () => void }) {
  return <Show when={props.exit}>{(exit) => (
    <div class="crash-note" role="status" data-testid="crash-note">
      <p>{t('crash.note')}</p>
      <p>{t('crash.noteWhen', { when: new Date(exit().at).toLocaleString(lang(), { dateStyle: 'medium', timeStyle: 'short' }) })} {t(exit().panic ? 'crash.notePanic' : 'crash.noteNoPanic')}</p>
      <div class="crash-note-actions">
        <button type="button" class="btn" data-action="open-logs" onClick={() => props.onOpenLogs?.()}>{t('crash.openLogs')}</button>
        <button type="button" class="btn ghost" data-action="dismiss-crash" onClick={() => props.onDismiss?.()}>{t('crash.dismiss')}</button>
      </div>
    </div>
  )}</Show>;
}

export function MissingFolder(props: { project?: Project; onFind?: (id: string) => void; onRemove?: (id: string) => void }) {
  return <Show when={props.project && (props.project.state === 'missing' || props.project.state === 'disconnected') ? props.project : undefined}>{(project) => (
    <div class="project-missing-card" role="status" data-testid="project-missing">
      <p>{t(project().state === 'disconnected' ? 'projects.disconnectedNote' : 'projects.missingNote', { path: project().path })}</p>
      <div class="project-missing-actions">
        <button type="button" class="btn" data-action="find-folder" onClick={() => props.onFind?.(project().id)}>{t('projects.findFolder')}</button>
        <button type="button" class="btn ghost" data-action="remove-project" onClick={(e) => confirmProjectRemoval(e, () => props.onRemove?.(project().id))}>{t('projects.removeFromFaber')}</button>
      </div>
    </div>
  )}</Show>;
}
