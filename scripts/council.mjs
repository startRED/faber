// The critique council and the climb (R541, after R172). One judge sees a screen through one model's eyes, and some
// judges invent defects; so the photos of `node scripts/shots.mjs <label>` go to several judges of different vendors at
// once, each scoring every photo on the eight lines of `.claude/skills/visual-critique/SKILL.md`:
//
//   node scripts/council.mjs <label> [--judges codex,agy,claude] [--refs <folder>] [--against <label>] [--target 36]
//
// A line's score is the judges' median (the lower one when two are left), so one generous or harsh judge moves nothing.
// A judge whose total for a photo sits more than `JESTER_GAP` points from the middle judge is the round's jester for
// that photo: its numbers leave the score and its rows are listed apart, never as work. A row is work only when two
// judges or more lost a point on the same line of the same photo.
//
// `--refs` names a folder of photos of other apps (never committed: they are not ours) that the judges score on the same
// lines, so the report says how far each screen is from the best of them and what they do better. `--against` compares
// this round with an earlier one and says what a climb does next: keep the change (no screen dropped, one rose), go back
// (a screen dropped, or nothing rose), or stop (every screen reached the target). The writer edits, photographs, asks the
// council, and keeps only rounds that climbed; `.claude/skills/visual-critique/SKILL.md` says how.
//
// The judges never see the code: each runs in an empty folder under the system temp folder with a copy of the photos,
// with no tools but reading those photos. Children start with no shell. It spends a little real quota of each vendor;
// nothing here runs in CI or `cargo t`. The pure functions are tested in `scripts/council.test.mjs`.
import { spawn } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** The eight lines of the visual critique, in the skill's order. */
export const LINES = ['Main action', 'Said once', 'Type', 'Art and text', 'Alignment', 'Its own area', 'Calm', 'Fits'];

/** Points out of 40 a judge may sit from the middle judge before its numbers leave the score. */
export const JESTER_GAP = 6;

/** A rise smaller than this is noise between two runs of the same judges, not a climb. */
export const NOISE = 1;

/** The judges, by name: one per vendor, each on the model that reads a screen best for its quota. */
export const JUDGES = {
  codex: { vendor: 'Codex', model: 'gpt-6.1-sol', effort: 'medium' },
  agy: { vendor: 'Gemini', model: 'gemini-4-argon-high' },
  claude: { vendor: 'Claude', model: 'sonnet', effort: 'medium' },
};

/** A label names one folder inside the critique folder, nothing else (the same rule as `shots.mjs`). */
export const LABEL = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;

/** What every judge reads: the rubric, the photos in order, and the answer it must give. */
export function judgePrompt(photos, refs = []) {
  const lines = LINES.map((l, i) => `${i + 1}. ${l}`).join('\n');
  return [
    'You judge screenshots of a desktop app. Score each photo 0 to 5 on each of these eight lines (5 is best):',
    '',
    lines,
    '',
    '1 Main action: one action leads, everything else is quieter. 2 Said once: no name or title repeats across sidebar,',
    'crumb and heading. 3 Type: at most three strong weights in view, sizes step clearly. 4 Art and text: art never behind',
    'text a person reads. 5 Alignment: columns share a baseline, edges line up. 6 Its own area: only this area\'s words and rows.',
    '7 Calm: only the work and its controls, no crowding, no unrelated settings. 8 Fits: nothing cut, no awkward wrap.',
    '',
    'Judge only what you can see in the photo. Never guess at a bug you cannot see; "nothing to report" is a correct answer.',
    'For every point lost, give one row: the line number, what is there, what it should be.',
    '',
    `Photos to score, in this order: ${photos.join(', ')}.`,
    ...(refs.length
      ? ['', `Reference photos of other apps, the bar to beat: ${refs.join(', ')}. Score them on the same lines, and in`,
          '"better" name up to three things they do better that the app\'s photos could learn from.']
      : []),
    '',
    'Answer with one JSON object and nothing else:',
    '{"photos":[{"photo":"<file name>","lines":[8 integers],"rows":[{"line":<1-8>,"what":"...","should":"..."}]}],',
    ' "refs":[{"photo":"<file name>","lines":[8 integers]}], "better":["..."]}',
  ].join('\n');
}

