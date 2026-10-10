import { createSignal, For, Match, Switch } from 'solid-js';
import type { Settings as Saved } from './api.ts';
import { ACCENTS, PRESETS, presetOf, type Appearance } from './appearance.ts';
import { LANGUAGES, t } from './i18n.ts';

type Option<T> = [T, string];

function Choices<T extends string>(props: { label: string; value: T | undefined; options: Option<T>[]; onPick: (v: T) => void }) {
  return (
    <div class="row">
      <span>{props.label}</span>
      <div class="opts" role="group" aria-label={props.label}>
        <For each={props.options}>
          {([v, text]) => (
            <button class="opt" aria-pressed={v === props.value} onClick={() => props.onPick(v)}>
              {text}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

const SECTIONS = [
  ['general', 'set.general'],
  ['appearance', 'set.appearance'],
  ['subscriptions', 'set.subscriptions'],
  ['projects', 'set.projects'],
] as const;

interface Props {
  appearance: Appearance;
  saved: Saved;
  onAppearance: (a: Appearance) => void;
  onSave: (patch: Saved) => void;
  onClose: () => void;
  onOpenSetup?: () => void;
}

export function Settings(props: Props) {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number][0]>('general');
  const set = (patch: Partial<Appearance>) => props.onAppearance({ ...props.appearance, ...patch });
  const a = () => props.appearance;
  return (
    <div class="settings">
      <nav class="s-nav nav" aria-label={t('set.label')}>
        <button class="item" onClick={() => props.onClose()}>
          <span>{t('set.back')}</span>
          <kbd>Esc</kbd>
        </button>
        <For each={SECTIONS}>
          {([id, key]) => (
            <button class="item" aria-current={id === section()} onClick={() => setSection(id)}>
              <span>{t(key)}</span>
            </button>
          )}
        </For>
      </nav>
      <div class="s-main">
        <h2>{t(SECTIONS.find(([id]) => id === section())![1])}</h2>
        <Switch>
          <Match when={section() === 'general'}>
            <div class="row">
              <span>{t('set.language')}</span>
              {/* An empty value means "follow the system"; names are shown in their own language. */}
              <select class="pill" aria-label={t('set.language')} value={props.saved.language ?? ''} onChange={(e) => props.onSave({ language: e.currentTarget.value })}>
                <option value="">{t('set.languageSystem')}</option>
                <For each={LANGUAGES}>{(l) => <option value={l.code}>{l.name}</option>}</For>
              </select>
            </div>
          </Match>
          <Match when={section() === 'appearance'}>
            <Choices label={t('set.preset')} value={presetOf(a())} onPick={(p) => props.onAppearance(PRESETS[p])} options={[['mono', t('set.presetMono')], ['studio', 'Studio'], ['cockpit', 'Cockpit']]} />
            <Choices
              label={t('set.mode')}
              value={a().mode}
              onPick={(mode) => set({ mode })}
              options={[['dark', t('set.modeDark')], ['black', t('set.modeBlack')], ['light', t('set.modeLight')], ['system', t('set.modeSystem')]]}
            />
            <div class="row">
              <span>{t('set.accent')}</span>
              <div class="opts" role="group" aria-label={t('set.accent')}>
                <For each={Object.keys(ACCENTS) as Appearance['accent'][]}>
                  {(accent) => (
                    <button
                      class="sw"
                      style={{ '--c': ACCENTS[accent][0] }}
                      aria-label={t(`set.accent.${accent}`)}
                      aria-pressed={!a().custom && a().accent === accent}
                      onClick={() => set({ accent, custom: null })}
                    />
                  )}
                </For>
                <label class="custom">
                  {t('set.anyColor')} <input type="color" value={a().custom ?? ACCENTS[a().accent][0]} onInput={(e) => set({ custom: e.currentTarget.value })} />
                </label>
              </div>
            </div>
            <Choices label={t('set.uiFont')} value={a().ui} onPick={(ui) => set({ ui })} options={[['geistmono', 'Geist Mono'], ['jetbrains', 'JetBrains Mono'], ['sans', t('set.uiFontSans')]]} />
            <Choices label={t('set.readFont')} value={a().read} onPick={(read) => set({ read })} options={[['geist', 'Geist'], ['mona', 'Mona Sans'], ['same', t('set.readFontSame')]]} />
            <Choices label={t('set.density')} value={a().density} onPick={(density) => set({ density })} options={[['comfortable', t('set.densityComfortable')], ['compact', t('set.densityCompact')]]} />
            <Choices label={t('set.corners')} value={a().corners} onPick={(corners) => set({ corners })} options={[['round', t('set.cornersRound')], ['square', t('set.cornersSquare')]]} />
            <p class="hint">{t('set.themeHint')}</p>
          </Match>
          <Match when={section() === 'subscriptions'}>
            <div class="row">
              <span>{t('set.subscriptions')}</span>
              <div>
                <button
                  type="button"
                  class="btn secondary"
                  onClick={() => props.onOpenSetup?.()}
                >
                  {t('set.openSetup')}
                </button>
              </div>
            </div>
          </Match>
          <Match when={section() === 'projects'}>
            <div class="row">
              <span>{t('set.hooks')}</span>
              <label class="check">
                <input type="checkbox" checked={props.saved.userHooks === true} onChange={(e) => props.onSave({ userHooks: e.currentTarget.checked })} />
                <span>
                  {t('set.hooksBody')}
                  <small>{t('set.hooksHint')}</small>
                </span>
              </label>
            </div>
          </Match>
        </Switch>
      </div>
    </div>
  );
}
