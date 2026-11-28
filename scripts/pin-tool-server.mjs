import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { gunzipSync } from 'node:zlib';
import semver from 'semver';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const REGISTRY = 'https://registry.npmjs.org';
const MAX_ARCHIVE = 64 * 1024 * 1024;
const MAX_TREE_BYTES = 64 * 1024 * 1024;
const MAX_TREE_FILES = 20_000;
const MAX_TREE_ARCHIVES = 64 * 1024 * 1024;
const MAX_PACKAGES = 512;
const MAX_METADATA_BYTES = 64 * 1024 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;

const candidates = [
  ['@playwright/mcp', '0.0.84'],
  ['@executeautomation/playwright-mcp-server', '1.0.12'],
  ['@upstash/context7-mcp', '4.5.0'],
  ['@modelcontextprotocol/server-github', '2025.4.8'],
  ['@modelcontextprotocol/server-postgres', '0.6.2'],
  ['@modelcontextprotocol/server-filesystem', '2.1.0'],
  ['@modelcontextprotocol/server-memory', '2.0.1'],
  ['@modelcontextprotocol/server-sequential-thinking', '2.0.0'],
  ['mcp-remote', '0.16.0'],
  ['mcp-debugger', '1.0.0'],
  ['chrome-devtools-mcp', '1.11.0'],
];

export function registryUrl(url) {
  const parsed = new URL(url);
  if (!url.startsWith('https://registry.npmjs.org/') || parsed.protocol !== 'https:' || parsed.hostname !== 'registry.npmjs.org' || parsed.port || parsed.username || parsed.password || parsed.hash) {
    throw new Error(`Refusing non-registry URL: ${url}`);
  }
  return parsed;
}

