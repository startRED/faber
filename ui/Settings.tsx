import { createEffect, createSignal, For, type JSX, Match, on, onCleanup, Show, Switch } from 'solid-js';
import { type Appearance, type Look, PERMISSION_MODES, type PermissionMode, type Project, type ReasoningSummary, type Settings as Saved, type Vendor, type VendorUsage, type ReplyStyle, type WebSearch } from './api.ts';
import { AccountsSection } from './Accounts.tsx';
import { AppearanceSection } from './Appearance.tsx';
import { Fold, Group, Opt, PickMenu, Segmented, Toggle } from './controls.tsx';
import { factory } from './draft.ts';
import { Icon } from './Glyphs.tsx';
import { LANGUAGES, t } from './i18n.ts';
import { openShortcuts } from './Shortcuts.tsx';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { ProjectsSection } from './ProjectSettings.tsx';
import { Recents } from './Recents.tsx';
import type { Quotas } from './sidebar.ts';

// Settings: five sections, one grammar. A group is a card, an option is one row (its name and one line at the left, its
// control at the right) and the long text of an option unfolds from the "?" next to its name (ui/controls.tsx).

const SECTIONS = [
  ['general', 'set.general', 'user'],
  ['projects', 'set.projects', 'folder'],
  ['appearance', 'set.appearance', 'palette'],
  ['accounts', 'set.subscriptions', 'plug'],
  ['privacy', 'set.privacy', 'shield'],
] as const;

export type Section = (typeof SECTIONS)[number][0];

/** What Settings keeps while the account cards are open: the section and the draft that is not applied yet. */
export interface Keep {
  section: Section;
  draft: Appearance;
}

const MORE = {
  ask: 'set.permissionsAskBody',
  acceptEdits: 'set.permissions.acceptEdits.more',
  plan: 'set.permissions.plan.more',
  auto: 'set.permissions.auto.more',
  dontAsk: 'set.permissionsDontAskBody',
} as const;
const WEB = [['off', 'set.webSearch.off'], ['cached', 'set.webSearch.cached'], ['live', 'set.webSearch.live']] as const;
const SUMMARY = [['default', 'set.default'], ['auto', 'set.summary.auto'], ['concise', 'set.summary.concise'], ['detailed', 'set.summary.detailed'], ['none', 'set.summary.none']] as const;
const REPLY_STYLE = [['default', 'set.replyStyle.default'], ['short', 'set.replyStyle.short'], ['explained', 'set.replyStyle.explained'], ['own', 'set.replyStyle.own']] as const;

interface Props {
  /** The look on the page. */
  look: Look;
  saved: Saved;
  /** The vendors that are connected. */
  vendors: Vendor[];
  quotas: Quotas;
  usage?: VendorUsage[];
  /** The projects Faber knows, each with its own settings. */
  projects?: Project[];
  /** The time now in milliseconds, which the screen moves on every minute: an old reading stops counting. */
  now: number;
  /** The version of Faber, shown once at the bottom of the list of sections. */
  version?: string;
  /** The section Settings opens at. */
  start?: Section;
  /** The draft Settings had when it was left for the account cards. */
  kept?: Appearance;
  /** Resolves to false when the save was refused or failed. */
  onSave: (patch: Saved) => Promise<boolean>;
  chatFolder?: string;
  chatFolderLocked?: boolean;
  onChatFolder?: (scratch: boolean, path?: string) => Promise<void>;
  /** Shows an error the section could not handle itself, such as a recents list that could not be saved. */
  onError?: (error: unknown) => void;
  onClose: () => void;
  /** Opens the account cards; Settings comes back as it was, from `keep`. */
  onOpenSetup: (keep: Keep) => void;
  /** Closes Settings and plays the tutorial again on a new conversation. */
  onTour?: () => void;
}

/** A select for a setting that saves at once: it goes back to the saved value when the save is refused. */
function Select<T extends string>(props: { label: string; value: T; options: [T, string][]; onPick: (v: T) => Promise<boolean> | void }) {
  return (
    <select
      class="pill"
      aria-label={props.label}
      value={props.value}
      onChange={async (e) => {
        const list = e.currentTarget;
        if ((await props.onPick(list.value as T)) === false) list.value = props.value;
      }}
    >
      <For each={props.options}>{([v, text]) => <option value={v}>{text}</option>}</For>
    </select>
  );
}

