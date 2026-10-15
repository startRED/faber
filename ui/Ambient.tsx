import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { Vendor } from './api.ts';
import { appliedLook } from './appearance.ts';
import { t } from './i18n.ts';
import { vendorName } from './models.ts';

// The effects of the new conversation (docs/design.md, "Ambient"). Which one shows comes from the look: the tokens
// --amb-beam, --amb-loom and --amb-flow are `none` for a theme that does not have it, and for every theme when the
// ambient switch is off. Effects sit behind the greeting and never behind text the user reads. Nothing here runs a frame
// loop: the beam and the dust are CSS, and the dust only twinkles (opacity of three layers, on the compositor).

const GRAINS = 72;
const FADE = 28; // a grain fades to nothing within this many pixels of a text block
const TILT = (22 * Math.PI) / 180;
const [COS, SIN] = [Math.cos(TILT), Math.sin(TILT)];
const [DUST_W, DUST_H] = [760, 320];
// The beam leaves the top of the greeting area, 80 px above it, 120 px in from the right edge but never past 740 px.
const [BEAM_TOP, BEAM_RIGHT, BEAM_MAX] = [-80, 120, 740];

export type Box = { l: number; t: number; r: number; b: number };
export type Grain = { x: number; y: number; r: number; alpha: number };

/** The grains of the dust, in the coordinates of the beam, from a fixed seed so the picture is the same at every launch. The
 * beam is narrow at the lens and wide at the bottom, so the grains spread with their depth. */
export function makeGrains(count = GRAINS): Grain[] {
  let seed = 7;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  return Array.from({ length: count }, () => {
    const y = random() * DUST_H;
    const u = random() * 2 - 1;
    const x = DUST_W / 2 + u * (34 + y * 0.42) * 0.92;
    return { x, y, r: 0.5 + random() * 1.3, alpha: (0.25 + 0.55 * random()) * (1 - Math.abs(u) * 0.6) };
  });
}

/** How clear of every text block a point is: 0 on a block, 1 at `FADE` pixels from the nearest one. */
export function clearance(boxes: Box[], x: number, y: number): number {
  let k = 1;
  for (const b of boxes) k = Math.min(k, Math.hypot(Math.max(b.l - x, 0, x - b.r), Math.max(b.t - y, 0, y - b.b)) / FADE);
  return Math.min(1, k);
}

/** The dust as `box-shadow` lists, one per twinkling layer. Every grain is a shadow of a 1 px dot; a grain within `FADE` of a
 * text block is dimmed to nothing, so the dust never lies behind words. `originX` is where the beam starts, in the
 * coordinates of the greeting area. */
export function dustShadows(grains: Grain[], boxes: Box[], originX: number, color: string, layers = 3): string[] {
  const out: string[][] = Array.from({ length: layers }, () => []);
  grains.forEach((g, i) => {
    const local = g.x - DUST_W / 2;
    // Where the grain lands on the screen: the beam is turned by 22 degrees around its top centre.
    const k = clearance(boxes, originX + local * COS - g.y * SIN, BEAM_TOP + local * SIN + g.y * COS);
    if (k <= 0) return;
    out[i % layers]!.push(`${g.x.toFixed(1)}px ${g.y.toFixed(1)}px 0 ${(g.r - 0.5).toFixed(2)}px rgba(${color},${(g.alpha * k).toFixed(3)})`);
  });
  return out.map((list) => list.join(','));
}

const THROUGH_CLAUDE = 'M40 100H86Q102 100 102 84V58Q102 46 116 46H146M206 46H226Q240 46 240 60V86Q240 100 254 100H292';
const THROUGH_CODEX = 'M40 100H86Q102 100 102 116V142Q102 154 116 154H146M206 154H226Q240 154 240 140V114Q240 100 254 100H292';
const STRAIGHT = 'M40 100H292';
const BOTH =
  'M40 100H86Q102 100 102 84V58Q102 46 116 46H146M102 100V142Q102 154 116 154H146M206 46H226Q240 46 240 60V140Q240 154 226 154H206M240 100H292';

/** The loom's thread rows, bottom to top, each a little fainter than the one below. */
const ROWS = Array.from({ length: 10 }, (_, j) => ({ y: 600 - 16 * j, odd: j % 2 === 1, opacity: (0.25 + 0.75 * Math.min(1, (16 * j) / 120) * 0.7).toFixed(2) }));

export function Ambient(props: { vendors: Vendor[]; avoid: () => Element[] }) {
  let layer!: HTMLDivElement;
  let boxes: Box[] = [];
  let width = 0;
  let color = '243,238,226';
  const grains = makeGrains();
  const [shadows, setShadows] = createSignal<string[]>(['', '', '']);

  const paint = () => setShadows(dustShadows(grains, boxes, Math.min(width - BEAM_RIGHT, BEAM_MAX), color));

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
    onCleanup(() => resize.disconnect());
  });

  createEffect(() => {
    const look = appliedLook();
    if (!look) return;
    color = look.vars.dust ?? color;
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
        <div class="dust" aria-hidden="true">
          <For each={shadows()}>{(shadow) => <i style={{ 'box-shadow': shadow }} />}</For>
        </div>
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
