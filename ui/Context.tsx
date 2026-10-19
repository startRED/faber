import type { ContextFill } from './api.ts';
import { near, percentOf } from './context.ts';
import { amount, t } from './i18n.ts';

/** A thin ring and a percent: how full the open conversation's own agent's context is. Status, so it only shows what is known
 * (the caller shows it once the CLI has said both numbers); the tooltip carries the tokens. It never moves and holds no action. */
export function ContextMeter(props: { fill: ContextFill }) {
  const pct = () => percentOf(props.fill);
  const tip = () => t('ctx.tip', { used: amount(props.fill.used, 0), window: amount(props.fill.window, 0) });
  return (
    <span class="ctx" classList={{ near: near(props.fill) }} role="img" aria-label={tip()} title={tip()} data-testid="context">
      <svg class="ctx-ring" viewBox="0 0 16 16" aria-hidden="true">
        <circle class="trk" cx="8" cy="8" r="6" />
        <circle class="arc" cx="8" cy="8" r="6" pathLength="100" stroke-dasharray={`${pct()} 100`} transform="rotate(-90 8 8)" />
      </svg>
      <span class="ctx-pct">{t('ctx.share', { pct: pct() })}</span>
    </span>
  );
}
