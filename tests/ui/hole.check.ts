import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { ARC, bornAt, type Cell, DISK, frame, Hole, hole, holeRuns, RIM, rippleTint, STAR, veil } from '../../ui/Hole.tsx';
import { check, equal, ok } from './check.ts';

const [W, H] = [1600, 1000];
const F = frame(W, H, 640);
const at = (x: number, y: number, t = 0): Cell => hole(x, y, 0, t, F, { d: 0, tone: STAR, dir: 255 });

check('hole: the shadow is dark, the photon ring around it is lit, and the disk crosses in front', () => {
  const { cx, cy, R } = F;
  equal(at(cx + 0.3 * R, cy - 0.5 * R).d, 0, 'the far half of the shadow shows nothing, not even stars');
  const ring = at(cx, cy - 1.015 * R);
  ok(ring.d > 0.4 && ring.tone === ARC, `the top of the photon ring is lit as arc, got ${ring.d} in tone ${ring.tone}`);
  // Along the disk's plane, to the lower left of the hole, the disk is lit in one of its warm tones.
  let lit = 0;
  for (let k = 2; k < 4; k += 0.05) {
    const c = at(cx - k * R * Math.cos(0.42), cy + k * R * Math.sin(0.42));
    if (c.tone >= DISK && c.tone < DISK + 5 && c.d > 0.08) lit++;
  }
  ok(lit > 10, `the disk should be lit along its plane, ${lit} of 40 samples were`);
});

check('hole: the disk keeps close to the shadow instead of filling the stage', () => {
  const { cx, cy, R } = F;
  for (const k of [3.8, 4.5, 5.5]) {
    const c = at(cx - k * R * Math.cos(0.42), cy + k * R * Math.sin(0.42));
    ok(!(c.tone >= DISK && c.tone < DISK + 5 && c.d > 0.08), `${k} radii out along its plane the disk is dark, got ${c.d} in tone ${c.tone}`);
  }
});

check('hole: the scene moves with time and is the same at the same moment', () => {
  const { cx, cy, R } = F;
  const points = Array.from({ length: 40 }, (_, k) => [cx - (2 + k * 0.05) * R * Math.cos(0.42), cy + (2 + k * 0.05) * R * Math.sin(0.42)] as const);
  const now = points.map(([x, y]) => at(x, y, 10).d);
  equal(JSON.stringify(points.map(([x, y]) => at(x, y, 10).d)), JSON.stringify(now), 'a fixed moment draws the same');
  ok(points.some(([x, y], k) => Math.abs(at(x, y, 12).d - now[k]!) > 0.05), 'the disk turns over two seconds');
});

check('hole: the whole shadow and its arc show right of the words, cut by no edge', () => {
  for (const [w, h, words] of [[1660, 935, 720], [1660, 935, 870], [1600, 1000, 640], [1200, 800, 700], [2400, 1300, 900]] as const) {
    const f = frame(w, h, words);
    ok(f.R >= 0.14 * h - 0.01 && f.R <= 0.22 * h + 0.01, `${w}x${h}: the radius stays between 14% and 22% of the height`);
    const inside = f.cx - 1.6 * f.R >= words + 150 - 0.01 && f.cx + 1.6 * f.R <= w - 27;
    ok(inside || f.R <= 0.14 * h + 0.01, `${w}x${h}: the arc stays clear of the column and off the right edge unless the room is too small`);
    ok(f.cy - 1.8 * f.R >= 27, `${w}x${h}: the arc stays off the top edge`);
    ok(f.qx > words && f.qx < w && f.qy + f.qr < h, `${w}x${h}: the planet sits inside the room, right of the words`);
  }
});

check('hole: no hard edge beside the column; the scene runs behind the words, dim enough to read over', () => {
  const edge = 720;
  ok(veil(edge - 300, edge) > 0 && veil(edge - 300, edge) <= 0.2, 'behind the words the scene keeps a faint part of its light');
  equal(veil(edge + 400, edge), 1, 'well past the column it is at full light');
  for (let x = edge - 300; x < edge + 400; x += 12) ok(veil(x + 12, edge) - veil(x, edge) < 0.1, `no step from ${x} px to the next cell`);
});

