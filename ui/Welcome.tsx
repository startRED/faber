import { For, type JSX, Show } from 'solid-js';
import type { Project, RunningItem, Summary } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { greetingParts } from './greeting.ts';
import { lang, t } from './i18n.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { Peek } from './Mark.tsx';
import { tail } from './ProjectSettings.tsx';
import { pickUp } from './running.ts';

const SUGGESTIONS = ['suggest.1', 'suggest.2', 'suggest.3'] as const;

const DAY = 86_400_000;
const startOfDay = (ms: number) => new Date(new Date(ms).toDateString()).getTime();

/** "today, 14:08", "yesterday, 14:08", else the date: the words for the days come from the language itself. */
export function when(ms: number): string {
  const days = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  const time = new Intl.DateTimeFormat(lang(), { hour: '2-digit', minute: '2-digit' }).format(ms);
  if (days === 0 || days === 1) return `${new Intl.RelativeTimeFormat(lang(), { numeric: 'auto' }).format(-days, 'day')}, ${time}`;
  return new Intl.DateTimeFormat(lang(), { day: 'numeric', month: 'short' }).format(ms);
}

/** The new conversation (s1): the greeting, the composer it was given, three suggestions that fill the composer without
 * sending anything, and the four conversations to pick up: those that wait for the user first, then the latest. The effects of the look sit behind it (ui/Ambient.tsx).
 * In a project it is one column (docs/design.md, "The Projects home"): Curioso over a quiet line with the project and its
 * folder, the project's greeting, the composer and at most three of this project's conversations. No suggestions: the
 * composer's hints already show what to ask, and the room right of the column is the black hole's. */
export function Welcome(props: {
  /** How the person asked to be called, if they did. */
  name?: string;
  recent: Summary[];
  running: RunningItem[];
  projects: Project[];
  /** The project this new conversation works in, if it is one: named above the greeting, with its folder on hover. */
  project?: Project;
  onSuggest: (text: string) => void;
  onOpen: (conversation: Summary) => void;
  children: JSX.Element;
}) {
  const title = () => {
    if (props.project) return greetingParts(t('projects.home.title'));
    return props.name ? greetingParts(t('empty.chatTitleNamed', { name: props.name }), props.name) : greetingParts(t('empty.chatTitle'));
  };
  const rows = () => (props.project ? pickUp(props.recent.filter((c) => c.projectId === props.project!.id), props.running, 3) : pickUp(props.recent, props.running));
  const live = (id: string) => props.running.find((i) => i.conversationId === id);
  // In a project every row is the project's own, so its row says only when.
  const where = (c: Summary) => (c.area === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects')));
  const resume = () => (
    <Show when={rows().length > 0}>
      <section class="resume" aria-labelledby="resume-title" ref={animateList}>
        <h2 id="resume-title">{t('s1.pickUp')}</h2>
        <For each={rows()}>
          {(c) => <ResumeRow conversation={c} item={live(c.id)} where={props.project ? undefined : where(c)} onOpen={props.onOpen} />}
        </For>
      </section>
    </Show>
  );
  return (
    <div class="welcome-grid" classList={{ project: !!props.project }} ref={animateList}>
      <div class="w-main">
        <Show when={props.project}>
          {(p) => (
            <>
              <Peek />
              <header class="p-header">
                <h2>{p().name}</h2>
                <div class="p-meta">
                  <span title={p().path}>{tail(p().path)}</span>
                </div>
              </header>
            </>
          )}
        </Show>
        <h1 class="display hello">
          {title()[0]}
          <Show when={title()[1]}>
            {' '}
            <em>{title()[1]}</em>
          </Show>
        </h1>
        <p class="lead">{props.project ? t('projects.chat.lead') : t('empty.chatBody')}</p>
        {props.children}
        <Show when={props.project} fallback={<Suggestions onSuggest={props.onSuggest} />}>
          {resume()}
        </Show>
      </div>
      <Show when={!props.project}>{resume()}</Show>
    </div>
  );
}

/** Three hand-written suggestions that fill the composer and send nothing. */
function Suggestions(props: { onSuggest: (text: string) => void }) {
  return (
    <section class="try" aria-labelledby="try-title">
      <h2 id="try-title">{t('s1.try')}</h2>
      <For each={SUGGESTIONS}>
        {(key) => (
          <button type="button" onClick={() => props.onSuggest(t(key))}>
            <span>{t(key)}</span>
            <Icon name="right" small />
          </button>
        )}
      </For>
    </section>
  );
}

/** One conversation to pick up: its title, and under it where it is (unless the screen already says) and when, or what it
 * is doing now. Shared by the homes. */
export function ResumeRow(props: { conversation: Summary; item?: RunningItem; where?: string; onOpen: (conversation: Summary) => void }) {
  return (
    <button type="button" class="rrow" classList={{ ask: props.item?.state === 'needsYou' }} onClick={() => props.onOpen(props.conversation)}>
      <span class="t">{props.conversation.title}</span>
      <span class="s">
        <Show
          when={props.item}
          fallback={
            <>
              {props.where ? `${props.where} · ` : ''}
              {when(props.conversation.updatedAt)}
            </>
          }
        >
          {(i) => (
            <Show
              when={i().state === 'needsYou'}
              fallback={
                <span class="vm" classList={{ claude: i().vendor === 'claude', codex: i().vendor === 'codex' }}>
                  <VendorMark vendor={i().vendor} />
                  <Show when={i().vendor}>{(v) => <b>{vendorName(v())}</b>}</Show>
                  <span class="verb">{i().vendor ? t('run.working') : t('run.routing')}</span>
                </span>
              }
            >
              <span class="amber-tag">
                <Icon name="ask" small />
                {t('run.needsYou')}
              </span>
            </Show>
          )}
        </Show>
      </span>
    </button>
  );
}
