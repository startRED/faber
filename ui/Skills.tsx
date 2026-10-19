import { createResource, createSignal, For, Show } from 'solid-js';
import { api, type ToolRow } from './api.ts';
import { Collapse, Toggle } from './controls.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import type { Key } from './locales/en.ts';

/**
 * The catalog's skills in Settings > Projects (R247): one card, closed like a project's, that says how many are on and
 * opens to one switch each. On downloads the skill from its own repository and checks its files; off removes it. A
 * project's agent gets them from its next conversation, on both AIs.
 */
export function SkillsCard(props: { onError?: (error: unknown) => void }) {
  const [rows, { mutate }] = createResource(() => api.toolsList().catch(() => [] as ToolRow[]));
  const [open, setOpen] = createSignal(false);
  const [busy, setBusy] = createSignal<string>();
  const on = () => rows()?.filter((r) => r.installed).length ?? 0;
  const flip = async (row: ToolRow, next: boolean): Promise<boolean> => {
    setBusy(row.id);
    try {
      await (next ? api.toolsInstall(row.id) : api.toolsRemove(row.id));
      mutate((list) => list?.map((r) => (r.id === row.id ? { ...r, installed: next } : r)));
      return true;
    } catch (err) {
      props.onError?.(err);
      return false;
    } finally {
      setBusy(undefined);
    }
  };
  return (
    <Show when={rows()?.length}>
      <section class="pset st-card" data-skills="">
        <button type="button" class="pset-head" aria-expanded={open()} onClick={() => setOpen(!open())}>
          <span class="t">{t('set.group.skills')}</span>
          <span class="s">{t('set.skills.count', { on: on(), all: rows()?.length ?? 0 })}</span>
          <Icon name={open() ? 'down' : 'right'} small />
        </button>
        <Collapse open={open()}>
          <div class="pset-body">
            <p class="hint st-note">{t('set.skills.line')}</p>
            <For each={rows()}>
              {(row) => (
                <Toggle
                  id={`set-skill-${row.id}`}
                  label={row.id}
                  hint={busy() === row.id ? t(row.installed ? 'set.skills.removing' : 'set.skills.installing') : `${t(`tools.${row.id}` as Key)} · ${row.license}`}
                  more={t('set.skills.from', { repo: row.repo, license: row.license })}
                  checked={row.installed}
                  disabled={busy() !== undefined}
                  onChange={(next) => flip(row, next)}
                />
              )}
            </For>
          </div>
        </Collapse>
      </section>
    </Show>
  );
}