export function Settings(props: Props) {
  const [section, setSection] = createSignal<Section>(props.start ?? 'general');
  // The look being worked on in Appearance. It is not on the page until "Use this look" has saved it.
  const [draft, setDraft] = createSignal<Appearance>(props.kept ?? props.saved.appearance ?? factory());
  const mode = () => props.saved.permissions ?? 'dontAsk';
  const title = () => t(SECTIONS.find(([id]) => id === section())![1]);
  let main: HTMLDivElement | undefined;
  // A section opens at its top, whatever the one before was scrolled to.
  createEffect(
    on(
      section,
      () => {
        if (main) main.scrollTop = 0;
      },
      { defer: true },
    ),
  );
  const isWindows = () => navigator.userAgent.includes('Windows');
  const save = (patch: Saved) => props.onSave(patch);
  let replyStyleDraft: string | undefined;
  onCleanup(() => {
    // Escape removes Settings without blurring the field, so its change event may never run.
    const words = replyStyleDraft?.trim();
    if (words !== undefined && words !== (props.saved.replyStyleText ?? '')) void save({ replyStyleText: words });
  });
  return (
    <div class="settings" data-screen="settings" data-surface="settings">
      <nav class="s-nav nav" aria-label={t('set.label')}>
        <button class="item" onClick={() => props.onClose()}>
          <span>{t('set.back')}</span>
          <kbd>{t('key.esc')}</kbd>
        </button>
        <For each={SECTIONS}>
          {([id, key, icon]) => (
            <button class="item" aria-current={id === section()} onClick={() => setSection(id)}>
              <Icon name={icon} />
              <span>{t(key)}</span>
            </button>
          )}
        </For>
        <Show when={props.version}>
          <p class="s-version">{t('notes.version', { version: props.version! })}</p>
        </Show>
      </nav>
      {/* Switching sections is a short cross-fade (the owner, 2026-10-17): the old section leaves as an inert copy while
          the new one arrives; nothing runs under reduced or no motion. */}
      <div class="s-main" classList={{ wide: section() === 'appearance' }} ref={(el) => { main = el; animateList(el); }}>
        <Show when={section()} keyed>
          {/* The sidebar's active item already names the section, so its title is only for screen readers. */}
          <h2 class="sr-only">{title()}</h2>
        </Show>
        <Switch>
          <Match when={section() === 'general'}>
            <Group title={t('set.group.you')}>
              <Opt id="set-language" label={t('set.language')}>
                {/* An empty value means "follow the system"; names are shown in their own language. */}
                <Select
                  label={t('set.language')}
                  value={props.saved.language ?? ''}
                  options={[['', t('set.languageSystem')], ...LANGUAGES.map((l): [string, string] => [l.code, l.name])]}
                  onPick={(language) => save({ language })}
                />
              </Opt>
              <Opt id="set-name" label={t('set.name')} hint={t('set.nameHint')}>
                {/* Saved when the field is left or Enter is pressed; the engine keeps it trimmed, and empty clears it. */}
                <input
                  class="field"
                  type="text"
                  maxLength={40}
                  autocomplete="off"
                  aria-labelledby="set-name-name"
                  aria-describedby="set-name-hint"
                  value={props.saved.userName ?? ''}
                  onChange={async (e) => {
                    const field = e.currentTarget;
                    if (field.value !== (props.saved.userName ?? '') && !(await save({ userName: field.value }))) field.value = props.saved.userName ?? '';
                  }}
                />
              </Opt>
              {/* The first run's question, here to change (the owner, 2026-10-18): no answer counts as a beginner. */}
              <Opt id="set-xp" label={t('first.xp.title')} hint={t('set.xpHint')}>
                <Segmented
                  bare
                  id="set-xp-pick"
                  label={t('first.xp.title')}
                  value={props.saved.experience ?? 'beginner'}
                  options={[
                    ['beginner', t('first.xp.beginner')],
                    ['experienced', t('first.xp.experienced')],
                  ]}
                  onPick={(experience) => void save({ experience })}
                />
              </Opt>
              <Show when={props.onTour}>
                <Opt id="set-tour" label={t('set.tour')} hint={t('set.tourHint')}>
                  <button type="button" class="btn" onClick={() => props.onTour?.()}>
                    {t('set.tourAgain')}
                  </button>
                </Opt>
              </Show>
              <Opt id="set-keys" label={t('set.keys')} hint={t('set.keysHint')}>
                <button type="button" class="btn" onClick={openShortcuts}>
                  {t('set.keysShow')}
                </button>
              </Opt>
            </Group>
            <Group title={t('set.group.ai')}>
              <Opt id="set-permissions" label={t('set.permissions')} hint={t(`set.permissions.${mode()}.line`)} more={t(MORE[mode()])}>
                <PickMenu
                  id="set-permissions-pick"
                  label={t('set.permissions')}
                  value={mode()}
                  options={PERMISSION_MODES.map((m): [PermissionMode, string, string] => [m, t(`set.permissions.${m}`), t(`set.permissions.${m}.line`)])}
                  onPick={(permissions) => void save({ permissions })}
                />
              </Opt>
              <Opt id="set-web" label={t('set.webSearch')} hint={t('set.webSearch.line')} more={t('set.webSearch.more')}>
                <Segmented
                  bare
                  id="set-web-pick"
                  label={t('set.webSearch')}
                  value={props.saved.webSearch ?? 'live'}
                  options={WEB.filter(([v]) => v !== 'cached' || props.vendors.includes('codex')).map(([v, key]): [WebSearch, string] => [v, t(key)])}
                  onPick={(webSearch) => void save({ webSearch })}
                />
              </Opt>
              {/* In Full access the person's hooks always run (the CLI's own way), so the switch shows that and cannot change it. */}
              <Toggle id="set-hooks" label={t('set.hooksBody')} hint={t(mode() === 'dontAsk' ? 'set.hooksLocked' : 'set.hooksHint')} more={t('set.hooksMore')} checked={mode() === 'dontAsk' || props.saved.userHooks === true} disabled={mode() === 'dontAsk'} onChange={(on) => save({ userHooks: on })} />
              {/* Guardrail 3: when the agent changes Faber itself it asks; this skips that card and nothing else (never sending, publishing, paying or deleting). */}
              <Toggle id="set-trust" label={t('set.faberTrust')} hint={t('set.faberTrustHint')} more={t('set.faberTrustMore')} checked={props.saved.faberTrust === true} onChange={(on) => save({ faberTrust: on })} />
              <Show when={props.onChatFolder}>
                <ChatFolder {...props} />
              </Show>
              <Opt id="set-reply-style" label={t('set.replyStyle')} hint={t('set.replyStyle.line')} more={t('set.replyStyle.more')}>
                <Select
                  label={t('set.replyStyle')}
                  value={props.saved.replyStyle ?? 'default'}
                  options={REPLY_STYLE.map(([v, key]): [ReplyStyle, string] => [v, t(key)])}
                  onPick={(replyStyle) => save({ replyStyle })}
                />
              </Opt>
              <Show when={props.saved.replyStyle === 'own'}>
                <div class="st-extra">
                  <textarea
                    class="field"
                    rows={3}
                    maxLength={400}
                    aria-labelledby="set-reply-style-name"
                    aria-describedby="set-reply-style-hint"
                    placeholder={t('set.replyStyle.words')}
                    value={props.saved.replyStyleText ?? ''}
                    onInput={(e) => { replyStyleDraft = e.currentTarget.value; }}
                    onChange={async (e) => {
                      const field = e.currentTarget;
                      replyStyleDraft = undefined;
                      if (!(await save({ replyStyleText: field.value.trim() }))) field.value = props.saved.replyStyleText ?? '';
                    }}
                  />
                </div>
              </Show>
              <Show when={props.vendors.includes('codex')}>
                <Fold id="set-codex" title={vendorName('codex')} hint={t('set.codexExtras.line')}>
                  <p class="hint st-note">{t('set.codexExtras.more')}</p>
                  <Opt id="set-summary" label={t('set.summary')}>
                    <Select
                      label={t('set.summary')}
                      value={(props.saved.reasoningSummary ?? 'default') as ReasoningSummary}
                      options={SUMMARY.map(([v, key]): [ReasoningSummary, string] => [v, t(key)])}
                      onPick={(reasoningSummary) => save({ reasoningSummary })}
                    />
                  </Opt>
                </Fold>
              </Show>
            </Group>
            <Group title={t('set.group.system')}>
              <Toggle id="set-notify" label={t('set.notifyMe')} hint={t('set.notifyHint')} checked={props.saved.notifications === true} onChange={(on) => save({ notifications: on })} />
              <Toggle id="set-sound" label={t('set.playSound')} hint={t('set.soundLine')} more={t('set.soundHint')} checked={props.saved.notificationSound === true} onChange={(on) => save({ notificationSound: on })} />
              <Show when={isWindows()}>
                <Toggle id="set-awake" label={t('set.keepAwake')} hint={t('set.keepAwakeHint')} checked={props.saved.keepAwake === true} onChange={(on) => save({ keepAwake: on })} />
                <Toggle id="set-autostart" label={t('set.autostart')} hint={t('set.autostartHint')} checked={props.saved.autostart === true} onChange={(on) => save({ autostart: on })} />
              </Show>
            </Group>
            <Group title={t('set.updates')}>
              <Toggle id="set-previews" label={t('set.previews')} hint={t('set.previewsHint')} more={t('set.previewsMore')} checked={props.saved.previews === true} onChange={(on) => save({ previews: on })} />
            </Group>
          </Match>
          <Match when={section() === 'appearance'}>
            <AppearanceSection look={props.look} saved={props.saved.appearance} draft={draft} setDraft={setDraft} onUse={(appearance) => save({ appearance })} />
          </Match>
          <Match when={section() === 'accounts'}>
            <AccountsSection vendors={props.vendors} quotas={props.quotas} usage={props.usage} now={props.now} onManage={() => props.onOpenSetup({ section: section(), draft: draft() })} />
          </Match>
          <Match when={section() === 'projects'}>
            <div class="psets">
              <ProjectsSection projects={props.projects ?? []} />
            </div>
          </Match>
          <Match when={section() === 'privacy'}>
            <div class="st-card">
              <ul class="facts">
                <For each={['telemetry', 'history', 'logins'] as const}>
                  {(id) => (
                    <li>
                      <Icon name="check" />
                      {t(`set.privacy.${id}`)}
                    </li>
                  )}
                </For>
              </ul>
            </div>
          </Match>
        </Switch>
      </div>
    </div>
  );
}

