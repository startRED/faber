import { createEffect, createSignal, For, type JSX, Match, Show, Switch } from 'solid-js';
import { api, type Appearance, type CheckResult, type Look, type Settings as Saved, type Vendor, type VendorUsage } from './api.ts';
import { AppearanceSection } from './Appearance.tsx';
import { Segmented, Toggle } from './controls.tsx';
import { factory } from './draft.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { LANGUAGES, t } from './i18n.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { accountLine, type Quotas, usageRows } from './sidebar.ts';
import { PlanName } from './QuotaCard.tsx';
import { untilReset, windowName } from './Sidebar.tsx';

const SECTIONS = [
  ['general', 'set.general', 'user'],
  ['appearance', 'set.appearance', 'palette'],
  ['accounts', 'set.subscriptions', 'plug'],
  ['usage', 'set.usage', 'gauge'],
  ['privacy', 'set.privacy', 'shield'],
] as const;

export type Section = (typeof SECTIONS)[number][0];

/** What Settings keeps while the account cards are open: the section and the draft that is not applied yet. */
export interface Keep {
  section: Section;
  draft: Appearance;
}

const VENDORS: Vendor[] = ['claude', 'codex'];
const ACCOUNT_WORDS = { checking: 'set.checking', connected: 'set.connected', notConnected: 'set.notConnected', unsure: 'set.unsure' } as const;

interface Props {
  /** The look on the page. */
  look: Look;
  saved: Saved;
  /** The vendors that are connected. */
  vendors: Vendor[];
  quotas: Quotas;
  usage?: VendorUsage[];
  /** The time now in milliseconds, which the screen moves on every minute: an old reading stops counting. */
  now: number;
  /** The section Settings opens at. */
  start?: Section;
  /** The draft Settings had when it was left for the account cards. */
  kept?: Appearance;
  /** Resolves to false when the save was refused or failed. */
  onSave: (patch: Saved) => Promise<boolean>;
  chatFolder?: string;
  chatFolderLocked?: boolean;
  onChatFolder?: (scratch: boolean) => Promise<void>;
  onClose: () => void;
  /** Opens the account cards; Settings comes back as it was, from `keep`. */
  onOpenSetup: (keep: Keep) => void;
}

function Row(props: { label: string; children: JSX.Element }) {
  return (
    <div class="row">
      <span>{props.label}</span>
      <div>{props.children}</div>
    </div>
  );
}

