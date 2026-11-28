import { Index, Show } from 'solid-js';
import type { FaberAct } from './api.ts';
import { ActorMark } from './icons/kinds.tsx';
import { fromEngine, lang, t } from './i18n.ts';
import { en, type Key } from './locales/en.ts';
import { prettyModel, vendorName } from './models.ts';

/** The engine sends plain ids; the sentence names the AI, the model and the effort as the rest of the screen does. */
const named = (params: Record<string, string> = {}) => {
  const effort = `effort.${params.effort}`;
  return {
    ...params,
    ...(params.vendor === 'claude' || params.vendor === 'codex' ? { vendor: vendorName(params.vendor) } : {}),
    ...(params.model ? { model: prettyModel(params.model) } : {}),
    // Mid-sentence: "chose Sonnet, medium effort", not "Medium" or the raw "medium" in another language.
    ...(effort in en ? { effort: t(effort as Key).toLocaleLowerCase(lang()) } : {}),
  };
};

/** `named`: inside a group whose line already says what happened, the row only names the helper. Reads `act` live: a row
 * keeps its place (and an open disclosure) while the reply's acts update. */
function Act(props: { act: FaberAct; named?: boolean; onChange?: (target: NonNullable<FaberAct['change']>) => void }) {
  const label = () => { const { actor } = props.act; return actor.who === 'faber' ? t('act.faber') : t(`act.${actor.role}`, { name: actor.model ? prettyModel(actor.model) : vendorName(actor.vendor) }); };
  const said = () => fromEngine(props.act.what, named(props.act.params));
  // A sentence that already names its actor ("Faber asked it to keep going") needs no label before it.
  const namesActor = () => said().includes(label());
  const mark = () => { const { actor } = props.act; return actor.who === 'faber' ? 'faber' : actor.vendor; };
  return <div class="faber-act">
    <div class="faber-act-line"><ActorMark actor={mark()} /><span>{props.named ? label() : <>{namesActor() ? '' : `${label()} · `}{said()}</>}<Show when={props.act.why}>{(why) => <> · {t('act.why', { why: fromEngine(why(), named(props.act.params)) })}</>}</Show></span>
      <Show when={props.act.change && props.onChange}><button type="button" onClick={() => props.onChange?.(props.act.change!)}>{t('act.change')}</button></Show>
    </div>
    <Show when={props.act.sent}><details class="disclosure"><summary>{t('act.sent')}</summary><div class="faber-act-sent">{props.act.sent}</div></details></Show>
  </div>;
}

/** Runs of the same kind: the helpers handed out together read as one line that opens to each of them. */
function runs(acts: FaberAct[]): FaberAct[][] {
  const out: FaberAct[][] = [];
  for (const act of acts) {
    const last = out.at(-1);
    if (act.kind === 'helper' && last?.[0]?.kind === 'helper') last.push(act);
    else out.push([act]);
  }
  return out;
}

// By position, not by array, and a helper run is a group from its first helper: one joining keeps the element, so an
// open group (and an open brief inside it) stays open and focused.
export function FaberActs(props: { acts?: FaberAct[]; onChange?: (target: NonNullable<FaberAct['change']>) => void }) {
  return <Index each={runs(props.acts ?? [])}>{(run) => <Show when={run()[0]?.kind === 'helper'} fallback={<Act act={run()[0]!} onChange={props.onChange} />}>
    <details class="faber-act faber-act-group">
      <summary class="faber-act-line"><ActorMark actor="faber" /><span>{t('act.helper.group', { n: run().length })}</span></summary>
      <Index each={run()}>{(act) => <Act act={act()} named onChange={props.onChange} />}</Index>
    </details>
  </Show>}</Index>;
}
