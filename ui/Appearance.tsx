import { createSignal, For, onCleanup, onMount } from 'solid-js';
import themes from '../src-tauri/src/themes.json';
import { api, type Appearance, type Look, type Motion } from './api.ts';
import { previewer, switchTheme } from './appearance.ts';
import { type Option, Segmented, Toggle } from './controls.tsx';
import { factory, failingPairs, groundColor, groundKey, normalizeHex, readings, sameChoices, shown } from './draft.ts';
import { Mini } from './FirstRun.tsx';
import { Icon, VendorMark } from './Glyphs.tsx';
import { failureText, lang, t } from './i18n.ts';
import type { Key } from './locales/en.ts';

// The Appearance section of Settings. Every change goes into a draft and to the look engine, and only the preview panel
// wears the answer ("Not applied yet"). "Use this look" saves the draft, and the page wears it only once the save worked.

// The lists of options come from themes.json, the same file the shell reads its tokens from.
const controls = themes['user-controls'];
const LOOKS = [...controls.themes, ...controls.presets];
const swatches = Object.entries(controls.accent.swatches);
const DENSITY: Record<number, Key> = { 1: 'set.densityComfortable', 0.8: 'set.densityCompact' };
const CORNERS: Record<number, Key> = { 0: 'set.cornersSquare', 1: 'set.cornersSoft', 1.6: 'set.cornersRound' };
const EFFECTS = ['ambient', 'texture', 'glow'] as const;

interface Props {
  /** The look on the page now. */
  look: Look;
  /** The appearance that is saved; none until the person changes the look. */
  saved: Appearance | undefined;
  /** The draft lives in Settings, so it survives a trip to the account cards. */
  draft: () => Appearance;
  setDraft: (next: Appearance) => void;
  /** Saves the draft and, once the save worked, puts it on the page. Resolves to false when the save was refused. */
  onUse: (appearance: Appearance) => Promise<boolean>;
  /** Asks the shell for a look; the checks put their own here. */
  request?: (draft: Appearance) => Promise<Look>;
}

