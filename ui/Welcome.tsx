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

/** The new conversation (s1), one column in both areas (docs/design/layout.md): the greeting, the composer it was
 * given, and the three conversations to pick up, those that wait for the user first. In Chat three short suggestions sit
 * under the composer and fill it without sending anything; the effects of the look sit behind (ui/Ambient.tsx). In a
 * project Curioso and a quiet line with the project and its folder come first, one line says where the agent works, and
 * the pick-up list holds only this project's conversations: the composer's hints already show what to ask. */
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
  const rows = () => pickUp(props.project ? props.recent.filter((c) => c.projectId === props.project!.id) : props.recent, props.running, 3);
  const live = (id: string) => props.running.find((i) => i.conversationId === id);
  // In a project every row is the project's own, so its row says only when.
  const where = (c: Summary) => (c.area === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects')));
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
        <Show when={props.project}>
          <p class="lead">{t('projects.chat.lead')}</p>
        </Show>
        {props.children}
        <Show when={!props.project}>
          <Suggestions onSuggest={props.onSuggest} />
        </Show>
        <Show when={rows().length > 0}>
          <section class="resume" aria-labelledby="resume-title" ref={animateList}>
            <h2 id="resume-title">{t('s1.pickUp')}</h2>
            <For each={rows()}>
              {(c) => <ResumeRow conversation={c} item={live(c.id)} where={props.project ? undefined : where(c)} onOpen={props.onOpen} />}
            </For>
          </section>
        </Show>
      </div>
    </div>
  );
}

/** Three short hand-written suggestions that fill the composer and send nothing. */
function Suggestions(props: { onSuggest: (text: string) => void }) {
  return (
    <section class="try chips" aria-label={t('s1.try')}>
      <For each={SUGGESTIONS}>
        {(key) => (
          <button type="button" class="chip" onClick={() => props.onSuggest(t(key))}>
            {t(key)}
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
