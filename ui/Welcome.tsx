import { For, type JSX, Show } from 'solid-js';
import type { Project, RunningItem, Summary } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { vendorName } from './models.ts';

const SUGGESTIONS = ['suggest.1', 'suggest.2', 'suggest.3'] as const;

/** The greeting with its last word set apart, so a look with an italic emphasis (Workshop) can draw it. */
function splitLast(text: string): [string, string] {
  const at = text.lastIndexOf(' ');
  return at < 0 ? [text, ''] : [text.slice(0, at), text.slice(at + 1)];
}

const DAY = 86_400_000;
const startOfDay = (ms: number) => new Date(new Date(ms).toDateString()).getTime();

/** "today, 14:08", "yesterday, 14:08", else the date: the words for the days come from the language itself. */
function when(ms: number): string {
  const days = Math.round((startOfDay(Date.now()) - startOfDay(ms)) / DAY);
  const time = new Intl.DateTimeFormat(lang(), { hour: '2-digit', minute: '2-digit' }).format(ms);
  if (days === 0 || days === 1) return `${new Intl.RelativeTimeFormat(lang(), { numeric: 'auto' }).format(-days, 'day')}, ${time}`;
  return new Intl.DateTimeFormat(lang(), { day: 'numeric', month: 'short' }).format(ms);
}

/** The new conversation (s1): the greeting, the composer it was given, three suggestions that fill the composer without
 * sending anything, and the four conversations to pick up: those that wait for the user first, then the latest. The effects of the look sit behind it (ui/Ambient.tsx). */
export function Welcome(props: {
  recent: Summary[];
  running: RunningItem[];
  projects: Project[];
  onSuggest: (text: string) => void;
  onOpen: (conversation: Summary) => void;
  children: JSX.Element;
}) {
  const title = () => splitLast(t('empty.chatTitle'));
  const rows = () => {
    const needs = (c: Summary) => props.running.some((i) => i.conversationId === c.id && i.state === 'needsYou');
    return [...props.recent.filter(needs), ...props.recent.filter((c) => !needs(c))].slice(0, 4);
  };
  const live = (id: string) => props.running.find((i) => i.conversationId === id);
  const where = (c: Summary) => (c.area === 'chat' ? t('area.chat') : (props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects')));
  return (
    <div class="welcome-grid">
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
        <section class="resume" aria-labelledby="resume-title">
          <h2 id="resume-title">{t('s1.pickUp')}</h2>
          <For each={rows()}>
            {(c) => {
              const item = () => live(c.id);
              return (
                <button type="button" class="rrow" classList={{ ask: item()?.state === 'needsYou' }} onClick={() => props.onOpen(c)}>
                  <span class="t">{c.title}</span>
                  <span class="s">
                    <Show
                      when={item()}
                      fallback={
                        <>
                          {where(c)} · {when(c.updatedAt)}
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
            }}
          </For>
        </section>
      </Show>
    </div>
  );
}