/** The first JSON object in a text, fences and words around it allowed. */
export function firstJson(text) {
  const start = text.indexOf('{');
  if (start < 0) return null;
  let depth = 0;
  let quoted = false;
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '\\') i++;
      else if (c === '"') quoted = false;
    } else if (c === '"') quoted = true;
    else if (c === '{') depth++;
    else if (c === '}' && --depth === 0) {
      try {
        return JSON.parse(text.slice(start, i + 1));
      } catch {
        return null;
      }
    }
  }
  return null;
}

const fair = (lines) => Array.isArray(lines) && lines.length === LINES.length && lines.every((n) => Number.isInteger(n) && n >= 0 && n <= 5);

/** A judge's answer, checked: every photo scored on eight lines of 0-5. A point lost with no row saying why is not a
 * point lost (the skill's rule: a score with no rows is not a score), so that line counts as 5 and is named in
 * `unexplained`; the rest of the judge's answer still counts. Dropping the whole answer for one missing row left a
 * council of two with one judge on 2026-11-04. */
export function parseVerdict(text, photos, refs = []) {
  const json = firstJson(text ?? '');
  if (!json || !Array.isArray(json.photos)) return { error: 'no JSON answer' };
  const scored = {};
  const unexplained = [];
  for (const photo of photos) {
    const entry = json.photos.find((p) => p?.photo === photo);
    if (!entry || !fair(entry.lines)) return { error: `no fair score for ${photo}` };
    const rows = (Array.isArray(entry.rows) ? entry.rows : [])
      .filter((r) => Number.isInteger(r?.line) && r.line >= 1 && r.line <= LINES.length)
      .map((r) => ({ line: r.line, what: String(r.what ?? ''), should: String(r.should ?? '') }));
    const lines = entry.lines.map((n, i) => {
      if (n === 5 || rows.some((r) => r.line === i + 1)) return n;
      unexplained.push(`${photo} line ${i + 1}`);
      return 5;
    });
    scored[photo] = { lines, rows };
  }
  const refScores = {};
  for (const photo of refs) {
    const entry = (Array.isArray(json.refs) ? json.refs : []).find((p) => p?.photo === photo);
    if (entry && fair(entry.lines)) refScores[photo] = { lines: entry.lines };
  }
  const better = (Array.isArray(json.better) ? json.better : []).map(String).slice(0, 3);
  return { photos: scored, refs: refScores, better, unexplained };
}

const sum = (xs) => xs.reduce((a, b) => a + b, 0);

/** The median, the lower middle one when the count is even: a council of two takes the harsher judge. */
export function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor((s.length - 1) / 2)];
}

/** One photo's score from the judges who answered: the jesters (far from the others' median) leave the numbers, each
 * line is the median of the rest, and a row is work only when two of them lost a point on its line. */
export function scorePhoto(byJudge) {
  const judges = Object.keys(byJudge);
  const totals = Object.fromEntries(judges.map((j) => [j, sum(byJudge[j].lines)]));
  // With two judges nobody can tell which one is off; from three on, the middle judge is the measure.
  const middle = median(Object.values(totals));
  const far = judges.length < 3 ? [] : judges.filter((j) => Math.abs(totals[j] - middle) > JESTER_GAP);
  // Judges spread so wide that only one would be left agree on nothing: then none is the jester and the median decides.
  const jesters = judges.length - far.length >= 2 ? far : [];
  const counted = judges.filter((j) => !jesters.includes(j));
  const lines = LINES.map((_, i) => median(counted.map((j) => byJudge[j].lines[i])));
  const agreed = [];
  const unconfirmed = [];
  for (let i = 0; i < LINES.length; i++) {
    const losing = counted.filter((j) => byJudge[j].lines[i] < 5);
    const rows = losing.flatMap((j) => byJudge[j].rows.filter((r) => r.line === i + 1).map((r) => ({ ...r, judge: j })));
    (losing.length >= 2 ? agreed : unconfirmed).push(...rows);
  }
  for (const j of jesters) unconfirmed.push(...byJudge[j].rows.map((r) => ({ ...r, judge: j, jester: true })));
  return { total: sum(lines), lines, totals, jesters, agreed, unconfirmed };
}

/** The council's result: each photo's score, and each reference's median total. Judges that gave no fair answer are
 * left out and named; a photo needs two judges, since one is the single view the council exists to avoid. */
