import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import {
  api,
  errorText,
  type AgentsView,
  type BuildView,
  type DecisionView,
  type EnrollView,
  type MissionView,
  type PermissionMode,
  type Project,
  type ProofView,
  type RunMode,
  type RunningItem,
  type Summary,
  type TaskState,
  type TaskView,
  type Vendor,
} from './api.ts';
import { Segmented } from './controls.tsx';
import { resizeComposer } from './composer-size.ts';
import { useDraft } from './unsent.ts';
import { Ambient } from './Ambient.tsx';
import { Empty } from './Empty.tsx';
import { Hole } from './Hole.tsx';
import { Ghost } from './Ghost.tsx';
import { greetingParts } from './greeting.ts';
import { Peek } from './Mark.tsx';
import { Recents } from './Recents.tsx';
import { Icon } from './Glyphs.tsx';
import { LocalBuild } from './LocalBuild.tsx';
import { ProofList, proofCount, tail } from './ProjectSettings.tsx';
import { duration, fromEngine, lang, t } from './i18n.ts';
import { vendorName } from './models.ts';
import { animateList } from './motion.ts';
import { pickUp } from './running.ts';
import { ResumeRow } from './Welcome.tsx';
import {
  formatCap,
  formatCost,
  formatPercent,
  isHumanDecision,
  recallGoal,
  forgetMission,
  recallMission,
  saveGoal,
  saveMission,
  taskReceiptStatus,
  taskStateKey,
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
  /** Opens recent folders or a saved environment as projects (the open-a-project screen). */
  onOpened?: (projects: Project[]) => void;
  onError?: (error: unknown) => void;
  /** The permission mode from Settings; missing means Full access, the default. */
  permissions?: PermissionMode;
}

/** Modes that never stop to ask before running a command: in them the start confirms the checks by itself. */
const ASKLESS: PermissionMode[] = ['auto', 'dontAsk'];

