import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
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
  type RunningItem,
  type Summary,
  type TaskState,
  type TaskView,
  type Vendor,
} from './api.ts';
import { Segmented, Toggle } from './controls.tsx';
import { resizeComposer } from './composer-size.ts';
import { Ambient } from './Ambient.tsx';
import { Ghost } from './Ghost.tsx';
import { greetingParts } from './greeting.ts';
import { Peek } from './Mark.tsx';
import { Icon } from './Glyphs.tsx';
import { LocalBuild } from './LocalBuild.tsx';
import { fromEngine, lang, t } from './i18n.ts';
import { animateList } from './motion.ts';
import { pickUp } from './running.ts';
import { ResumeRow } from './Welcome.tsx';
import {
  formatCap,
  formatCost,
  formatPercent,
  isHumanDecision,
  recallGoal,
  recallMission,
  saveGoal,
  saveMission,
  taskReceiptStatus,
  taskStateKey,
  type PresenceChoice,
} from './projects.ts';

interface Props {
  project?: Project;
  vendors: Vendor[];
  now: number;
  /** What the first screen of a project offers to pick up. All optional: a board needs none of it. */
  projects?: Project[];
  recent?: Summary[];
  running?: RunningItem[];
  onOpen?: (conversation: Summary) => void;
  onProject?: (id: string) => void;
  onAdd?: () => void;
}

/** The last two parts of a folder path: enough to tell two projects apart without the whole drive. */
const tail = (path: string) => path.split(/[\\/]/).filter(Boolean).slice(-2).join(' / ');

const TASK_STATES: TaskState[] = ['ready', 'running', 'review', 'done', 'blocked'];