export function aggregate(verdicts, photos, refs = []) {
  const answered = Object.keys(verdicts).filter((j) => !verdicts[j].error);
  const failed = Object.fromEntries(Object.keys(verdicts).filter((j) => verdicts[j].error).map((j) => [j, verdicts[j].error]));
  if (answered.length < 2) return { error: `the council needs two judges, ${answered.length} answered`, failed };
  const scores = Object.fromEntries(photos.map((p) => [p, scorePhoto(Object.fromEntries(answered.map((j) => [j, verdicts[j].photos[p]])))]));
  const refTotals = {};
  for (const r of refs) {
    const totals = answered.filter((j) => verdicts[j].refs[r]).map((j) => sum(verdicts[j].refs[r].lines));
    if (totals.length) refTotals[r] = median(totals);
  }
  const better = refs.length ? answered.flatMap((j) => verdicts[j].better.map((text) => ({ judge: j, text }))) : [];
  const unexplained = Object.fromEntries(answered.filter((j) => verdicts[j].unexplained?.length).map((j) => [j, verdicts[j].unexplained]));
  return { judges: answered, failed, scores, refTotals, better, unexplained };
}

/** What a climb does after a round, comparing it with the best round so far, photo by photo. A drop past the noise is
 * never kept (R172: a score never drops), not even one that still clears the target; a screen the round no longer shows
 * counts as dropped. A drop of one point is let through on purpose: the same judges on the same photo wobble that much,
 * and refusing it would revert every round. A round that raises nothing past the noise is no step up either. */
export function climb(best, round, target) {
  const photos = Object.keys(round);
  if (best) {
    const gone = Object.keys(best).filter((p) => !(p in round));
    const dropped = photos.filter((p) => p in best && round[p] < best[p] - NOISE);
    if (gone.length || dropped.length) {
      const said = [...gone.map((p) => `${p} missing`), ...dropped.map((p) => `${p} ${best[p]}→${round[p]}`)];
      return { step: 'revert', why: `dropped: ${said.join(', ')}` };
    }
  }
  if (photos.every((p) => round[p] >= target)) return { step: 'stop', why: `every screen reached ${target}` };
  if (!best) return { step: 'keep', why: 'the first round is where the climb starts' };
  const rose = photos.filter((p) => !(p in best) || round[p] > best[p] + NOISE);
  if (!rose.length) return { step: 'revert', why: 'nothing rose past the noise' };
  return { step: 'keep', why: `rose: ${rose.map((p) => `${p} ${best[p] ?? '-'}→${round[p]}`).join(', ')}` };
}

/** The report the writer works from: the scores, then only the agreed rows as the next round's work. */
export function report(label, result, photos, verdict) {
  const out = [`# Council: ${label}`, ''];
  out.push(`Judges: ${result.judges.join(', ')}.${Object.keys(result.failed).length ? ` No fair answer: ${Object.entries(result.failed).map(([j, e]) => `${j} (${e})`).join(', ')}.` : ''}`, '');
  const noRow = Object.entries(result.unexplained ?? {});
  if (noRow.length) out.push(`Points lost with no row, counted as 5: ${noRow.map(([j, ls]) => `${j} (${ls.join(', ')})`).join('; ')}.`, '');
  out.push(`| Photo | Score | ${result.judges.join(' | ')} | Jester |`, `| :--- | ---: | ${result.judges.map(() => '---:').join(' | ')} | :--- |`);
  for (const p of photos) {
    const s = result.scores[p];
    out.push(`| ${p} | ${s.total}/40 | ${result.judges.map((j) => s.totals[j]).join(' | ')} | ${s.jesters.join(', ') || '-'} |`);
  }
  const refs = Object.entries(result.refTotals);
  if (refs.length) {
    const top = Math.max(...refs.map(([, t]) => t));
    out.push('', `References: ${refs.map(([r, t]) => `${r} ${t}/40`).join(', ')}. Gap to the best: ${photos.map((p) => `${p} ${result.scores[p].total - top}`).join(', ')}.`);
  }
  if (verdict) out.push('', `Climb: **${verdict.step}**, ${verdict.why}.`);
  out.push('', '## Work (two judges or more agree)', '');
  const agreed = photos.flatMap((p) => result.scores[p].agreed.map((r) => ({ ...r, photo: p })));
  if (!agreed.length) out.push('Nothing the judges agree on.');
  for (const r of agreed) out.push(`- ${r.photo}, ${LINES[r.line - 1]} (${r.judge}): ${r.what} → ${r.should}`);
  if (result.better.length) {
    out.push('', '## What the references do better', '');
    for (const b of result.better) out.push(`- (${b.judge}) ${b.text}`);
  }
  const loose = photos.flatMap((p) => result.scores[p].unconfirmed.map((r) => ({ ...r, photo: p })));
  if (loose.length) {
    out.push('', '## Unconfirmed (one judge, or a jester): not work', '');
    for (const r of loose) out.push(`- ${r.photo}, ${LINES[r.line - 1]} (${r.judge}${r.jester ? ', jester' : ''}): ${r.what}`);
  }
  return `${out.join('\n')}\n`;
}

