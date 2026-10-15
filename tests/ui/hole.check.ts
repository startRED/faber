import themes from '../../src-tauri/src/themes.json';
import { ARC, bornAt, type Cell, DISK, frame, hole, holeRuns, RIM, rippleTint, STAR } from '../../ui/Hole.tsx';
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

check('hole: the scene moves with time and is the same at the same moment', () => {
  const { cx, cy, R } = F;
  const points = Array.from({ length: 40 }, (_, k) => [cx - (2 + k * 0.05) * R * Math.cos(0.42), cy + (2 + k * 0.05) * R * Math.sin(0.42)] as const);
  const now = points.map(([x, y]) => at(x, y, 10).d);
  equal(JSON.stringify(points.map(([x, y]) => at(x, y, 10).d)), JSON.stringify(now), 'a fixed moment draws the same');
  ok(points.some(([x, y], k) => Math.abs(at(x, y, 12).d - now[k]!) > 0.05), 'the disk turns over two seconds');
});

check('hole: the frame fits the room right of the words and keeps the arc off them', () => {
  for (const [w, h, words] of [[1660, 935, 870], [1600, 1000, 640], [1200, 800, 700], [2400, 1300, 900]] as const) {
    const f = frame(w, h, words);
    ok(f.R >= 0.22 * h - 0.01 && f.R <= 0.4 * h + 0.01, `${w}x${h}: the radius stays between 22% and 40% of the height`);
    ok(f.cx - 1.6 * f.R >= words + 29 || f.R <= 0.22 * h + 0.01, `${w}x${h}: the arc keeps 30 px off the words unless the room is too small`);
    ok(f.qx > words && f.qx < w && f.qy + f.qr < h, `${w}x${h}: the planet sits inside the room, right of the words`);
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
