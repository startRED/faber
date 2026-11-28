import assert from 'node:assert/strict';
import { mkdir, mkdtemp, rm, rmdir, symlink, unlink } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { resolveConfig } from 'vite';

test('Vite validates the test output before opening a production build folder', async () => {
  const parent = path.resolve('.faber/verify');
  await mkdir(parent, { recursive: true });
  const copy = await mkdtemp(path.join(parent, 'build-isolation-'));
  try {
    const root = path.join(copy, 'ui');
    const production = path.join(copy, 'dist');
    const isolated = path.join(copy, 'dist-test');
    await mkdir(root);
    await mkdir(production);
    const configure = (mode, outDir) => resolveConfig({
      configFile: path.resolve('vite.config.ts'),
      root,
      mode,
      build: { outDir },
    }, 'build');

    await assert.rejects(configure('test', '../dist'), /test build.*dist-test/i);
    await configure('test', '../dist-test');
    await configure('production', '../dist');

    await symlink(production, isolated, 'junction');
    await assert.rejects(configure('test', '../dist-test'), /linked output/i);

    await unlink(isolated);
    await rmdir(production);
    await symlink(isolated, production, 'junction');
    await assert.rejects(configure('test', '../dist-test'), /linked output/i);
  } finally {
    await rm(copy, { recursive: true, force: true });
  }
});