check('hole: when the words grow wider, the veil follows their edge and they stay on its faint part', async () => {
  const root = document.createElement('div');
  root.style.cssText = 'position:fixed;left:0;top:0;width:1600px;height:560px';
  root.dataset.motion = 'none';
  for (const [key, value] of Object.entries(themes.css.base)) root.style.setProperty(`--${key}`, value);
  const words = document.createElement('div');
  words.style.cssText = 'position:absolute;left:24px;top:40px;width:640px;font:600 40px/1.4 sans-serif';
  words.textContent = 'Hi';
  root.append(words);
  document.body.append(root);
  const dispose = render(() => createComponent(Hole, { avoid: () => [words] }), root);
  const settle = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  try {
    await settle(200);
    // The stage keeps its size: only the words change, as when a list of earlier talks opens under a short greeting.
    words.textContent = 'A much longer block of words that runs wide across the column and down several lines';
    await settle(200);
    const canvas = root.querySelector<HTMLCanvasElement>('canvas.amb-hole')!;
    ok(canvas.width > 0, 'Dark Room draws the black hole');
    // A canvas still dark from the entrance would pass for nothing: the scene must be painted before it is measured.
    const all = canvas.getContext('2d')!.getImageData(0, 0, canvas.width, canvas.height).data;
    let lit = 0;
    for (let i = 3; i < all.length; i += 4) if (all[i]! > 20) lit++;
    ok(lit > 1000, `the scene is drawn, ${lit} lit pixels`);
    const box = canvas.getBoundingClientRect();
    const scale = canvas.width / box.width;
    const range = document.createRange();
    range.selectNodeContents(words);
    const r = range.getBoundingClientRect();
    const px = canvas.getContext('2d')!.getImageData(0, Math.floor((r.top - box.top) * scale), Math.floor((r.right - box.left) * scale), Math.ceil(r.height * scale)).data;
    let worst = 0;
    for (let i = 3; i < px.length; i += 4) worst = Math.max(worst, px[i]!);
    // Same bound as the project screen's own check (projects.check.ts): a third of the light at most.
    ok(worst <= 77, `the words sit on the bright part of the scene (${worst})`);
  } finally {
    dispose();
    root.remove();
  }
});

check('hole: the planet has a lit rim facing the disk and a dark body', () => {
  const [px, py, s] = [F.qx, F.qy, F.qr];
  equal(at(px, py).d, 0, 'the middle of the planet is dark');
  ok(at(px + 0.85 * s, py).tone === RIM && at(px + 0.85 * s, py).d > 0.3, 'the right rim is lit');
});

check('hole: a ripple is a band that travels out and fades in 1.3 s', () => {
  ok(rippleTint(0, 0) > 0.9, 'it starts at the click');
  ok(rippleTint(210, 500) > 0.3, 'half a second later it is about 210 px out');
  equal(rippleTint(0, 500), 0, 'the middle is clear once the band has passed');
  equal(rippleTint(500, 1300), 0, 'gone after 1.3 s');
});

check('hole: it animates only when on, with full motion, in a shown and focused window', () => {
  const live = { on: true, motion: 'full', hidden: false, focused: true };
  ok(holeRuns(live));
  ok(!holeRuns({ ...live, motion: 'reduced' }) && !holeRuns({ ...live, hidden: true }) && !holeRuns({ ...live, focused: false }) && !holeRuns({ ...live, on: false }));
});

check('hole: only Dark Room shows it, and it has a colour for each of its ten tones', () => {
  equal(themes.css.base['amb-hole'], 'block');
  equal(themes.css.base['hole-colors'].split(' ').length, 10);
  for (const [name, vars] of Object.entries({ ...themes.css.themes, ...themes.css.presets }))
    if (name !== 'sala') equal((vars as Record<string, string>)['amb-hole'], 'none', `${name} should not show the hole`);
});

check('hole: the entrance lights the scene from the shadow outward, after the spark, within three seconds', () => {
  ok(bornAt(0, 1) >= 1300, 'nothing lights before the spark lands');
  ok(bornAt(400, 1) < bornAt(900, 1), 'farther cells light later');
  ok(bornAt(2200, 7) < 3000, 'a cell across a wide window lights within three seconds');
});
