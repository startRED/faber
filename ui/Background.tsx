import { createMemo, Index, Show } from 'solid-js';
import { api, type Follow } from './api.ts';
import { liveReferences, workStatus } from './background.ts';
import { t } from './i18n.ts';

/** A reference carries only the state the existing follow has observed; passing checks never claim it landed. */
export function FollowChip(props: { follow: Follow; text: string }) {
  return <a class="work-chip live-reference" data-state={props.follow.state} data-need={workStatus({ follow: props.follow }).need} href={props.follow.url}
    title={`${props.follow.repo}#${props.follow.number}`}
    onClick={(e) => { e.preventDefault(); void api.openLink(props.follow.url); }}>
    <span>{props.text}</span><span class="work-state">{t(`background.link.${props.follow.state}`)}</span>
  </a>;
}

/** Text stays literal, including unknown references and markup. A message event updates the existing chips. */
export function LiveText(props: { text: string; follows: Follow[] }) {
  const parts = createMemo(() => liveReferences(props.text, props.follows));
  return <Index each={parts()}>{(part) => <Show when={part().follow} fallback={part().text}>{(f) => <FollowChip follow={f()} text={part().text} />}</Show>}</Index>;
}
