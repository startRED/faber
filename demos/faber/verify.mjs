import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { ensureDirectory, sourcePin, verifiedSource, writeGenerated } from '../../scripts/stageshow.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const pin = sourcePin(JSON.parse(await readFile(path.join(root, 'engine-projects/src/tools.json'), 'utf8')));
const upstream = await verifiedSource(pin);
const audit = path.join(upstream, 'audit');
await ensureDirectory(audit);
const content = JSON.parse(await readFile(path.join(upstream, 'examples/faber/content.json'), 'utf8'));
const { launch } = await import(pathToFileURL(path.join(upstream, 'scripts/browser.mjs')).href);
const browser = await launch();
const observations = [];
try {
  for (const width of [960, 390]) {
    const page = await browser.newPage({ viewport: { width, height: 1000 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      window.__demoStatus = null;
      window.addEventListener('message', e => { if (e.origin === location.origin && e.data?.type === 'stageshow:status') window.__demoStatus = e.data; });
      if (parent === window) return;
      // Capture native frames before the iframe installs its virtual playback clock.
      const nativeFrame = requestAnimationFrame.bind(window);
      const nativeNow = performance.now.bind(performance);
      const nativeTimer = setTimeout.bind(window);
      const samples = [];
      window.__faberTrace = { samples, done: false };
      let start;
      // Observe after the frame's playback callbacks, including their React commits.
      const next = () => nativeFrame(() => nativeTimer(tick, 0));
      function tick() {
        const timestamp = nativeNow();
        const scene = document.querySelector('.faber-frame');
        const stage = document.querySelector('.ss-stage');
        if (!scene || stage?.hasAttribute('data-placing')) { next(); return; }
        start ??= timestamp;
        const rect = (selector) => {
          const element = document.querySelector(selector);
          if (!element) return null;
          const r = element.getBoundingClientRect();
          return { x: r.x, y: r.y, width: r.width, height: r.height, opacity: Number(getComputedStyle(element).opacity) };
        };
        samples.push({ t: timestamp - start, sceneT: performance.now(), phase: Number(scene.getAttribute('data-phase')), mark: stage?.getAttribute('data-mark'), preview: scene.hasAttribute('data-preview'), draft: document.querySelector('[data-target="request"] .ss-field__chars')?.textContent ?? '', field: rect('[data-target="request"]'), conversation: rect('.faber-conversation'), progress: rect('.faber-progress'), result: rect('.faber-result'), pane: rect('.faber-preview'), cursor: rect('.ss-cursor') });
        if (timestamp - start < 15500) next();
        else window.__faberTrace.done = true;
      }
      next();
    });
    await page.goto('http://127.0.0.1:4187/faber.html');
    await page.waitForFunction(() => !document.querySelector('#toggle').disabled);
    await page.locator('#toggle').focus();
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
    const paused = await page.evaluate(() => window.__demoStatus.elapsed);
    await page.waitForTimeout(300);
    assert.equal(await page.evaluate(() => window.__demoStatus.elapsed), paused, 'Pause freezes virtual time');
    await page.keyboard.press('Tab');
    assert.equal(await page.evaluate(() => document.activeElement.id), 'step');
    await page.keyboard.press('Space');
    await page.waitForFunction(before => window.__demoStatus.elapsed > before, paused);
    await page.locator('#toggle').click();
    await page.locator('#slow').check();
    assert.equal(await page.locator('#slow').isChecked(), true);
    await page.locator('#replay').click();
    await page.waitForFunction(() => !document.querySelector('#toggle').disabled);
    await page.locator('#slow').uncheck();
    const frame = page.frames().find(frame => frame.url().includes('embed=faber'));
    await frame.waitForSelector('.faber-frame[data-phase="1"]', { timeout: 8000 });
    await frame.waitForSelector('.faber-frame[data-phase="3"]', { timeout: 8000 });
    await frame.waitForSelector('.faber-frame[data-preview]', { timeout: 15000 });
    await frame.waitForFunction(() => window.__faberTrace?.done, undefined, { timeout: 20000 });
    const trace = await frame.evaluate(() => window.__faberTrace.samples);
    await writeGenerated(path.join(audit, `faber-trace-observed-${Date.now()}-${width}.json`), JSON.stringify({ width, samples: trace }));
    const intervals = trace.slice(1).map((sample, i) => sample.t - trace[i].t);
    const maxGapMs = Math.max(...intervals);
    assert.ok(trace.length > 300 && trace.at(-1).t >= 15500);
    assert.ok(intervals.every((gap) => gap > 0 && gap <= 100), `Native playback had an unobserved interval of ${maxGapMs.toFixed(1)}ms`);
    const marks = [...new Set(trace.map((sample) => sample.mark).filter(Boolean))];
    assert.deepEqual(marks, ['request', 'work', 'result', 'review']);
    assert.deepEqual([...new Set(trace.map((sample) => sample.phase))], [0, 1, 2, 3, 4]);
    let typed = '';
    const writing = trace.filter((sample) => sample.phase === 0);
    for (const sample of writing) {
      assert.ok(content.request.startsWith(sample.draft) && sample.draft.startsWith(typed), 'Typing only appends the requested characters');
      typed = sample.draft;
    }
    assert.equal(typed, content.request);
    assert.ok(Math.max(...writing.map((sample) => sample.field.width)) - Math.min(...writing.map((sample) => sample.field.width)) < 0.5, 'Characters may grow; their field must stay still');
    let maxSceneSpatialSpeed = 0;
    let maxNativeSpatialSpeed = 0;
    for (let i = 1; i < trace.length; i++) {
      for (const name of ['conversation', 'progress', 'result', 'pane', 'cursor']) {
        const previous = trace[i - 1][name];
        const current = trace[i][name];
        if (!previous || !current || previous.opacity < 0.1 || current.opacity < 0.1 || previous.width === 0 || current.width === 0 || trace[i - 1].phase !== trace[i].phase) continue;
        const distance = Math.hypot(current.x - previous.x, current.y - previous.y);
        maxNativeSpatialSpeed = Math.max(maxNativeSpatialSpeed, Math.max(0, distance - 2) / intervals[i - 1] * 1000);
        const sceneElapsed = trace[i].sceneT - trace[i - 1].sceneT;
        assert.ok(sceneElapsed >= 0 && (sceneElapsed > 0 || distance <= 2), `${name} moved without advancing its animation clock`);
        const speed = sceneElapsed > 0 ? Math.max(0, distance - 2) / sceneElapsed * 1000 : 0;
        maxSceneSpatialSpeed = Math.max(maxSceneSpatialSpeed, speed);
        assert.ok(speed < 6500, `${name} made a large authored spatial leap (${speed.toFixed(0)}px per scene second)`);
      }
    }
    const final = trace.filter((sample) => sample.t >= trace.at(-1).t - 2000);
    assert.ok(final.every((sample) => sample.phase === 4 && sample.preview && sample.pane.opacity > 0.99));
    assert.ok(Math.max(...final.map((sample) => sample.conversation.x)) - Math.min(...final.map((sample) => sample.conversation.x)) < 0.5, 'The final result must hold still');
    await writeGenerated(path.join(audit, `faber-trace-${width}.json`), JSON.stringify({ width, frames: trace.length, maxGapMs, maxSceneSpatialSpeed, maxNativeSpatialSpeed, marks, stableFinalMs: 2000, samples: trace }));
    assert.equal(await frame.locator('.faber-frame').getAttribute('data-phase'), '4');
    assert.ok(await frame.locator('.faber-disclosure').isVisible());
    assert.ok(await frame.locator('.faber-preview').isVisible());
    await page.locator('#toggle').click();
    await writeGenerated(path.join(audit, `faber-viewer-${width}.png`), await page.screenshot({ fullPage: true }));
    assert.equal(await frame.evaluate(() => getComputedStyle(document.body).margin), '0px', 'The embedded frame must not retain the browser’s white default margin');
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Performance.enable');
    const first = (await cdp.send('Performance.getMetrics')).metrics;
    await page.waitForTimeout(600);
    const second = (await cdp.send('Performance.getMetrics')).metrics;
    const value = (metrics, name) => metrics.find(metric => metric.name === name)?.value;
    observations.push({ width, pausedVirtualTime: paused, fullStory: true, playPauseStepSlowReplay: true, trace: { frames: trace.length, maxGapMs, maxSceneSpatialSpeed, maxNativeSpatialSpeed, marks, stableFinalMs: 2000 }, pageErrors: errors, pausedHeapBytes: value(second, 'JSHeapUsedSize'), pausedTaskSecondsOver600ms: value(second, 'TaskDuration') - value(first, 'TaskDuration') });
    assert.deepEqual(errors, []);
    await page.close();
  }
  const reduced = await browser.newPage({ viewport: { width: 390, height: 1000 }, reducedMotion: 'reduce' });
  await reduced.goto('http://127.0.0.1:4187/faber.html');
  await reduced.waitForFunction(() => document.querySelector('#toggle').disabled);
  await reduced.frameLocator('#scene').locator('.ss-stage[data-still]').waitFor();
  const still = reduced.frames().find(frame => frame.url().includes('embed=faber'));
  await still.waitForSelector('.ss-stage[data-still]');
  assert.equal(await still.locator('.faber-frame').getAttribute('data-phase'), '4');
  assert.equal(await still.locator('.ss-cursor').count(), 0);
  await writeGenerated(path.join(audit, 'faber-reduced-390.png'), await reduced.screenshot({ fullPage: true }));
  observations.push({ reducedMotion: true, posterResult: true, cursorAbsent: true });
  await reduced.close();
  const recovery = await browser.newPage({ viewport: { width: 960, height: 1000 } });
  await recovery.route('**/*embed=faber*', route => route.abort());
  await recovery.goto('http://127.0.0.1:4187/faber.html');
  await recovery.locator('#error').waitFor({ state: 'visible', timeout: 13000 });
  await recovery.unroute('**/*embed=faber*');
  await recovery.locator('#replay').click();
  await recovery.waitForFunction(() => !document.querySelector('#toggle').disabled);
  assert.equal(await recovery.locator('#error').isHidden(), true);
  observations.push({ blockedFrameTimeout: true, replayRecovery: true });
  await recovery.close();
  await writeGenerated(path.join(audit, 'faber-controls.json'), JSON.stringify(observations, null, 2));
  console.log(JSON.stringify(observations, null, 2));
} finally { await browser.close(); }

