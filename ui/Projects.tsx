import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import {
  api,
  errorText,
  type BuildView,
  type DecisionView,
  type EnrollView,
  type GitMode,
  type MissionView,
  type Project,
  type RunMode,
  type TaskState,
  type TaskView,
  type Vendor,
} from './api.ts';
import { Segmented, Toggle } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { animateList } from './motion.ts';
import {
  formatCap,
  formatCost,
  formatPercent,
  isHumanDecision,
  recallGoal,
  saveGoal,
  taskReceiptStatus,
  taskStateKey,
  type PresenceChoice,
} from './projects.ts';

interface Props {
  project?: Project;
  vendors: Vendor[];
  now: number;
}

const TASK_STATES: TaskState[] = ['ready', 'running', 'review', 'done', 'blocked'];

export function Projects(props: Props) {
  const [enroll, setEnroll] = createSignal<EnrollView>();
  const [mission, setMission] = createSignal<MissionView>();
  const [goal, setGoal] = createSignal<string>('');
  const [presence, setPresence] = createSignal<PresenceChoice>('continueWithout');
  const [gitMode, setGitMode] = createSignal<GitMode>('seguro');
  const [selfUpdate, setSelfUpdate] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [customAnswers, setCustomAnswers] = createSignal<Record<string, string>>({});

  let field: HTMLTextAreaElement | undefined;

  // Auto-resize composer textarea
  const resize = () => {
    if (!field?.isConnected) return;
    const previous = field.getBoundingClientRect().height;
    const style = getComputedStyle(field);
    field.style.transition = 'none';
    field.style.height = '0px';
    const minH = parseFloat(style.minHeight) || 60;
    const maxH = parseFloat(style.maxHeight) || (typeof innerHeight !== 'undefined' ? innerHeight * 0.4 : 300);
    const scrollH = field.scrollHeight;
    const height = Math.min(maxH, Math.max(minH, goal() ? scrollH : minH));
    field.style.height = `${previous}px`;
    field.getBoundingClientRect(); // force reflow
    field.style.removeProperty('transition');
    field.style.height = `${height}px`;
  };

  createEffect(on(goal, () => queueMicrotask(resize), { defer: true }));

  onMount(() => {
    let width = 0;
    const observer = new ResizeObserver(([entry]) => {
      if (entry.contentRect.width === width) return;
      const first = width === 0;
      width = entry.contentRect.width;
      if (!first) resize();
    });
    if (field) observer.observe(field);
    window.addEventListener('resize', resize);
    onCleanup(() => {
      observer.disconnect();
      window.removeEventListener('resize', resize);
    });

    // Listen for mission events
    const unlisten = api.onMission((view) => {
      const cur = mission();
      // Ignore stale mission events with older revision
      if (!cur || view.missionId !== cur.missionId || view.revision >= cur.revision) {
        setMission(view);
      }
    });
    onCleanup(() => {
      void unlisten.then((fn) => fn());
    });
  });

  // When project changes, restore saved goal and enroll
  createEffect(
    on(
      () => props.project?.id,
      async (id) => {
        if (!id) {
          setEnroll(undefined);
          setMission(undefined);
          setGoal('');
          return;
        }
        setGoal(recallGoal(id));
        setError(undefined);
        setBusy(true);
        try {
          const enrolled = await api.projectEnroll(id);
          setEnroll(enrolled);
          setGitMode(enrolled.mode);
          setSelfUpdate(enrolled.selfUpdate);
        } catch (err) {
          setError(errorText(err));
          // Goal text is NOT lost on failed enrollment
        } finally {
          setBusy(false);
        }
      },
    ),
  );

  const onGoalInput = (val: string) => {
    setGoal(val);
    if (props.project) {
      saveGoal(props.project.id, val);
    }
  };

  const startMission = async () => {
    if (!props.project || !goal().trim() || busy()) return;
    setError(undefined);
    setBusy(true);
    try {
      const rev = enroll()?.revision ?? 1;
      await api.projectGrant(props.project.id, gitMode(), selfUpdate(), rev);
      const missionId = await api.missionStart(props.project.id, goal().trim());
      const view = await api.missionState(missionId);
      setMission(view);
    } catch (err) {
      setError(errorText(err));
      // Goal text is preserved on failed start
    } finally {
      setBusy(false);
    }
  };

  const stopMission = async () => {
    const m = mission();
    if (!m) return;
    try {
      await api.missionStop(m.missionId);
      const updated = await api.missionState(m.missionId);
      setMission(updated);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const resumeMission = async () => {
    const m = mission();
    if (!m) return;
    try {
      await api.missionResume(m.missionId);
      const updated = await api.missionState(m.missionId);
      setMission(updated);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const toggleRunMode = async (mode: RunMode) => {
    const m = mission();
    if (!m) return;
    try {
      await api.missionMode(m.missionId, mode);
      const updated = await api.missionState(m.missionId);
      setMission(updated);
    } catch (err) {
      setError(errorText(err));
    }
  };

  const answerDecision = async (decisionId: string, answer: string) => {
    const m = mission();
    if (!m) return;
    try {
      await api.missionAnswer(m.missionId, decisionId, answer);
      const updated = await api.missionState(m.missionId);
      setMission(updated);
      setCustomAnswers((prev) => {
        const next = { ...prev };
        delete next[decisionId];
        return next;
      });
    } catch (err) {
      setError(errorText(err));
    }
  };

  const humanDecisions = createMemo(() => {
    const m = mission();
    if (!m) return [];
    return m.decisions.filter((d) => isHumanDecision(d.kind));
  });

  const costDisplay = createMemo(() => {
    const m = mission();
    return formatCost(m?.cost.ownMicros);
  });

  return (
    <div class="projects-stage" ref={(el) => animateList(el)}>
      <Show
        when={props.project}
        fallback={
          <div class="projects-empty">
            <p>{t('projects.empty.selectProject')}</p>
          </div>
        }
      >
        <Show when={error()}>
          <div class="note err" role="alert">
            {t(error() as any)}
          </div>
        </Show>

        <Show
          when={mission()}
          fallback={
            /* Start Card */
            <div class="start-card" data-testid="start-card">
              <header class="p-header">
                <h2>{props.project?.name}</h2>
                <div class="p-meta">
                  <span>
                    <b>{t('projects.repo')}:</b> {enroll()?.repo ?? props.project?.path}
                  </span>
                  <Show when={enroll()?.base}>
                    <span>
                      <b>{t('projects.base')}:</b> {enroll()?.base}
                    </span>
                  </Show>
                  <Show when={enroll()?.author}>
                    <span>
                      <b>{t('projects.author')}:</b> {enroll()?.author?.name} &lt;{enroll()?.author?.email}&gt;
                    </span>
                  </Show>
                </div>
              </header>

              {/* Two presence choices */}
              <div class="ctl-section">
                <Segmented<PresenceChoice>
                  id="presence-choice"
                  label={t('projects.presence.title')}
                  value={presence()}
                  options={[
                    ['continueWithout', t('projects.presence.continueWithout')],
                    ['findTools', t('projects.presence.findTools')],
                  ]}
                  onPick={(val) => setPresence(val)}
                />
                <Show when={presence() === 'findTools'}>
                  <p class="note" role="note">
                    {t('reason.kit.findToolsInstallParked')}
                  </p>
                </Show>
                <Show when={presence() === 'continueWithout'}>
                  <p class="note" role="note">
                    {t('reason.kit.continueWithout')}
                  </p>
                </Show>
              </div>

              {/* Git mode selection */}
              <div class="ctl-section">
                <Segmented<GitMode>
                  id="git-mode-choice"
                  label={t('projects.gitMode.title')}
                  value={gitMode()}
                  options={[
                    ['seguro', t('projects.gitMode.seguro')],
                    ['equilibrado', t('projects.gitMode.equilibrado')],
                  ]}
                  onPick={(val) => setGitMode(val)}
                />
                <p class="note">
                  {gitMode() === 'seguro' ? t('projects.gitMode.seguroDesc') : t('projects.gitMode.equilibradoDesc')}
                </p>
              </div>

              {/* Self-update trust */}
              <div class="ctl-section">
                <Toggle
                  label={t('projects.selfUpdate.trust')}
                  hint={t('projects.selfUpdate.trustDesc')}
                  checked={selfUpdate()}
                  onChange={(checked) => {
                    setSelfUpdate(checked);
                  }}
                />
              </div>

              {/* Goal composer */}
              <div class="composer-container" style={{ "margin-top": "64px" }}>
                <form
                  class="composer"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void startMission();
                  }}
                >
                  <label class="sr-only" for="mission-goal">
                    {t('projects.goal.label')}
                  </label>
                  <textarea
                    id="mission-goal"
                    ref={(el) => (field = el)}
                    rows={1}
                    placeholder={t('projects.goal.placeholder')}
                    value={goal()}
                    onInput={(e) => onGoalInput(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
                        e.preventDefault();
                        void startMission();
                      }
                    }}
                  />
                  <div class="cbar" style={{ "justify-content": "flex-end" }}>
                    <button
                      type="submit"
                      class="btn primary"
                      disabled={!goal().trim() || busy()}
                      aria-label={t('projects.action.start')}
                    >
                      {t('projects.action.start')}
                      <Icon name="up" small />
                    </button>
                  </div>
                </form>
              </div>
            </div>
          }
        >
          {/* Active Mission Board */}
          {(m) => (
            <div class="mission-board" data-testid="mission-board">
              <header class="m-header">
                <div class="m-title-row">
                  <h2 class="m-goal">{m().goal}</h2>
                  <span class="badge" classList={{ [m().state]: true }}>
                    {m().state}
                  </span>
                </div>
                <div class="m-controls">
                  <Segmented<RunMode>
                    id="run-mode"
                    label=""
                    value={m().mode}
                    options={[
                      ['normal', t('projects.mode.normal')],
                      ['autonomous', t('projects.mode.autonomous')],
                    ]}
                    onPick={(mode) => void toggleRunMode(mode)}
                  />
                  <Show when={m().state === 'running'}>
                    <button type="button" class="btn" onClick={() => void stopMission()}>
                      {t('projects.action.stop')}
                    </button>
                  </Show>
                  <Show when={m().state === 'paused' || m().state === 'blocked'}>
                    <button type="button" class="btn primary" onClick={() => void resumeMission()}>
                      {t('projects.action.resume')}
                    </button>
                  </Show>
                </div>
                <Show when={m().reason}>
                  <div class="note" role="status">
                    {t(m().reason as any)}
                  </div>
                </Show>
              </header>

              {/* Amber Needs-You Decisions (R77, R92) */}
              <Show when={humanDecisions().length > 0}>
                <section class="needs-you" role="region" aria-label={t('projects.needsYou')}>
                  <div class="ny-header">
                    <Icon name="ask" />
                    <b>{t('projects.needsYou')}</b>
                    <span class="note">{t('projects.needsYou.desc')}</span>
                  </div>
                  <For each={humanDecisions()}>
                    {(dec: DecisionView) => (
                      <div class="ask ny-card" role="alertdialog">
                        <div class="ny-question">
                          <span class="chip">{t(`decision.kind.${dec.kind}` as any)}</span>
                          <b>{dec.question}</b>
                        </div>
                        <div class="ny-options">
                          <For each={dec.options}>
                            {(opt) => (
                              <button
                                type="button"
                                class="btn"
                                classList={{ primary: dec.default === opt }}
                                onClick={() => void answerDecision(dec.id, opt)}
                              >
                                {opt}
                                <Show when={dec.default === opt}>
                                  <small>({t('projects.decision.default')})</small>
                                </Show>
                              </button>
                            )}
                          </For>
                        </div>
                        <div class="ny-custom">
                          <input
                            type="text"
                            placeholder={t('projects.decision.customPlaceholder')}
                            value={customAnswers()[dec.id] ?? ''}
                            onInput={(e) => {
                              const val = e.currentTarget.value;
                              setCustomAnswers((prev) => ({ ...prev, [dec.id]: val }));
                            }}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter') {
                                e.preventDefault();
                                const ans = (customAnswers()[dec.id] ?? '').trim();
                                if (ans) void answerDecision(dec.id, ans);
                              }
                            }}
                          />
                          <button
                            type="button"
                            class="btn"
                            disabled={!(customAnswers()[dec.id] ?? '').trim()}
                            onClick={() => {
                              const ans = (customAnswers()[dec.id] ?? '').trim();
                              if (ans) void answerDecision(dec.id, ans);
                            }}
                          >
                            {t('projects.decision.answer')}
                          </button>
                        </div>
                      </div>
                    )}
                  </For>
                </section>
              </Show>

              {/* Tasks Board by State */}
              <section class="tasks-section" aria-label={t('projects.tasks.title')}>
                <div class="task-columns">
                  <For each={TASK_STATES}>
                    {(state) => {
                      const stateTasks = () => m().tasks.filter((task) => task.state === state);
                      return (
                        <div class="task-col" classList={{ [state]: true }}>
                          <div class="task-col-header">
                            <b>{t(taskStateKey(state) as any)}</b>
                            <span class="task-count">{stateTasks().length}</span>
                          </div>
                          <div class="task-list">
                            <For each={stateTasks()}>
                              {(task: TaskView) => {
                                const receipt = taskReceiptStatus(task);
                                return (
                                  <div class="task-card">
                                    <div class="tc-top">
                                      <span class="tc-title">{task.title}</span>
                                    </div>
                                    <div class="tc-meta">
                                      <Show when={task.builder}>
                                        <span class="chip tc-vendor">
                                          {t('projects.task.builder')}: {task.builder}
                                        </span>
                                      </Show>
                                      <Show when={task.reviewer}>
                                        <span class="chip tc-vendor">
                                          {t('projects.task.reviewer')}: {task.reviewer}
                                        </span>
                                      </Show>
                                    </div>
                                    <Show when={task.reason}>
                                      <div class="note">{t(task.reason as any)}</div>
                                    </Show>
                                    <div class="tc-footer">
                                      <span
                                        class="receipt-badge"
                                        classList={{ unverified: receipt.isUnverified }}
                                      >
                                        {t(receipt.textKey as any)}
                                      </span>
                                    </div>
                                  </div>
                                );
                              }}
                            </For>
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </section>

              {/* Live Cost & Account Pace Panel */}
              <div class="cost-pace-panel">
                <div class="cost-card">
                  <h3>{t('projects.cost.title')}</h3>
                  <div class="cost-reading">
                    <span class="cost-value">
                      {costDisplay().isUnknown ? t('projects.cost.unknown') : costDisplay().display}
                    </span>
                    <span class="cost-cap">
                      {t('projects.cost.cap')}: {formatCap(m().cost.capMicros)}
                    </span>
                  </div>
                </div>

                <div class="pace-card">
                  <h3>{t('projects.pace.title')}</h3>
                  <div class="pace-list">
                    <For each={m().pace}>
                      {(p) => (
                        <div class="pace-item">
                          <span class="chip">{p.vendor}</span>
                          <span class="pace-window">{p.window}</span>
                          <span class="pace-stat">
                            {formatPercent(p.used)} {t('projects.pace.used')}
                          </span>
                          <span class="pace-stat">
                            {formatPercent(p.room)} {t('projects.pace.room')}
                          </span>
                        </div>
                      )}
                    </For>
                  </div>
                </div>

                <Show when={m().build}>
                  {(b: () => BuildView) => (
                    <div class="build-card">
                      <h3>{t('projects.build.title')}</h3>
                      <div class="build-slots">
                        <div>
                          <b>{t('projects.build.current')}:</b> {b().current ?? '—'}
                        </div>
                        <div>
                          <b>{t('projects.build.next')}:</b> {b().next ?? '—'}
                        </div>
                        <div>
                          <b>{t('projects.build.lastGood')}:</b> {b().lastGood ?? '—'}
                        </div>
                        <Show when={b().waiting}>
                          <div class="note">
                            <b>{t('projects.build.waiting')}:</b> {t(b().waiting as any)}
                          </div>
                        </Show>
                      </div>
                    </div>
                  )}
                </Show>
              </div>
            </div>
          )}
        </Show>
      </Show>
    </div>
  );
}
