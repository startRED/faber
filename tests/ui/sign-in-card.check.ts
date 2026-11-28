import { createComponent } from 'solid-js';
import { render } from 'solid-js/web';
import { api, type Message } from '../../ui/api.ts';
import { t } from '../../ui/i18n.ts';
import { SignInCard } from '../../ui/SignInCard.tsx';
import { check, equal, ok } from './check.ts';

const frame = () => new Promise((done) => setTimeout(done, 30));
const refused = { id: 'r1', role: 'assistant', text: '', status: 'error', vendor: 'claude', error: 'error.billingUnknown' } as unknown as Message;

// 2026-11-27: a fresh install with Claude installed but not signed in sent its first message and read only that Auto needs a
// subscription confirmed. The reply now says the account is signed out and signs in from there.
check('sign-in card: a refused reply on a signed-out account offers its sign-in, then says to send again', async () => {
  const before = { check: api.setupCheck, signIn: api.setupSignIn };
  let account: 'signedOut' | 'signedIn' = 'signedOut';
  const signIns: string[] = [];
  api.setupCheck = async (vendor) => ({ vendor, installed: true, account, canInstall: true });
  api.setupSignIn = async (vendor) => { signIns.push(vendor); };
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => createComponent(SignInCard, { m: refused }), host);
  try {
    await frame();
    ok(host.textContent!.includes(t('signIn.needed', { vendor: 'Claude' })), 'the reply says Claude is not signed in');
    const button = host.querySelector<HTMLButtonElement>('.sign-in-card button')!;
    equal(button.textContent, t('setup.signIn'));
    button.click();
    await frame();
    equal(signIns.join(), 'claude', 'its own sign-in starts, for that account');
    ok(host.textContent!.includes(t('signIn.waiting')), 'then it says to finish in the window that opened');
    account = 'signedIn';
    host.querySelector<HTMLButtonElement>('.sign-in-card button')!.click();
    await frame();
    ok(host.textContent!.includes(t('signIn.done', { vendor: 'Claude' })), 'signed in, it says to send again');
  } finally {
    dispose();
    host.remove();
    api.setupCheck = before.check;
    api.setupSignIn = before.signIn;
  }
});

check('sign-in card: nothing for a signed-in account or another refusal', async () => {
  const before = api.setupCheck;
  api.setupCheck = async (vendor) => ({ vendor, installed: true, account: 'signedIn', canInstall: true });
  const host = document.createElement('div');
  document.body.append(host);
  const dispose = render(() => [
    createComponent(SignInCard, { m: refused }),
    createComponent(SignInCard, { m: { ...refused, error: 'error.quotaOut' } as Message }),
  ], host);
  try {
    await frame();
    equal(host.querySelector('.sign-in-card'), null, 'a confirmed or other refusal keeps its own words only');
  } finally {
    dispose();
    host.remove();
    api.setupCheck = before;
  }
});
