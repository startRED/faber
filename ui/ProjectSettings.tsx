import { createSignal, For, type JSX, onCleanup, onMount, Show } from 'solid-js';
import { api, errorText, type GoalsView, type Project, type ProofView } from './api.ts';
import { Collapse, Opt, Toggle } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { fromEngine, money, t } from './i18n.ts';
import { animateList } from './motion.ts';
import { keep, kept, release, useDraft } from './unsent.ts';

// Settings, Projects: whether each project's conversations work in separate copies, and the checks Faber runs on their
// work. The project screen itself shows none of it.

/** The last two parts of a folder path: enough to tell two projects apart without the whole drive. */
export const tail = (path: string) => path.split(/[\\/]/).filter(Boolean).slice(-2).join(' / ');

/** How many checks a project has and where they were read from, in one short line. */
export const proofCount = (p: ProofView) =>
  p.source === 'none' || p.commands.length === 0 ? t('projects.status.none') : t(`projects.proof.count.${p.source}`, { n: p.commands.length });

/** A disclosure that keeps nothing in the page while it is closed. */
export function More(props: { label: string; open?: string; class?: string; children: JSX.Element }) {
  const [open, setOpen] = createSignal(false);
  return (
    <>
      <button type="button" class={props.class ?? 'more'} aria-expanded={open()} onClick={() => setOpen(!open())}>
        {open() && props.open ? props.open : props.label}
      </button>
      <Collapse open={open()}>{props.children}</Collapse>
    </>
  );
}

/** The project's checks, the setup first. */
export function ProofList(props: { proof: ProofView; setupOnly?: boolean }) {
  return (
    <div class="proof-cmds">
      <Show when={(props.proof.setup ?? []).length > 0}>
        <p class="hint">{t('projects.proof.setup')}</p>
        <ul class="proof-list" data-testid="proof-setup">
          <For each={props.proof.setup}>{(command) => <li><code>{command}</code></li>}</For>
        </ul>
      </Show>
      <Show when={!props.setupOnly}>
        <ul class="proof-list" aria-label={t('projects.proof.title')} data-testid="proof-commands">
          <For each={props.proof.commands}>{(command) => <li><code>{command}</code></li>}</For>
        </ul>
      </Show>
    </div>
  );
}

export function ProjectsSection(props: { projects: Project[] }) {
  return (
    <Show when={props.projects.length > 0} fallback={<p class="hint">{t('set.projects.none')}</p>}>
      <div class="psets" ref={animateList}>
        <For each={props.projects}>{(p) => <Entry project={p} />}</For>
      </div>
    </Show>
  );
}

/** One project, closed until it is opened; it reads its checks only then. It is a card of rows like the rest of
 * Settings. */
function Entry(props: { project: Project }) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="pset st-card" data-project={props.project.id}>
      <button type="button" class="pset-head" aria-expanded={open()} onClick={() => setOpen(!open())}>
        <span class="t">{props.project.name}</span>
        <span class="s" title={props.project.path}>{tail(props.project.path)}</span>
        <Icon name={open() ? 'down' : 'right'} small />
      </button>
      <Collapse open={open()}>
        <Body project={props.project} />
      </Collapse>
    </section>
  );
}

