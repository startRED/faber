import { For, Show } from 'solid-js';
import type { FaberAct } from './api.ts';
import { ToolGlyph } from './icons/kinds.tsx';
import { fromEngine, t } from './i18n.ts';
import { prettyModel, vendorName } from './models.ts';

/** The engine sends plain ids; the sentence names the AI and the model as the rest of the screen does. */
const named = (params: Record<string, string> = {}) => ({
  ...params,
  ...(params.vendor === 'claude' || params.vendor === 'codex' ? { vendor: vendorName(params.vendor) } : {}),
  ...(params.model ? { model: prettyModel(params.model) } : {}),
});

export function FaberActs(props: { acts?: FaberAct[]; onChange?: (target: NonNullable<FaberAct['change']>) => void }) {
  return <For each={props.acts}>{(act) => {
    const label = () => act.actor.who === 'faber' ? t('act.faber') : t(`act.${act.actor.role}`, { name: act.actor.model ? prettyModel(act.actor.model) : vendorName(act.actor.vendor) });
    const said = () => fromEngine(act.what, named(act.params));
    // A sentence that already names its actor ("Faber asked it to keep going") needs no label before it.
    const namesActor = () => said().includes(label());
    return <div class="faber-act">
      <div class="faber-act-line"><ToolGlyph kind="decision" small /><span>{namesActor() ? '' : `${label()} · `}{said()}<Show when={act.why}> · {t('act.why', { why: fromEngine(act.why, named(act.params)) })}</Show></span>
        <Show when={act.change && props.onChange}><button type="button" onClick={() => props.onChange?.(act.change!)}>{t('act.change')}</button></Show>
      </div>
      <Show when={act.sent}><details class="disclosure"><summary>{t('act.sent')}</summary><div class="faber-act-sent">{act.sent}</div></details></Show>
    </div>;
  }}</For>;
}