/** The folder a Chat works in. It is a row only where its control exists: Settings opened from Chat, as the choice acts on the
 * open conversation while it has no message. */
function ChatFolder(props: Pick<Props, 'chatFolder' | 'chatFolderLocked' | 'onChatFolder' | 'onError'>): JSX.Element {
  return (
    <>
      <Opt id="set-folder" label={t('set.chatFolder')} hint={props.chatFolderLocked ? t('error.chatFolderLocked') : t('set.chatFolderHint')} more={t('set.chatFolderMore')}>
        <span class="st-val" title={props.chatFolder}>
          {props.chatFolder ?? t('set.privateFolder')}
        </span>
        <button class="btn" disabled={props.chatFolderLocked} onClick={() => void props.onChatFolder?.(false)}>
          {t('set.chooseFolder')}
        </button>
        <Show when={props.chatFolder}>
          <button class="btn" disabled={props.chatFolderLocked} onClick={() => void props.onChatFolder?.(true)}>
            {t('set.privateFolder')}
          </button>
        </Show>
      </Opt>
      <div class="st-extra">
        <Recents mode="chat" disabled={props.chatFolderLocked} onPick={(path) => void props.onChatFolder?.(false, path)} onError={(error) => props.onError?.(error)} />
      </div>
    </>
  );
}
