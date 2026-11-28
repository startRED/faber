import { createEffect, createMemo, createResource, createSignal, createUniqueId, For, onCleanup, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api, type PlanPart } from './api.ts';
import { idleLife } from './idle.ts';
import { windowHidden } from './appearance.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { elapsed } from './running.ts';
import './plan-map.css';

export interface PlanComparison {
  conversationId: string;
  part: PlanPart;
  before: string;
  after: string;
}

type Area = { name: string; parts: PlanPart[] };
export const capture = (id: string) => id.replace(/-s\d+$/, '');
const inProgress = (part: PlanPart) => part.state === 'now' || part.state === 'redo';
const newestFirst = (a: PlanPart, b: PlanPart) => b.since - a.since;
const areaDone = (area: Area) => area.parts.some((part) => !part.removed)
  && area.parts.every((part) => part.removed || part.state === 'done');

function liveTime(since: number, clock: number) {
  const { key, n } = elapsed(since, clock);
  return t(key, { n });
}

function PlanMapLive(props: { current?: PlanPart; lastDone?: PlanPart }) {
  const [clock, setClock] = createSignal(Date.now());
  createEffect(() => {
    if (windowHidden()) return;
    setClock(Date.now());
    const timer = setInterval(() => setClock(Date.now()), 15_000);
    onCleanup(() => clearInterval(timer));
  });
  return <p class="plan-map-live">
    <Show when={props.current}>{(part) =>
      <strong>{t('planMap.current', { part: part().name, time: liveTime(part().since, clock()) })}</strong>
    }</Show>
    <Show when={props.lastDone}>{(part) =>
      <span>{t('planMap.lastWorking', { time: liveTime(part().since, clock()) })}</span>
    }</Show>
  </p>;
}

function PartBlock(props: {
  part: PlanPart;
  conversationId: string;
  preview?: boolean;
  proof: boolean;
  toggleProof: () => void;
  proofMount: HTMLElement;
  onCompare?: (comparison: PlanComparison, opener: HTMLButtonElement) => void;
}) {
  const proofId = createUniqueId();
  let proofButton: HTMLButtonElement | undefined;
  const pictures = () => props.part.pictures ?? [];
  const pictureCall = () => props.part.state === 'redo'
    ? props.part.before ?? pictures()[0]
    : pictures().at(-1);
  const pictureSource = createMemo(() => {
    const call = pictureCall();
    return call && (props.preview || props.proof) ? { id: props.conversationId, call } : undefined;
  }, undefined, { equals: (a, b) => a?.id === b?.id && a?.call === b?.call });
  const [picture] = createResource(pictureSource, ({ id, call }) => api.sitePicture(id, call).catch(() => null));
  const comparison = createMemo(() => {
    const calls = pictures();
    const first = props.part.before ?? calls[0];
    if (!first) return;
    const distinct = new Map<string, string>();
    for (const id of calls) {
      const key = capture(id);
      if (!distinct.has(key)) distinct.set(key, id);
    }
    const after = [...distinct.values()].at(-1);
    if (after && capture(after) !== capture(first)) return { conversationId: props.conversationId, part: props.part, before: first, after };
  });
  return <div class="plan-map-part" data-state={props.part.removed ? 'removed' : props.part.state}>
    <div class="plan-map-card">
      <Show when={props.preview && picture()}>{(src) =>
        <img class="plan-map-thumb" src={src()} alt="" />
      }</Show>
      <strong>{props.part.name}</strong>
      <span class="plan-map-state">{t(props.part.removed ? 'planMap.removed' : `planMap.${props.part.state}`)}</span>
      <Show when={!props.part.removed && props.part.state === 'done'}>
        <button type="button" ref={(el) => { proofButton = el; }} class="link plan-map-proof-toggle" aria-expanded={props.proof}
          aria-controls={props.proof ? proofId : undefined} onClick={props.toggleProof}>
          {t(props.proof ? 'planMap.hideProof' : 'planMap.seeProof')}
        </button>
      </Show>
      <Show when={!props.part.removed && props.onCompare && comparison()}>{(selection) =>
        <button type="button" class="link plan-map-compare" aria-haspopup="dialog"
          onClick={(e) => props.onCompare?.(selection(), e.currentTarget)}>
          {t('planMap.compare')}
        </button>
      }</Show>
    </div>
    <Show when={(props.part.removed || props.part.state === 'redo') && props.part.why}>
      <div class="plan-map-hanging">
        <Show when={!props.part.removed}><strong>{t('planMap.why')}</strong></Show>
        <span>{props.part.why}</span>
      </div>
    </Show>
    <Show when={!props.part.removed && props.part.example && props.part.state !== 'done'}>
      <div class="plan-map-hanging"><strong>{t('planMap.example')}</strong><span>{props.part.example}</span></div>
    </Show>
    <Show when={props.proof && !props.part.removed && props.part.state === 'done'}>
      <Portal mount={props.proofMount}>
        <section id={proofId} class="plan-map-proof" aria-label={props.part.name} tabindex="-1"
          ref={(el) => queueMicrotask(() => el.focus())}>
          <strong>{props.part.name}</strong>
          <Show when={props.part.evidence}><p>{props.part.evidence}</p></Show>
          <Show when={picture()}>{(src) =>
            <img src={src()} alt={t('planMap.screenshot', { part: props.part.name })} />
          }</Show>
          <button type="button" class="link" onClick={() => { props.toggleProof(); proofButton?.focus(); }}>{t('planMap.hideProof')}</button>
        </section>
      </Portal>
    </Show>
  </div>;
}

