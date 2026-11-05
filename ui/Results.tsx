import { createResource, createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type Message, type Summary } from './api.ts';
import { Collapse } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { resultsLine } from './results.ts';
import { t } from './i18n.ts';

export function ResultsNote(props: { conversationId: string; message: Message }) {
  const [revision, setRevision] = createSignal(0);
  let alive = true;
  let unlisten: (() => void) | undefined;
  void api.on((event) => { if (event.type === 'list') setRevision((n) => n + 1); }).then((stop) => { if (alive) unlisten = stop; else stop(); }).catch(() => {});
  onCleanup(() => { alive = false; unlisten?.(); });
  const [results] = createResource(
    // A reply still streaming is not a turn yet: nothing to read until it ends.
    () => props.message.status === 'streaming' ? false : `${props.conversationId}:${props.message.id}:${props.message.status}:${props.message.took ?? ''}:${props.message.costUsd ?? ''}:${revision()}`,
    async () => { const id = props.conversationId; return { id, value: await api.results(id).catch(() => null) }; },
  );
  return <Show when={props.message.status !== 'streaming' && results()?.id === props.conversationId && results()?.value?.cost.turns ? results()?.value : null}>{(value) =>
    <div class="note" data-results-note>
      <div>{resultsLine(value())}</div>
      <Show when={value().dry.turns >= 3}><div>{t('results.dry', { n: value().dry.turns })}</div></Show>
    </div>
  }</Show>;
}

export function ProjectResults(props: { projectId: string; summaries: Summary[] }) {
  const [each, setEach] = createSignal(false);
  const [view] = createResource(
    // Read again only when a conversation of this project gained a turn or a cost, never for another project's news.
    () => `${props.projectId}|${props.summaries.filter((s) => s.projectId === props.projectId).map((s) => `${s.id}:${s.results?.cost.turns ?? 0}:${s.results?.cost.usd ?? 0}`).join(',')}`,
    async (key) => { const id = key.slice(0, key.indexOf('|')); return { id, value: await api.projectResults(id).catch(() => null) }; },
  );
  return <Show when={view()?.id === props.projectId && view()?.value?.conversations.some((row) => row.results.cost.turns) ? view()?.value : null}>{(value) =>
    <section class="resume">
      <h2>{t('results.title')}</h2>
      <p class="note">{t('results.total')}: {resultsLine(value().total)}</p>
      {/* Each conversation's line waits behind one quiet button: the total is what a person reads on the start screen. */}
      <button type="button" class="results-each" aria-expanded={each()} onClick={() => setEach(!each())}>
        <Icon name={each() ? 'down' : 'right'} small />{t('results.each')}
      </button>
      <Collapse open={each()}>
        <For each={value().conversations.filter((row) => row.results.cost.turns).sort((a, b) => b.results.cost.usd - a.results.cost.usd)}>{(row) =>
          <p class="note"><span>{row.title}</span>: {resultsLine(row.results)}</p>
        }</For>
      </Collapse>
    </section>
  }</Show>;
}
