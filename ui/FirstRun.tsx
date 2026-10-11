import { createSignal, For, type JSX, onCleanup, onMount, Show } from 'solid-js';
import themes from '../src-tauri/src/themes.json';
import { api, type Appearance, type Experience, type Motion } from './api.ts';
import { appliedLook, applyLook, motion as motionNow, previewer, switchTheme } from './appearance.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { Mark } from './Mark.tsx';
import { type SizeChoice, sizeOf, withSize } from './setup.ts';

// The first run: a stage with no sidebar, a stepper on top, and the step below it. Step 1 is `Setup` (ui/Setup.tsx) and
// step 2 is `LookStep`; step 3 is the new conversation itself. The stage is a modal dialog: nothing else is on screen.

const THEMES = themes['user-controls'].themes;
const STEPS = ['accounts', 'look', 'chat'] as const;

/** Three steps: the ones before `at` are done, `at` is the current one. */
export function Stepper(props: { at: 1 | 2 }) {
  return (
    <ol class="stepper" aria-label={t('first.steps')}>
      <For each={STEPS}>
        {(id, i) => (
          <li classList={{ done: i() < props.at - 1, cur: i() === props.at - 1 }} aria-current={i() === props.at - 1 ? 'step' : undefined}>
            <Show when={i() < props.at - 1}>
              <Icon name="check" small />
            </Show>
            {t(`first.step.${id}` as Key)}
          </li>
        )}
      </For>
    </ol>
  );
}

/** The window of the first run. `step` is the stepper's place; the cards that only reconnect an account have none. */
export function Stage(props: { step?: 1 | 2; labelledby: string; children: JSX.Element }) {
  return (
    <div class="fr-stage" classList={{ plain: !props.step }} role="dialog" aria-modal="true" aria-labelledby={props.labelledby}>
      <Show when={props.step}>
        {(at) => (
          <div class="fr-top">
            <Stepper at={at()} />
          </div>
        )}
      </Show>
      <div class="firstrun">{props.children}</div>
    </div>
  );
}

/** A heading whose last word is set apart, so a look with an italic or dimmed emphasis (Workshop, Dark Room) can draw it. */
export function Title(props: { id: string; text: string }) {
  const at = () => props.text.lastIndexOf(' ');
  // The heading takes focus when the step opens, so a keyboard or screen reader user starts at the top of it.
  return (
    <h1 id={props.id} class="display fr-title" tabindex="-1" ref={(el) => queueMicrotask(() => el.focus())}>
      {at() < 0 ? props.text : props.text.slice(0, at())}
      <Show when={at() >= 0}>
        {' '}
        <em>{props.text.slice(at() + 1)}</em>
      </Show>
    </h1>
  );
}

/** One small live render of the app, drawn with the tokens of its own look. It is a picture: nothing in it can be used. */
function Mini(props: { base: string }) {
  let el: HTMLSpanElement | undefined;
  onMount(() => {
    void api
      .previewLook({ base: props.base })
      .then((look) => el && applyLook(look, el))
      .catch(() => undefined);
  });
  const title = () => t('first.mini.title');
  const at = () => title().lastIndexOf(' ');
  return (
    <span class="mini" ref={(node) => (el = node)} aria-hidden="true">
      <span class="m-side">
        <Mark size={16} />
        <i class="on" />
        <i style={{ width: '80%' }} />
        <i style={{ width: '60%' }} />
        <i style={{ width: '72%' }} />
      </span>
      <span class="m-main">
        <span class="grain" />
        <span class="m-h display">
          {title().slice(0, at())} <em>{title().slice(at() + 1)}</em>
        </span>
        <span class="m-bar">
          <i />
        </span>
        <span class="m-ask">
          <Icon name="ask" small />
          {t('run.needsYou')}
        </span>
        <span class="m-row">
          <span class="vm claude">
            <VendorMark vendor="claude" />
            {t('first.mini.claude')} <span class="verb">{t('run.working')}</span>
          </span>
        </span>
        <span class="m-row">
          <span class="vm codex">
            <VendorMark vendor="codex" />
            {t('first.mini.codex')} <span class="verb">{t('run.working')}</span>
          </span>
        </span>
        <span class="m-foot">
          <span class="m-in">{t('composer.chatPlaceholder')}</span>
          <span class="m-btn">{t('composer.send')}</span>
        </span>
      </span>
    </span>
  );
}