export function Settings(props: Props) {
  const [section, setSection] = createSignal<Section>(props.start ?? 'general');
  // The look being worked on in Appearance. It is not on the page until "Use this look" has saved it.
  const [draft, setDraft] = createSignal<Appearance>(props.kept ?? props.saved.appearance ?? factory());
  const title = () => t(SECTIONS.find(([id]) => id === section())![1]);
  // Whether each CLI says someone is signed in. Asked every time the section opens, so it is fresh after the account cards.
  const [checks, setChecks] = createSignal<Partial<Record<Vendor, CheckResult | 'failed'>>>({});
  let asked = 0;
  createEffect(() => {
    if (section() !== 'accounts') return;
    const mine = ++asked;
    setChecks({});
    for (const v of VENDORS) {
      void api.setupCheck(v).then(
        (res) => mine === asked && setChecks((prev) => ({ ...prev, [v]: res })),
        () => mine === asked && setChecks((prev) => ({ ...prev, [v]: 'failed' })),
      );
    }
  });
  return (
    <div class="settings" data-screen="settings" data-surface="settings">
      <nav class="s-nav nav" aria-label={t('set.label')}>
        <button class="item" onClick={() => props.onClose()}>
          <span>{t('set.back')}</span>
          <kbd>Esc</kbd>
        </button>
        <For each={SECTIONS}>
          {([id, key, icon]) => (
            <button class="item" aria-current={id === section()} onClick={() => setSection(id)}>
              <Icon name={icon} />
              <span>{t(key)}</span>
            </button>
          )}
        </For>
      </nav>
      <div class="s-main" classList={{ wide: section() === 'appearance' }} ref={animateList}>
        <header class="s-head">
          <h2 class="display">{title()}</h2>
          <Show when={section() === 'appearance'}>
            <p class="hint">{t('set.themeHint')}</p>
          </Show>
        </header>
        <Switch>
          <Match when={section() === 'general'}>
            <Row label={t('set.language')}>
              {/* An empty value means "follow the system"; names are shown in their own language. */}
              <select
                class="pill"
                aria-label={t('set.language')}
                value={props.saved.language ?? ''}
                onChange={async (e) => {
                  const list = e.currentTarget;
                  if (!(await props.onSave({ language: list.value }))) list.value = props.saved.language ?? '';
                }}
              >
                <option value="">{t('set.languageSystem')}</option>
                <For each={LANGUAGES}>{(l) => <option value={l.code}>{l.name}</option>}</For>
              </select>
            </Row>
            <Row label={t('set.name')}>
              {/* Saved when the field is left or Enter is pressed; the engine keeps it trimmed, and empty clears it. */}
              <input
                class="name-in"
                type="text"
                maxLength={40}
                autocomplete="off"
                aria-label={t('set.name')}
                aria-describedby="set-name-hint"
                value={props.saved.userName ?? ''}
                onChange={async (e) => {
                  const field = e.currentTarget;
                  if (field.value !== (props.saved.userName ?? '') && !(await props.onSave({ userName: field.value }))) field.value = props.saved.userName ?? '';
                }}
              />
              <p class="hint" id="set-name-hint">
                {t('set.nameHint')}
              </p>
            </Row>
            <Row label={t('set.hooks')}>
              <div class="toggles">
                <Toggle label={t('set.hooksBody')} hint={t('set.hooksHint')} checked={props.saved.userHooks === true} onChange={(on) => props.onSave({ userHooks: on })} />
              </div>
            </Row>
            <Segmented
              id="set-permissions"
              label={t('set.permissions')}
              value={props.saved.permissions ?? 'dontAsk'}
              options={[
                ['dontAsk', t('set.permissions.dontAsk')],
                ['ask', t('set.permissions.ask')],
              ]}
              onPick={(permissions) => void props.onSave({ permissions })}
            />
            <Row label={t('set.chatFolder')}>
              <p class="hint">{t('set.chatFolderHint')}</p>
              <Show when={props.onChatFolder}>
                <p>{props.chatFolder ?? t('set.privateFolder')}</p>
                <button class="btn" disabled={props.chatFolderLocked} onClick={() => void props.onChatFolder?.(false)}>{t('set.chooseFolder')}</button>
                <Show when={props.chatFolder}>
                  <button class="btn" disabled={props.chatFolderLocked} onClick={() => void props.onChatFolder?.(true)}>{t('set.privateFolder')}</button>
                </Show>
                <Show when={props.chatFolderLocked}><p class="hint">{t('error.chatFolderLocked')}</p></Show>
              </Show>
            </Row>
            <Row label={t('set.notifications')}>
              <div class="toggles">
                <Toggle label={t('set.notifyMe')} hint={t('set.notifyHint')} checked={props.saved.notifications !== false} onChange={(on) => props.onSave({ notifications: on })} />
                <Toggle label={t('set.playSound')} hint={t('set.soundHint')} checked={props.saved.notificationSound !== false} onChange={(on) => props.onSave({ notificationSound: on })} />
              </div>
            </Row>
            <Show when={navigator.userAgent.includes('Windows')}>
              <Row label={t('set.system')}>
                <div class="toggles">
                  <Toggle label={t('set.keepAwake')} hint={t('set.keepAwakeHint')} checked={props.saved.keepAwake !== false} onChange={(on) => props.onSave({ keepAwake: on })} />
                  <Toggle label={t('set.autostart')} hint={t('set.autostartHint')} checked={props.saved.autostart === true} onChange={(on) => props.onSave({ autostart: on })} />
                </div>
              </Row>
            </Show>
          </Match>
          <Match when={section() === 'appearance'}>
            <AppearanceSection
              look={props.look}
              saved={props.saved.appearance}
              draft={draft}
              setDraft={setDraft}
              onUse={(appearance) => props.onSave({ appearance })}
            />
          </Match>
          <Match when={section() === 'accounts'}>
            <p class="hint">{t('set.accountsHint')}</p>
            <ul class="accounts">
              <For each={VENDORS}>
                {(v) => (
                  <li>
                    <span class="vm">
                      <VendorMark vendor={v} />
                      {vendorName(v)}
                    </span>
                    <span class="state" classList={{ on: accountLine(checks()[v]) === 'connected' }}>
                      {t(ACCOUNT_WORDS[accountLine(checks()[v])])}
                    </span>
                  </li>
                )}
              </For>
            </ul>
            <div>
              <button type="button" class="btn secondary" data-restore="manage" onClick={() => props.onOpenSetup({ section: section(), draft: draft() })}>
                {t('set.openSetup')}
              </button>
            </div>
          </Match>
          <Match when={section() === 'usage'}>
            <p class="hint">{t('set.usageHint')}</p>
            <Show when={props.vendors.length > 0} fallback={<p class="note">{t('side.noAi')}</p>}>
              <For each={props.vendors}>
                {(v) => (
                  <section class="usage-v">
                    <h3 class="vm">
                      <VendorMark vendor={v} />
                      {vendorName(v)}
                    </h3>
                    <Show when={props.usage?.find((u) => u.vendor === v)}>{(u) => <>
                      <PlanName u={u()} />
                      <p class="usage-state">{t(`usage.state.${u().state}`)}</p>
                      <p>{t('usage.ownSpend', { cost: u().ownSpendUsd.toFixed(4) })}</p>
                      <p class="hint">{t('usage.shared')}</p>
                      <Show when={u().unmetered}><p class="note">{t('usage.meterIncomplete', { n: u().unmetered })}</p></Show>
                      <Show when={u().relearning}><p class="note">{t('usage.relearning')}</p></Show>
                      <Show when={u().learning && !u().relearning}><p class="note">{t('usage.learning')}</p></Show>
                      <Show when={u().foreignUse}><p class="note">{t('usage.foreign')}</p></Show>
                      <Show when={u().outsideShare != null}><p class="note">{t('usage.outside', { share: `${Math.round((u().outsideShare ?? 0) * 100)}%` })}</p></Show>
                      <Show when={u().overageOn}><p class="note">{t('usage.overageOn')}</p></Show>
                      <Show when={u().billedPerUse}><p class="note">{t('usage.billedPerUse')}</p></Show>
                      <Show when={u().billingUnknown}><p class="note">{t('usage.authNeeded')}</p><button class="btn secondary" onClick={() => props.onOpenSetup({ section: section(), draft: draft() })}>{t('set.openSetup')}</button></Show>
                    </>}</Show>
                    <For each={usageRows(props.quotas, v, props.now)}>
                      {(row) => (
                        <div class="urow">
                          <span class="uname">{row.window ? t('quota.window', { window: windowName(row.window) }) : t('usage.noReading')}</span>
                          <span class="share num">{row.used === undefined ? t('quota.unknown') : `${Math.round(row.used * 100)}%`}</span>
                          <span class="meter" aria-hidden="true">
                            <Show when={props.usage?.find((u) => u.vendor === v)?.windows.find((w) => w.name === row.window)}>{(w) => <b class="expected" style={{ left: `${Math.round(w().expected * 100)}%` }} />}</Show>
                            <i style={{ width: `${Math.round(Math.min(1, Math.max(0, row.used ?? 0)) * 100)}%` }} />
                          </span>
                          <Show when={props.usage?.find((u) => u.vendor === v)?.windows.find((w) => w.name === row.window)}>{(w) => <span class="pace-projection">{t('usage.expect', { elapsed: `${Math.round(w().elapsed * 100)}%`, share: `${Math.round(w().used / Math.max(w().elapsed, 0.1) * 100)}%` })}</span>}</Show>
                          <span class="note">{row.used !== undefined && row.resetsAt ? t('usage.resets', { when: untilReset(row.resetsAt) }) : ''}</span>
                        </div>
                      )}
                    </For>
                  </section>
                )}
              </For>
            </Show>
          </Match>
          <Match when={section() === 'privacy'}>
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
          </Match>
        </Switch>
      </div>
    </div>
  );
}