export function Projects(props: Props) {
  const [enroll, setEnroll] = createSignal<EnrollView>();
  const [localBuild, setLocalBuild] = createSignal<BuildView>();
  const [missionData, setMissionData] = createStore<{ view?: MissionView }>({});
  const mission = () => missionData.view;
  const setMission = (view?: MissionView) => {
    if (view) setMissionData('view', reconcile(view));
    else setMissionData('view', undefined);
  };
  const [goal, setGoal] = createSignal<string>('');
  const [presence, setPresence] = createSignal<PresenceChoice>('continueWithout');
  const [gitMode, setGitMode] = createSignal<GitMode>('seguro');
  const [selfUpdate, setSelfUpdate] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [customAnswers, setCustomAnswers] = createSignal<Record<string, string>>({});
  const customAnswer = (id: string) => Object.hasOwn(customAnswers(), id) ? customAnswers()[id] : '';
  let generation = 0;
  let selectedMission: string | undefined;
  const accept = (view: MissionView, token = generation) => {
    if (token !== generation || view.missionId !== selectedMission) return;
    const current = mission();
    if (!current || view.revision > current.revision) setMission(view);
  };

  let field: HTMLTextAreaElement | undefined;

  const resize = () => resizeComposer(field, goal());

  createEffect(on(goal, () => queueMicrotask(resize), { defer: true }));

  let disposed = false;
  onCleanup(() => { disposed = true; });

  const refreshLocalBuild = () => {
    void api.localUpdateState().then((view) => {
      if (!disposed) setLocalBuild(view ?? undefined);
    }, (err) => { if (!disposed) setError(errorText(err)); });
  };

  onMount(() => {
    refreshLocalBuild();
    const timer = setInterval(refreshLocalBuild, 30_000);
    window.addEventListener('focus', refreshLocalBuild);
    onCleanup(() => { clearInterval(timer); window.removeEventListener('focus', refreshLocalBuild); });
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
      accept(view);
      refreshLocalBuild();
    }).catch((err) => { setError(errorText(err)); return () => {}; });
    onCleanup(() => {
      void unlisten.then((fn) => fn(), () => {});
    });
  });

  createEffect(
    on(
      mission,
      () => {
        refreshLocalBuild();
      },
      { defer: true },
    ),
  );

  // When project changes, restore saved goal and enroll
  const projectIdentity = createMemo(() => props.project?.id);
  createEffect(
    on(
      projectIdentity,
      async (id) => {
        const token = ++generation;
        selectedMission = undefined;
        setEnroll(undefined);
        setMission(undefined);
        setCustomAnswers({});
        setError(undefined);
        setGitMode('seguro');
        setSelfUpdate(false);
        if (!id) {
          setEnroll(undefined);
          setMission(undefined);
          setGoal('');
          return;
        }
        setGoal(recallGoal(id));
        selectedMission = recallMission(id);
        setError(undefined);
        setBusy(true);
        try {
          const enrolled = await api.projectEnroll(id);
          if (token !== generation) return;
          setEnroll(enrolled);
          setGitMode(enrolled.mode);
          setSelfUpdate(enrolled.selfUpdate);
          if (selectedMission) accept(await api.missionState(selectedMission), token);
        } catch (err) {
          if (token !== generation) return;
          setError(errorText(err));
          // Goal text is NOT lost on failed enrollment
        } finally {
          if (token === generation) setBusy(false);
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
    if (!props.project || !goal().trim() || busy() || !enroll()) return;
    const projectId = props.project.id;
    const text = goal().trim();
    const token = generation;
    setError(undefined);
    setBusy(true);
    try {
      const rev = enroll()!.revision;
      await api.projectGrant(projectId, gitMode(), selfUpdate(), rev);
      if (token !== generation) return;
      const missionId = await api.missionStart(projectId, text);
      if (token !== generation) return;
      selectedMission = missionId;
      saveMission(projectId, missionId);
      const view = await api.missionState(missionId);
      accept(view, token);
    } catch (err) {
      if (token !== generation) return;
      setError(errorText(err));
      // Refresh the grant revision before a retry; do not clear the person's goal.
      try { const next = await api.projectEnroll(projectId); if (token === generation) setEnroll(next); } catch {}
      // Goal text is preserved on failed start
    } finally {
      if (token === generation) setBusy(false);
    }
  };

  const stopMission = async () => {
    const m = mission();
    if (!m || busy()) return;
    const token = generation;
    setBusy(true); setError(undefined);
    try {
      const updated = await api.missionStop(m.missionId);
      accept(updated, token);
    } catch (err) {
      if (token === generation) setError(errorText(err));
    } finally { if (token === generation) setBusy(false); }
  };

  const resumeMission = async () => {
    const m = mission();
    if (!m || busy()) return;
    const token = generation;
    setBusy(true); setError(undefined);
    try {
      const updated = await api.missionResume(m.missionId);
      accept(updated, token);
    } catch (err) {
      if (token === generation) setError(errorText(err));
    } finally { if (token === generation) setBusy(false); }
  };

  const toggleRunMode = async (mode: RunMode) => {
    const m = mission();
    if (!m || busy()) return;
    const token = generation;
    setBusy(true); setError(undefined);
    try {
      const updated = await api.missionMode(m.missionId, mode);
      accept(updated, token);
    } catch (err) {
      if (token === generation) setError(errorText(err));
    } finally { if (token === generation) setBusy(false); }
  };

  const answerDecision = async (decisionId: string, answer: string) => {
    const m = mission();
    if (!m || busy()) return;
    const token = generation;
    setBusy(true); setError(undefined);
    try {
      const updated = await api.missionAnswer(m.missionId, decisionId, answer);
      if (token !== generation) return;
      accept(updated, token);
      setCustomAnswers((prev) => {
        const next = { ...prev };
        delete next[decisionId];
        return next;
      });
    } catch (err) {
      if (token === generation) setError(errorText(err));
    } finally { if (token === generation) setBusy(false); }
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
  const build = (): BuildView => localBuild() ?? mission()?.build ?? { waiting: 'reason.build.unavailable' };

  // The first screen of a project: no mission yet, so the screen is a home, with its own light behind it.
  const home = () => !mission();
  const title = () => greetingParts(t('projects.home.title'));
  let root: HTMLDivElement | undefined;
  const [focused, setFocused] = createSignal(false);
  const running = () => props.running ?? [];
  const conversations = createMemo(() => pickUp((props.recent ?? []).filter((c) => c.area === 'project'), running(), 3));
  const projectName = (c: Summary) => (props.projects ?? []).find((p) => p.id === c.projectId)?.name ?? t('area.projects');
  const others = createMemo(() => (props.projects ?? []).filter((p) => p.id !== props.project?.id).slice(0, 4));
  const needs = (id: string) => running().some((i) => i.projectId === id && i.state === 'needsYou');
  // What the lists keep clear of: the text blocks, so the dust never lies behind words.
  const avoid = () => (root ? [...root.querySelectorAll('.hello, .lead, .composer, .setup, .p-header, .p-side section')] : []);

  const pickUpList = () => (
    <Show when={conversations().length > 0 && props.onOpen}>
      <section aria-labelledby="pickup-title">
        <h3 id="pickup-title">{t('s1.pickUp')}</h3>
        <For each={conversations()}>
          {(c) => <ResumeRow conversation={c} item={running().find((i) => i.conversationId === c.id)} where={projectName(c)} onOpen={props.onOpen!} />}
        </For>
      </section>
    </Show>
  );

  const otherProjects = () => (
    <Show when={others().length > 0}>
      <section aria-labelledby="others-title">
        <h3 id="others-title">{t('projects.home.others')}</h3>
        <For each={others()}>
          {(p) => (
            <button type="button" class="rrow" classList={{ ask: needs(p.id) }} onClick={() => props.onProject?.(p.id)}>
              <span class="t">{p.name}</span>
              <span class="s">
                <Show when={needs(p.id)} fallback={tail(p.path)}>
                  <span class="amber-tag">
                    <Icon name="ask" small />
                    {t('run.needsYou')}
                  </span>
                </Show>
              </span>
            </button>
          )}
        </For>
      </section>
    </Show>
  );

  return (
    <div class="projects-root" ref={(el) => (root = el)}>
      {/* The light behind a project's first screen; a mission board has none. */}
      <Show when={home()}>
        <Ambient variant="project" vendors={props.vendors} avoid={avoid} />
      </Show>
      <div class="projects-stage" classList={{ home: home() }} ref={(el) => animateList(el)}>
        <Show
          when={props.project}
          fallback={
            <div class="welcome-grid">
              <div class="w-main empty">
                <Peek />
                <h1 class="display hello">{t('empty.openProjectTitle')}</h1>
                <p class="lead">{t('empty.openProjectBody')}</p>
                <button type="button" class="btn primary" onClick={() => props.onAdd?.()}>
                  {t('side.addProject')}
                </button>
              </div>
              <Show when={others().length > 0 || conversations().length > 0}>
                <div class="p-side">
                  {pickUpList()}
                  {otherProjects()}
                </div>
              </Show>
            </div>
          }
        >
          <Show when={!mission() && localBuild()}>{(view) => <LocalBuild view={view()} />}</Show>
          <Show when={error()}>
            <div class="note err" role="alert">
              {fromEngine(error())}
            </div>
          </Show>

          <Show
            when={mission()}
            fallback={
              /* Start Card: the first screen of a project, in the family of the new conversation. */
              <div class="start-card welcome-grid" data-testid="start-card">
                <div class="w-main">
                <h1 class="display hello">
                  {title()[0]}
                  <Show when={title()[1]}>
                    {' '}
                    <em>{title()[1]}</em>
                  </Show>
                </h1>
                <p class="lead">{t('projects.home.lead')}</p>

                {/* Goal composer */}
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
                    onFocus={() => setFocused(true)}
                    onBlur={() => setFocused(false)}
                    onInput={(e) => onGoalInput(e.currentTarget.value)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
                        e.preventDefault();
                        void startMission();
                      }
                    }}
                  />
                  <Show when={!goal()}>
                    <Ghost hints={[t('projects.home.hint.1'), t('projects.home.hint.2'), t('projects.home.hint.3')]} focused={focused()} />
                  </Show>
                  <div class="cbar" style={{ "justify-content": "flex-end" }}>
                    <button
                      type="submit"
                      class="btn primary"
                      disabled={!goal().trim() || busy() || !enroll()}
                      aria-label={t('projects.action.start')}
                    >
                      {t('projects.action.start')}
                      <Icon name="up" small />
                    </button>
                  </div>
                </form>

                <section class="setup" aria-labelledby="setup-title">
                <h3 id="setup-title">{t('projects.home.rules')}</h3>
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

                {/* The technical detail behind the plain rows above, and what is kept of the repository. */}
                <details class="setup-more setup-details">
                  <summary>{t('projects.home.details')}</summary>
                  <p>{t('projects.gitMode.seguroTech')}</p>
                  <p>{t('projects.selfUpdate.trustTech')}</p>
                  <p>
                    <b>{t('projects.repo')}:</b> {enroll()?.repo ?? props.project?.path}
                  </p>
                  <Show when={enroll()?.base}>
                    <p>
                      <b>{t('projects.base')}:</b> {enroll()?.base}
                    </p>
                  </Show>
                  <Show when={enroll()?.author}>
                    <p>
                      <b>{t('projects.author')}:</b> {enroll()?.author?.name} &lt;{enroll()?.author?.email}&gt;
                    </p>
                  </Show>
                </details>
                </section>
                </div>

                {/* What this project is, and where to go from here. */}
                <div class="p-side">
                  <header class="p-header">
                    <Peek />
                    <h2>{props.project?.name}</h2>
                    <div class="p-meta">
                      <span title={enroll()?.repo ?? props.project?.path}>
                        <b>{t('projects.repo')}:</b> {tail(enroll()?.repo ?? props.project?.path ?? '')}
                      </span>
                    </div>
                  </header>
                  {pickUpList()}
                  {otherProjects()}
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
                      {fromEngine(`projects.state.${m().state}`)}
                    </span>
                  </div>
                  <div class="m-controls">
                    <Segmented<RunMode>
                      id="run-mode"
                      label={t('projects.mode.label')}
                      value={m().mode}
                      options={[
                        ['normal', t('projects.mode.normal')],
                        ['autonomous', t('projects.mode.autonomous')],
                      ]}
                      onPick={(mode) => void toggleRunMode(mode)}
                    />
                    <Show when={m().state === 'running'}>
                      <button type="button" class="btn" disabled={busy()} onClick={() => void stopMission()}>
                        {t('projects.action.stop')}
                      </button>
                    </Show>
                    <Show when={m().state === 'paused' || m().state === 'blocked'}>
                      <button type="button" class="btn primary" disabled={busy()} onClick={() => void resumeMission()}>
                        {t('projects.action.resume')}
                      </button>
                    </Show>
                  </div>
                  <Show when={m().reason}>
                    <div class="note" role="status">
                      {fromEngine(m().reason ?? undefined)}
                    </div>
                  </Show>
                </header>

                {/* Amber Needs-You Decisions (R77, R92) */}
                <Show when={m().plan}><p class="note">{m().plan}</p></Show>
                <Show when={humanDecisions().length > 0}>
                  <section class="needs-you" role="region" aria-label={t('projects.needsYou')}>
                    <div class="ny-header">
                      <Icon name="ask" />
                      <b>{t('projects.needsYou')}</b>
                      <span class="note">{t('projects.needsYou.desc')}</span>
                    </div>
                    <For each={humanDecisions()}>
                      {(dec: DecisionView) => (
                        <div class="ask ny-card" role="alertdialog" aria-label={dec.question}>
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
                                  disabled={busy()}
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
                              aria-label={t('projects.decision.customPlaceholder')}
                              placeholder={t('projects.decision.customPlaceholder')}
                              value={customAnswer(dec.id)}
                              onInput={(e) => {
                                const val = e.currentTarget.value;
                                setCustomAnswers((prev) => ({ ...prev, [dec.id]: val }));
                              }}
                              onKeyDown={(e) => {
                                if (e.key === 'Enter' && !e.isComposing) {
                                  e.preventDefault();
                                  const ans = customAnswer(dec.id).trim();
                                  if (ans) void answerDecision(dec.id, ans);
                                }
                              }}
                            />
                            <button
                              type="button"
                              class="btn"
                              disabled={busy() || !customAnswer(dec.id).trim()}
                              onClick={() => {
                                const ans = customAnswer(dec.id).trim();
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
                                  const receipt = () => taskReceiptStatus(task);
                                  const [detailsOpen, setDetailsOpen] = createSignal(false);
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
                                        <div class="note">{fromEngine(task.reason)}</div>
                                      </Show>
                                      <div class="tc-footer">
                                        <span
                                          class="receipt-badge"
                                          classList={{ unverified: receipt().isUnverified }}
                                        >
                                          {fromEngine(receipt().textKey)}
                                        </span>
                                      </div>
                                      <details onToggle={(e) => setDetailsOpen(e.currentTarget.open)}>
                                        <summary aria-label={`${t('projects.details')}: ${task.title}`}>{t('projects.details')}</summary>
                                        <Show when={detailsOpen() && task.detail}>
                                          {(detail) => <>
                                            <p>{t('projects.details.scope')}: {detail().scope.join(', ')}</p>
                                            <p>{t('projects.details.proofs')}: {detail().proofs.join(', ')}</p>
                                            <Show when={detail().artifacts.length > 0} fallback={<p>{t('projects.details.noEvidence')}</p>}>
                                              <For each={detail().artifacts}>{(artifact) => <div>
                                                <p>{artifact.summary}</p>
                                                <pre>{JSON.stringify(artifact, null, 2)}</pre>
                                              </div>}</For>
                                            </Show>
                                          </>}
                                        </Show>
                                      </details>
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
                        {t('projects.cost.cap')}: {m().cost.capMicros == null ? t('projects.cost.unknown') : formatCap(m().cost.capMicros)}
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
                            <span class="pace-window">{fromEngine(`quota.${p.window}`)}</span>
                            <span class="pace-stat">
                              {formatPercent(p.used)} {t('projects.pace.used')}
                            </span>
                            <span class="pace-stat">
                              {formatPercent(p.room)} {t('projects.pace.room')}
                            </span>
                            <span class="note">{t('projects.pace.freshness', { when: p.readAt == null ? t('projects.cost.unknown') : new Date(p.readAt * 1000).toLocaleString(lang()) })}</span>
                          </div>
                        )}
                      </For>
                    </div>
                  </div>

                  <Show when={build()}>
                    {(b: () => BuildView) => (
                      <LocalBuild view={b()} />
                    )}
                  </Show>
                </div>
              </div>
            )}
          </Show>
        </Show>
      </div>
    </div>
  );
}