/** True when `path` is inside the repository, where a judge must never run. */
export function insideRepo(path, root = ROOT) {
  const rel = relative(root, path);
  return rel === '' || (!rel.startsWith('..') && !isAbsolute(rel));
}

function option(args, name, otherwise) {
  const i = args.indexOf(name);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : otherwise;
}

/** The real program behind a name on the PATH; npm's `.cmd` shim is read for the program it starts, never run. */
function program(name) {
  const windows = process.platform === 'win32';
  const extra = name === 'agy' && process.env.LOCALAPPDATA ? [join(process.env.LOCALAPPDATA, 'agy', 'bin')] : [];
  for (const dir of [...(process.env.PATH || '').split(delimiter).filter(Boolean), ...extra]) {
    const exe = join(dir, name + (windows ? '.exe' : ''));
    if (existsSync(exe)) return [exe];
    const shim = join(dir, `${name}.cmd`);
    if (!windows || !existsSync(shim)) continue;
    const targets = [...readFileSync(shim, 'utf8').matchAll(/"%(?:~dp0|dp0%)\\?([^"%]+)"/gi)].map((m) => join(dir, m[1]));
    const native = targets.findLast((t) => /\.exe$/i.test(t) && !/node\.exe$/i.test(t) && existsSync(t));
    if (native) return [native];
    const script = targets.findLast((t) => /\.[cm]?js$/i.test(t) && existsSync(t));
    if (script) return [process.execPath, script];
  }
  throw new Error(`${name} is not on the PATH`);
}

