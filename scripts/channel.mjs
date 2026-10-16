// The release channels (R156), in one place so the three workflows that need them (auto-release, promote-stable, release)
// agree and the rules can be tested without GitHub.
//
//   Stable   vX.Y.Z                 everyone; built weekly from a preview, or by hand for a finished area (minor bump)
//   Preview  vX.Y.(Z+1)-preview.N   people who opted in; one per green feat, fix or perf commit
//
// The version scheme is plain semver, where a pre-release sorts below the release it precedes (0.3.16-preview.9 < 0.3.16),
// which is what the updater compares. Every bundle Faber builds accepts it: NSIS (Windows) and the Debian package keep the
// text and the NSIS numeric file version drops the pre-release part, and the macOS bundle and the AppImage only carry it in
// text. A Windows MSI (WiX) would reject it, since WiX wants numbers only; MSI is not in `bundle.targets`, and the list is
// checked by scripts/channel.test.mjs so that adding it fails loudly instead of at release time.
//
//   node scripts/channel.mjs kind <tag>                  prints stable or preview; exits 1 for any other tag
//   node scripts/channel.mjs next-preview                prints the tag the next preview takes (reads git tags)
//   node scripts/channel.mjs notes-base <tag>            prints the tag whose "what's new" entries the release notes start after
//   node scripts/channel.mjs promotion [<preview-tag>]   prints the stable tag to push and the commit; exits 3 when none is due
//   node scripts/channel.mjs user-facing <from> <to>     exits 3 unless a feat, fix or perf touching what ships landed in from..to
//   node scripts/channel.mjs newest <tag>...             prints the tag whose build is the newest of those given
//
// Nothing here goes through a shell: git runs with an argument list, and every tag is checked against TAG before it is used.

import { execFileSync } from 'node:child_process';

