import assert from 'node:assert/strict';
import { mkdtemp, mkdir, realpath, rm, writeFile, link, symlink, readdir } from 'node:fs/promises';
import path from 'node:path';
import { test } from 'node:test';
import { PaneExtensions, resetExtensionStorage, resolveExtensionFolder } from '../electron/pane-extensions.ts';

test('an agent can only load a real unpacked folder within its working copy', async (t) => {
  const temporary = await mkdtemp(path.resolve('node_modules', '.extension-test-'));
  t.after(() => rm(temporary, { recursive: true, force: true }));
  const root = await realpath(temporary);
  const project = path.join(root, 'project');
  const extension = path.join(project, 'extension');
  const outside = path.join(root, 'project-other');
  await mkdir(extension, { recursive: true });
  await mkdir(outside);
  const manifest = JSON.stringify({ name: 'Test extension', version: '1.0', manifest_version: 3 });
  await writeFile(path.join(extension, 'manifest.json'), manifest);
  await writeFile(path.join(outside, 'manifest.json'), manifest);

  assert.equal(await resolveExtensionFolder(extension, project), extension);
  await assert.rejects(resolveExtensionFolder(outside, project));
  await assert.rejects(resolveExtensionFolder(project, project));
  const external = path.join(outside, 'content.js');
  await writeFile(external, '// Example content script');
  await link(external, path.join(extension, 'content.js'));
  await assert.rejects(resolveExtensionFolder(extension, project));
});

test('folder validation rejects directory aliases, linked resources and a non-file manifest', async (t) => {
  const temporary = await mkdtemp(path.resolve('node_modules', '.extension-test-'));
  t.after(() => rm(temporary, { recursive: true, force: true }));
  const root = await realpath(temporary);
  const folder = path.join(root, 'unpacked');
  const alias = path.join(root, 'alias');
  await mkdir(folder);
  await mkdir(path.join(folder, 'manifest.json'));
  await assert.rejects(resolveExtensionFolder(folder));
  await rm(path.join(folder, 'manifest.json'), { recursive: true });
  await writeFile(path.join(folder, 'manifest.json'), '{"name":"Test","version":"1","manifest_version":3}');
  await symlink(folder, alias, 'junction');
  await assert.rejects(resolveExtensionFolder(alias));
  await symlink(root, path.join(folder, 'resources'), 'junction');
  await assert.rejects(resolveExtensionFolder(folder));
});

test('startup removes old developer profiles and refuses an aliased storage root', async (t) => {
  const parent = await realpath(await mkdtemp(path.resolve('node_modules', '.extension-storage-test-')));
  t.after(() => rm(parent, { recursive: true, force: true }));
  const root = path.join(parent, 'profiles');
  const other = path.join(parent, 'other');
  await mkdir(path.join(root, '0'), { recursive: true });
  await mkdir(other);
  await writeFile(path.join(root, '0', 'leftover'), 'old browser data');
  assert.equal(await resetExtensionStorage(root), root);
  assert.deepEqual(await readdir(root), []);
  await rm(root, { recursive: true });
  await symlink(other, root, 'junction');
  await assert.rejects(resetExtensionStorage(root));
  assert.deepEqual(await readdir(other), []);
});

test('registrations survive restart and protected locations are rejected', async (t) => {
  const root = await realpath(await mkdtemp(path.resolve('node_modules', '.extension-registry-test-')));
  t.after(() => rm(root, { recursive: true, force: true }));
  const registry = path.join(root, 'registry.json');
  const folder = path.join(root, 'extension');
  const configuredHome = path.join(root, 'configured-home');
  const protectedNested = path.join(configuredHome, 'extension');
  const alias = path.join(root, 'home-alias');
  for (const directory of [folder, protectedNested]) {
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, 'manifest.json'), '{"name":"Test","version":"1","manifest_version":3}');
  }
  await symlink(configuredHome, alias, 'junction');
  const app = () => new PaneExtensions(() => undefined, () => assert.fail('no profile needed'), async () => {}, registry, [alias]);
  const first = app();
  await first.mode(true);
  await assert.rejects(first.act({ scope: 'one', action: 'load', folder: protectedNested }));
  await assert.rejects(resolveExtensionFolder(root, undefined, [configuredHome]));
  await first.act({ scope: 'one', action: 'load', folder });
  await first.mode(false);
  const restarted = app();
  assert.throws(() => restarted.act({ scope: 'one', action: 'list' }), /mode is off/);
  await restarted.mode(true);
  assert.deepEqual(await restarted.act({ scope: 'one', action: 'list' }), [{ name: 'Test', version: '1', folder }]);
  assert.deepEqual(await restarted.act({ scope: 'other', action: 'list' }), []);
  await restarted.mode(false);
  await restarted.forget('one');
  const last = app();
  await last.mode(true);
  assert.deepEqual(await last.act({ scope: 'one', action: 'list' }), []);
});