function Segmented<T extends string | number>(props: { id: string; label: string; value: T; options: [T, string][]; onPick: (v: T) => void }) {
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

const SIZE_PX: Record<SizeChoice, number> = { small: 13, normal: 15, large: 18 };

/** Step 2: the three looks side by side, the first settings, and the question that decides how much the tutorial says. */
export function LookStep(props: {
  /** The saved appearance, none for a first run. */
  initial?: Appearance;
  experience?: Experience;
  /** Going back keeps the choices made so far, for when the person comes forward again. */
  onBack: (appearance: Appearance, experience: Experience | undefined) => void;
  /** Saves everything. Resolves to false when the save failed, and the step stays open. */
  onContinue: (appearance: Appearance, experience: Experience | undefined) => Promise<boolean>;
  onError: (err: unknown) => void;
}) {
  const [draft, setDraft] = createSignal<Appearance>(props.initial ?? { base: themes.default });
  const [experience, setExperience] = createSignal<Experience | undefined>(props.experience);
  const [busy, setBusy] = createSignal(false);
  // Every choice goes to the look engine at once, and only the answer to the newest request is applied to the page.
  const preview = previewer(api.previewLook, (look) => applyLook(look), props.onError);
  // A request still on its way when this step closes must not land on the next screen.
  onCleanup(() => preview.cancel());
  const change = (next: Appearance) => {
    setDraft(next);
    void preview(next);
  };
  const density = () => draft().density ?? appliedLook()?.appearance.density ?? 1;
  // Nothing chosen yet means the look's own level, held down by a system that asks for less motion.
  const shownMotion = () => draft().motion ?? motionNow();
  const go = async () => {
    setBusy(true);
    // The saved look is applied when the save answers, so no preview may land after it.
    preview.cancel();
    try {
      await props.onContinue(draft(), experience());
    } finally {
      setBusy(false);
    }
  };

  return (
    <Stage step={2} labelledby="fr-title">
      <Title id="fr-title" text={t('first.look.title')} />
      <p class="sub">{t('first.look.sub')}</p>
      <div class="themes" role="group" aria-labelledby="fr-title" inert={busy()}>
        <For each={THEMES}>
          {(id) => (
            <button
              type="button"
              class="tcard"
              aria-pressed={draft().base === id}
              aria-labelledby={`fr-${id}-name`}
              aria-describedby={`fr-${id}-desc`}
              onClick={() => change(switchTheme(draft(), id))}
            >
              <Mini base={id} />
              <span class="meta">
                <span class="radio" aria-hidden="true" />
                <span class="txt">
                  <span class="nm" id={`fr-${id}-name`}>
                    {t(`look.${id}.name` as Key)}
                    <Show when={id === themes.default}>
                      <small>{t('look.default')}</small>
                    </Show>
                  </span>
                  <span class="ds" id={`fr-${id}-desc`}>
                    {t(`look.${id}.desc` as Key)}
                  </span>
                </span>
              </span>
            </button>
          )}
        </For>
      </div>
      <div class="first-ctl" inert={busy()}>
        <div class="ctl">
          <span class="lbl" id="fr-size">
            {t('set.textSize')}
          </span>
          <div class="sizes" role="group" aria-labelledby="fr-size">
            <For each={['small', 'normal', 'large'] as const}>
              {(size) => (
                <button
                  type="button"
                  aria-pressed={sizeOf(draft()) === size}
                  aria-label={t(`first.size.${size}` as Key)}
                  style={{ 'font-size': `${SIZE_PX[size]}px` }}
                  onClick={() => change(withSize(draft(), size))}
                >
                  Aa
                </button>
              )}
            </For>
          </div>
        </div>
        <Segmented
          id="fr-density"
          label={t('set.density')}
          value={density()}
          onPick={(d) => change({ ...draft(), density: d })}
          options={[
            [1, t('set.densityComfortable')],
            [0.8, t('set.densityCompact')],
          ]}
        />
        <div class="ctl">
          <Segmented
            id="fr-motion"
            label={t('set.motion')}
            value={shownMotion()}
            onPick={(m) => change({ ...draft(), motion: m })}
            options={(['full', 'reduced', 'none'] as Motion[]).map((m): [Motion, string] => [m, t(`look.motion.${m}` as Key)])}
          />
          <p class="hint">{t('first.motionHint')}</p>
        </div>
        <div class="ctl">
          <span class="lbl" id="fr-xp">
            {t('first.xp.title')}
          </span>
          <div class="choice2" role="group" aria-labelledby="fr-xp">
            <For each={['beginner', 'experienced'] as const}>
              {(level) => (
                <button type="button" aria-pressed={experience() === level} onClick={() => setExperience(level)}>
                  {t(`first.xp.${level}` as Key)}
                  <small>{t(`first.xp.${level}Hint` as Key)}</small>
                </button>
              )}
            </For>
          </div>
        </div>
      </div>
      <div class="first-foot">
        <p>{t('first.foot')}</p>
        <button type="button" class="btn ghost" disabled={busy()} onClick={() => props.onBack(draft(), experience())}>
          <Icon name="left" small />
          {t('first.back')}
        </button>
        <button type="button" class="btn primary big" disabled={busy()} onClick={() => void go()}>
          {t('first.continue')}
          <Icon name="right" small />
        </button>
      </div>
    </Stage>
  );
}