const TAG = /^v(\d+)\.(\d+)\.(\d+)(?:-preview\.(\d+))?$/;
/** Files whose change a person can see; the same list auto-release.yml has always used. */
export const SHIPS = ['ui', 'engine', 'src-tauri', 'package.json', 'vite.config.ts'];
const USER_FACING = /^(feat|fix|perf)(\(|!|:)/;
const DAY = 24 * 3600;

/** `{ major, minor, patch, preview }` (preview is null for a stable tag), or null when the text is not a release tag. */
export function parse(tag) {
  const m = TAG.exec(tag);
  if (!m) return null;
  return { major: Number(m[1]), minor: Number(m[2]), patch: Number(m[3]), preview: m[4] === undefined ? null : Number(m[4]) };
}

/** Semver order: a preview sorts below the stable of the same numbers and by its count among previews. */
export function compare(a, b) {
  const x = parse(a);
  const y = parse(b);
  for (const key of ['major', 'minor', 'patch']) if (x[key] !== y[key]) return x[key] - y[key];
  if (x.preview === y.preview) return 0;
  if (x.preview === null) return 1;
  if (y.preview === null) return -1;
  return x.preview - y.preview;
}

/** The tags that are releases, newest first. Anything else (a tag a person made for another reason) is ignored. */
export function releases(tags) {
  return tags.filter((tag) => parse(tag)).sort((a, b) => compare(b, a));
}

export const stables = (tags) => releases(tags).filter((tag) => parse(tag).preview === null);
export const previews = (tags) => releases(tags).filter((tag) => parse(tag).preview !== null);

/** The tag the next preview takes: after the newest stable, the next patch, counting the previews already on it. */
export function nextPreview(tags) {
  const [stable] = stables(tags);
  const base = stable ? parse(stable) : { major: 0, minor: 0, patch: 0 };
  // Previews already cut for a number above the stable go on counting there, so a hand-made minor bump (v0.4.0-preview.1) is kept.
  const pending = previews(tags).filter((tag) => !stable || compare(tag, stable) > 0);
  const top = pending.length ? parse(pending[0]) : { ...base, patch: base.patch + 1, preview: 0 };
  const taken = pending.map(parse).filter((p) => p.major === top.major && p.minor === top.minor && p.patch === top.patch);
  const count = taken.reduce((most, p) => Math.max(most, p.preview), 0);
  return `v${top.major}.${top.minor}.${top.patch}-preview.${count + 1}`;
}

/** The stable tag a preview becomes: the same numbers without the pre-release part. */
export function stableOf(previewTag) {
  const p = parse(previewTag);
  return `v${p.major}.${p.minor}.${p.patch}`;
}

/**
 * The preview to promote: the newest whose commit is at least a day old (so it has run a day without a newer fix landing
 * over it), or null. `candidates` is `[{ tag, time }]` with the commit time in seconds.
 */
export function pickPreview(candidates, now, minAge = DAY) {
  const old = candidates.filter((c) => now - c.time >= minAge).map((c) => c.tag);
  return releases(old)[0] ?? null;
}

/** Whether any commit subject is a feat, fix or perf. */
export const userFacing = (subjects) => subjects.some((subject) => USER_FACING.test(subject));

/** The tag whose "what's new" entries the notes of `tag` start after (see scripts/release-notes.mjs). */
export function notesBase(tag, all, nearest) {
  // A stable tells everything since the previous stable, since that is what its people last installed. A preview tells what
  // is new since the build before it, whichever channel that was.
  if (parse(tag).preview === null) return stables(all).find((other) => compare(other, tag) < 0) ?? '';
  return nearest();
}

function git(...args) {
  return execFileSync('git', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}

const allTags = () => git('tag', '--list', 'v*').split('\n').filter(Boolean);
const commitOf = (tag) => git('rev-parse', `${tag}^{commit}`);
const isAncestor = (a, b) => {
  try {
    git('merge-base', '--is-ancestor', a, b);
    return true;
  } catch {
    return false;
  }
};

/** The newest of the given release tags by the code it holds: a later commit wins, and on the same commit the higher version. */
export function newest(tags, commit = commitOf, ancestor = isAncestor) {
  return tags.reduce((best, tag) => {
    const [a, b] = [commit(best), commit(tag)];
    if (a === b) return compare(tag, best) > 0 ? tag : best;
    if (ancestor(a, b)) return tag;
    return best;
  });
}

/** Whether a commit between the two touched what ships and is a feat, fix or perf. */
function changedForPeople(from, to) {
  const range = `${git('rev-parse', '--verify', `${from}^{commit}`)}..${git('rev-parse', '--verify', `${to}^{commit}`)}`;
  return userFacing(git('log', '--no-merges', '--format=%s', range, '--', ...SHIPS).split('\n'));
}

function need(tag) {
  if (!parse(tag ?? '')) throw new Error(`not a release tag: ${tag}`);
  return tag;
}

function main([command, ...args]) {
  const tags = () => allTags();
  switch (command) {
    case 'kind':
      return parse(need(args[0])).preview === null ? 'stable' : 'preview';
    case 'next-preview':
      return nextPreview(tags());
    case 'notes-base':
      return notesBase(need(args[0]), tags(), () => {
        try {
          return git('describe', '--tags', '--abbrev=0', '--match', 'v*', `${args[0]}^`);
        } catch {
          return '';
        }
      });
    case 'user-facing':
      return changedForPeople(args[0], args[1]) ? undefined : process.exit(3);
    case 'newest':
      return newest(args.map(need));
    case 'promotion': {
      const [stable] = stables(tags());
      let chosen = args[0] ? need(args[0]) : null;
      if (chosen && parse(chosen).preview === null) throw new Error(`${chosen} is already a stable tag`);
      if (!chosen) {
        const now = Math.floor(Date.now() / 1000);
        const candidates = previews(tags()).map((tag) => ({ tag, time: Number(git('log', '-1', '--format=%ct', commitOf(tag))) }));
        chosen = pickPreview(candidates, now);
      }
      if (!chosen) return process.exit(3);
      const commit = commitOf(chosen);
      const target = stableOf(chosen);
      if (tags().includes(target) || (stable && compare(target, stable) <= 0)) return process.exit(3);
      if (stable) {
        // Nothing to ship when the stable already holds this code.
        if (isAncestor(commit, commitOf(stable))) return process.exit(3);
        // A weekly pick needs something a person can see; an urgent one named by hand does not.
        if (!args[0]) {
          if (!changedForPeople(commitOf(stable), commit)) return process.exit(3);
        }
      }
      return `${target} ${commit}`;
    }
    default:
      throw new Error(`unknown command: ${command}`);
  }
}

if (process.argv[1]?.endsWith('channel.mjs')) {
  try {
    const out = main(process.argv.slice(2));
    if (out !== undefined) process.stdout.write(`${out}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exit(1);
  }
}
