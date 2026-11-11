import { createSignal, Show } from 'solid-js';
import type { Message, Stall } from './api.ts';
import { fromEngine, t } from './i18n.ts';
import { stopping as stopped, stopRun } from './stopping.ts';
import { StopAsk } from './Composer.tsx';
import { helpersAtWork } from './worklog.ts';

/** What the sign says: how long a step or a compaction has gone without news, or what the agent keeps doing. */
export const stallText = (stall: Stall): string => t(`stall.${stall.kind}` as const, { n: stall.minutes ?? 0 });

/** The agent's own work seems stuck (task 110): one amber line under the reply, like a stuck helper's, and Stop. Nothing
 * stopped by itself, so the person decides; Stop asks first when helpers would end with it. */
export function StallNote(props: { conversationId: string; m: Message }) {
  const [stopping, setStopping] = createSignal(false);
  let stopButton: HTMLButtonElement | undefined;
  const helpers = () => helpersAtWork([props.m]);
  const [error, setError] = createSignal<string>();
  const stop = () => void stopRun(props.conversationId).catch((e) => setError(fromEngine(String(e))));
  return (
    <Show when={props.m.stall}>
      {(stall) => (
        <div class="agent-stuck stall" role="status">
          <span class="agent-why">{stallText(stall())}</span>
          <Show
            when={stopping() && helpers()}
            fallback={
              <button ref={(el) => (stopButton = el)} type="button" class="btn" disabled={stopped(props.conversationId)} onClick={() => (helpers() ? setStopping(true) : stop())}>
                {t(stopped(props.conversationId) ? 'commands.stopping' : 'composer.stop')}
              </button>
            }
          >
            <StopAsk helpers={helpers()} onStop={stop} onKeep={() => setStopping(false)} back={() => stopButton} />
          </Show>
          <Show when={error()}><span class="agent-why" role="alert">{error()}</span></Show>
        </div>
      )}
    </Show>
  );
}
