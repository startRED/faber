import { createEffect, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import type { Vendor } from './api.ts';
import { appliedLook, motion, windowFocused, windowHidden } from './appearance.ts';
import { t } from './i18n.ts';
import { vendorName } from './models.ts';

// The effects of the new conversation (docs/design.md, "Ambient"). Which one shows comes from the look: the tokens
// --amb-beam, --amb-loom, --amb-flow and --amb-glyphs are `none` for a theme that does not have it, and for every theme
// when the ambient switch is off. Effects sit behind the greeting and never behind text the user reads. The beam and the
// dust are CSS, and the dust only twinkles (opacity of three layers, on the compositor); the glyph field is the one
// canvas loop.

// The dust's three layers and the glow breathe out of step, eased in and out. A timer sets their opacity ten times a
// second: as CSS animations they made a 240 Hz screen composite the stage on every frame, about half a core.
const BREATHS = [
  { period: 6000, offset: 0, from: 1, to: 0.4 },
  { period: 8000, offset: 3000, from: 1, to: 0.4 },
  { period: 10000, offset: 6000, from: 1, to: 0.4 },
  { period: 7000, offset: 0, from: 0, to: 0.5 }, // the glow
];
const BREATH_STEP = 100;

/** How far a breath of `period` ms is, `t` ms in: from 0 to 1 and back, eased in and out. */
export function breath(t: number, period: number): number {
  const x = 1 - Math.abs(((t / period) % 2) - 1);
  return x < 0.5 ? 4 * x ** 3 : 1 - (-2 * x + 2) ** 3 / 2;
}

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

// The glyph field: a faint grid of letters and digits on one canvas. A few cells change every `TICK` ms, and a click on
// an empty spot sends a ring that tints the cells it crosses with the accent. Only cells that change are repainted, the
// loop sleeps between ticks, and it stops entirely when the window is hidden or unfocused or motion is not `full`.
const CELL = 16;
const CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789';
const TICK = 140;
// A ripple repaints at most this often: a ring moves less than a cell between two paints, and a 240 Hz screen would
// otherwise composite the stage 240 times a second.
const RIPPLE_STEP = 33;
const [RIPPLE_MS, RIPPLE_SPEED, RIPPLE_BAND] = [1100, 0.34, 34];
const RIPPLE_REACH = RIPPLE_MS * RIPPLE_SPEED + RIPPLE_BAND;
// Clicks on these keep their own meaning and send no ripple.
const BUSY = 'button, a, input, textarea, select, label, [contenteditable], .composer, .try, .resume, .runcol';

/** How strongly a ripple tints a cell `d` px from the click, `age` ms after it: a band that travels out and fades, 0 to 1. */
export function rippleTint(d: number, age: number): number {
  if (age < 0 || age >= RIPPLE_MS) return 0;
  const band = 1 - Math.abs(d - age * RIPPLE_SPEED) / RIPPLE_BAND;
  return band <= 0 ? 0 : band * (1 - age / RIPPLE_MS) ** 2;
}

/** Whether the field animates: the effect is on, motion is full, and the window is shown and focused. */
export const fieldRuns = (s: { on: boolean; motion: string; hidden: boolean; focused: boolean }) => s.on && s.motion === 'full' && !s.hidden && s.focused;

type Ripple = { x: number; y: number; t: number };

function GlyphField(props: { boxes: () => Box[] }) {
  let canvas!: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D | null = null;
  let [cols, rows] = [0, 0];
  let glyph = new Uint8Array(0);
  let alpha = new Float32Array(0);
  let clear = new Float32Array(0);
  let tint = new Float32Array(0);
  let [ink, accent] = ['', ''];
  let ripples: Ripple[] = [];
  let [raf, timer, last] = [0, 0, 0];
  let seed = 11;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const [on, setOn] = createSignal(false);
  const live = () => fieldRuns({ on: on(), motion: motion(), hidden: windowHidden(), focused: windowFocused() });

  const cell = (i: number) => {
    const [x, y] = [(i % cols) * CELL, Math.floor(i / cols) * CELL];
    ctx!.clearRect(x, y, CELL, CELL);
    const k = clear[i]!;
    if (k <= 0) return;
    const ch = CHARS[glyph[i]!]!;
    ctx!.globalAlpha = alpha[i]! * k;
    ctx!.fillStyle = ink;
    ctx!.fillText(ch, x + CELL / 2, y + CELL / 2);
    if (tint[i]! > 0.01) {
      ctx!.globalAlpha = Math.min(1, tint[i]! * 0.9) * k;
      ctx!.fillStyle = accent;
      ctx!.fillText(ch, x + CELL / 2, y + CELL / 2);
    }
  };

  // Size the canvas to the greeting area, read the colours of the look, and paint every cell once.
  const layout = () => {
    const box = canvas.parentElement?.getBoundingClientRect();
    const style = getComputedStyle(canvas);
    setOn(style.display !== 'none');
    // A look without the field gives its pixels back.
    if (!on()) canvas.width = canvas.height = 0;
    if (!box || !on()) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    [cols, rows] = [Math.ceil(box.width / CELL), Math.ceil(box.height / CELL)];
    canvas.width = Math.round(box.width * dpr);
    canvas.height = Math.round(box.height * dpr);
    [ink, accent] = [style.color, style.caretColor];
    const n = cols * rows;
    if (glyph.length !== n) {
      glyph = Uint8Array.from({ length: n }, () => Math.floor(random() * CHARS.length));
      alpha = Float32Array.from({ length: n }, () => 0.05 + random() * 0.17);
      tint = new Float32Array(n);
    }
    const boxes = props.boxes();
    // A glyph reaches half a cell's diagonal from its centre, so the fade starts that much further from the text.
    const reach = CELL * 0.71;
    clear = Float32Array.from({ length: n }, (_, i) => {
      const room = clearance(boxes, (i % cols) * CELL + CELL / 2, Math.floor(i / cols) * CELL + CELL / 2) * FADE;
      return Math.max(0, Math.min(1, (room - reach) / (FADE - reach)));
    });
    ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `${CELL - 5}px ${style.fontFamily}`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (let i = 0; i < n; i++) cell(i);
  };

  const shimmer = () => {
    for (let j = Math.ceil((cols * rows) / 60); j > 0; j--) {
      const i = Math.floor(random() * cols * rows);
      glyph[i] = Math.floor(random() * CHARS.length);
      alpha[i] = 0.05 + random() * 0.17;
      cell(i);
    }
  };

  // Repaint only the cells whose tint changed, inside the reach of every ripple still alive or just ended.
  const spread = (now: number) => {
    const ended = ripples.filter((r) => now - r.t >= RIPPLE_MS);
    ripples = ripples.filter((r) => now - r.t < RIPPLE_MS);
    for (const area of [...ripples, ...ended]) {
      const [c0, c1] = [Math.max(0, Math.floor((area.x - RIPPLE_REACH) / CELL)), Math.min(cols - 1, Math.floor((area.x + RIPPLE_REACH) / CELL))];
      const [r0, r1] = [Math.max(0, Math.floor((area.y - RIPPLE_REACH) / CELL)), Math.min(rows - 1, Math.floor((area.y + RIPPLE_REACH) / CELL))];
      for (let r = r0; r <= r1; r++)
        for (let c = c0; c <= c1; c++) {
          const i = r * cols + c;
          let k = 0;
          for (const ring of ripples) k = Math.max(k, rippleTint(Math.hypot(c * CELL + CELL / 2 - ring.x, r * CELL + CELL / 2 - ring.y), now - ring.t));
          if (Math.abs(k - tint[i]!) < 0.01) continue;
          tint[i] = k;
          cell(i);
        }
    }
  };

  const stop = () => {
    cancelAnimationFrame(raf);
    clearTimeout(timer);
    [raf, timer] = [0, 0];
  };
  const frame = (now: number) => {
    raf = 0;
    if (!live() || !ctx) return;
    if (now - last >= TICK) {
      last = now;
      shimmer();
    }
    spread(now);
    schedule();
  };
  // The loop sleeps between paints: about 30 a second while a ripple runs, about seven for the shimmer alone.
  const schedule = () => {
    if (raf || timer || !live()) return;
    timer = window.setTimeout(
      () => {
        timer = 0;
        raf = requestAnimationFrame(frame);
      },
      ripples.length ? RIPPLE_STEP : TICK,
    );
  };

  const press = (e: PointerEvent) => {
    if (e.button !== 0 || !live() || (e.target as Element).closest?.(BUSY)) return;
    const box = canvas.getBoundingClientRect();
    ripples = [...ripples.slice(-3), { x: e.clientX - box.left, y: e.clientY - box.top, t: performance.now() }];
    if (!raf) {
      stop();
      raf = requestAnimationFrame(frame);
    }
  };

  onMount(() => {
    const area = canvas.parentElement!.parentElement!;
    area.addEventListener('pointerdown', press);
    onCleanup(() => {
      stop();
      area.removeEventListener('pointerdown', press);
    });
  });

  // Ambient measures the text blocks on every resize and look change; the field follows.
  createEffect(() => {
    props.boxes();
    layout();
  });
  createEffect(() => (live() ? schedule() : stop()));

  return <canvas ref={(el) => (canvas = el)} class="amb-glyphs" data-live={live() ? 'on' : 'off'} />;
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
  const [blocks, setBlocks] = createSignal<Box[]>([]);
  const [beam, setBeam] = createSignal(false);
  const breathes = () => fieldRuns({ on: beam(), motion: motion(), hidden: windowHidden(), focused: windowFocused() });
  let breathed = 0;

  const paint = () => setShadows(dustShadows(grains, boxes, Math.min(width - BEAM_RIGHT, BEAM_MAX), color));

  // Where the text blocks are, in the coordinates of the greeting area.
  const measure = () => {
    const area = layer.parentElement?.getBoundingClientRect();
    if (!area) return;
    setBeam(getComputedStyle(layer.querySelector('.amb-beam')!).display !== 'none');
    width = area.width;
    boxes = props.avoid().map((e) => {
      const r = e.getBoundingClientRect();
      return { l: r.left - area.left, t: r.top - area.top, r: r.right - area.left, b: r.bottom - area.top };
    });
    setBlocks(boxes);
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

  // The breath keeps its place while it pauses, so it goes on from there; with less motion the dust stands lit and the glow off.
  createEffect(() => {
    if (!breathes()) {
      if (motion() !== 'full') layer.querySelectorAll<HTMLElement>('.dust i, .amb-glow').forEach((el) => el.style.removeProperty('opacity'));
      return;
    }
    let last = performance.now();
    const timer = window.setInterval(() => {
      const now = performance.now();
      breathed += now - last;
      last = now;
      layer.querySelectorAll<HTMLElement>('.dust i, .amb-glow').forEach((el, i) => {
        const b = BREATHS[i];
        if (b) el.style.opacity = (b.from + (b.to - b.from) * breath(breathed + b.offset, b.period)).toFixed(3);
      });
    }, BREATH_STEP);
    onCleanup(() => clearInterval(timer));
  });

  const claude = () => props.vendors.includes('claude');
  const codex = () => props.vendors.includes('codex');
  // The path from the idea to the result runs through each connected vendor, and only those.
  const trace = () => (claude() ? THROUGH_CLAUDE : codex() ? THROUGH_CODEX : STRAIGHT);

  return (
    <div ref={(el) => (layer = el)} class="ambient" aria-hidden="true">
      <GlyphField boxes={blocks} />
      <div class="amb amb-beam">
        <div class="beam">
          <i />
          <i />
        </div>
        <div class="dust" aria-hidden="true" data-live={breathes() ? 'on' : 'off'}>
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