function Body(props: { project: Project }) {
  const [proof, setProof] = createSignal<ProofView>();
  // Unsent text in the proof editor is kept per project and blocks an unattended install, like the goal on the start screen.
  const proofKey = () => `proof.${props.project.id}`;
  const unsent = kept(proofKey());
  const [editing, setEditing] = createSignal(unsent !== '');
  const [draft, setDraft] = createSignal(unsent);
  useDraft(() => editing());
  onCleanup(() => release(proofKey()));
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  let gone = false;
  onCleanup(() => (gone = true));
  const id = () => props.project.id;

  onMount(() => {
    api.projectProof(id()).then((p) => !gone && setProof(p), (err) => !gone && setError(errorText(err)));
  });

  const run = async (work: () => Promise<void>) => {
    if (busy()) return;
    setBusy(true);
    setError(undefined);
    try {
      await work();
    } catch (err) {
      if (!gone) setError(errorText(err));
    } finally {
      if (!gone) setBusy(false);
    }
  };
  const confirm = (commands: string[]) =>
    run(async () => {
      const next = await api
        .projectProofConfirm(id(), commands, proof()?.setup ?? [], proof()?.declared ?? [])
        .catch(async (err) => {
          // The project's files moved on since the list was shown: drop the edit and show the new list and setup, in full,
          // before anything is confirmed. Edit starts again from them, so a draft never goes out against a list not seen.
          const latest = await api.projectProof(id()).catch(() => undefined);
          if (latest && !gone) {
            const seen = proof();
            const moved = JSON.stringify([seen?.declared, seen?.setup]) !== JSON.stringify([latest.declared, latest.setup]);
            setProof(latest);
            if (moved) {
              setEditing(false);
              keep(proofKey(), '');
            }
          }
          throw err;
        });
      if (gone) return;
      setProof(next);
      setEditing(false);
      keep(proofKey(), '');
    });
  // Whether each new conversation works in a separate copy; the conversations that already have one keep it.
  const [separate, setSeparate] = createSignal(props.project.separateCopies !== false);
  const copies = async (on: boolean) => {
    let saved = false;
    await run(async () => {
      await api.projectCopies(id(), on);
      if (!gone) setSeparate(on);
      saved = true;
    });
    return saved;
  };

  // Standing goals: the one switch that lets Faber start a fix by itself, with the day's limit always in view.
  const [goals, setGoals] = createSignal<GoalsView>();
  const [goalsError, setGoalsError] = createSignal<string>();
  onMount(() => {
    api.projectGoals(id()).then((g) => !gone && setGoals(g), (err) => !gone && setGoalsError(errorText(err)));
  });
  const goalsAuto = async (on: boolean) => {
    let saved = false;
    await run(async () => {
      const next = await api.projectGoalsAuto(id(), on);
      if (!gone) setGoals(next);
      saved = true;
    });
    return saved;
  };
  const goalsHint = () => {
    const g = goals();
    if (g) return t('set.projects.goalsHint', { cap: money(g.capUsd, 0), spent: money(g.spentUsd) });
    // A read that failed says why in the row, instead of a switch that stays off and says nothing.
    const failed = goalsError();
    return failed ? fromEngine(failed) : undefined;
  };

  const checksHint = () => {
    const p = proof();
    if (!p) return t('projects.status.reading');
    if (p.commands.length === 0) return t('set.projects.noChecks');
    return `${proofCount(p)} · ${p.confirmed ? t('projects.proof.confirmed') : t('projects.proof.unconfirmed')}`;
  };
  const row = (name: string) => `pset-${props.project.id}-${name}`;

  return (
    <div class="pset-body" ref={animateList}>
      <Show when={error()}>
        <p class="note err" role="alert">{fromEngine(error())}</p>
      </Show>
      <Toggle
        id={row('copies')}
        label={t('set.projects.copies')}
        more={[t('set.projects.copiesDesc'), t('set.projects.copiesTech')]}
        checked={separate()}
        onChange={copies}
      />
      <Toggle
        id={row('goals')}
        label={t('set.projects.goals')}
        hint={goalsHint()}
        more={[t('set.projects.goalsDesc'), t('set.projects.goalsTech')]}
        checked={goals()?.auto === true}
        disabled={!goals()}
        onChange={goalsAuto}
      />
      <div class="pset-checks" data-testid="pset-checks" ref={animateList}>
        <Opt id={row('checks')} label={t('set.projects.checks')} hint={checksHint()} more={[t('projects.proof.desc')]}>
          <Show when={proof()}>
            {(p) => (
              <span class="pset-acts">
                <Show when={!p().confirmed && p().commands.length > 0 && !editing()}>
                  <button type="button" class="btn primary" data-testid="proof-confirm" disabled={busy()} onClick={() => void confirm(p().declared.length ? p().declared : p().commands)}>
                    {t('projects.proof.confirm')}
                  </button>
                </Show>
                <Show when={!editing()}>
                  <button type="button" class="btn" data-testid="proof-edit" disabled={busy()} onClick={() => { setDraft(p().commands.join('\n')); setEditing(true); }}>
                    {t('projects.proof.edit')}
                  </button>
                </Show>
              </span>
            )}
          </Show>
        </Opt>
        <Show when={proof()}>
          {(p) => (
            <div class="st-extra" ref={animateList}>
              <Show when={p().changed}>
                <p class="hint" role="status">{t('projects.proof.changed')}</p>
              </Show>
              <Show
                when={editing()}
                fallback={
                  // Confirm is pressable only while the list and the setup it sends are on screen; a confirmed list may fold.
                  <Show when={p().commands.length > 0}>
                    <Show when={p().confirmed} fallback={<ProofList proof={p()} />}>
                      <More label={t('projects.proof.view')} open={t('projects.proof.hide')}>
                        <ProofList proof={p()} />
                      </More>
                    </Show>
                  </Show>
                }
              >
                <ProofList proof={p()} setupOnly />
                <label class="sr-only" for={`proof-draft-${id()}`}>{t('projects.proof.edit.label')}</label>
                <textarea
                  id={`proof-draft-${id()}`}
                  class="field proof-draft"
                  data-testid="proof-draft"
                  rows={Math.min(10, Math.max(3, draft().split('\n').length))}
                  value={draft()}
                  onInput={(e) => (setDraft(e.currentTarget.value), keep(proofKey(), e.currentTarget.value))}
                />
                <span class="pset-acts">
                  <button type="button" class="btn primary" data-testid="proof-save" disabled={busy()} onClick={() => void confirm(draft().split('\n').map((l) => l.trim()).filter(Boolean))}>
                    {t('projects.proof.save')}
                  </button>
                  <button type="button" class="btn" disabled={busy()} onClick={() => (setEditing(false), keep(proofKey(), ''))}>
                    {t('projects.proof.cancel')}
                  </button>
                </span>
              </Show>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
