import type { StorageMoveProgress } from './api.ts';
import { bytes, t } from './i18n.ts';

/**
 * A tool move runs while Faber starts, before its host answers anything (host/src/storage_choice/storage_move.rs), so the
 * screen has only its first frame. This line sits over that frame and says what is moving until the move is done.
 */
export function MovingTools(props: { progress: StorageMoveProgress }) {
  const line = () => t('moving.progress', {
    number: props.progress.number,
    total: props.progress.total,
    copied: bytes(props.progress.copied),
    size: bytes(props.progress.bytes),
  });
  return (
    <div class="moving-tools" role="status" aria-live="polite">
      <p class="moving-tools-title">{t('moving.title')}</p>
      <p class="moving-tools-line">{line()}</p>
      <p class="moving-tools-note">{t('moving.note')}</p>
    </div>
  );
}
