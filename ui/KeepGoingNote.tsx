import { Show } from 'solid-js';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

interface Props {
  /** The words of ui/keep-going.ts, or undefined while the quota is fine. */
  warning?: string;
  /** Keep going is on: the line stays as a notice, without the question. */
  on: boolean;
  /** The chip was pressed while the quota is poor: the line asks before keep going turns on. */
  asking: boolean;
  onYes: () => void;
  onNo: () => void;
}

/** Keep going and poor quota (R216): the amber line above the composer, a question before it turns on and a notice
 * while it is on. Nothing moves. */
export function KeepGoingNote(props: Props) {
  return (
    <Show when={props.warning && (props.on || props.asking)}>
      <div class="kg-note" role="status">
        <Icon name="loop" small />
        <span>{props.warning}</span>
        <Show when={!props.on}>
          <button type="button" class="kg-yes" onClick={() => props.onYes()}>{t('keepGoing.warn.anyway')}</button>
          <button type="button" class="kg-no" onClick={() => props.onNo()}>{t('keepGoing.warn.cancel')}</button>
        </Show>
      </div>
    </Show>
  );
}
