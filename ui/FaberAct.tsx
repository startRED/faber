import { For, Show } from 'solid-js';
import type { FaberAct } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { prettyModel, vendorName } from './models.ts';

export function FaberActs(props: { acts?: FaberAct[]; onChange?: (target: NonNullable<FaberAct['change']>) => void }) {
  return <For each={props.acts}>{(act) => {
    const label = () => act.actor.who === 'faber' ? t('act.faber') : t(`act.${act.actor.role}`, { name: act.actor.model ? prettyModel(act.actor.model) : vendorName(act.actor.vendor) });
    return <div class="faber-act">
      <div class="faber-act-line"><Icon name="loop" small /><span>{label()} · {fromEngine(act.what, act.params)}<Show when={act.why}> · {t('act.why', { why: fromEngine(act.why, act.params) })}</Show></span>
        <Show when={act.change && props.onChange}><button type="button" onClick={() => props.onChange?.(act.change!)}>{t('act.change')}</button></Show>
      </div>
      <Show when={act.sent}><details class="disclosure"><summary>{t('act.sent')}</summary><div class="faber-act-sent">{act.sent}</div></details></Show>
    </div>;
  }}</For>;
}
