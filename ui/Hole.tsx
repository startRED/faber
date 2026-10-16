import { createEffect, createSignal, onCleanup, onMount } from 'solid-js';
import type { Box } from './Ambient.tsx';
import { appliedLook, motion, windowFocused, windowHidden } from './appearance.ts';

// The black hole behind a new project session (docs/design.md, "Effects"): one canvas of monospace characters. A huge
// shadow cut by the right edge, an accretion disk sweeping in from the lower left on a tilted plane, the far side of the
// disk bent over the top as an arc, a thin photon ring, a dim glow, a small planet and stars. The
// disk's fibres turn faster near the hole, the arc and the ring shimmer, stars twinkle and now and then one shoots across; a click
// on an empty spot sends a ring of the accent. The look's --amb-hole token shows it (Dark Room only) and --hole-colors
// colours it.

const CELL = 12;
/** The scene moves ten times a second: the disk turns, the camera drifts, a shooting star crosses. */
const TICK = 100;
/** Stars change their light this often. */
const TWINKLE = 700;
// A ripple repaints at most 20 times a second, and only cells whose tint moved by a visible step.
const RIPPLE_STEP = 50;
const [RIPPLE_MS, RIPPLE_SPEED, RIPPLE_BAND] = [1300, 0.42, 28];
const RIPPLE_REACH = RIPPLE_MS * RIPPLE_SPEED + RIPPLE_BAND;
// Clicks on these keep their own meaning and send no ripple.
const BUSY = 'button, a, input, textarea, select, label, [contenteditable], .composer, .runcol';
/** The light the owner picked on the prototype (115%). */
const BRIGHT = 1.15;
/** No hard edge where the column of words ends: the scene runs behind it at FLOOR of its light, and comes up to full
 * from FROM px left of the column's edge to TO px right of it. The hole itself is dimmed a little more within CALM px.
 * It is a band across the whole height, so words that scroll in the column always sit on the same veil. */
const [FLOOR, FROM, TO, CALM] = [0.14, 40, 240, 320];
/** The shadow keeps this far right of the column, so its arc sits where the scene is nearly at full light. */
const SIDE = 150;
/** How much of its light the scene keeps at `x` px, for a column of words ending at `edge` px. */
export const veil = (x: number, edge: number) => FLOOR + (1 - FLOOR) * smooth(edge - FROM, edge + TO, x);
/** Less room than this right of the column and the scene does not show at all, nor run: a sliver of it reads as broken. */
const ROOM = 400;
/** How far the arc around the shadow reaches across and up, in radii, and the margin it keeps from the stage's edges, in px. */
const [WIDE, TALL, EDGE] = [1.6, 1.8, 28];

// The entrance, in ms: Curioso rises and leans toward the hole (styles.css, "wake"), a spark leaves his eyes and lands in
// the shadow, and the scene lights up from there outward at SPEED px/ms, each character scrambling in the accent before
// it settles. It plays once, when a new session opens in a shown and focused window with full motion.
const [SPARK_AT, SPARK_MS, SPEED, JITTER, SCRAMBLE, INTRO_STEP] = [700, 650, 1.4, 120, 240, 33];
const NOISE = '#@%*+=-:/|\\';

// Tones, in the order of --hole-colors: star, dust, five steps of disk from hot to ember, glow, arc, planet rim.
export const STAR = 0;
export const DUST = 1;
export const DISK = 2;
export const GLOW = 7;
export const ARC = 8;
export const RIM = 9;
const NONE = 255;
const RAMP = ' .·:-=+*%#@';
const DIRS = ['-', '\\', '|', '/'];

