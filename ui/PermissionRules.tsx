import { createSignal, For, onMount, Show } from 'solid-js';
import { api } from './api.ts';
import { Opt } from './controls.tsx';
import { fromEngine, t } from './i18n.ts';

/** Loaded only while its Settings fold is open. The engine returns only rules whose grant epoch still stands. */
export function PermissionRules() {
  const [rules, setRules] = createSignal<Awaited<ReturnType<typeof api.permissionRules>>>([]);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [problem, setProblem] = createSignal<string>();
  onMount(async () => {
    try { setRules(await api.permissionRules()); }
    catch (err) { setProblem(fromEngine(String(err))); }
    finally { setLoading(false); }
  });
  const revoke = async (id: string, tool: string) => {
    if (busy()) return;
    setBusy(true);
    setProblem(undefined);
    try {
      await api.revokePermissionRule(id, tool);
      setRules(await api.permissionRules());
    } catch (err) { setProblem(fromEngine(String(err))); }
    finally { setBusy(false); }
  };
  return (
    <div class="permission-rules" aria-busy={loading() || busy()}>
      <Show when={loading()}><p class="hint" role="status">{t('set.rules.loading')}</p></Show>
      <Show when={!loading() && !problem() && rules().length === 0}><p class="hint">{t('set.rules.empty')}</p></Show>
      <For each={rules()}>{rule => <section aria-label={rule.title}>
        <h4>{rule.title}</h4>
        <For each={rule.tools}>{tool => <Opt id={`grant-${rule.conversationId}-${tool}`} label={tool}>
          <button class="btn" disabled={busy()} aria-label={`${t('set.rules.remove')}: ${tool} (${rule.title})`} onClick={() => void revoke(rule.conversationId, tool)}>{t('set.rules.remove')}</button>
        </Opt>}</For>
      </section>}</For>
      <Show when={problem()}><p class="note err" role="alert">{problem()}</p></Show>
    </div>
  );
}
