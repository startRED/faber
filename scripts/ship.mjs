import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, symlinkSync } from 'node:fs';
import { delimiter, dirname, join, resolve } from 'node:path';
import { verdict, failures, message as baseMessage, tipPassed } from './base.mjs';

const words = (file) => file.startsWith('docs/') || file.endsWith('.md');
export function proofsFor(files) {
  if (files.every(words)) return [['npm', 'run', 'docs']];
  const proofs = [['npm', 'run', 'lint'], ['npm', 'run', 'typecheck'], ['npm', 'run', 'docs'],
    ['npm', 'run', 'build'], ['cargo', 'fmt', '--all', '--check']];
  if (files.some((f) => f.startsWith('scripts/'))) proofs.push(['npm', 'run', 'test:scripts']);
  if (files.some((f) => /(^|\/)(Cargo\.toml|Cargo\.lock)$|\.rs$/.test(f)))
    proofs.push(['node', 'scripts/cargo.mjs', 'check', '--workspace', '--all-targets']);
  return proofs;
}
export function withTrailer(message, number) {
  const text = message.trimEnd();
  return number && !/^Pull-request:\s*#\d+\s*$/im.test(text) ? `${text}\n\nPull-request: #${number}\n` : `${text}\n`;
}

const run = (cmd, args, options = {}) => execFileSync(cmd, args, {
  encoding: 'utf8', windowsHide: true, maxBuffer: 64 << 20, ...options,
});
const git = (...args) => run('git', args).trim();
const list = (text) => text.split('\0').filter(Boolean);
export function temporaryWorktree(original, directory = resolve('.faber/verify'), execute = run) {
  const source = execute('git', ['rev-parse', '--absolute-git-dir']).trim();
  directory = resolve(directory);
  mkdirSync(directory, { recursive: true });
  const dir = mkdtempSync(join(directory, 'ship-'));
  const sourceGit = (...args) => execute('git', [`--git-dir=${source}`, ...args]).trim();
  const env = { ...process.env };
  // The caller may keep its identity in config.worktree; inherit it without changing the shared configuration.
  for (const [key, field] of [['user.name', 'NAME'], ['user.email', 'EMAIL']]) {
    let value;
    try { value = sourceGit('config', '--get', key); } catch { /* Git can use its normal identity fallback. */ }
    if (value) for (const role of ['AUTHOR', 'COMMITTER']) env[`GIT_${role}_${field}`] ??= value;
  }
  const remove = () => {
    if (dirname(dir) !== directory) throw Error('Temporary worktree is outside the verification folder.');
    sourceGit('worktree', 'remove', '--force', '--force', dir);
  };
  try {
    // Reuse objects, but give each attempt its own detached HEAD, index and worktree-only fetch ref.
    sourceGit('worktree', 'add', '--quiet', '--detach', '--no-checkout', '--lock', dir, original);
    const marker = readFileSync(join(dir, '.git'), 'utf8').match(/^gitdir: (.+)\r?\n?$/);
    if (!marker) throw Error('Temporary worktree has no valid Git directory.');
    const gitDir = resolve(dir, marker[1].trim());
    // An explicit Git directory fails closed instead of discovering the caller's repository if this folder disappears.
    env.GIT_DIR = gitDir;
    env.GIT_WORK_TREE = dir;
    const git = (...args) => execute('git', [`--git-dir=${gitDir}`, `--work-tree=${dir}`, ...args], { cwd: dir, env }).trim();
    return { dir, env, gitDir, git, remove };
  } catch (error) {
    try { remove(); } catch (cleanup) { console.error(`Temporary worktree retained at ${dir}: ${cleanup.message}`); }
    throw error;
  }
}
function redBase() {
  try {
    const fields = 'status,conclusion,headSha,databaseId,createdAt,displayTitle,url';
    const query = (event) => JSON.parse(run('gh', ['run', 'list', '--workflow', 'ci.yml', '--branch', 'phase-1-base', '--event', event, '--limit', '20', '--json', fields]));
    const runs = [...query('push'), ...query('workflow_dispatch')].sort((a, b) => b.createdAt.localeCompare(a.createdAt));
    const red = verdict(runs);
    if (!red) return '';
    try {
      const tip = () => git('ls-remote', 'origin', 'refs/heads/phase-1-base').split(/\s+/)[0];
      const sha = tip();
      const candidates = JSON.parse(run('gh', ['run', 'list', '--workflow', 'ci.yml', '--commit', sha, '--status', 'completed', '--limit', '20', '--json', `${fields},event,headBranch`]));
      if (tipPassed(red, sha, candidates, (id) => JSON.parse(run('gh', ['run', 'view', String(id), '--json', 'jobs'])).jobs) && tip() === sha) return '';
    } catch { /* Keep the warning without evidence that the tip passed. */ }
    let checks = [];
    try { checks = failures(run('gh', ['run', 'view', String(red.run.databaseId), '--log-failed'])); } catch { /* Logs can expire. */ }
    if (!checks.length) {
      const jobs = JSON.parse(run('gh', ['run', 'view', String(red.run.databaseId), '--json', 'jobs'])).jobs;
      checks = jobs.filter((j) => ['failure', 'timed_out', 'startup_failure'].includes(j.conclusion))
        .map((j) => `${j.name}: ${j.steps.filter((s) => ['failure', 'timed_out', 'startup_failure'].includes(s.conclusion)).map((s) => s.name).join(', ') || 'failed'}`);
    }
    return baseMessage(red, checks);
  } catch { return ''; } // Match base.mjs: unknown status does not block.
}
export function proofEnvironment(directory, inherited = process.env) {
  const env = { ...inherited };
  // Proofs create their own Git fixtures; pinning GIT_DIR would make those fixtures operate on the candidate instead.
  for (const key of ['GIT_DIR', 'GIT_WORK_TREE', 'GIT_COMMON_DIR', 'GIT_INDEX_FILE']) delete env[key];
  env.GIT_CEILING_DIRECTORIES = [dirname(directory), env.GIT_CEILING_DIRECTORIES].filter(Boolean).join(delimiter);
  return env;
}
export function proof(args, cwd, env, execute = spawnSync) {
  console.log(`Checking: ${args.join(' ')}`);
  // npm is a .cmd shim on Windows; use its JavaScript entry point without a shell.
  let [cmd, ...rest] = args;
  if (cmd === 'npm' && process.platform === 'win32') {
    const npm = run('where.exe', ['npm.cmd']).trim().split(/\r?\n/)[0];
    cmd = process.execPath;
    rest = [resolve(npm, '..', 'node_modules', 'npm', 'bin', 'npm-cli.js'), ...rest];
  }
  const result = execute(cmd, rest, { cwd, env, encoding: 'utf8', windowsHide: true, maxBuffer: 64 << 20 });
  if (result.status === 0) return true;
  console.error(`${args.join(' ')} failed:\n${(result.stdout + '\n' + result.stderr).split('\n').slice(-40).join('\n')}${result.error || ''}`);
  return false;
}
export function main(args) {
  let workspace;
  try {
    const dry = args.includes('--dry-run');
    const idx = args.indexOf('--message-file');
    if (args.some((a, i) => a !== '--dry-run' && a !== '--message-file' && !(idx >= 0 && i === idx + 1)) || (idx !== -1 && (!args[idx + 1] || args[idx + 1].startsWith('--')))) throw Error('Usage: ship.mjs [--message-file <path>] [--dry-run]');
    if (git('status', '--porcelain')) throw Error('Working tree has uncommitted changes.');
    const red = redBase();
    if (red) { console.error(red); if (process.env.FABER_PUSH_ON_RED !== '1') throw Error('Base is red; push refused.'); }
    const original = git('rev-parse', 'HEAD');
    const msg = idx < 0 ? run('git', ['log', '-1', '--format=%B', original]) : readFileSync(args[idx + 1], 'utf8');
    let pr;
    try { const p = JSON.parse(run('gh', ['pr', 'view', '--json', 'number,state'])); if (p.state === 'OPEN') pr = p.number; } catch { /* A branch need not have a pull request. */ }
    const remote = git('remote', 'get-url', 'origin');
    workspace = temporaryWorktree(original);
    const { dir, git: g } = workspace;
    const proofEnv = proofEnvironment(dir);
    if (existsSync('node_modules')) symlinkSync(resolve('node_modules'), join(dir, 'node_modules'), process.platform === 'win32' ? 'junction' : 'dir');
    for (let attempt = 1; attempt <= 3; attempt++) {
      g('fetch', '--no-tags', '--no-write-fetch-head', remote, '+refs/heads/phase-1-base:refs/worktree/faber-ship-base');
      const base = g('rev-parse', 'refs/worktree/faber-ship-base');
      const ancestor = g('merge-base', base, original);
      const changed = list(g('diff', '--name-only', '-z', ancestor, original));
      if (!changed.length || g('rev-list', '--count', `${base}..${original}`) === '0') throw Error('Nothing is ahead of origin/phase-1-base.');
      g('checkout', '--quiet', '--detach', base);
      g('merge', '--squash', original);
      run('git', [`--git-dir=${workspace.gitDir}`, `--work-tree=${dir}`, 'commit', '--quiet', '-F', '-'], { cwd: dir, env: workspace.env, input: withTrailer(msg, pr) });
      run(process.execPath, ['scripts/numbers.mjs', 'assign', '--amend', '--base', base], {
        cwd: dir, env: workspace.env,
      });
      const finalFiles = list(g('diff', '--name-only', '-z', base, 'HEAD'));
      // Every candidate tree is proven again: a base change in another file can still break this one.
      const proofs = proofsFor(finalFiles);
      // Without installed packages lint and typecheck fail on files the change never touched; say why instead.
      if (proofs.length > 1 && !existsSync('node_modules')) throw Error('node_modules is missing here: run `npm ci` first.');
      for (const args of proofs) if (!proof(args, dir, proofEnv)) return 1;
      const sha = g('rev-parse', 'HEAD');
      if (dry) { console.log(`Dry run: ${sha} ${g('log', '-1', '--format=%s')}`); return 0; }
      try { g('push', remote, 'HEAD:refs/heads/phase-1-base'); }
      catch (e) {
        const error = String(e.stderr || e.message);
        if (!/non-fast-forward|fetch first/i.test(error)) { console.error(error); return 2; }
        continue;
      }
      console.log(`Landed: ${sha} ${g('log', '-1', '--format=%s')}`);
      if (pr) {
        run('gh', ['pr', 'comment', String(pr), '--body', `Landed directly on phase-1-base as ${sha} (fast lane).`]);
        run('gh', ['pr', 'close', String(pr)]);
      }
      return 0;
    }
    console.error('Push failed after 3 attempts because phase-1-base kept moving.');
    return 2;
  } catch (e) { console.error(String(e.stderr || e.message)); return 1; }
  finally {
    if (workspace) try { workspace.remove(); }
    catch (error) { console.error(`Temporary worktree retained at ${workspace.dir}: ${error.message}`); return 1; }
  }
}
if (process.argv[1] && resolve(process.argv[1]) === resolve(import.meta.filename)) process.exitCode = main(process.argv.slice(2));