const hash = (i: number) => {
  const s = Math.sin(i * 12.9898 + 78.233) * 43758.5453;
  return s - Math.floor(s);
};
const lattice = (x: number, y: number) => {
  const s = Math.sin(x * 127.1 + y * 311.7) * 43758.5453;
  return s - Math.floor(s);
};
const noise = (x: number, y: number) => {
  const [xi, yi] = [Math.floor(x), Math.floor(y)];
  const [xf, yf] = [x - xi, y - yi];
  const [u, v] = [xf * xf * (3 - 2 * xf), yf * yf * (3 - 2 * yf)];
  const [a, b, c, d] = [lattice(xi, yi), lattice(xi + 1, yi), lattice(xi, yi + 1), lattice(xi + 1, yi + 1)];
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
};
const fbm = (x: number, y: number) => 0.6 * noise(x, y) + 0.3 * noise(x * 2.3, y * 2.3) + 0.1 * noise(x * 5.1, y * 5.1);
const smooth = (a: number, b: number, x: number) => {
  const k = Math.max(0, Math.min(1, (x - a) / (b - a)));
  return k * k * (3 - 2 * k);
};
/** Which of - \ | / follows a line at screen angle `a`. */
const along = (a: number) => Math.round((((a % Math.PI) + Math.PI) % Math.PI) / (Math.PI / 4)) % 4;

/** One cell of the scene: how much light, which tone, and which way its line runs (or `NONE`). */
export type Cell = { d: number; tone: number; dir: number };
const set = (out: Cell, d: number, tone: number, dir = NONE) => {
  out.d = d;
  out.tone = tone;
  out.dir = dir;
  return out;
};

/** The space behind the hole: sparse stars, fixed per cell. */
export function sky(i: number, out: Cell): Cell {
  out.dir = NONE;
  return hash(i) < 0.009 ? set(out, 0.25 + 0.75 * hash(i + 7), STAR) : set(out, 0, STAR);
}

/** Where the scene sits in a `w` by `h` stage whose words end at `words` px from the left: the whole shadow and its
 * arc in view, centred in the room from SIDE px right of the words, its radius as big as that room allows,
 * between 16% and 30% of the height; and the small planet below it, clear of the disk. The disk alone runs off the edges. */
export type Frame = { cx: number; cy: number; R: number; qx: number; qy: number; qr: number };
export function frame(w: number, h: number, words: number): Frame {
  const lit = words + SIDE;
  // Tall enough for the arc above and the planet below (1.45 R down, 0.08 R wide), each EDGE px off the stage edge.
  const R = Math.max(0.16 * h, Math.min(0.3 * h, (w - lit - 2 * EDGE) / (2 * WIDE), (h - 2 * EDGE) / (TALL + 1.53)));
  const [cx, cy] = [(lit + w) / 2, Math.max(0.45 * h, TALL * R + EDGE)];
  return { cx, cy, R, qx: cx + 0.1 * R, qy: cy + 1.45 * R, qr: Math.max(9, 0.08 * R) };
}

/** The hole, its disk, arc, ring, glow and planet at (`x`, `y`), `t` seconds in. Cells the scene leaves dark show the
 * sky. The frame holds still, so only the disk, the arc and the ring change with time. */
