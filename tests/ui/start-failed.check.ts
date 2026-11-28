import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { StartFailed } from '../../ui/StartFailed.tsx';
import { mac } from '../../ui/WindowControls.tsx';
import { check, equal, ok } from './check.ts';
import { t } from '../../ui/i18n.ts';

// 2026-11-27: on a fresh Windows the host could not load (0xC0000135) and Faber closed without a word. The screen that
// replaces everything then says so, names the code and offers its three ways out, in every language.
check('start failure: Faber says it could not start, names the code and offers start again, report and quit', async () => {
  for (const messages of [en, ptBR, es, fr, de] as Record<string, unknown>[]) {
    for (const key of ['title', 'body', 'code', 'restart', 'report', 'quit']) {
      const text = messages[`startFailed.${key}`];
      ok(typeof text === 'string' && text.trim(), `startFailed.${key} is written in every language`);
    }
    ok(String(messages['startFailed.code']).includes('{code}'), 'every language shows the code');
  }
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(StartFailed, { code: '0xC0000135' }), host);
  try {
    await new Promise((done) => requestAnimationFrame(done));
    equal(host.querySelector('h1')?.textContent, t('startFailed.title'));
    ok(host.querySelector('[role="alert"]')?.textContent === t('startFailed.body'), 'the explanation is announced');
    ok(host.textContent!.includes('0xC0000135'), 'the exit code is on screen to report');
    const buttons = [...host.querySelectorAll<HTMLButtonElement>('.start-failed-actions button')].map((b) => b.textContent);
    equal(JSON.stringify(buttons), JSON.stringify([t('startFailed.restart'), t('startFailed.report'), t('startFailed.quit')]));
    equal(document.activeElement, host.querySelector('.start-failed-actions button'), 'Start again has the focus');
    // macOS draws its own traffic lights over the bar; Windows and Linux get Faber's buttons.
    equal(Boolean(host.querySelector('.caption')), !mac, 'the window keeps its own buttons');
  } finally {
    dispose();
    host.remove();
  }
});
