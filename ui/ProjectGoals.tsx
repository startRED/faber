import { createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { api, errorText, type GoalRow, type GoalsView } from './api.ts';
import { Collapse } from './controls.tsx';
import { fromEngine, lang, t } from './i18n.ts';
import type { Key } from './locales/en.ts';
import { useDraft } from './unsent.ts';

// Two activations inside one double-click interval cannot confirm a removal (as in the Memory fold and the context menu).
const SAME_GESTURE = 500;
const WORDS_MAX = 160;

const checked = (seconds: number) =>
  new Intl.DateTimeFormat(lang(), { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }).format(seconds * 1000);

/** The project's standing goals (docs/design/projects.md): things it should keep true, each one of its confirmed checks.
 * A fold on the project page like Memory, read when it opens and after each change; no process, no timer. The switch that
 * lets Faber start a fix by itself, and the day's limit, are Settings > Projects. */
export function ProjectGoals(props: { projectId: string }) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="project-memory project-goals">
      <button type="button" class="btn ghost" aria-expanded={open()} onClick={() => setOpen(!open())}>{t('goals.title')}</button>
      <Collapse open={open()}>
        <Show when={open() && props.projectId} keyed>{(id) => <GoalList projectId={id} />}</Show>
      </Collapse>
    </section>
  );
}

function GoalList(props: { projectId: string }) {
  let alive = true;
  onCleanup(() => { alive = false; });
  const [view, setView] = createSignal<GoalsView>();
  const [error, setError] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [words, setWords] = createSignal('');
  const [check, setCheck] = createSignal('');
  const [confirm, setConfirm] = createSignal<string>();
  let armedAt = 0;
  let keyDown = false;
  let needsRelease = false;
  // Words typed and not added yet keep an unattended update from closing the window, like any unsent text.
  useDraft(() => words().trim() !== '');

  const take = (next: GoalsView) => {
    setView(next);
    if (!next.checks.includes(check())) setCheck(next.checks[0] ?? '');
  };
  const run = async (work: () => Promise<GoalsView>) => {
    if (busy()) return false;
    setBusy(true); setError('');
    try {
      const next = await work();
      if (alive) take(next);
      return true;
    } catch (err) {
      if (alive) setError(fromEngine(errorText(err)));
      return false;
    } finally { if (alive) setBusy(false); }
  };
  onMount(() => void run(() => api.projectGoals(props.projectId)));

  const add = async (event: Event) => {
    event.preventDefault();
    if (!words().trim() || !check()) return;
    if (await run(() => api.projectGoalAdd(props.projectId, words(), check()))) setWords('');
  };
  const remove = (goal: GoalRow) => {
    if (confirm() !== goal.id) {
      armedAt = performance.now();
      needsRelease = keyDown;
      return setConfirm(goal.id);
    }
    if (needsRelease || performance.now() - armedAt < SAME_GESTURE) return;
    setConfirm(undefined);
    void run(() => api.projectGoalRemove(props.projectId, goal.id));
  };
  const when = (goal: GoalRow) => (goal.lastRun === undefined ? t('goals.never') : t('goals.checked', { when: checked(goal.lastRun) }));

  return (
    <div class="memory-body goals-body" aria-busy={busy()}>
      <div class="memory-head">
        <p class="hint">{t('goals.scope')}</p>
        <button type="button" class="btn ghost" disabled={busy()} onClick={() => void run(() => api.projectGoals(props.projectId))}>{t('goals.refresh')}</button>
      </div>
      <Show when={error()}><p class="note err" role="alert">{error()}</p></Show>
      <Show when={view()}>
        {(v) => (
          <>
            {/* With no goal the empty line is enough: the switch only matters once there is something to watch. */}
            <Show when={!v().auto && v().goals.length}><p class="hint">{t('goals.off')}</p></Show>
            <Show when={!v().goals.length}><p class="hint">{t('goals.empty')}</p></Show>
            <ul class="memory-entries goals">
              <For each={v().goals}>{(goal) => (
                <li class="memory-entry goal" data-goal={goal.id} data-state={goal.state}>
                  <p class="memory-text">{goal.words}</p>
                  <div class="memory-from">
                    <span class="goal-state" classList={{ ask: goal.state === 'needsYou' }}>{t(`goals.state.${goal.state}` as Key)}</span> · {when(goal)} · <code>{goal.check}</code>
                  </div>
                  <Show when={goal.state === 'needsYou' && goal.reason}><p class="hint" role="status">{fromEngine(goal.reason)}</p></Show>
                  <div class="memory-actions">
                    <Show when={goal.state === 'needsYou' || goal.state === 'paused'}>
                      <button type="button" class="btn ghost" disabled={busy()} onClick={() => void run(() => api.projectGoalResume(props.projectId, goal.id))}>{t('goals.resume')}</button>
                    </Show>
                    <button type="button" class="btn ghost danger" disabled={busy()}
                      onKeyDown={(event) => {
                        if (event.key !== 'Enter' && event.key !== ' ') return;
                        if (event.repeat) event.preventDefault();
                        else keyDown = true;
                      }}
                      onKeyUp={(event) => { if (event.key === 'Enter' || event.key === ' ') keyDown = needsRelease = false; }}
                      onFocusOut={() => { keyDown = needsRelease = false; if (confirm() === goal.id) setConfirm(undefined); }}
                      onClick={() => remove(goal)}>{t(confirm() === goal.id ? 'goals.confirmRemove' : 'goals.remove')}</button>
                    <Show when={confirm() === goal.id}><button type="button" class="btn ghost" disabled={busy()} onClick={() => setConfirm(undefined)}>{t('goals.cancel')}</button></Show>
                  </div>
                </li>
              )}</For>
            </ul>
            <Show when={v().checks.length} fallback={<p class="hint">{t('goals.noChecks')}</p>}>
              <form class="goal-add" onSubmit={(event) => void add(event)}>
                <label class="sr-only" for={`goal-words-${props.projectId}`}>{t('goals.words')}</label>
                <input id={`goal-words-${props.projectId}`} class="field" type="text" maxlength={WORDS_MAX} placeholder={t('goals.placeholder')} value={words()} disabled={busy()} onInput={(e) => setWords(e.currentTarget.value)} />
                <label class="sr-only" for={`goal-check-${props.projectId}`}>{t('goals.pickCheck')}</label>
                <select id={`goal-check-${props.projectId}`} class="field" value={check()} disabled={busy()} onChange={(e) => setCheck(e.currentTarget.value)}>
                  <For each={v().checks}>{(command) => <option value={command}>{command}</option>}</For>
                </select>
                <button type="submit" class="btn ghost" disabled={busy() || !words().trim()}>{t('goals.add')}</button>
              </form>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