export function hole(x: number, y: number, i: number, t: number, f: Frame, out: Cell): Cell {
  out.dir = NONE;
  const { cx, cy, R } = f;
  const [px, py] = [(x - cx) / R, (y - cy) / R];
  const r = Math.hypot(px, py);
  const al = -0.42;
  const [ca, sa, k] = [Math.cos(al), Math.sin(al), 0.13];
  const [u, v] = [px * ca + py * sa, -px * sa + py * ca];
  const [X, Y] = [u, v / k];
  const rd = Math.hypot(X, Y);
  const ph = Math.atan2(Y, X);
  // A small planet in front of everything: a dark body with a lit rim on the side that faces the disk.
  const [qx, qy] = [(x - f.qx) / f.qr, (y - f.qy) / f.qr];
  const q = Math.hypot(qx, qy);
  if (q < 1) return set(out, smooth(0.62, 0.98, q) * Math.max(0, 0.3 + 0.7 * qx) * 0.9, RIM);
  let [d, a, arc] = [0, 0, false];
  // The disk: brighter on the side that comes at us, its fibres turning faster near the hole.
  const front = Y > 0;
  if (rd > 1.45 && rd < 6.5 && (front || r > 1)) {
    const omega = 1.2 * Math.pow(rd, -1.5);
    const fib = fbm(rd * 7.5 + Math.sin(ph * 2) * 0.4, ph * 2.2 - t * omega * 2.4);
    const profile = smooth(1.42, 1.8, rd) * (0.35 + 0.65 * Math.exp(-(rd - 1.8) / 1.1)) * smooth(6.5, 4.2, rd);
    const disk = profile * (0.45 + 0.75 * fib) * (1 - 0.3 * Math.cos(ph)) * 1.25;
    if (disk > d) {
      d = disk;
      a = Math.atan2(k * Math.cos(ph) * ca - Math.sin(ph) * sa, -Math.sin(ph) * ca - k * Math.cos(ph) * sa);
    }
  }
  // The far side of the disk, bent by the lens: a thick arc over the top, a thin one under the bottom.
  if (r > 1 && !(front && d > 0.25)) {
    const th = Math.atan2(py, px);
    const [up, down] = [Math.max(0, -Math.sin(th - al)), Math.max(0, Math.sin(th - al))];
    const fib = fbm(r * 12, th * 3 + t * 0.35);
    const top = Math.exp(-(((r - 1.3) / (0.12 + 0.22 * up)) ** 2)) * (0.15 + 0.85 * up ** 0.8);
    const under = Math.exp(-(((r - 1.07) / 0.06) ** 2)) * down * 0.7;
    const lens = (top + under) * (0.6 + 0.6 * fib) * 1.5;
    if (lens > d) [d, a, arc] = [lens, th + Math.PI / 2, true];
  }
  // The photon ring, with a slow wobble so its edge is not ruler straight.
  const th = Math.atan2(py, px);
  const ring = Math.exp(-(((r - 1.015 - 0.012 * Math.sin(th * 9 + t * 0.6)) / 0.016) ** 2)) * 0.85;
  if (ring > d) [d, a, arc] = [ring, th + Math.PI / 2, true];
  if (r < 1 && d < 0.15) return r > 0.93 && hash(i + 41) < 0.35 ? set(out, 0.1, GLOW) : set(out, 0, STAR);
  if (d > 0.08) {
    d = Math.min(1, d);
    const tone = arc && d > 0.45 ? ARC : d > 0.8 ? DISK : d > 0.6 ? DISK + 1 : d > 0.4 ? DISK + 2 : d > 0.22 ? DISK + 3 : DISK + 4;
    return set(out, d, tone, along(a));
  }
  // A dim, sparse, warm glow around the light.
  const glow = 0.55 * Math.exp(-Math.abs(r - 1.3) / 1.3) * smooth(9, 1.5, rd) + 0.35 * Math.exp(-Math.abs(v) / 0.12) * smooth(7, 2, rd);
  if (r > 1 && hash(i + 29) < glow * 1.6) return set(out, 0.12 + glow * 0.55, GLOW);
  if (r < 1) return set(out, 0, STAR);
  return sky(i, out);
}

/** How strongly a ripple tints a cell `dist` px from the click, `age` ms after it: a band that travels out and fades. */
export function rippleTint(dist: number, age: number): number {
  if (age < 0 || age >= RIPPLE_MS) return 0;
  const band = 1 - Math.abs(dist - age * RIPPLE_SPEED) / RIPPLE_BAND;
  return band <= 0 ? 0 : band * (1 - age / RIPPLE_MS) ** 1.5;
}

/** When the cell `dist` px from the shadow's centre lights up, in ms after the entrance starts. */
export const bornAt = (dist: number, i: number) => SPARK_AT + SPARK_MS + dist / SPEED + hash(i + 51) * JITTER;

/** Whether the scene animates: the look has it, motion is full, and the window is shown and focused. */
export const holeRuns = (s: { on: boolean; motion: string; hidden: boolean; focused: boolean }) => s.on && s.motion === 'full' && !s.hidden && s.focused;

type Meteor = { x: number; y: number; vx: number; vy: number; t: number };
type Ripple = { x: number; y: number; t: number };

