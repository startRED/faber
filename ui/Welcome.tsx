import { For, type JSX, Show } from 'solid-js';
import type { Project, RunningItem, Summary } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { greetingParts } from './greeting.ts';
import { lang, t } from './i18n.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
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
 * sending anything, and the four conversations to pick up: those that wait for the user first, then the latest. The effects of the look sit behind it (ui/Ambient.tsx). */
export function Welcome(props: {
  /** How the person asked to be called, if they did. */
  name?: string;
  recent: Summary[];
  running: RunningItem[];
  projects: Project[];
  onSuggest: (text: string) => void;
  onOpen: (conversation: Summary) => void;
  children: JSX.Element;
}) {
  const title = () => (props.name ? greetingParts(t('empty.chatTitleNamed', { name: props.name }), props.name) : greetingParts(t('empty.chatTitle')));
  const rows = () => pickUp(props.recent, props.running);
  const live = (id: string) => props.running.find((i) => i.conversationId === id);
  const where = (c: Summary) => (c.area === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects')));
  return (
    <div class="welcome-grid" ref={animateList}>
      <div class="w-main">
        <h1 class="display hello">
          {title()[0]}
          <Show when={title()[1]}>
            {' '}
            <em>{title()[1]}</em>
          </Show>
        </h1>
        <p class="lead">{t('empty.chatBody')}</p>
        {props.children}
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
      </div>
      <Show when={rows().length > 0}>
        <section class="resume" aria-labelledby="resume-title" ref={animateList}>
          <h2 id="resume-title">{t('s1.pickUp')}</h2>
          <For each={rows()}>
            {(c) => <ResumeRow conversation={c} item={live(c.id)} where={where(c)} onOpen={props.onOpen} />}
          </For>
        </section>
      </Show>
    </div>
  );
}

/** One conversation to pick up: its title, and under it where it is and when, or what it is doing now. Shared by the homes. */
export function ResumeRow(props: { conversation: Summary; item?: RunningItem; where: string; onOpen: (conversation: Summary) => void }) {
  return (
    <button type="button" class="rrow" classList={{ ask: props.item?.state === 'needsYou' }} onClick={() => props.onOpen(props.conversation)}>
      <span class="t">{props.conversation.title}</span>
      <span class="s">
        <Show
          when={props.item}
          fallback={
            <>
              {props.where} · {when(props.conversation.updatedAt)}
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
