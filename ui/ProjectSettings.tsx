import { createSignal, For, type JSX, onCleanup, onMount, Show } from 'solid-js';
import { api, errorText, type EnrollView, type Project, type ProofView } from './api.ts';
import { Toggle } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';

// Settings, Projects: what Faber may do in each project and the checks it proves its work with. The project screen itself
// shows none of it; it asks for the checks only once, the first time a mission starts (ui/Projects.tsx).

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
      <Show when={open()}>{props.children}</Show>
    </>
  );
}

/** The commands a mission runs, the setup first. */
export function ProofList(props: { proof: ProofView }) {
  return (
    <div class="proof-cmds">
      <Show when={(props.proof.setup ?? []).length > 0}>
        <p class="hint">{t('projects.proof.setup')}</p>
        <ul class="proof-list" data-testid="proof-setup">
          <For each={props.proof.setup}>{(command) => <li><code>{command}</code></li>}</For>
        </ul>
      </Show>
      <ul class="proof-list" aria-label={t('projects.proof.title')} data-testid="proof-commands">
        <For each={props.proof.commands}>{(command) => <li><code>{command}</code></li>}</For>
      </ul>
    </div>
  );
}

export function ProjectsSection(props: { projects: Project[] }) {
  return (
    <Show when={props.projects.length > 0} fallback={<p class="hint">{t('set.projects.none')}</p>}>
      <div class="psets">
        <For each={props.projects}>{(p) => <Entry project={p} />}</For>
      </div>
    </Show>
  );
}

/** One project, closed until it is opened; it reads its checks and its grant only then. */
function Entry(props: { project: Project }) {
  const [open, setOpen] = createSignal(false);
  return (
    <section class="pset" data-project={props.project.id}>
      <button type="button" class="pset-head" aria-expanded={open()} onClick={() => setOpen(!open())}>
        <span class="t">{props.project.name}</span>
        <span class="s" title={props.project.path}>{tail(props.project.path)}</span>
        <Icon name={open() ? 'up' : 'down'} small />
      </button>
      <Show when={open()}>
        <Body project={props.project} />
      </Show>
    </section>
  );
}

function Body(props: { project: Project }) {
  const [proof, setProof] = createSignal<ProofView>();
  const [enroll, setEnroll] = createSignal<EnrollView>();
  const [editing, setEditing] = createSignal(false);
  const [draft, setDraft] = createSignal('');
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal<string>();
  let gone = false;
  onCleanup(() => (gone = true));
  const id = () => props.project.id;

  onMount(() => {
    api.projectProof(id()).then((p) => !gone && setProof(p), (err) => !gone && setError(errorText(err)));
    api.projectEnroll(id()).then((e) => !gone && setEnroll(e), (err) => !gone && setError(errorText(err)));
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
      const next = await api.projectProofConfirm(id(), commands, proof()?.setup ?? []);
      if (gone) return;
      setProof(next);
      setEditing(false);
    });
  // The grant keeps the mode and the self-update trust; it is written again with the new choice and a fresh revision.
  const trust = async (on: boolean) => {
    let saved = false;
    await run(async () => {
      const fresh = await api.projectEnroll(id());
      await api.projectGrant(id(), fresh.mode, on, fresh.revision);
      if (!gone) setEnroll({ ...fresh, selfUpdate: on, revision: fresh.revision });
      saved = true;
    });
    return saved;
  };

  return (
    <div class="pset-body">
      <Show when={error()}>
        <p class="note err" role="alert">{fromEngine(error())}</p>
      </Show>
      <div class="row">
        <span>{t('set.projects.checks')}</span>
        <div class="pset-line" data-testid="pset-checks">
          <Show when={proof()} fallback={<span class="hint">{t('projects.status.reading')}</span>}>
            {(p) => (
              <>
                <span class="v">
                  {proofCount(p())}
                  <Show when={p().commands.length > 0}>
                    {' · '}
                    {p().confirmed ? t('projects.proof.confirmed') : t('projects.proof.unconfirmed')}
                  </Show>
                </span>
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
                <Show when={p().changed}>
                  <p class="hint" role="status">{t('projects.proof.changed')}</p>
                </Show>
                <Show
                  when={editing()}
                  fallback={
                    <Show when={p().commands.length > 0}>
                      <More label={t('projects.proof.view')} open={t('projects.proof.hide')}>
                        <ProofList proof={p()} />
                      </More>
                    </Show>
                  }
                >
                  <label class="sr-only" for={`proof-draft-${id()}`}>{t('projects.proof.edit.label')}</label>
                  <textarea
                    id={`proof-draft-${id()}`}
                    class="proof-draft"
                    data-testid="proof-draft"
                    rows={Math.min(10, Math.max(3, draft().split('\n').length))}
                    value={draft()}
                    onInput={(e) => setDraft(e.currentTarget.value)}
                  />
                  <span class="pset-acts">
                    <button type="button" class="btn primary" data-testid="proof-save" disabled={busy()} onClick={() => void confirm(draft().split('\n').map((l) => l.trim()).filter(Boolean))}>
                      {t('projects.proof.save')}
                    </button>
                    <button type="button" class="btn" disabled={busy()} onClick={() => setEditing(false)}>
                      {t('projects.proof.cancel')}
                    </button>
                  </span>
                </Show>
                <More label={t('set.more')}>
                  <p class="hint">{t('projects.proof.desc')}</p>
                  <p class="hint">{t('projects.proofs.isolationTech')}</p>
                </More>
              </>
            )}
          </Show>
        </div>
      </div>
      <div class="row">
        <span>{t('set.projects.tools')}</span>
        <div class="pset-line">
          <span class="v">{t('projects.presence.continueWithout')}</span>
          <More label={t('set.more')}>
            <p class="hint">{t('reason.kit.continueWithout')}</p>
            <p class="hint">{t('reason.kit.findToolsInstallParked')}</p>
          </More>
        </div>
      </div>
      <div class="row">
        <span>{t('set.projects.code')}</span>
        <div class="pset-line">
          <span class="v">{enroll()?.mode === 'equilibrado' ? t('projects.gitMode.equilibrado') : t('projects.gitMode.seguro')}</span>
          <More label={t('set.more')}>
            <p class="hint">{enroll()?.mode === 'equilibrado' ? t('projects.gitMode.equilibradoDesc') : t('projects.gitMode.seguroDesc')}</p>
            <p class="hint">{t('projects.gitMode.seguroTech')}</p>
            <p class="hint">
              {t('projects.repo')}: {enroll()?.repo ?? props.project.path}
            </p>
            <Show when={enroll()?.base}>{(base) => <p class="hint">{t('projects.base')}: {base()}</p>}</Show>
            <Show when={enroll()?.author}>{(a) => <p class="hint">{t('projects.author')}: {a().name} &lt;{a().email}&gt;</p>}</Show>
          </More>
        </div>
      </div>
      <div class="row">
        <span>{t('set.projects.faber')}</span>
        <div class="pset-line">
          <Show when={enroll()} fallback={<span class="hint">{t('projects.status.reading')}</span>}>
            {(e) => <Toggle label={t('projects.selfUpdate.trust')} checked={e().selfUpdate} onChange={trust} />}
          </Show>
          <More label={t('set.more')}>
            <p class="hint">{t('projects.selfUpdate.trustDesc')}</p>
            <p class="hint">{t('projects.selfUpdate.trustTech')}</p>
          </More>
        </div>
      </div>
    </div>
  );
}
