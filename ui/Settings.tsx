import { createSignal, For, Match, Show, Switch } from 'solid-js';
import themes from '../src-tauri/src/themes.json';
import type { Appearance, Look, Motion, Settings as Saved } from './api.ts';
import { switchTheme } from './appearance.ts';
import { LANGUAGES, t } from './i18n.ts';
import type { Key } from './locales/en.ts';

// The lists of options come from themes.json, the same file the shell reads its tokens from.
const controls = themes['user-controls'];
const LOOKS = [...controls.themes, ...controls.presets];
const swatches = Object.entries(controls.accent.swatches);
const DENSITY: Record<number, Key> = { 1: 'set.densityComfortable', 0.8: 'set.densityCompact' };
const CORNERS: Record<number, Key> = { 0: 'set.cornersSquare', 1: 'set.cornersSoft', 1.6: 'set.cornersRound' };

type Option<T> = [T, string];

function Segmented<T extends string | number>(props: { id: string; label: string; value: T; options: Option<T>[]; onPick: (v: T) => void }) {
  return (
    <div class="ctl">
      <span class="lbl" id={props.id}>
        {props.label}
      </span>
      <div class="seg-ctl" role="group" aria-labelledby={props.id}>
        <For each={props.options}>
          {([v, text]) => (
            <button type="button" aria-pressed={v === props.value} onClick={() => props.onPick(v)}>
              {text}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

/** A switch moves before its save is answered; a refused save puts it back where the look says it is. */
function Toggle(props: { label: string; checked: boolean; onChange: (on: boolean) => Promise<boolean> }) {
  return (
    <label class="tg">
      <span>{props.label}</span>
      <input
        class="sw2"
        type="checkbox"
        role="switch"
        checked={props.checked}
        onChange={async (e) => {
          const box = e.currentTarget;
          if (!(await props.onChange(box.checked))) box.checked = props.checked;
        }}
      />
    </label>
  );
}

const SECTIONS = [
  ['general', 'set.general'],
  ['appearance', 'set.appearance'],
  ['subscriptions', 'set.subscriptions'],
  ['projects', 'set.projects'],
] as const;

const ALL_ON = { ambient: true, texture: true, glow: true };

interface Props {
  /** The look on the page, with every choice filled in from its base. */
  look: Look;
  saved: Saved;
  /** Builds the next appearance from the one saved at that moment (undefined until the user changes the look), so
   * choices made one after another never overwrite each other. Resolves to false when the save failed. */
  onAppearance: (next: (current: Appearance | undefined) => Appearance) => Promise<boolean>;
  /** Resolves to false when the save was refused or failed. */
  onSave: (patch: Saved) => Promise<boolean>;
  onClose: () => void;
  onOpenSetup?: () => void;
}

export function Settings(props: Props) {
  const [section, setSection] = createSignal<(typeof SECTIONS)[number][0]>('general');
  const set = (patch: Partial<Appearance>) => props.onAppearance((current) => ({ ...(current ?? { base: themes.default }), ...patch }));
  const setEffect = (key: keyof typeof ALL_ON) => (on: boolean) =>
    props.onAppearance((current) => ({ ...(current ?? { base: themes.default }), effects: { ...ALL_ON, ...current?.effects, [key]: on } }));
  const a = () => props.look.appearance;
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
        <h2 class="display">{t(SECTIONS.find(([id]) => id === section())![1])}</h2>
        <Switch>
          <Match when={section() === 'general'}>
            <div class="row">
              <span>{t('set.language')}</span>
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
            </div>
          </Match>
          <Match when={section() === 'appearance'}>
            <p class="hint">{t('set.themeHint')}</p>
            <div class="presets" role="group" aria-label={t('set.look')}>
              <For each={LOOKS}>
                {(id) => (
                  <button
                    type="button"
                    class="preset"
                    aria-pressed={a().base === id}
                    aria-labelledby={`look-${id}-name`}
                    aria-describedby={`look-${id}-desc`}
                    onClick={() => void props.onAppearance((current) => switchTheme(current, id))}
                  >
                    <span class="nm" id={`look-${id}-name`}>
                      {t(`look.${id}.name` as Key)}
                      <Show when={id === themes.default}>
                        <small>{t('look.default')}</small>
                      </Show>
                    </span>
                    <span class="ds" id={`look-${id}-desc`}>
                      {t(`look.${id}.desc` as Key)}
                    </span>
                  </button>
                )}
              </For>
            </div>
            <div class="ctl-grid">
              <div class="ctl">
                <span class="lbl" id="set-accent">
                  {t('set.accent')}
                  <small>{a().accent}</small>
                </span>
                <div class="dots" role="group" aria-labelledby="set-accent">
                  <For each={swatches}>
                    {([id, color]) => (
                      <button
                        type="button"
                        class="dot"
                        style={{ '--c': color }}
                        aria-label={t(`look.accent.${id}` as Key)}
                        aria-pressed={a().accent === color}
                        onClick={() => void set({ accent: color })}
                      />
                    )}
                  </For>
                  <label class="dot custom" title={t('set.anyColor')}>
                    <input
                      type="color"
                      aria-label={t('set.anyColor')}
                      value={a().accent}
                      onChange={async (e) => {
                        const picker = e.currentTarget;
                        if (!(await set({ accent: picker.value }))) picker.value = a().accent!;
                      }}
                    />
                  </label>
                </div>
                <p class="hint">{props.look.notices[0] ? t(props.look.notices[0] as Key) : t('look.locked')}</p>
              </div>
              <Segmented
                id="set-ground"
                label={t('set.ground')}
                value={a().ground}
                onPick={(ground) => void set({ ground })}
                options={controls.grounds.map((id): Option<string> => [id, t(`look.ground.${id}` as Key)])}
              />
              <Segmented
                id="set-text-font"
                label={t('set.textFont')}
                value={a().textFont}
                onPick={(textFont) => void set({ textFont })}
                options={Object.keys(controls['text-fonts']).map((id): Option<string> => [id, t(`look.font.${id}` as Key)])}
              />
              <Segmented
                id="set-heading-font"
                label={t('set.headingFont')}
                value={a().headingFont ?? ''}
                onPick={(headingFont) => void set({ headingFont })}
                options={Object.keys(controls['heading-fonts']).map((id): Option<string> => [id, t(`look.heading.${id}` as Key)])}
              />
              <div class="ctl">
                <label class="lbl" for="set-text-size">
                  {t('set.textSize')}
                  <small>{t('set.textSizeValue', { n: a().textSize })}</small>
                </label>
                <input
                  id="set-text-size"
                  class="range"
                  type="range"
                  min={controls['text-size'].min}
                  max={controls['text-size'].max}
                  step={controls['text-size'].step}
                  value={a().textSize}
                  onChange={async (e) => {
                    const slider = e.currentTarget;
                    if (!(await set({ textSize: slider.valueAsNumber }))) slider.value = String(a().textSize);
                  }}
                />
              </div>
              <Segmented
                id="set-density"
                label={t('set.density')}
                value={a().density}
                onPick={(density) => void set({ density })}
                options={controls.density.map((d): Option<number> => [d, t(DENSITY[d]!)])}
              />
              <Segmented
                id="set-corners"
                label={t('set.corners')}
                value={a().corners}
                onPick={(corners) => void set({ corners })}
                options={controls.corners.map((c): Option<number> => [c, t(CORNERS[c]!)])}
              />
              <div class="ctl">
                <Segmented
                  id="set-motion"
                  label={t('set.motion')}
                  value={a().motion}
                  onPick={(motion) => void set({ motion })}
                  options={(Object.keys(controls.motion) as Motion[]).map((m): Option<Motion> => [m, t(`look.motion.${m}` as Key)])}
                />
                <p class="hint">{t(`look.motion.${a().motion}Hint` as Key)}</p>
              </div>
              <div class="ctl">
                <span class="lbl">{t('set.effects')}</span>
                <div class="toggles">
                  <Toggle label={t('look.effect.ambient')} checked={a().effects.ambient} onChange={setEffect('ambient')} />
                  <Toggle label={t('look.effect.texture')} checked={a().effects.texture} onChange={setEffect('texture')} />
                  <Toggle label={t('look.effect.glow')} checked={a().effects.glow} onChange={setEffect('glow')} />
                </div>
              </div>
            </div>
          </Match>
          <Match when={section() === 'subscriptions'}>
            <div class="row">
              <span>{t('set.subscriptions')}</span>
              <div>
                <button type="button" class="btn secondary" onClick={() => props.onOpenSetup?.()}>
                  {t('set.openSetup')}
                </button>
              </div>
            </div>
          </Match>
          <Match when={section() === 'projects'}>
            <div class="row">
              <span>{t('set.hooks')}</span>
              <label class="check">
                <input
                  type="checkbox"
                  checked={props.saved.userHooks === true}
                  onChange={async (e) => {
                    const box = e.currentTarget;
                    if (!(await props.onSave({ userHooks: box.checked }))) box.checked = props.saved.userHooks === true;
                  }}
                />
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