export function PlanMap(props: {
  conversationId: string;
  parts: PlanPart[];
  running: boolean;
  onCompare?: (comparison: PlanComparison, opener: HTMLButtonElement) => void;
}) {
  const panelId = createUniqueId();
  const listId = createUniqueId();
  const [picked, setPicked] = createSignal<string>();
  const [expanded, setExpanded] = createSignal(false);
  const [proof, setProof] = createSignal<string>();
  const [proofMount, setProofMount] = createSignal<HTMLElement>();
  const live = createMemo(() => props.parts.filter((part) => !part.removed));
  const count = (state: PlanPart['state']) => live().filter((part) => part.state === state).length;
  const lastDone = createMemo(() => live().filter((part) => part.state === 'done').sort(newestFirst)[0]);
  const current = createMemo(() => {
    const work = live().filter(inProgress).sort(newestFirst);
    return work.find((part) => part.state === 'now') ?? work[0];
  });
  const areas = createMemo(() => {
    const byName = new Map<string, Area>();
    for (const part of props.parts) {
      if (!byName.has(part.area)) byName.set(part.area, { name: part.area, parts: [] });
      byName.get(part.area)!.parts.push(part);
    }
    return [...byName.values()];
  });
  const selected = createMemo(() => areas().find((area) => area.name === picked())
    ?? areas().find((area) => area.name === current()?.area) ?? areas()[0]);
  const strip = createMemo(() => {
    const all = areas();
    if (all.length <= 6) return { visible: all, folded: [] };
    const open = all.filter((area) => !areaDone(area));
    const done = all.filter(areaDone);
    if (open.length <= 5) return { visible: open, folded: done };
    const active = open.find((area) => area.name === current()?.area);
    const visible = open.slice(0, 5);
    if (active && !visible.includes(active)) visible[4] = active;
    return { visible, folded: all.filter((area) => !visible.includes(area)) };
  });
  const [foldOpen, setFoldOpen] = createSignal(false);
  const select = (area: string) => {
    setPicked(area);
    setExpanded(false);
    setProof(undefined);
  };
  const blocks = createMemo(() => {
    const parts = selected()?.parts ?? [];
    const shown = parts.filter((part) => !part.removed);
    const working = shown.filter((part) => part.state === 'done').sort(newestFirst);
    const active = shown.filter(inProgress);
    const now = active.find((part) => part.state === 'now') ?? active[0];
    const other = active.find((part) => part.id !== now?.id && part.state === 'redo') ?? active.find((part) => part.id !== now?.id);
    const next = shown.find((part) => part.state === 'missing');
    const main = [other ?? working[0], now, next].filter((part) => part !== undefined);
    return {
      main: main.map((part) => part.id),
      left: parts.filter((part) => !main.includes(part)).map((part) => part.id),
      foldedDone: working.length - Number(!!working.length && !other),
      later: shown.filter((part) => part.state === 'missing').length - Number(!!next),
    };
  });
  const areaButton = (name: string) => {
    const areaParts = () => areas().find((area) => area.name === name)?.parts.filter((part) => !part.removed) ?? [];
    return <button type="button" class="plan-map-area" aria-controls={panelId}
      aria-pressed={selected()?.name === name} onClick={() => select(name)}>
      <strong><Icon name="folder" small />{name}</strong>
      <span class="plan-map-marks" aria-hidden="true"><For each={areaParts()}>{(part) => <i data-state={part.state} />}</For></span>
      <span>{t('planMap.areaWorking', { done: areaParts().filter((part) => part.state === 'done').length, total: areaParts().length })}</span>
    </button>;
  };
  const partBlock = (id: string, preview?: boolean) => {
    const part = () => selected()!.parts.find((item) => item.id === id)!;
    return <Show when={proofMount()}>{(mount) =>
      <PartBlock part={part()} preview={preview} conversationId={props.conversationId} proofMount={mount()}
        onCompare={props.onCompare} proof={proof() === id} toggleProof={() => setProof(proof() === id ? undefined : id)} />
    }</Show>;
  };
  const toggleList = () => {
    if (expanded() && blocks().left.includes(proof() ?? '')) setProof(undefined);
    setExpanded(!expanded());
  };
  const extra = () => blocks().left.length - blocks().foldedDone - blocks().later;
  return <section class="plan-map" aria-label={t('planMap.label')} classList={{ 'plan-map-pulse': props.running && idleLife() }}>
    <p class="plan-map-summary">
      <strong>{t('planMap.working', { done: count('done'), total: live().length })}</strong>
      <Show when={count('redo')}>{(n) => <span>{' · '}{t('planMap.redoCount', { n: n() })}</span>}</Show>
      <Show when={count('now')}>{(n) => <span>{' · '}{t('planMap.nowCount', { n: n() })}</span>}</Show>
      <Show when={count('missing')}>{(n) => <span>{' · '}{t('planMap.missingCount', { n: n() })}</span>}</Show>
    </p>
    <Show when={areas().length > 1}>
      <div class="plan-map-areas">
        <For each={strip().visible.map((area) => area.name)}>{areaButton}</For>
        <Show when={strip().folded.length}>{(n) =>
          <button type="button" class="plan-map-area plan-map-area-pile" aria-expanded={foldOpen()} onClick={() => setFoldOpen(!foldOpen())}>
            <strong><Icon name="layers" small />{t(strip().folded.every(areaDone) ? 'planMap.areasDone' : 'planMap.moreAreas', { n: n() })}</strong>
            <Show when={strip().folded.some((area) => area.name === selected()?.name)}><span>{selected()?.name}</span></Show>
          </button>
        }</Show>
      </div>
      <Show when={foldOpen() && strip().folded.length}>
        <div class="plan-map-areas plan-map-folded"><For each={strip().folded.map((area) => area.name)}>{areaButton}</For></div>
      </Show>
    </Show>
    <section class="plan-map-panel" id={panelId} aria-labelledby={`${panelId}-title`}>
      <h3 id={`${panelId}-title`}>{t('planMap.inside', { area: selected()?.name ?? '' })}</h3>
      <div class="plan-map-blocks">
        <Show when={blocks().foldedDone > 0}>
          <div class="plan-map-pile" data-state="done"><Icon name="layers" small />
            <strong>{t('planMap.workingCount', { n: blocks().foldedDone })}</strong>
            <button type="button" class="link" aria-expanded={expanded()} aria-controls={expanded() ? listId : undefined} onClick={toggleList}>
              {t(expanded() ? 'planMap.showLess' : 'planMap.seeAll')}
            </button>
          </div>
        </Show>
        <For each={blocks().main}>{(id) => partBlock(id, true)}</For>
        <Show when={blocks().later > 0}>
          <div class="plan-map-pile" data-state="missing">
            <strong>{t('planMap.later', { n: blocks().later })}</strong>
            <button type="button" class="link" aria-expanded={expanded()} aria-controls={expanded() ? listId : undefined} onClick={toggleList}>
              {t(expanded() ? 'planMap.showLess' : 'planMap.seeAll')}
            </button>
          </div>
        </Show>
      </div>
      <Show when={extra() > 0 && !blocks().foldedDone && !blocks().later}>
        <button type="button" class="link plan-map-more" aria-expanded={expanded()} aria-controls={expanded() ? listId : undefined} onClick={toggleList}>
          {t(expanded() ? 'planMap.showLess' : 'planMap.seeAll')}
        </button>
      </Show>
      <Show when={expanded() && blocks().left.length}>
        <div id={listId} class="plan-map-list"><For each={blocks().left}>{(id) => partBlock(id)}</For></div>
      </Show>
      <div class="plan-map-proof-mount" ref={setProofMount} />
    </section>
    <Show when={props.running && (current() || lastDone())}>
      <PlanMapLive current={current()} lastDone={lastDone()} />
    </Show>
  </section>;
}
