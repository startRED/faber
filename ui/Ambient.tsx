import { createEffect, For, onCleanup, onMount, Show } from 'solid-js';
import type { Motion, Vendor } from './api.ts';
import { appliedLook, motion, windowHidden } from './appearance.ts';
import { t } from './i18n.ts';
import { vendorName } from './models.ts';

// The effects of the new conversation (docs/design.md, "Ambient"). Which one shows comes from the look: the tokens
// --amb-beam, --amb-loom and --amb-flow are `none` for a theme that does not have it, and for every theme when the
// ambient switch is off. Effects sit behind the greeting and never behind text the user reads.

/** Effect loops draw only at full motion, with the ambient switch on and the window visible. */
export const shouldRun = (state: { motion: Motion; ambient: boolean; hidden: boolean }) =>
  state.motion === 'full' && state.ambient && !state.hidden;

/** A frame loop at a fixed rate. A check passes its own `schedule` and `cancel` to step it by hand. */
export function createLoop(
  draw: (elapsed: number) => void,
  fps = 24,
  schedule: (step: FrameRequestCallback) => number = requestAnimationFrame,
  cancel: (id: number) => void = cancelAnimationFrame,
) {
  const interval = 1000 / fps;
  let id = 0;
  let drawn = 0; // when the last frame was drawn
  let due = 0; // when the next one is due
  const step: FrameRequestCallback = (now) => {
    id = schedule(step);
    // A frame a millisecond early still counts; without that a 60 Hz screen would settle at 20 draws a second.
    if (now < due - 1) return;
    const elapsed = Math.min(now - drawn, 100);
    drawn = now;
    // The time past the due moment carries over, so the rate holds; after a long stall the count starts afresh.
    due = now - due > interval ? now + interval : due + interval;
    draw(elapsed);
  };
  return {
    run(on: boolean) {
      if (on && !id) {
        drawn = performance.now();
        due = drawn + interval;
        id = schedule(step);
      } else if (!on && id) {
        cancel(id);
        id = 0;
      }
    },
    running: () => id !== 0,
  };
}

const GRAINS = 72;
const FADE = 28; // a grain fades to nothing within this many pixels of a text block
const TILT = (22 * Math.PI) / 180;
const [COS, SIN] = [Math.cos(TILT), Math.sin(TILT)];
const [CANVAS_W, CANVAS_H] = [760, 320];
// The beam leaves the top of the greeting area, 80 px above it, 120 px in from the right edge but never past 740 px.
const [BEAM_TOP, BEAM_RIGHT, BEAM_MAX] = [-80, 120, 740];

const THROUGH_CLAUDE = 'M40 100H86Q102 100 102 84V58Q102 46 116 46H146M206 46H226Q240 46 240 60V86Q240 100 254 100H292';
const THROUGH_CODEX = 'M40 100H86Q102 100 102 116V142Q102 154 116 154H146M206 154H226Q240 154 240 140V114Q240 100 254 100H292';
const STRAIGHT = 'M40 100H292';
const BOTH =
  'M40 100H86Q102 100 102 84V58Q102 46 116 46H146M102 100V142Q102 154 116 154H146M206 46H226Q240 46 240 60V140Q240 154 226 154H206M240 100H292';

type Box = { l: number; t: number; r: number; b: number };

/** The loom's thread rows, bottom to top, each a little fainter than the one below. */
const ROWS = Array.from({ length: 10 }, (_, j) => ({ y: 600 - 16 * j, odd: j % 2 === 1, opacity: (0.25 + 0.75 * Math.min(1, (16 * j) / 120) * 0.7).toFixed(2) }));