// The board reads what moves first: running, then what waits on someone, then what is queued, then what is finished.
const TASK_STATES: TaskState[] = ['running', 'blocked', 'review', 'ready', 'done'];

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
  // What a mission proves its work with (R137): confirmed by the owner the first time a mission starts, or in Settings.
  const [proof, setProof] = createSignal<ProofView>();
  // Whether the start is asking for that confirmation.
  const [asking, setAsking] = createSignal(false);
  // The check of the agents that runs before the first mission (and again when a receipt is stale).
  const [agents, setAgents] = createSignal<AgentsView>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [customAnswers, setCustomAnswers] = createSignal<Record<string, string>>({});
  const customAnswer = (id: string) => Object.hasOwn(customAnswers(), id) ? customAnswers()[id] : '';
  // A typed goal or decision answer is unsent text: an unattended install waits for it.
  useDraft(() => goal().trim() !== '');
  useDraft(() => Object.values(customAnswers()).some((v) => v.trim() !== ''));
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

    const agentsListener = api.onAgents((view) => setAgents(view.state === 'ready' ? undefined : view)).catch(() => () => {});
    onCleanup(() => {
      void agentsListener.then((fn) => fn(), () => {});
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
        setProof(undefined);
        setMission(undefined);
        setCustomAnswers({});
        setError(undefined);
        setAsking(false);
        if (!id) {
          setEnroll(undefined);
          setProof(undefined);
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
          setProof(await api.projectProof(id));
          if (token !== generation) return;
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

  // Reads the project's checks again and opens the sheet on what they are now; a project left behind gets nothing.
  const showChecks = async (projectId: string, token: number) => {
    const latest = await api.projectProof(projectId).catch(() => proof());
    if (token !== generation) return;
    if (latest) setProof(latest);
    setAsking(true);
  };

  // In a mode that never asks, the start confirms exactly what the project declares now, as the sheet would. The engine
  // still checks every command; a refusal falls back to the sheet.
  const askless = () => ASKLESS.includes(props.permissions ?? 'dontAsk');
  const confirmByMode = async (projectId: string, p: ProofView, token: number) => {
    if (!askless() || p.source === 'none' || p.commands.length === 0) return false;
    try {
      const next = await api.projectProofConfirm(projectId, p.commands, p.setup ?? [], p.declared);
      if (token !== generation) return false;
      setProof(next);
      return next.confirmed;
    } catch { return false; }
  };

  const startMission = async () => {
    if (!props.project || !goal().trim() || busy() || !enroll()) return;
    const projectId = props.project.id;
    const token = generation;
    // Checks the owner has not confirmed yet: ask once, just in time, and start from there.
    // Settings may have confirmed them since this screen read them, so read them again first.
    if (!proof()?.confirmed) {
      const latest = await api.projectProof(projectId).catch(() => proof());
      // The owner moved to another project while the checks were read: they belong to the one left behind.
      if (token !== generation) return;
      if (latest) setProof(latest);
      if (!latest?.confirmed && !(latest && (await confirmByMode(projectId, latest, token)))) {
        if (token === generation) setAsking(true);
        return;
      }
    }
    const text = goal().trim();
    setError(undefined);
    setAgents(undefined);
    setBusy(true);
    try {
      // Settings may have changed the grant since this screen opened: grant what the project holds now.
      const fresh = await api.projectEnroll(projectId);
      if (token !== generation) return;
      await api.projectGrant(projectId, fresh.mode, fresh.selfUpdate, fresh.revision);
      if (token !== generation) return;
      const missionId = await api.missionStart(projectId, text);
      if (token !== generation) return;
      selectedMission = missionId;
      saveMission(projectId, missionId);
      const view = await api.missionState(missionId);
      accept(view, token);
    } catch (err) {
      if (token !== generation) return;
      // The checks changed since they were confirmed, while this screen stayed open: show the new list.
      if (errorText(err) === 'reason.proof.unconfirmed') {
        await showChecks(projectId, token);
        return;
      }
      setError(errorText(err));
      // Refresh the grant revision before a retry; do not clear the person's goal.
      try { const next = await api.projectEnroll(projectId); if (token === generation) setEnroll(next); } catch {}
      // Goal text is preserved on failed start
    } finally {
      if (token === generation) setBusy(false);
    }
  };

  // The sheet confirms exactly the list the owner is shown (and the declaration it came from), then starts the mission it
  // was asked for. If the project's files changed meanwhile the engine refuses, and the sheet shows the new list instead.
  const confirmAndStart = async () => {
    const p = proof();
    if (!props.project || !p || busy()) return;
    const projectId = props.project.id;
    const token = generation;
    setBusy(true); setError(undefined);
    try {
      const next = await api.projectProofConfirm(projectId, p.commands, p.setup ?? [], p.declared);
      if (token !== generation) return;
      setProof(next);
      setAsking(false);
    } catch (err) {
      if (token === generation) {
        setError(errorText(err));
        await showChecks(projectId, token);
      }
      return;
    } finally { if (token === generation) setBusy(false); }
    if (token === generation && proof()?.confirmed) await startMission();
  };

  // The one line beside the start: why it waits, or what it will do.
  const status = () => {
    if (!enroll()) return busy() ? t('projects.status.reading') : undefined;
    if (busy()) return t('projects.status.starting');
    const p = proof();
    if (!p) return t('projects.status.reading');
    if (p.confirmed) return t('projects.status.ready');
    if (p.source === 'none' || p.commands.length === 0) return t('projects.status.none');
    if (askless()) return `${proofCount(p)} · ${t('projects.status.byMode')}`;
    return `${proofCount(p)} · ${t(p.changed ? 'projects.status.changed' : 'projects.status.atStart')}`;
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

  // A mission that does not move can be left for a new one; a paused or blocked one asks first, since nothing lists it later.
  const [leaving, setLeaving] = createSignal(false);
  const leavable = () => ['done', 'paused', 'blocked'].includes(mission()?.state ?? '');
  const leave = () => {
    const id = props.project?.id;
    if (id) forgetMission(id);
    generation++;
    selectedMission = undefined;
    setLeaving(false);
    setError(undefined);
    setMission(undefined);
    queueMicrotask(() => field?.focus());
  };
  const askLeave = () => (mission()?.state === 'done' ? leave() : setLeaving(true));
  createEffect(on(() => mission()?.missionId, () => setLeaving(false), { defer: true }));

  const humanDecisions = createMemo(() => {
    const m = mission();
    if (!m) return [];
    return m.decisions.filter((d) => isHumanDecision(d.kind));
  });

  const costDisplay = createMemo(() => {
    const m = mission();
    return formatCost(m?.cost.ownMicros);
  });
  // Only the development channel builds Faber locally; anywhere else there is no card at all.
  const build = (): BuildView | undefined => localBuild() ?? mission()?.build ?? undefined;

  // The first screen of a project: no mission yet, so the screen is a home, with its own light behind it.
  const home = () => !mission();
  const title = () => greetingParts(t('projects.home.title'));
  let root: HTMLDivElement | undefined;
  const [focused, setFocused] = createSignal(false);
  const running = () => props.running ?? [];
  const conversations = createMemo(() => pickUp((props.recent ?? []).filter((c) => c.area === 'project'), running(), 3));
  const projectName = (c: Summary) => (props.projects ?? []).find((p) => p.id === c.projectId)?.name ?? t('area.projects');
  // What the dust keeps clear of: the text blocks, so it never lies behind words.
  const avoid = () => (root ? [...root.querySelectorAll('.hello, .lead, .composer, .p-header, .p-resume, .p-sheet')] : []);

  // At most three, under the composer: the sidebar already lists every project, so this screen does not list them again.
  const pickUpList = () => (
    <Show when={conversations().length > 0 && props.onOpen}>
      <section class="p-resume" aria-labelledby="pickup-title">
        <h3 id="pickup-title">{t('s1.pickUp')}</h3>
        <For each={conversations()}>
          {(c) => <ResumeRow conversation={c} item={running().find((i) => i.conversationId === c.id)} where={projectName(c)} onOpen={props.onOpen!} />}
        </For>
      </section>
    </Show>
  );

  return (
    <div class="projects-root" ref={(el) => (root = el)}>
      {/* The effects behind a project's first screen (no beam of light here); a mission board has none. */}
      <Show when={home()}>
        <Ambient variant="project" vendors={props.vendors} avoid={avoid} />
        {/* A project's first screen is where a new session starts: in Dark Room the black hole sits behind it too. */}
        <Show when={props.project}>
          {/* It sits right of the column and runs behind it under a soft veil, so the words stay easy to read. */}
          <Hole avoid={() => (root ? [...root.querySelectorAll('.start-card .peek, .start-card .w-main')] : [])} />
        </Show>
      </Show>
      <div class="projects-stage" classList={{ home: home() }} ref={(el) => animateList(el)}>
        <Show
          when={props.project}
          fallback={
            <div class="welcome-grid">
              <Empty
                class="w-main"
                title={t('empty.openProjectTitle')}
                line={t('empty.openProjectBody')}
                button={{ label: t('side.addProject'), onClick: () => props.onAdd?.() }}
              >
                <Recents
                  mode="project"
                  onPick={(path) => void api.openFolders([path]).then(props.onOpened, (error) => props.onError?.(error))}
                  onOpened={props.onOpened}
                  onError={(error) => props.onError?.(error)}
                />
              </Empty>
              <Show when={conversations().length > 0}>
                <div class="p-side">{pickUpList()}</div>
              </Show>
            </div>
          }
        >
          <Show when={!mission() && localBuild()}>{(view) => <LocalBuild view={view()} />}</Show>
          <Show when={agents()}>
            {(a) => (
              <div class="note" classList={{ err: a().state === 'failed' }} role={a().state === 'failed' ? 'alert' : 'status'} data-testid="agents-status">
                {a().state === 'checking'
                  ? t('projects.agents.checking', { seconds: a().estimateSeconds, step: a().step, total: a().total })
                  : t('projects.agents.failed', { detail: fromEngine(a().detail ?? undefined) })}
              </div>
            )}
          </Show>
          <Show when={error()}>
            <div class="note err" role="alert">
              {fromEngine(error())}
            </div>
          </Show>

          <Show
            when={mission()}
            fallback={
              /* Start Card: the first screen of a project, in the family of the new conversation. Only the goal lives here;
                 every setting of the project is in Settings, Projects (ui/ProjectSettings.tsx). */
              <div class="start-card welcome-grid" data-testid="start-card">
                <div class="w-main">
                  <Peek />
                  <header class="p-header">
                    <h2>{props.project?.name}</h2>
                    <div class="p-meta">
                      <span title={enroll()?.repo ?? props.project?.path}>{tail(enroll()?.repo ?? props.project?.path ?? '')}</span>
                    </div>
                  </header>
                  <h1 class="display hello">
                    {title()[0]}
                    <Show when={title()[1]}>
                      {' '}
                      <em>{title()[1]}</em>
                    </Show>
                  </h1>
                  <p class="lead">{t('projects.home.lead')}</p>

                  {/* Goal composer, with the one quiet line of what the start will do beside its button. */}
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
                    <div class="cbar p-cbar">
                      <Show when={status()}>
                        {(line) => (
                          <span class="p-status" id="p-status" role="status" data-testid="p-status" title={line()}>
                            {line()}
                          </span>
                        )}
                      </Show>
                      <button
                        type="submit"
                        class="btn primary"
                        disabled={!goal().trim() || busy() || !enroll()}
                        aria-describedby={status() ? 'p-status' : undefined}
                      >
                        {t('projects.action.start')}
                        <Icon name="up" small />
                      </button>
                    </div>
                  </form>

                  {/* The checks, asked for just in time: the first start on checks the owner has not confirmed (R137). */}
                  <Show when={asking() && proof()}>
                    {(p) => (
                      <section
                        class="p-sheet"
                        role="dialog"
                        aria-labelledby="sheet-title"
                        data-testid="proof-sheet"
                        onKeyDown={(e) => e.key === 'Escape' && setAsking(false)}
                      >
                        <h3 id="sheet-title">{t('projects.sheet.title')}</h3>
                        <Show when={p().commands.length > 0} fallback={<p class="hint">{t('projects.sheet.none')}</p>}>
                          <p class="p-sheet-count">{proofCount(p())}</p>
                          <p class="hint">{t('projects.sheet.body')}</p>
                          <ProofList proof={p()} />
                        </Show>
                        <div class="proof-actions">
                          <button
                            type="button"
                            class="btn primary"
                            data-testid="proof-confirm"
                            ref={(el) => queueMicrotask(() => el.focus())}
                            disabled={busy() || p().commands.length === 0}
                            onClick={() => void confirmAndStart()}
                          >
                            {t('projects.sheet.go')}
                          </button>
                          <button type="button" class="btn" onClick={() => setAsking(false)}>
                            {t('projects.sheet.later')}
                          </button>
                        </div>
                      </section>
                    )}
                  </Show>
                  {pickUpList()}
                </div>
              </div>
            }
          >
            {/* Active Mission Board */}
            {(m) => (
              <div class="mission-board" data-testid="mission-board">
                {/* One calm block: where the mission stands, the goal, the step it is on, why it waits, and the one action that moves it. */}
                <header class="m-header">
                  <div class="m-top">
                    <span class="m-state" data-state={m().state} data-testid="mission-state">
                      <i aria-hidden="true" />
                      {fromEngine(`projects.state.${m().state}`)}
                    </span>
                    <Show when={leavable()}>
                      <button type="button" class="btn ghost m-new" disabled={busy()} onClick={() => askLeave()} data-testid="mission-leave">
                        <Icon name="plus" small />
                        {t('projects.board.new')}
                      </button>
                    </Show>
                  </div>
                  <h2 class="m-goal" title={m().goal}>{m().goal}</h2>
                  <Show when={m().progress}>
                    {(p) => (
                      <p class="m-progress" role="status" data-testid="mission-progress">
                        <span>{t('projects.progress.step', { number: p().number, total: p().total, step: t(`projects.progress.${p().step}` as any) })}</span>
                        <Show when={p().role && p().vendor}>
                          <span>{t('projects.progress.by', { role: t(`projects.role.${p().role}` as any), vendor: vendorName(p().vendor!) })}</span>
                        </Show>
                        <Show when={!costDisplay().isUnknown}>
                          <span>{t('projects.progress.cost', { cost: costDisplay().display })}</span>
                        </Show>
                      </p>
                    )}
                  </Show>
                  <Show when={m().reason && !m().needs}>
                    <p class="m-reason" role="status">
                      {fromEngine(m().reason ?? undefined)}
                    </p>
                  </Show>
                  <Show when={leaving()}>
                    <div class="m-leave" role="alertdialog" aria-labelledby="leave-text" onKeyDown={(e) => e.key === 'Escape' && setLeaving(false)}>
                      <p id="leave-text">{t('projects.board.leave')}</p>
                      <div class="proof-actions">
                        <button type="button" class="btn primary" ref={(el) => queueMicrotask(() => el.focus())} onClick={() => setLeaving(false)}>
                          {t('projects.board.stay')}
                        </button>
                        <button type="button" class="btn" data-testid="mission-leave-yes" onClick={() => leave()}>
                          {t('projects.board.leaveYes')}
                        </button>
                      </div>
                    </div>
                  </Show>
                  <Show when={!leaving() && (m().state === 'running' || m().state === 'paused' || m().state === 'blocked')}>
                    <div class="m-controls">
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
                  </Show>
                </header>

                {/* Amber Needs-You Decisions (R77, R92) */}
                <Show when={m().plan}><p class="note">{m().plan}</p></Show>
                <Show when={m().report}>
                  {(r) => (
                    <section class="m-report" aria-labelledby="report-title" data-testid="mission-report">
                      <h3 id="report-title">{t('projects.report.title')}</h3>
                      <p><b>{t('projects.report.branch', { branch: r().branch })}</b></p>
                      <Show when={r().head}>{(head) => <p class="note">{t('projects.report.head', { head: head().slice(0, 8) })}</p>}</Show>
                      <Show when={r().seconds != null}>
                        <p class="note">{t('projects.report.duration', { time: duration(r().seconds!) })}</p>
                      </Show>
                      <Show when={r().checks.length > 0}>
                        <h4>{t('projects.report.checks')}</h4>
                        <ul class="proof-list">
                          <For each={r().checks}>
                            {(c) => (
                              <li>
                                <code>{c.id}</code>:{' '}
                                {c.result === 'pass' ? t('projects.report.pass') : c.result === 'fail' ? t('projects.report.fail') : c.result}
                              </li>
                            )}
                          </For>
                        </ul>
                      </Show>
                      <p class="note">{t('projects.report.untouched')}</p>
                    </section>
                  )}
                </Show>
                <Show when={humanDecisions().length > 0 || m().needs}>
                  <section class="needs-you" role="region" aria-label={t('projects.needsYou')}>
                    <div class="ny-header">
                      <Icon name="ask" />
                      <b>{t('projects.needsYou')}</b>
                      <span class="note">{t('projects.needsYou.desc')}</span>
                    </div>
                    <Show when={m().needs}>
                      {(reason) => (
                        <div class="ask ny-card" role="alertdialog" aria-label={fromEngine(reason())} data-testid="mission-needs">
                          <div class="ny-question">
                            <b>{fromEngine(reason())}</b>
                          </div>
                          <p class="note">{t('projects.needs.desc')}</p>
                          <div class="ny-options">
                            <button type="button" class="btn primary" disabled={busy()} onClick={() => void resumeMission()}>
                              {t('projects.action.resume')}
                            </button>
                          </div>
                        </div>
                      )}
                    </Show>
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

                {/* The tasks, one list grouped by state, the groups that hold something only; before the plan, one quiet line. */}
                <section class="tasks-section" aria-labelledby="tasks-title">
                  <h3 class="m-section" id="tasks-title">
                    {t('projects.tasks.title')}
                    <Show when={m().tasks.length > 0}>
                      <span>{t('projects.tasks.count', { done: m().tasks.filter((task) => task.state === 'done').length, total: m().tasks.length })}</span>
                    </Show>
                  </h3>
                  <Show when={m().tasks.length === 0}>
                    <p class="m-empty">{t('projects.tasks.none')}</p>
                  </Show>
                  <div class="task-columns">
                    <For each={TASK_STATES}>
                      {(state) => {
                        const stateTasks = () => m().tasks.filter((task) => task.state === state);
                        return (
                          <Show when={stateTasks().length > 0}>
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
                          </Show>
                        );
                      }}
                    </For>
                  </div>
                </section>

                {/* What tunes or measures the mission, folded away: the work above stays the screen (C011). */}
                <details class="m-more">
                  <summary>
                    <Icon name="right" small />
                    {t('projects.board.more')}
                  </summary>
                  <div class="m-more-body">
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
                            <span class="chip">{vendorName(p.vendor)}</span>
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
                </details>
              </div>
            )}
          </Show>
        </Show>
      </div>
    </div>
  );
}