export function Hole(props: { avoid: () => Element[] }) {
  let canvas!: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D | null = null;
  let [w, h, cols, rows] = [0, 0, 0, 0];
  let place: Frame = frame(0, 0, 0);
  let light = new Float32Array(0);
  let tone = new Uint8Array(0);
  let dirs = new Uint8Array(0);
  let tint = new Float32Array(0);
  let glyph: string[] = [];
  let clear = new Float32Array(0); // FLOOR behind the column of words, 1 from TO px right of it (veil)
  let calm = new Float32Array(0); // 0 at the column's edge, 1 from CALM px right of it: the hole is dimmed beside it
  let moving: number[] = []; // the cells the hole can reach; the rest is sky, still but for the stars
  let stars: number[] = [];
  let colors: string[] = [];
  let accent = '';
  let meteors: Meteor[] = [];
  let ripples: Ripple[] = [];
  let [tick, twinkle, rippling, raf, half] = [0, 0, 0, 0, 0];
  let seed = 11;
  const random = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  const cell: Cell = { d: 0, tone: STAR, dir: NONE };
  // The entrance: when it started, when each cell lights up, the lit cells in that order, the spark's trail.
  let [introAt, from, last, decided, introTimer] = [0, 0, 0, false, 0];
  let born: Float32Array | null = null;
  let order: number[] = [];
  let eye = { x: 0, y: 0 };
  let trail = new Map<number, number>();
  const [intro, setIntro] = createSignal(false);
  const [on, setOn] = createSignal(false);
  const live = () => holeRuns({ on: on(), motion: motion(), hidden: windowHidden(), focused: windowFocused() });
  const clock = () => performance.now() / 1000;

  const pick = (i: number) => {
    const d = light[i]!;
    if (d < 0.05) return ' ';
    switch (tone[i]) {
      case STAR:
        return d > 0.85 ? '*' : d > 0.6 ? '+' : d > 0.4 ? '·' : '.';
      case DUST:
      case GLOW:
        return hash(i + 2) < 0.65 ? '.' : '·';
    }
    if (d > 0.82) return '#@%'[Math.floor(random() * 3)]!;
    const dir = dirs[i]!;
    if (dir !== NONE && random() < 0.8) return dir === 2 && random() < 0.3 ? ':' : DIRS[dir]!;
    return RAMP[1 + Math.min(9, Math.floor(d * 10))]!;
  };

  /** Work the cell out again at `t`; true when it looks different. */
  const compute = (i: number, t: number) => {
    const [x, y] = [(i % cols) * CELL + CELL / 2, Math.floor(i / cols) * CELL + CELL / 2];
    const meteor = shooting(x, y, t);
    if (meteor) set(cell, meteor, STAR);
    else hole(x, y, i, t, place, cell);
    const [before, was, wasDir] = [light[i]!, tone[i]!, dirs[i]!];
    // Words stay clear, and the hole is dimmed further around them; stars and dust only keep clear.
    light[i] = cell.d * clear[i]! * (cell.tone >= DISK ? 0.75 + 0.25 * calm[i]! : 1);
    tone[i] = cell.tone;
    dirs[i] = cell.dir;
    const step = (d: number) => (d < 0.05 ? -1 : Math.min(9, Math.floor(d * 10)));
    if (glyph[i] !== ' ' && was === cell.tone && wasDir === cell.dir && step(before) === step(light[i]!)) return Math.abs(before - light[i]!) > 0.04;
    glyph[i] = pick(i);
    return true;
  };

  const shooting = (x: number, y: number, t: number) => {
    for (const m of meteors) {
      const age = t - m.t;
      if (age < 0 || age > 1.1) continue;
      const [ox, oy] = [x - m.x - m.vx * age, y - m.y - m.vy * age];
      const speed = Math.hypot(m.vx, m.vy);
      const [behind, across] = [-(ox * m.vx + oy * m.vy) / speed, Math.abs(ox * m.vy - oy * m.vx) / speed];
      if (behind > -6 && behind < 220 && across < 6) return (1 - behind / 220) * (1 - across / 6) * Math.sin((Math.PI * age) / 1.1);
    }
    return 0;
  };

  const paint = (i: number, now: number) => {
    if (!ctx) return;
    const [x, y] = [(i % cols) * CELL, Math.floor(i / cols) * CELL];
    ctx.clearRect(x, y, CELL, CELL);
    const d = light[i]!;
    let ch = glyph[i]!;
    // During the entrance a cell stays dark until its moment, then scrambles in the accent while it fades into its own glyph.
    const age = born ? now - introAt - born[i]! : Infinity;
    if (d >= 0.05 && age >= 0) {
      const flicker = tone[i] === STAR ? 0.55 + 0.45 * hash(i + Math.floor(now / TWINKLE)) : 1;
      const base = tone[i]! >= DISK && tone[i] !== GLOW ? 0.15 + 0.47 * Math.sqrt(d) : 0.12 + 0.78 * d;
      const settled = Math.min(1, age / SCRAMBLE);
      if (settled < 1) {
        ctx.globalAlpha = (1 - settled) * Math.min(1, 0.45 + d);
        ctx.fillStyle = accent;
        ctx.fillText(NOISE[Math.floor(random() * NOISE.length)]!, x + CELL / 2, y + CELL / 2);
      }
      ctx.globalAlpha = Math.min(1, BRIGHT * base) * flicker * settled;
      ctx.fillStyle = colors[tone[i]!] ?? '';
      ctx.fillText(ch, x + CELL / 2, y + CELL / 2);
    }
    const k = tint[i]!;
    if (k > 0.02) {
      if (ch === ' ' || age < 0) ch = k > 0.93 ? '*' : RAMP[2 + Math.floor(k * 7)]!;
      // The spark may cross the ledge Curioso stands on; a ripple keeps off the words.
      ctx.globalAlpha = Math.min(1, k) * (trail.has(i) ? 1 : clear[i]!);
      ctx.fillStyle = accent;
      ctx.fillText(ch, x + CELL / 2, y + CELL / 2);
    }
  };

  // Size the canvas to the stage, read the look's colours, find the cells the hole can reach, and paint every cell once.
  const layout = () => {
    const box = canvas.parentElement?.getBoundingClientRect();
    const style = getComputedStyle(canvas);
    // The mark counts whole; a block of words by its glyphs, not its box, which may run wider than what it holds.
    const range = document.createRange();
    const els = props.avoid();
    const words: Box[] = box
      ? els.map((e) => {
          if (!e.matches('.peek')) range.selectNodeContents(e);
          const r = e.matches('.peek') ? e.getBoundingClientRect() : range.getBoundingClientRect();
          return { l: r.left - box.left, t: r.top - box.top, r: r.right - box.left, b: r.bottom - box.top };
        })
      : [];
    // Where the column of words ends: the scene lives only right of it.
    const edge = Math.max(0, ...words.map((b) => b.r));
    setOn(style.display !== 'none' && !!box && box.width - edge >= ROOM);
    // A look without the hole, or a stage with no room beside the words, gives its pixels back.
    if (!on()) canvas.width = canvas.height = 0;
    if (!box || !on()) return;
    const dpr = Math.min(devicePixelRatio || 1, 2);
    [w, h] = [box.width, box.height];
    [cols, rows] = [Math.ceil(w / CELL), Math.ceil(h / CELL)];
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    colors = style.getPropertyValue('--hole-colors').trim().split(/\s+/);
    accent = style.caretColor;
    const n = cols * rows;
    light = new Float32Array(n);
    tone = new Uint8Array(n);
    dirs = new Uint8Array(n).fill(NONE);
    tint = new Float32Array(n);
    glyph = Array.from({ length: n }, () => ' ');
    place = frame(w, h, edge);
    // Curioso's eyes, where the spark of the entrance leaves from: 38 px into the drawing, 27 px down.
    const mark = words[els.findIndex((e) => e.matches('.peek'))];
    eye = mark ? { x: mark.l + 38, y: mark.t + 27 } : { x: place.cx, y: place.cy };
    if (!decided) {
      decided = true;
      if (live()) [introAt, last] = [performance.now(), 0];
      setIntro(live());
    }
    born = intro() ? new Float32Array(n) : null;
    for (let i = 0; born && i < n; i++) born[i] = bornAt(Math.hypot((i % cols) * CELL + CELL / 2 - place.cx, Math.floor(i / cols) * CELL + CELL / 2 - place.cy), i);
    clear = new Float32Array(n);
    calm = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const x = (i % cols) * CELL + CELL / 2;
      clear[i] = veil(x, edge);
      calm[i] = smooth(edge, edge + CALM, x);
    }
    ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.font = `${CELL - 1}px ${style.fontFamily}`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const t = clock();
    [moving, stars] = [[], []];
    for (let i = 0; i < n; i++) {
      compute(i, t);
      const [x, y] = [(i % cols) * CELL + CELL / 2, Math.floor(i / cols) * CELL + CELL / 2];
      // Only the disk and the ring of the arc change with time; the glow, the planet and the sky stay put.
      const [px, py] = [(x - place.cx) / place.R, (y - place.cy) / place.R];
      const [u, v] = [px * Math.cos(-0.42) + py * Math.sin(-0.42), -px * Math.sin(-0.42) + py * Math.cos(-0.42)];
      const [r, rd] = [Math.hypot(px, py), Math.hypot(u, v / 0.13)];
      if ((rd > 1.4 && rd < 6.6) || (r > 0.98 && r < 1.9)) moving.push(i);
      else if (tone[i] === STAR && light[i]! >= 0.05) stars.push(i);
    }
    const now = performance.now();
    ctx.clearRect(0, 0, w, h);
    for (let i = 0; i < n; i++) paint(i, now);
    if (born) {
      const at = born;
      order = [];
      for (let i = 0; i < n; i++) if (light[i]! >= 0.05) order.push(i);
      order.sort((a, b) => at[a]! - at[b]!);
      from = 0;
      if (!introTimer) introTimer = window.setTimeout(play, INTRO_STEP);
    }
  };

  // One frame of the entrance, about 30 a second: the spark and its fading trail on a curve from Curioso's eyes to the
  // shadow, then every cell that is lighting up or has just settled.
  const play = () => {
    introTimer = 0;
    if (!born) return;
    const now = performance.now();
    const since = now - introAt;
    const s = (since - SPARK_AT) / SPARK_MS;
    const next = new Map<number, number>();
    if (s > 0 && s < 1.4) {
      const [ex, ey, cx, cy] = [eye.x, eye.y, place.cx, place.cy];
      const [mx, my] = [(ex + cx) / 2, Math.max(CELL, Math.min(ey, cy) - 0.3 * Math.hypot(cx - ex, cy - ey))];
      for (let p = 0; p <= Math.min(1, s); p += 0.005) {
        const k = 1 - (s - p) / 0.35;
        if (k <= 0) continue;
        const q = 1 - p;
        const [x, y] = [q * q * ex + 2 * q * p * mx + p * p * cx, q * q * ey + 2 * q * p * my + p * p * cy];
        const i = Math.floor(y / CELL) * cols + Math.floor(x / CELL);
        if (x >= 0 && x < w && i >= 0 && i < light.length) next.set(i, Math.max(next.get(i) ?? 0, k));
        // The head glows into the cells around it.
        if (k > 0.97) for (const j of [i - 1, i + 1, i - cols, i + cols]) if (j >= 0 && j < light.length) next.set(j, Math.max(next.get(j) ?? 0, 0.55));
      }
    }
    const old = trail;
    trail = next;
    for (const i of old.keys())
      if (!next.has(i)) {
        tint[i] = 0;
        paint(i, now);
      }
    for (const [i, k] of next) {
      tint[i] = k;
      paint(i, now);
    }
    const at = born;
    while (from < order.length && at[order[from]!]! + SCRAMBLE < last) from++;
    for (let j = from; j < order.length && at[order[j]!]! <= since; j++) paint(order[j]!, now);
    last = since;
    if (s < 1.4 || from < order.length) introTimer = window.setTimeout(() => requestAnimationFrame(play), INTRO_STEP);
    else finish();
  };

  /** The entrance ends, or is cut short when the window loses focus or hides: everything shows as it is. */
  const finish = () => {
    clearTimeout(introTimer);
    introTimer = 0;
    if (!born) return;
    [born, order] = [null, []];
    for (const i of trail.keys()) tint[i] = 0;
    trail = new Map();
    setIntro(false);
    if (!ctx) return;
    const now = performance.now();
    ctx.clearRect(0, 0, w, h);
    for (let i = 0; i < cols * rows; i++) paint(i, now);
  };

  const step = () => {
    const now = performance.now();
    const t = now / 1000;
    if (random() < 0.012 && meteors.length < 2)
      meteors.push({ x: w * (0.4 + random() * 0.6), y: h * random() * 0.5, vx: -(260 + random() * 200), vy: 120 + random() * 120, t });
    const gone = meteors.filter((m) => t - m.t >= 1.2);
    meteors = meteors.filter((m) => t - m.t < 1.2);
    // Half the moving cells each tick, in turn: each one changes five times a second, at half the work.
    half ^= 1;
    for (let j = half; j < moving.length; j += 2) if (compute(moving[j]!, t)) paint(moving[j]!, now);
    // The sky under a shooting star, and under one that just ended.
    for (const m of [...meteors, ...gone]) {
      const [x0, x1] = [Math.max(0, Math.floor((m.x + m.vx * 1.2 - 10) / CELL)), Math.min(cols - 1, Math.floor((m.x + 230) / CELL))];
      const [y0, y1] = [Math.max(0, Math.floor((m.y - 230) / CELL)), Math.min(rows - 1, Math.floor((m.y + m.vy * 1.2 + 10) / CELL))];
      for (let r = y0; r <= y1; r++) for (let c = x0; c <= x1; c++) if (compute(r * cols + c, t)) paint(r * cols + c, now);
    }
    if (now - twinkle >= TWINKLE) {
      twinkle = now;
      for (const i of stars) paint(i, now);
    }
  };

  // Repaint only the cells whose tint changed, inside the reach of every ripple still alive or just ended.
  const spread = () => {
    raf = 0;
    const now = performance.now();
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
          if (Math.abs(k - tint[i]!) < 0.05 && (k > 0 || tint[i] === 0)) continue;
          tint[i] = k;
          paint(i, now);
        }
    }
    if (ripples.length && live()) rippling = window.setTimeout(() => (raf = requestAnimationFrame(spread)), RIPPLE_STEP);
  };

  const press = (e: PointerEvent) => {
    if (e.button !== 0 || !live() || (e.target as Element).closest?.(BUSY)) return;
    const box = canvas.getBoundingClientRect();
    ripples = [...ripples.slice(-3), { x: e.clientX - box.left, y: e.clientY - box.top, t: performance.now() }];
    if (!raf && !rippling) raf = requestAnimationFrame(spread);
  };

  const stop = () => {
    finish();
    clearInterval(tick);
    clearTimeout(rippling);
    cancelAnimationFrame(raf);
    [tick, rippling, raf] = [0, 0, 0];
  };

  onMount(() => {
    const area = canvas.parentElement!.parentElement!;
    area.addEventListener('pointerdown', press);
    const resize = new ResizeObserver(layout);
    resize.observe(canvas.parentElement!);
    void document.fonts.ready.then(layout);
    onCleanup(() => {
      stop();
      resize.disconnect();
      area.removeEventListener('pointerdown', press);
    });
  });

  // A new look may switch the hole on or off and changes its colours.
  createEffect(() => {
    appliedLook();
    if (canvas) layout();
  });
  createEffect(() => {
    if (!live()) return stop();
    tick = window.setInterval(step, TICK);
    onCleanup(() => clearInterval(tick));
  });

  return (
    <div class="ambient" aria-hidden="true">
      <canvas ref={(el) => (canvas = el)} class="amb-hole" data-live={live() ? 'on' : 'off'} data-intro={intro() ? 'on' : undefined} />
    </div>
  );
}