async function getJson(url, tracker, compact = false) {
  registryUrl(url);
  const accept = compact ? 'application/vnd.npm.install-v1+json' : 'application/json';
  const response = await fetch(url, { redirect: 'error', headers: { accept }, signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
  if (!response.ok) throw new Error(`Registry returned ${response.status} for ${url}`);
  if (Number(response.headers.get('content-length')) > MAX_ARCHIVE) throw new Error('Registry metadata exceeds 64 MiB');
  const chunks = [];
  let length = 0;
  for await (const chunk of response.body) {
    length += chunk.length;
    if (length > MAX_ARCHIVE || tracker.metadataBytes + length > MAX_METADATA_BYTES) throw new Error('Registry metadata exceeds the 64 MiB cap');
    chunks.push(chunk);
  }
  tracker.metadataBytes += length;
  return JSON.parse(Buffer.concat(chunks, length).toString('utf8'));
}

export function satisfies(version, range) {
  return typeof range === 'string' && semver.valid(version) !== null && semver.validRange(range) !== null && semver.satisfies(version, range);
}

function tarString(buffer) {
  const end = buffer.indexOf(0);
  return buffer.subarray(0, end < 0 ? buffer.length : end).toString('utf8');
}

function tarNumber(buffer) {
  const value = tarString(buffer).trim();
  return value ? Number.parseInt(value, 8) : 0;
}

function tarChecksum(header) {
  const expected = tarNumber(header.subarray(148, 156));
  let actual = 0;
  for (let i = 0; i < header.length; i += 1) actual += i >= 148 && i < 156 ? 0x20 : header[i];
  if (expected !== actual) throw new Error('Tar header checksum mismatch');
}

function validRelative(name) {
  return name && !name.includes('\\') && !name.startsWith('/') && !/^[A-Za-z]:/.test(name)
    && name.split('/').every((part) => part && part !== '.' && part !== '..');
}

function parsePax(body) {
  const out = {};
  let offset = 0;
  while (offset < body.length) {
    const space = body.indexOf(0x20, offset);
    if (space < 0) throw new Error('Malformed PAX header');
    const length = Number(body.subarray(offset, space).toString('ascii'));
    if (!Number.isInteger(length) || length <= space - offset + 1 || offset + length > body.length) throw new Error('Malformed PAX length');
    const line = body.subarray(space + 1, offset + length - 1).toString('utf8');
    const equals = line.indexOf('=');
    if (equals > 0) out[line.slice(0, equals)] = line.slice(equals + 1);
    offset += length;
  }
  return out;
}

export function unpackPackage(archiveBytes, packageName, version, limits = {}) {
  if (archiveBytes.length > MAX_ARCHIVE) throw new Error('Archive exceeds 64 MiB');
  const byteLimit = Math.min(MAX_TREE_BYTES, limits.bytes ?? MAX_TREE_BYTES);
  const fileLimit = Math.min(MAX_TREE_FILES, limits.files ?? MAX_TREE_FILES);
  const rawLimit = Math.min(MAX_TREE_BYTES + MAX_ARCHIVE, byteLimit + fileLimit * 512 + 1024);
  const raw = gunzipSync(archiveBytes, { maxOutputLength: rawLimit });
  const files = new Map();
  let offset = 0;
  let longName = null;
  let pax = {};
  let archiveRoot = null;
  let extractedBytes = 0;
  let entries = 0;
  while (offset + 512 <= raw.length) {
    const header = raw.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    tarChecksum(header);
    const size = tarNumber(header.subarray(124, 136));
    const type = String.fromCharCode(header[156] || 48);
    const dataStart = offset + 512;
    if (!Number.isSafeInteger(size) || size < 0 || dataStart + size > raw.length) throw new Error('Malformed tar entry size');
    const body = raw.subarray(dataStart, dataStart + size);
    let name = `${tarString(header.subarray(345, 500)) ? `${tarString(header.subarray(345, 500))}/` : ''}${tarString(header.subarray(0, 100))}`;
    if (type === 'L') longName = tarString(body);
    else if (type === 'x' || type === 'g') pax = { ...pax, ...parsePax(body) };
    else {
      name = pax.path ?? longName ?? name;
      pax = {};
      longName = null;
      const cleanName = name.replace(/\/$/, '');
      const slash = cleanName.indexOf('/');
      const root = slash < 0 ? cleanName : cleanName.slice(0, slash);
      if (!root || !validRelative(root) || /[<>:"|?*\x00-\x1f]/.test(root)) throw new Error(`Invalid package root in archive: ${name}`);
      if (archiveRoot === null) archiveRoot = root;
      if (archiveRoot !== root) throw new Error(`Multiple top-level roots in package archive: ${archiveRoot}, ${root}`);
      const rel = slash < 0 ? '' : cleanName.slice(slash + 1);
      if (type !== '5' && rel) {
        entries += 1;
        if (entries > fileLimit) throw new Error('Laid-out tree exceeds 20,000 files');
        if (type !== '0' && type !== '\0') throw new Error(`Not a plain file: ${name}`);
        if (!validRelative(rel) || /[<>:"|?*\x00-\x1f]/.test(rel) || /[. ]$/.test(rel.split('/').at(-1))) throw new Error(`Non-portable archive path: ${name}`);
        const key = rel.toLocaleLowerCase('en-US');
        if (files.has(key)) throw new Error(`Duplicate or case-colliding path: ${name}`);
        extractedBytes += size;
        if (extractedBytes > byteLimit) throw new Error('Laid-out tree exceeds cumulative 64 MiB extracted cap');
        files.set(key, { path: rel, bytes: Buffer.from(body) });
      }
    }
    offset = dataStart + Math.ceil(size / 512) * 512;
  }
  if (!files.size) throw new Error('Empty package archive');
  const manifestEntry = files.get('package.json');
  if (!manifestEntry) throw new Error('Package has no root package.json');
  const manifest = JSON.parse(manifestEntry.bytes.toString('utf8'));
  if (manifest.name !== packageName || manifest.version !== version) throw new Error(`Package manifest identity mismatch: ${manifest.name}@${manifest.version}`);
  return { files, extractedBytes, fileCount: files.size, archiveBytes: archiveBytes.length, packageName, version, manifest };
}

export function packageTreeDigest(files) {
  const hash = createHash('sha256');
  const entries = [...files].map(([fullPath, file]) => [file.path ?? fullPath, file.bytes ?? file]).sort(([a], [b]) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  for (const [filePath, bytes] of entries) hash.update(filePath).update(Buffer.from([0])).update(String(bytes.length)).update(Buffer.from([0])).update(bytes);
  return hash.digest('hex');
}

export function checkPackageFiles(files) {
  const lifecycle = [];
  const native = [];
  for (const { path: filePath, bytes } of files.values()) {
    const lower = filePath.toLowerCase();
    if (lower === 'package.json') {
      const manifest = JSON.parse(bytes.toString('utf8'));
      for (const key of ['preinstall', 'install', 'postinstall']) if (Object.hasOwn(manifest.scripts ?? {}, key)) lifecycle.push({ hook: key, value: manifest.scripts[key] });
    }
    if (lower.endsWith('/package.json')) {
      const manifest = JSON.parse(bytes.toString('utf8'));
      for (const key of ['preinstall', 'install', 'postinstall']) if (Object.hasOwn(manifest.scripts ?? {}, key)) lifecycle.push({ file: filePath, hook: key, value: manifest.scripts[key] });
    }
    if (lower.endsWith('binding.gyp') || lower.endsWith('.node')) native.push(filePath);
  }
  return { lifecycle, native };
}

function runtimeDependencies(metadata) {
  return { ...(metadata.peerDependencies ?? {}), ...(metadata.dependencies ?? {}), ...(metadata.optionalDependencies ?? {}) };
}

function sameObject(a, b) {
  return JSON.stringify(Object.entries(a ?? {}).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0))
    === JSON.stringify(Object.entries(b ?? {}).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0));
}

async function resolvePackage(name, range, cache, tracker) {
  if (semver.valid(range) === range) {
    const metadata = await getJson(`${REGISTRY}/${encodeURIComponent(name)}/${encodeURIComponent(range)}`, tracker);
    if (metadata.name !== name || metadata.version !== range) throw new Error(`Registry metadata identity mismatch for ${name}@${range}`);
    if (!metadata.dist?.tarball || !/^sha512-[A-Za-z0-9+/]+={0,2}$/.test(metadata.dist.integrity ?? '')) throw new Error(`Missing tarball or SHA-512 integrity for ${name}@${range}`);
    return { metadata, name, version: range, integrity: metadata.dist.integrity, url: registryUrl(metadata.dist.tarball).href };
  }
  const versions = cache.get(name) ?? await getJson(`${REGISTRY}/${encodeURIComponent(name)}`, tracker, true);
  cache.set(name, versions);
  const choices = Object.keys(versions.versions ?? {}).filter((version) => satisfies(version, range)).sort(semver.rcompare);
  if (!choices.length) throw new Error(`No registry version of ${name} satisfies ${range}`);
  const version = choices[0];
  const metadata = versions.versions[version];
  if (!metadata.dist?.tarball || !/^sha512-[A-Za-z0-9+/]+={0,2}$/.test(metadata.dist.integrity ?? '')) throw new Error(`Missing tarball or SHA-512 integrity for ${name}@${version}`);
  const tarballUrl = registryUrl(metadata.dist.tarball).href;
  return { metadata, name, version, integrity: metadata.dist.integrity, url: tarballUrl };
}

async function downloadAndUnpack(packageInfo, remainingArchiveBytes) {
  const archiveLimit = Math.min(MAX_ARCHIVE, remainingArchiveBytes);
  const cachePath = path.join(packageInfo.cacheDirectory, `${Buffer.from(packageInfo.integrity.slice('sha512-'.length), 'base64').toString('hex')}.tgz`);
  let archive;
  try { archive = await readFile(cachePath); }
  catch (error) {
    if (error.code !== 'ENOENT') throw error;
    const response = await fetch(packageInfo.url, { redirect: 'error', signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS) });
    if (!response.ok) throw new Error(`Registry tarball returned ${response.status} for ${packageInfo.url}`);
    if (Number(response.headers.get('content-length')) > archiveLimit) throw new Error('Cumulative archive bytes exceed 64 MiB');
    const parts = [];
    let length = 0;
    for await (const part of response.body) {
      length += part.length;
      if (length > archiveLimit) throw new Error('Cumulative archive bytes exceed 64 MiB');
      parts.push(part);
    }
    archive = Buffer.concat(parts, length);
    await writeFile(cachePath, archive, { flag: 'wx' }).catch((writeError) => { if (writeError.code !== 'EEXIST') throw writeError; });
  }
  if (archive.length > archiveLimit) throw new Error('Cumulative archive bytes exceed 64 MiB');
  const actual = `sha512-${createHash('sha512').update(archive).digest('base64')}`;
  if (actual !== packageInfo.integrity) throw new Error(`SHA-512 integrity mismatch for ${packageInfo.name}@${packageInfo.version}`);
  try {
    return unpackPackage(archive, packageInfo.name, packageInfo.version, { bytes: packageInfo.remainingExtractedBytes, files: packageInfo.remainingFiles });
  } catch (error) {
    error.archiveBytes = archive.length;
    throw error;
  }
}

export async function curateCandidate(name, version, outputDirectory) {
  const tracker = { metadataBytes: 0 };
  const metadata = await getJson(`${REGISTRY}/${encodeURIComponent(name)}/${encodeURIComponent(version)}`, tracker);
  if (metadata.name !== name || metadata.version !== version) throw new Error(`Registry returned metadata for ${metadata.name}@${metadata.version}, expected ${name}@${version}`);
  const root = { metadata, name, version, integrity: metadata.dist?.integrity, url: metadata.dist?.tarball };
  if (!/^sha512-[A-Za-z0-9+/]+={0,2}$/.test(root.integrity ?? '')) throw new Error(`Missing root SHA-512 for ${name}@${version}`);
  registryUrl(root.url);
  const cache = new Map([[name, { versions: { [version]: metadata } }]]);
  const archiveCacheDirectory = path.join(outputDirectory, 'archive-cache');
  await mkdir(archiveCacheDirectory, { recursive: true });
  const packages = new Map();
  const locations = new Map();
  const combined = new Map();
  const packageAudits = [];
  const queue = [{ info: root, installPath: '' }];
  let extractedBytes = 0;
  let archiveBytes = 0;
  let fileCount = 0;
  const reasons = [];
  let treeComplete = true;
  while (queue.length) {
    const current = queue.shift();
    if (packages.size >= MAX_PACKAGES) {
      reasons.push(`Laid-out tree exceeds ${MAX_PACKAGES} packages`);
      treeComplete = false;
      break;
    }
    const packageKey = current.installPath.toLocaleLowerCase('en-US');
    if (packages.has(packageKey)) continue;
    let unpacked;
    try {
      unpacked = await downloadAndUnpack({ ...current.info, cacheDirectory: archiveCacheDirectory, remainingExtractedBytes: MAX_TREE_BYTES - extractedBytes, remainingFiles: MAX_TREE_FILES - fileCount }, MAX_TREE_ARCHIVES - archiveBytes);
    } catch (error) {
      archiveBytes += error.archiveBytes ?? 0;
      reasons.push(`${current.info.name}@${current.info.version}: ${error.message}`);
      treeComplete = false;
      break;
    }
    if (!sameObject(runtimeDependencies(current.info.metadata), runtimeDependencies(unpacked.manifest))) {
      reasons.push(`${current.info.name}@${current.info.version} registry runtime dependencies differ from package.json`);
      treeComplete = false;
      break;
    }
    const bundles = unpacked.manifest.bundleDependencies ?? unpacked.manifest.bundledDependencies ?? [];
    extractedBytes += unpacked.extractedBytes;
    archiveBytes += unpacked.archiveBytes;
    fileCount += unpacked.fileCount;
    const audit = checkPackageFiles(unpacked.files);
    if (audit.lifecycle.length) reasons.push(`${current.info.name}@${current.info.version} lifecycle scripts: ${audit.lifecycle.map(({ file, hook, value }) => `${file ?? 'package.json'} ${hook}=${JSON.stringify(value)}`).join(', ')}`);
    if (audit.native.length) reasons.push(`${current.info.name}@${current.info.version} native artifacts: ${audit.native.join(', ')}`);
    if (bundles === true || (Array.isArray(bundles) && bundles.length)) {
      reasons.push(`${current.info.name}@${current.info.version} bundles dependencies: ${bundles === true ? 'all package dependencies' : bundles.join(', ')}`);
      treeComplete = false;
    }
    for (const { path: relative, bytes } of unpacked.files.values()) {
      const fullPath = current.installPath ? `${current.installPath}/${relative}` : relative;
      const normalized = fullPath.toLocaleLowerCase('en-US');
      if (combined.has(normalized)) throw new Error(`Laid-out tree path collision: ${fullPath}`);
      combined.set(normalized, { path: fullPath, bytes });
    }
    const packageRecord = {
      path: current.installPath,
      package: current.info.name,
      version: current.info.version,
      integrity: current.info.integrity,
      url: current.info.url,
    };
    packages.set(packageKey, packageRecord);
    locations.set(packageKey, { name: current.info.name, version: current.info.version });
    packageAudits.push({ path: current.installPath, archiveBytes: unpacked.archiveBytes, extractedBytes: unpacked.extractedBytes, fileCount: unpacked.fileCount, lifecycle: audit.lifecycle, native: audit.native, dependencies: runtimeDependencies(current.info.metadata) });
    for (const [dependencyName, dependencyRange] of Object.entries(runtimeDependencies(current.info.metadata)).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
      try {
        const resolved = await resolvePackage(dependencyName, dependencyRange, cache, tracker);
        let base = current.installPath;
        let existing;
        while (true) {
          const possible = `${base ? `${base}/` : ''}node_modules/${dependencyName}`.toLocaleLowerCase('en-US');
          existing = locations.get(possible) ?? queue.find((item) => item.installPath.toLocaleLowerCase('en-US') === possible)?.info;
          if (existing) break;
          const marker = base.lastIndexOf('/node_modules/');
          if (marker >= 0) base = base.slice(0, marker);
          else if (base.startsWith('node_modules/')) base = '';
          else break;
        }
        if (existing && satisfies(existing.version, dependencyRange)) continue;
        const hoistedPath = `node_modules/${dependencyName}`;
        const hoistedKey = hoistedPath.toLocaleLowerCase('en-US');
        const occupied = locations.has(hoistedKey) || queue.some((item) => item.installPath.toLocaleLowerCase('en-US') === hoistedKey);
        const installPath = occupied ? `${current.installPath ? `${current.installPath}/` : ''}node_modules/${dependencyName}` : hoistedPath;
        const installKey = installPath.toLocaleLowerCase('en-US');
        if (locations.has(installKey) || queue.some((item) => item.installPath.toLocaleLowerCase('en-US') === installKey)) {
          reasons.push(`${dependencyName} (${dependencyRange}) has an incompatible installed version at ${installPath}`);
          treeComplete = false;
          continue;
        }
        queue.push({ info: resolved, installPath });
      } catch (error) {
        reasons.push(`${dependencyName} (${dependencyRange}) required by ${current.info.name}@${current.info.version}: ${error.message}`);
        treeComplete = false;
      }
    }
  }
  const tree = [...packages.values()].sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const report = {
    candidate: name,
    requestedVersion: version,
    admitted: reasons.length === 0 && treeComplete && packages.size > 0,
    reason: [...new Set(reasons)],
    root: { path: '', package: name, version, integrity: root.integrity, url: root.url },
    tree,
    treeDigest: packageTreeDigest(combined),
    treeComplete,
    treeDigestScope: treeComplete ? 'Complete laid-out file tree.' : 'Partial tree only; digest is not a candidate pin.',
    metrics: { archiveBytes, extractedBytes, fileCount, packageCount: tree.length },
    audit: { packages: packageAudits, registryOnly: true, metadataBytes: tracker.metadataBytes, license: metadata.license ?? null, gitHead: metadata.gitHead ?? null, repository: metadata.repository ?? null, bin: metadata.bin ?? null, policyReview: 'Credentials, account requirements, behavior, and runtime entry selection remain manual.' },
  };
  report.technicalStatus = report.reason.length ? 'excluded' : 'eligible';
  await mkdir(outputDirectory, { recursive: true });
  const fileName = `${name.replaceAll('/', '__').replaceAll('@', '')}.json`;
  await writeFile(path.join(outputDirectory, fileName), `${JSON.stringify(report, null, 2)}\n`);
  return report;
}

export async function generateAll(outputDirectory) {
  await mkdir(outputDirectory, { recursive: true });
  const results = [];
  for (let offset = 0; offset < candidates.length; offset += 2) {
    const batch = candidates.slice(offset, offset + 2);
    const batchResults = await Promise.all(batch.map(async ([name, version]) => {
      process.stdout.write(`START ${name}@${version}\n`);
      try {
        const report = await curateCandidate(name, version, outputDirectory);
        process.stdout.write(`DONE ${name}@${version} ${report.technicalStatus} (${report.tree.length} packages, ${report.metrics.extractedBytes} bytes)\n`);
        return report;
      } catch (error) {
        process.stdout.write(`DONE ${name}@${version} excluded (${error.message})\n`);
        return { candidate: name, requestedVersion: version, admitted: false, technicalStatus: 'excluded', reason: [error.message] };
      }
    }));
    results.push(...batchResults);
  }
  await writeFile(path.join(outputDirectory, 'report.json'), `${JSON.stringify({ generatedAt: new Date().toISOString(), results }, null, 2)}\n`);
  return results;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  const outputDirectory = process.argv[2];
  if (!outputDirectory) throw new Error('Usage: node scripts/pin-tool-server.mjs <outside-repo-report-directory>');
  const results = await generateAll(outputDirectory);
  process.stdout.write(`${JSON.stringify(results.map(({ candidate, admitted, reason }) => ({ candidate, admitted, reason })))}\n`);
}