function run([exe, ...pre], args, { cwd, input, timeout = 600_000 }) {
  return new Promise((done) => {
    const child = spawn(exe, [...pre, ...args], { cwd, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    let out = '';
    let err = '';
    const timer = setTimeout(() => {
      // A launcher runs the real program as its own child, which a plain kill leaves running.
      if (process.platform === 'win32') spawn(join(process.env.SystemRoot || 'C:\\Windows', 'System32', 'taskkill.exe'), ['/pid', String(child.pid), '/t', '/f'], { windowsHide: true, stdio: 'ignore' });
      else child.kill();
      setTimeout(() => done({ out, err: `${err}\ntimed out`, code: null }), 5_000).unref();
    }, timeout);
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', (e) => (clearTimeout(timer), done({ out, err: String(e), code: -1 })));
    child.on('close', (code) => (clearTimeout(timer), done({ out, err, code })));
    // A judge that quits before reading all the photos closes its input; that is its failure, not the council's.
    child.stdin.on('error', () => {});
    child.stdin.end(input ?? '');
  });
}

/** Asks one judge, in its own empty folder holding only the photos; resolves with its raw answer. */
async function ask(name, home, prompt, files) {
  const judge = JUDGES[name];
  if (name === 'claude') {
    const content = [...files.map((f) => ({ type: 'image', source: { type: 'base64', media_type: 'image/png', data: readFileSync(join(home, f)).toString('base64') } })), { type: 'text', text: prompt }];
    const args = ['-p', '--safe-mode', '--model', judge.model, '--effort', judge.effort, '--tools', '', '--strict-mcp-config', '--disable-slash-commands',
      '--no-session-persistence', '--input-format', 'stream-json', '--output-format', 'stream-json', '--verbose'];
    const r = await run(program('claude'), args, { cwd: home, input: JSON.stringify({ type: 'user', message: { role: 'user', content } }) + '\n' });
    const result = r.out.split('\n').map((l) => firstJson(l)).find((m) => m?.type === 'result');
    return result?.result ?? '';
  }
  if (name === 'codex') {
    const off = ['shell_tool', 'unified_exec', 'apps', 'browser_use', 'browser_use_external', 'computer_use'].flatMap((f) => ['--disable', f]);
    const last = join(home, 'last.txt');
    // --ignore-user-config leaves out the person's MCP servers too, which could otherwise read files.
    const args = ['exec', '--ignore-user-config', '-m', judge.model, '-c', `model_reasoning_effort="${judge.effort}"`, ...off, '-s', 'read-only', '-C', home,
      '--skip-git-repo-check', '--ephemeral', '--color', 'never', '-o', last, ...files.flatMap((f) => ['-i', join(home, f)]), '-'];
    await run(program('codex'), args, { cwd: home, input: prompt });
    return existsSync(last) ? readFileSync(last, 'utf8') : '';
  }
  // agy reads the photos with its own file tool, inside its sandbox, from the folder it is given and nothing else.
  const args = ['-p', `${prompt}\n\nThe photos are files in the folder ${home}. Read them; do not run any command or change any file.`,
    '--model', judge.model, '--add-dir', home, '--sandbox', '--dangerously-skip-permissions', '--disable-slash-commands', '--print-timeout', '540s'];
  const r = await run(program('agy'), args, { cwd: home, timeout: 600_000 });
  // Signed out, it waits a minute for a sign-in nobody gives; the person signs in through agy's own flow.
  if (/Authentication required|authentication failed/i.test(r.out + r.err)) throw new Error('signed out of agy');
  return r.out;
}

async function main(argv) {
  const label = argv[0];
  if (!label || !LABEL.test(label)) {
    console.error('usage: node scripts/council.mjs <label> [--judges codex,agy,claude] [--refs <folder>] [--against <label>] [--target 36]');
    return 2;
  }
  const configured = process.env.CARGO_TARGET_DIR;
  const target = join(configured ? (isAbsolute(configured) ? configured : join(ROOT, configured)) : join(ROOT, 'target'), 'faber-test', 'critique');
  const folder = join(target, label);
  const photos = existsSync(folder) ? readdirSync(folder).filter((f) => f.endsWith('.png')).sort() : [];
  if (!photos.length) {
    console.error(`no photos in ${folder}: run node scripts/shots.mjs ${label} first`);
    return 2;
  }
  const names = option(argv, '--judges', 'codex,agy,claude').split(',').filter(Boolean);
  const unknown = names.filter((n) => !JUDGES[n]);
  if (unknown.length || names.length < 2) {
    console.error(`judges are two or more of ${Object.keys(JUDGES).join(', ')}`);
    return 2;
  }
  const refsDir = option(argv, '--refs', process.env.FABER_COUNCIL_REFS);
  const refs = refsDir && existsSync(refsDir) ? readdirSync(refsDir).filter((f) => /\.png$/i.test(f)).sort().map((f) => `ref-${f}`) : [];
  const goal = Number(option(argv, '--target', '36'));
  // Checked before any judge runs, so a mistyped label spends no quota and never passes for a first round.
  const against = option(argv, '--against');
  const bestFile = against && LABEL.test(against) ? join(target, against, 'council.json') : null;
  if (against && !(bestFile && existsSync(bestFile))) {
    console.error(`no council for ${against}: run node scripts/council.mjs ${against} first`);
    return 2;
  }
  const prompt = judgePrompt(photos, refs);
  const verdicts = Object.fromEntries(
    await Promise.all(
      names.map(async (name) => {
        const home = mkdtempSync(join(tmpdir(), `faber-council-${name}-`));
        if (insideRepo(home)) throw new Error(`a judge may not run inside the repository: ${home}`);
        try {
          for (const p of photos) copyFileSync(join(folder, p), join(home, p));
          for (const r of refs) copyFileSync(join(refsDir, r.slice(4)), join(home, r));
          const answer = await ask(name, home, prompt, [...photos, ...refs]);
          return [name, parseVerdict(answer, photos, refs)];
        } catch (e) {
          return [name, { error: String(e?.message ?? e) }];
        } finally {
          rmSync(home, { recursive: true, force: true });
        }
      }),
    ),
  );
  const result = aggregate(verdicts, photos, refs);
  if (result.error) {
    console.error(`${result.error}: ${JSON.stringify(result.failed)}`);
    return 1;
  }
  const totals = Object.fromEntries(photos.map((p) => [p, result.scores[p].total]));
  const best = against ? JSON.parse(readFileSync(bestFile, 'utf8')).totals : null;
  const verdict = against ? climb(best, totals, goal) : null;
  mkdirSync(folder, { recursive: true });
  writeFileSync(join(folder, 'council.json'), `${JSON.stringify({ label, judges: result.judges, totals, verdict, result }, null, 2)}\n`);
  writeFileSync(join(folder, 'council.md'), report(label, result, photos, verdict));
  console.log(report(label, result, photos, verdict));
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).then((code) => (process.exitCode = code));
}