export function Ambient(props: { vendors: Vendor[]; avoid: () => Element[] }) {
  let layer!: HTMLDivElement;
  let canvas!: HTMLCanvasElement;
  let boxes: Box[] = [];
  let width = 0;
  let color = '243,238,226';
  let seed = 7;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const grains = Array.from({ length: GRAINS }, () => ({
    y: random() * CANVAS_H,
    u: random() * 2 - 1,
    r: 0.5 + random() * 1.3,
    vy: -(0.04 + random() * 0.12),
    phase: random() * 6.28,
    speed: 0.4 + random() * 0.8,
  }));
  let time = 0;

  const clearance = (x: number, y: number) => {
    let k = 1;
    for (const b of boxes) k = Math.min(k, Math.hypot(Math.max(b.l - x, 0, x - b.r), Math.max(b.t - y, 0, y - b.b)) / FADE);
    return Math.min(1, k);
  };

  const paint = () => {
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, CANVAS_W, CANVAS_H);
    const originX = Math.min(width - BEAM_RIGHT, BEAM_MAX);
    for (const g of grains) {
      const x = CANVAS_W / 2 + g.u * (34 + g.y * 0.42) * 0.92 + Math.sin(time * 0.0004 * g.speed + g.phase) * 6;
      const local = x - CANVAS_W / 2;
      const k = clearance(originX + local * COS - g.y * SIN, BEAM_TOP + local * SIN + g.y * COS);
      if (k <= 0) continue;
      const alpha = (0.25 + 0.55 * (0.5 + 0.5 * Math.sin(time * 0.0011 * g.speed + g.phase))) * (1 - Math.abs(g.u) * 0.6) * k;
      ctx.beginPath();
      ctx.arc(x, g.y, g.r, 0, 6.283);
      ctx.fillStyle = `rgba(${color},${alpha.toFixed(3)})`;
      ctx.fill();
    }
  };

  const loop = createLoop((elapsed) => {
    time += elapsed;
    for (const g of grains) {
      g.y += g.vy * elapsed * 0.06;
      if (g.y < -4) {
        g.y = CANVAS_H + 4;
        g.u = random() * 2 - 1;
      }
    }
    paint();
  });

  // Where the text blocks are, in the coordinates of the greeting area.
  const measure = () => {
    const area = layer.parentElement?.getBoundingClientRect();
    if (!area) return;
    width = area.width;
    boxes = props.avoid().map((e) => {
      const r = e.getBoundingClientRect();
      return { l: r.left - area.left, t: r.top - area.top, r: r.right - area.left, b: r.bottom - area.top };
    });
    paint();
  };

  onMount(() => {
    const resize = new ResizeObserver(measure);
    if (layer.parentElement) resize.observe(layer.parentElement);
    void document.fonts.ready.then(measure);
    onCleanup(() => {
      resize.disconnect();
      loop.run(false);
    });
  });

  createEffect(() => {
    const look = appliedLook();
    if (!look) return;
    color = look.vars.dust ?? color;
    const shown = look.vars['amb-beam'] !== 'none';
    loop.run(shown && shouldRun({ motion: motion(), ambient: look.effects.ambient, hidden: windowHidden() }));
    if (layer) measure();
  });

  const claude = () => props.vendors.includes('claude');
  const codex = () => props.vendors.includes('codex');
  // The path from the idea to the result runs through each connected vendor, and only those.
  const trace = () => (claude() ? THROUGH_CLAUDE : codex() ? THROUGH_CODEX : STRAIGHT);

  return (
    <div ref={(el) => (layer = el)} class="ambient" aria-hidden="true">
      <div class="amb amb-beam">
        <div class="beam">
          <i />
          <i />
        </div>
        <canvas ref={(el) => (canvas = el)} width={CANVAS_W} height={CANVAS_H} />
      </div>
      <div class="amb amb-glow">
        <div class="amb-beam">
          <div class="beam">
            <i />
            <i />
          </div>
        </div>
      </div>
      <div class="amb amb-loom">
        <svg viewBox="0 0 660 600">
          <g class="loom-warp">
            <For each={Array.from({ length: 42 }, (_, i) => 8 + 16 * i)}>{(x) => <line x1={x} x2={x} y1={400} y2={600} />}</For>
          </g>
          <g class="loom-cloth">
            <For each={ROWS}>{(row) => <line x1={0} x2={660} y1={row.y} y2={row.y} style={{ 'stroke-dashoffset': row.odd ? 16 : 0, opacity: row.opacity }} />}</For>
          </g>
          <line class="loom-row" x1={0} x2={660} y1={440} y2={440} style={{ 'stroke-dashoffset': 16 }} />
          <rect class="loom-shuttle" x={-18} y={437.5} width={18} height={5} rx={2.5} />
        </svg>
      </div>
      <div class="amb amb-flow">
        <svg viewBox="0 0 340 200">
          <path class="flow-path" d={claude() && codex() ? BOTH : trace()} />
          <path class="flow-trace" d={trace()} />
          <circle class="flow-node" cx={30} cy={100} r={10} />
          <circle class="flow-node" cx={302} cy={100} r={10} />
          <Show when={claude()}>
            <rect x={166} y={38} width={16} height={16} rx={1.5} style={{ fill: 'var(--claude)' }} />
            <text class="flow-label" x={174} y={26} text-anchor="middle" style={{ fill: 'var(--claude-ink)' }}>
              {vendorName('claude')}
            </text>
          </Show>
          <Show when={codex()}>
            <rect x={166} y={146} width={16} height={16} rx={1.5} style={{ fill: 'var(--codex)' }} transform="rotate(45 174 154)" />
            <text class="flow-label" x={174} y={190} text-anchor="middle" style={{ fill: 'var(--codex-ink)' }}>
              {vendorName('codex')}
            </text>
          </Show>
          <path d="M297 100l4 4 7-8" style={{ fill: 'none', stroke: 'var(--accent)' }} stroke-width={1.8} stroke-linecap="round" />
          <text class="flow-label" x={30} y={132} text-anchor="middle">
            {t('ambient.idea')}
          </text>
          <text class="flow-label" x={302} y={132} text-anchor="middle">
            {t('ambient.done')}
          </text>
        </svg>
      </div>
      <div class="grain" />
    </div>
  );
}