export function AppearanceSection(props: Props) {
  // What the preview panel wears: the answer to the newest change.
  const [preview, setPreview] = createSignal<Look>(props.look);
  // Why the last change was refused, or what the last file said.
  const [problem, setProblem] = createSignal<string>();
  const [info, setInfo] = createSignal<string>();
  const [busy, setBusy] = createSignal(false);
  // The last draft the shell accepted. A change it refuses (an accent too close to a meaning colour) goes back to it.
  let good = props.draft();
  const ask = previewer(
    (draft) => (props.request ?? api.previewLook)(draft),
    (look, draft) => {
      good = draft;
      setPreview(look);
    },
    (err) => {
      setProblem(failureText(err));
      props.setDraft(good);
    },
  );
  // An answer that arrives after the section is gone must not touch the draft, which outlives it.
  onCleanup(() => ask.cancel());
  const change = (next: Appearance) => {
    setProblem(undefined);
    setInfo(undefined);
    props.setDraft(next);
    void ask(next);
  };
  // Coming back from the account cards with a draft that is not saved: the panel shows it again.
  onMount(() => {
    if (dirty()) void ask(props.draft());
  });

  const a = () => shown(props.draft(), preview());
  const dirty = () => !sameChoices(props.draft(), props.saved);
  const failing = () => failingPairs(preview());
  const customAccent = () => !swatches.some(([, color]) => color === a().accent.toLowerCase());
  const number = (n: number) => new Intl.NumberFormat(lang(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(n);
  const message = () => problem() ?? (failing() ? failureText({ key: 'error.look.pairFailed', detail: failing() }) : info());

  const use = async () => {
    setBusy(true);
    try {
      await props.onUse(props.draft());
    } finally {
      setBusy(false);
    }
  };
  const saveFile = async () => {
    setProblem(undefined);
    try {
      await api.saveTheme(props.draft(), t('set.themeName'));
    } catch (err) {
      setProblem(failureText(err));
    }
  };
  const openFile = async () => {
    try {
      const file = await api.openTheme();
      if (!file) return;
      const { name, ...choices } = file;
      change(choices);
      setInfo(t('set.fileOpened', { name }));
    } catch (err) {
      setInfo(undefined);
      setProblem(failureText(err));
    }
  };
  const typedAccent = (input: HTMLInputElement) => {
    const color = normalizeHex(input.value);
    if (color) return change({ ...props.draft(), accent: color });
    setProblem(t('set.hexInvalid'));
    input.value = a().accent;
  };

  return (
    <div class="ap">
      <div class="ap-main">
        <div class="presets" role="group" aria-label={t('set.look')}>
          <For each={LOOKS}>
            {(id) => (
              <button
                type="button"
                class="preset"
                aria-pressed={a().base === id}
                aria-labelledby={`look-${id}-name`}
                aria-describedby={`look-${id}-desc`}
                title={t(`look.${id}.desc` as Key)}
                onClick={() => change(switchTheme(props.draft(), id))}
              >
                <span class="thumb" aria-hidden="true">
                  <Mini base={id} />
                </span>
                <span class="nm" id={`look-${id}-name`}>
                  {t(`look.${id}.name` as Key)}
                </span>
                <span class="kind">{t(id === themes.default ? 'set.kindDefault' : controls.themes.includes(id) ? 'set.kindTheme' : 'set.kindPreset')}</span>
                <span class="sr" id={`look-${id}-desc`}>
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
            </span>
            <div class="dots" role="group" aria-labelledby="set-accent">
              <For each={swatches}>
                {([id, color]) => (
                  <button
                    type="button"
                    class="dot"
                    style={{ '--c': color }}
                    aria-label={t(`look.accent.${id}` as Key)}
                    aria-pressed={a().accent.toLowerCase() === color}
                    onClick={() => change({ ...props.draft(), accent: color })}
                  />
                )}
              </For>
              <label class="dot custom" classList={{ on: customAccent() }} title={t('set.anyColor')}>
                <input type="color" aria-label={t('set.anyColor')} value={a().accent} onChange={(e) => change({ ...props.draft(), accent: e.currentTarget.value })} />
              </label>
              <input
                class="hex num"
                type="text"
                aria-label={t('set.hexLabel')}
                value={a().accent}
                maxLength={7}
                spellcheck={false}
                autocomplete="off"
                onChange={(e) => typedAccent(e.currentTarget)}
              />
            </div>
            <p class="hint">{preview().notices[0] ? t(preview().notices[0] as Key) : t('look.locked')}</p>
          </div>
          <div class="ctl">
            <span class="lbl" id="set-ground">
              {t('set.ground')}
              <small>{t(groundKey(a().ground) as Key)}</small>
            </span>
            <div class="seg-ctl chips" role="group" aria-labelledby="set-ground">
              <For each={controls.grounds}>
                {(id) => (
                  <button
                    type="button"
                    aria-pressed={a().ground === id}
                    aria-label={t(`look.ground.${id}` as Key)}
                    title={t(`look.ground.${id}` as Key)}
                    onClick={() => change({ ...props.draft(), ground: id })}
                  >
                    <span class="sq" style={{ background: groundColor(id) }} />
                  </button>
                )}
              </For>
            </div>
          </div>
          <Segmented
            id="set-text-font"
            label={t('set.textFont')}
            value={a().textFont}
            onPick={(textFont) => change({ ...props.draft(), textFont })}
            options={Object.keys(controls['text-fonts']).map((id): Option<string> => [id, t(`look.font.${id}` as Key)])}
          />
          <Segmented
            id="set-heading-font"
            label={t('set.headingFont')}
            value={a().headingFont ?? ''}
            onPick={(headingFont) => change({ ...props.draft(), headingFont })}
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
              onInput={(e) => change({ ...props.draft(), textSize: e.currentTarget.valueAsNumber })}
            />
          </div>
          <Segmented
            id="set-density"
            label={t('set.density')}
            value={a().density}
            onPick={(density) => change({ ...props.draft(), density })}
            options={controls.density.map((d): Option<number> => [d, t(DENSITY[d]!)])}
          />
          <Segmented
            id="set-corners"
            label={t('set.corners')}
            value={a().corners}
            onPick={(corners) => change({ ...props.draft(), corners })}
            options={controls.corners.map((c): Option<number> => [c, t(CORNERS[c]!)])}
          />
          <div class="ctl">
            <Segmented
              id="set-motion"
              label={t('set.motion')}
              value={a().motion}
              onPick={(motion) => change({ ...props.draft(), motion })}
              options={(Object.keys(controls.motion) as Motion[]).map((m): Option<Motion> => [m, t(`look.motion.${m}` as Key)])}
            />
            <p class="hint">{t(`look.motion.${a().motion}Hint` as Key)}</p>
          </div>
        </div>
        <div class="ctl">
          <span class="lbl">
            <Icon name="lock" small />
            {t('set.lockedTitle')}
          </span>
          <div class="locked">
            <span class="lt claude">
              <VendorMark vendor="claude" />
              {t('set.lockedClaude')}
            </span>
            <span class="lt codex">
              <VendorMark vendor="codex" />
              {t('set.lockedCodex')}
            </span>
            <span class="lt amber">
              <Icon name="ask" small />
              {t('run.needsYou')}
            </span>
          </div>
          <p class="hint">{t('set.lockedHint')}</p>
        </div>
        <div class="filerow">
          <button type="button" class="btn" onClick={() => void saveFile()}>
            <Icon name="download" small />
            {t('set.saveFile')}
          </button>
          <button type="button" class="btn" onClick={() => void openFile()}>
            <Icon name="upload" small />
            {t('set.openFile')}
          </button>
          <span class="hint">{t('set.fileHint')}</span>
        </div>
      </div>
      <aside class="s-prev" aria-label={t('set.preview')}>
        <div class="ph">
          {t('set.preview')}
          <small>{dirty() ? t('set.previewPending') : t('set.previewApplied')}</small>
        </div>
        <div class="pvbox">
          <Mini look={preview()} />
        </div>
        <div class="contrast" role="list">
          <For each={readings(preview())}>
            {(r) => (
              <div role="listitem" classList={{ bad: !r.pass }}>
                {t(`set.contrast.${r.id}` as Key)}
                <b class="num">
                  {number(r.ratio)}:1<em>{t(r.pass ? 'set.contrastPass' : 'set.contrastFail')}</em>
                </b>
              </div>
            )}
          </For>
        </div>
        <div class="toggles">
          <For each={EFFECTS}>
            {(key) => (
              <Toggle
                label={t(`look.effect.${key}` as Key)}
                hint={t(`look.effect.${key}Hint` as Key)}
                checked={a().effects[key]}
                onChange={(on) => change({ ...props.draft(), effects: { ...a().effects, [key]: on } })}
              />
            )}
          </For>
        </div>
      </aside>
      <div class="ap-bar">
        <p class="ap-msg" classList={{ bad: !!problem() || !!failing() }} aria-live="polite">
          {message()}
        </p>
        <button type="button" class="btn" onClick={() => change(factory())}>
          {t('set.reset')}
        </button>
        <button type="button" class="btn primary" disabled={busy() || !dirty() || !!failing()} onClick={() => void use()}>
          {t('set.use')}
        </button>
      </div>
    </div>
  );
}
