import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import type { Appearance, CheckResult, Look, QuotaReading, Settings as Saved } from '../../ui/api.ts';
import { AppearanceSection } from '../../ui/Appearance.tsx';
import { previewer } from '../../ui/appearance.ts';
import { Mini } from '../../ui/FirstRun.tsx';
import { factory, failingPairs, groundColor, groundKey, normalizeHex, readings, sameChoices, shown } from '../../ui/draft.ts';
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { accountLine, seedQuotas, usageRows } from '../../ui/sidebar.ts';
import { Settings } from '../../ui/Settings.tsx';
import { check, deepEqual, equal, ok } from './check.ts';

check('R107: Settings names Chat and Projects and discloses the shared default', () => {
  ok(en['set.permissions'].includes('Chat'));
  ok(en['set.permissions'].includes('Projects'));
  ok('project.permissionsDontAsk' in en);
  ok(en['set.chatFolderHint'].includes('first message'));
  ok(en['set.chatFolderHint'].includes('private folder'));
  ok(en['set.chatFolderHint'].includes('delete the conversation'));
});

check('R121: Do not ask is one calm line in the thread, with the risk and the credential limits spelled out in Settings', () => {
  const line = en['project.permissionsDontAsk'];
  ok(line.length < 60 && !/risk|danger|warning|error|denied|blocked/i.test(line), line);
  ok(en['project.permissionsDetails'].includes('Settings'));
  const body = en['set.permissionsDontAskBody'];
  for (const word of ['internet', 'any folder', 'steer', 'Ask before commands', '.ssh', 'Both tools can still reach', 'through commands']) ok(body.includes(word), word);
  ok(!/is kept out|cannot be kept out|Codex cannot/.test(body), 'no claim that one tool is kept away from the sign-in folders');
  ok(body.split('. ').length <= 5, 'one short paragraph');
  ok(!/sandbox|too broad|disabled/i.test(body), 'full access no longer describes a sandbox or disabled integrations');
  const ask = en['set.permissionsAskBody'];
  ok(ask.includes('stops tasks that are running') && ask.includes('before changing a file') && ask.includes('plain reading commands'), ask);
  ok(en['error.workingFolder'].includes('system files') && en['error.workingFolder'].includes('sign-in keys'));
});

const look = (over: Partial<Look> = {}): Look => ({
  vars: {},
  scheme: 'dark',
  motion: 'full',
  effects: { ambient: true, texture: true, glow: true },
  checks: [],
  notices: [],
  appearance: {
    base: 'sala',
    ground: 'sala',
    accent: '#f3f0e8',
    textFont: 'archivo',
    headingFont: 'narrow',
    textSize: 14,
    density: 1,
    corners: 1,
    motion: 'full',
    effects: { ambient: true, texture: true, glow: true },
  },
  ...over,
});

check('settings: no appearance is the factory look, and key order or an empty field changes nothing', () => {
  ok(sameChoices(undefined, factory()));
  ok(sameChoices({ base: 'sala', accent: '#abcdef', corners: 1 }, { corners: 1, accent: '#abcdef', base: 'sala' }));
  ok(sameChoices({ base: 'sala', accent: undefined }, { base: 'sala' }));
  ok(sameChoices({ base: 'sala', effects: { ambient: true, texture: false, glow: true } }, { effects: { glow: true, texture: false, ambient: true }, base: 'sala' }));
  equal(sameChoices({ base: 'sala' }, { base: 'oficina' }), false);
  equal(sameChoices({ base: 'sala', accent: '#abcdef' }, { base: 'sala' }), false);
  equal(sameChoices(undefined, { base: 'oficina' }), false);
});

check('settings: the controls show the draft over the last answer, and the answer fills in what the draft leaves out', () => {
  const answer = look();
  const own: Appearance = { base: 'sala', accent: '#123456', headingFont: undefined };
  const seen = shown(own, answer);
  equal(seen.accent, '#123456', 'the draft wins before its answer arrives');
  equal(seen.textFont, 'archivo', 'a field the draft leaves out shows what the look gives it');
  equal(seen.headingFont, 'narrow', 'an undefined field does not hide the look\'s own');
});

check('settings: the preview panel reads the three contrast pairs, and a failing pair blocks "Use this look"', () => {
  const pairs = [
    { a: 'ink', b: 'bg', ratio: 14.2, min: 7, pass: true },
    { a: 'ink-3', b: 'bg', ratio: 4.1, min: 4.5, pass: false },
    { a: 'accent', b: 'bg', ratio: 9, min: 3, pass: true },
    { a: 'ink-2', b: 's1', ratio: 6, min: 4.5, pass: true },
  ];
  const l = look({ checks: pairs });
  deepEqual(
    readings(l).map((r) => [r.id, r.pass]),
    [
      ['text', true],
      ['faint', false],
      ['accent', true],
    ],
  );
  equal(failingPairs(l), 'ink-3 / bg');
  equal(failingPairs(look({ checks: pairs.map((p) => ({ ...p, pass: true })) })), '');
  deepEqual(readings(look()), [], 'a look with no readings shows none');
});

check('settings: a typed colour is read as #rrggbb in lower case or refused', () => {
  equal(normalizeHex('#F0A030'), '#f0a030');
  equal(normalizeHex('f0a030'), '#f0a030');
  equal(normalizeHex(' #abc '), '#aabbcc');
  equal(normalizeHex('abc'), '#aabbcc');
  for (const bad of ['', '#', '#12', '#12345', '#1234567', 'red', '#gggggg', '#12 456']) equal(normalizeHex(bad), undefined, bad);
});

check('settings: every ground chip has a colour, taken from the theme or preset that uses it', () => {
  for (const id of themes['user-controls'].grounds) ok(/^(#|rgb|hsl|color-mix)/.test(groundColor(id)), `${id}: ${groundColor(id)}`);
});

check('settings: every look names the ground it reports, even High contrast, which keeps its own', () => {
  const controls = themes['user-controls'];
  for (const id of [...controls.themes, ...controls.presets]) {
    ok(groundKey(id) in en, `${id}: no words for ${groundKey(id)}`);
  }
  equal(groundKey('manha'), 'look.ground.manha');
  equal(groundKey('alto'), 'look.alto.name');
});

check('settings: the previewer hands the draft back with its answer, and only for the newest request', async () => {
  const calls: Array<(l: Look) => void> = [];
  const got: string[] = [];
  const ask = previewer(
    () => new Promise<Look>((resolve) => calls.push(resolve)),
    (_look, draft) => got.push(draft.base ?? ''),
    () => {},
  );
  const run = [ask({ base: 'sala' }), ask({ base: 'oficina' })];
  calls[0]!(look());
  calls[1]!(look());
  await Promise.all(run);
  deepEqual(got, ['oficina']);
});

const reading = (vendor: 'claude' | 'codex', window: string, used: number, resetsAt: number): QuotaReading => ({ vendor, window, used, resetsAt });

check('settings: usage lines are the short window first; an old reading keeps its name and loses its numbers; none gives one empty line', () => {
  const now = 1_000_000_000_000;
  const future = Math.floor(now / 1000) + 3600;
  const past = Math.floor(now / 1000) - 60;
  const quotas = seedQuotas([reading('claude', 'seven_day', 0.4, future), reading('claude', 'five_hour', 0.8, future), reading('claude', 'opus', 0.1, past), reading('codex', 'five_hour', 0.2, 0)]);
  deepEqual(usageRows(quotas, 'claude', now), [
    { window: 'five_hour', used: 0.8, resetsAt: future },
    { window: 'seven_day', used: 0.4, resetsAt: future },
    { window: 'opus' },
  ]);
  deepEqual(usageRows(quotas, 'codex', now), [{ window: 'five_hour', used: 0.2, resetsAt: 0 }], 'a reading with no reset time stands');
  deepEqual(usageRows({}, 'claude', now), [{}]);
});

check('settings: the new sections are written in every language, and the file hint, the locks and the privacy facts are not left in English', () => {
  const keys = [
    'set.notifications',
    'set.notifyMe',
    'set.notifyHint',
    'set.playSound',
    'set.soundHint',
    'set.system',
    'set.keepAwake',
    'set.keepAwakeHint',
    'set.autostart',
    'set.autostartHint',
    'set.usage',
    'set.privacy',
    'set.accountsHint',
    'set.connected',
    'set.notConnected',
    'set.usageHint',
    'usage.noReading',
    'usage.resets',
    'set.privacy.telemetry',
    'set.privacy.history',
    'set.privacy.logins',
    'set.preview',
    'set.previewPending',
    'set.previewApplied',
    'set.contrast.text',
    'set.contrast.faint',
    'set.contrast.accent',
    'set.lockedTitle',
    'set.lockedHint',
    'set.saveFile',
    'set.openFile',
    'set.fileHint',
    'set.reset',
    'set.use',
    'look.effect.ambientHint',
    'look.effect.textureHint',
    'look.effect.glowHint',
  ] as const;
  const others = { 'pt-BR': ptBR, es, fr, de } as const;
  for (const key of keys) {
    ok(en[key] !== undefined, `en has ${key}`);
    for (const [code, messages] of Object.entries(others)) {
      ok(messages[key] !== undefined, `${code} has ${key}`);
      // A sentence copied from English into another language means nobody translated it.
      if (typeof en[key] === 'string' && String(en[key]).length > 24) ok(messages[key] !== en[key], `${code} ${key} is still English`);
    }
  }
});

// ---- the sections on a real page ----

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

/** Draws a component into the page, where the stylesheet applies, and takes it away again. */
function mount(view: () => ReturnType<typeof createComponent>) {
  const host = document.createElement('div');
  document.body.append(host);
  const stop = render(view, host);
  return {
    host,
    stop: () => {
      stop();
      host.remove();
    },
  };
}

type AppearanceProps = Parameters<typeof AppearanceSection>[0];
const section = (props: AppearanceProps) => mount(() => createComponent(AppearanceSection, props));
const useButton = (host: HTMLElement) => host.querySelector<HTMLButtonElement>('.ap-bar .btn.primary')!;
const presetButton = (host: HTMLElement, id: string) => host.querySelector<HTMLButtonElement>(`[aria-labelledby="look-${id}-name"]`)!;

check('settings: notification switches default on, save separately and restore a refused change without playing anything', async () => {
  const [saved, setSaved] = createSignal<Saved>({});
  const patches: Saved[] = [];
  let accept = false;
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {},
    onSave: async (patch) => {
      patches.push(patch);
      if (accept) setSaved({ ...saved(), ...patch });
      return accept;
    },
  }));
  try {
    const switches = view.host.querySelectorAll('.toggles')[1]!.querySelectorAll<HTMLInputElement>('input[role="switch"]');
    equal(switches.length, 2);
    const notify = switches[0]!;
    const sound = switches[1]!;
    equal(notify.checked, true);
    equal(sound.checked, true);
    notify.click();
    await tick();
    equal(notify.checked, true, 'a refused save restores the switch');
    deepEqual(saved(), {});
    accept = true;
    notify.click();
    await tick();
    equal(notify.checked, false);
    equal(sound.checked, true, 'disabling notifications keeps the sound preference');
    sound.click();
    await tick();
    notify.click();
    await tick();
    deepEqual(saved(), { notifications: true, notificationSound: false });
    deepEqual(patches, [{ notifications: false }, { notifications: false }, { notificationSound: false }, { notifications: true }]);
  } finally { view.stop(); }
});

check('settings: keep-awake defaults on and autostart defaults off, saving separately and restoring on refusal', async () => {
  const original = Object.getOwnPropertyDescriptor(navigator, 'userAgent');
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Windows' });
  const [saved, setSaved] = createSignal<Saved>({});
  const patches: Saved[] = [];
  let accept = false;
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {},
    onSave: async (patch) => {
      patches.push(patch);
      if (accept) setSaved({ ...saved(), ...patch });
      return accept;
    },
  }));
  try {
    const switches = view.host.querySelectorAll<HTMLInputElement>('.row:last-child input[role="switch"]');
    equal(switches.length, 2);
    const awake = switches[0]!;
    const autostart = switches[1]!;
    equal(awake.checked, true, 'keep-awake defaults on');
    equal(autostart.checked, false, 'autostart defaults off');
    awake.click();
    await tick();
    equal(awake.checked, true, 'a refused save restores the keep-awake switch');
    deepEqual(saved(), {});
    accept = true;
    awake.click();
    await tick();
    equal(awake.checked, false);
    autostart.click();
    await tick();
    equal(autostart.checked, true);
    deepEqual(saved(), { keepAwake: false, autostart: true });
    deepEqual(patches, [{ keepAwake: false }, { keepAwake: false }, { autostart: true }]);
  } finally {
    view.stop();
    if (original) Object.defineProperty(navigator, 'userAgent', original);
    else Reflect.deleteProperty(navigator, 'userAgent');
  }
});

check('settings: an answer that arrives after Appearance closed leaves the draft alone', async () => {
  const [draft, setDraft] = createSignal<Appearance>({ base: 'oficina' });
  const asks: Array<{ resolve: (l: Look) => void; reject: (e: unknown) => void }> = [];
  const request = () => new Promise<Look>((resolve, reject) => asks.push({ resolve, reject }));
  const props: AppearanceProps = { look: look(), saved: undefined, draft, setDraft, onUse: async () => true, request };
  // The draft is not saved, so the section asks for its preview as it opens; then it is closed with the answer still on its way.
  const first = section(props);
  equal(asks.length, 1);
  first.stop();
  const second = section(props);
  presetButton(second.host, 'manha').click();
  equal(draft().base, 'manha');
  asks.at(-1)!.resolve(look());
  asks[0]!.reject({ key: 'error.look.nearAmber' });
  await tick();
  equal(draft().base, 'manha', 'the old refusal put the old draft back');
  second.stop();
});

check('settings: a failing contrast pair keeps "Use this look" shut, and a look that reads well opens it', async () => {
  const [draft, setDraft] = createSignal<Appearance>({ base: 'oficina' });
  const bad = look({ checks: [{ a: 'ink-3', b: 'bg', ratio: 4.1, min: 4.5, pass: false }] });
  const answers = [bad];
  let used = 0;
  const props: AppearanceProps = {
    look: look(),
    saved: undefined,
    draft,
    setDraft,
    onUse: async () => {
      used++;
      return true;
    },
    request: async () => answers.shift() ?? look(),
  };
  const view = section(props);
  await tick();
  const button = useButton(view.host);
  ok(button.disabled, 'a failing pair is on show and the button is open');
  button.click();
  equal(used, 0, 'a refused look was sent to be saved');
  presetButton(view.host, 'manha').click();
  await tick();
  ok(!button.disabled, 'a look that reads well should be usable');
  button.click();
  equal(used, 1);
  view.stop();
});

check('settings: the preview draws the effect of the new conversation and the two lights, and wears none of them when they are off', () => {
  const lines = { 'amber-line': 'rgb(9, 9, 9)', 'line-2': 'rgb(8, 8, 8)' };
  const on = { ...lines, 'amb-beam': 'block', 'amb-loom': 'none', 'amb-flow': 'none', 'amber-light': '0 14px 34px -18px rgb(1, 2, 3)', 'focus-light': '0 18px 50px -22px rgb(4, 5, 6)' };
  const off = { ...lines, 'amb-beam': 'none', 'amb-loom': 'none', 'amb-flow': 'none', 'amber-light': '0 0 0 0 transparent', 'focus-light': '0 0 0 0 transparent' };
  const [vars, setVars] = createSignal<Record<string, string>>(on);
  const view = mount(() => createComponent(Mini, { get look() { return look({ vars: vars() }); } }));
  const style = (selector: string) => getComputedStyle(view.host.querySelector(selector)!);
  equal(style('.m-beam').display, 'block');
  ok(style('.m-ask').boxShadow.includes('rgb(1, 2, 3)'), 'the amber light is not on the question');
  ok(style('.m-in').boxShadow.includes('rgb(4, 5, 6)'), 'the focus light is not on the text box');
  setVars(off);
  equal(style('.m-beam').display, 'none');
  ok(!style('.m-ask').boxShadow.includes('rgb(1, 2, 3)'));
  ok(!style('.m-in').boxShadow.includes('rgb(4, 5, 6)'));
  view.stop();
});

const cli = (account: CheckResult['account'], installed = true): CheckResult => ({ vendor: 'claude', installed, account, canInstall: false });

check('settings: only a CLI that says it is signed in is a connected account', () => {
  equal(accountLine(undefined), 'checking');
  equal(accountLine('failed'), 'unsure');
  equal(accountLine(cli('signedIn')), 'connected');
  equal(accountLine(cli('signedOut')), 'notConnected', 'an installed CLI nobody signed in to');
  equal(accountLine(cli('signedOut', false)), 'notConnected');
  equal(accountLine(cli('unknown')), 'unsure');
  equal(accountLine(cli('unknown', false)), 'notConnected', 'nothing installed, nothing to check');
});

check('settings: unsupported system controls are absent outside Windows', () => {
  const original = Object.getOwnPropertyDescriptor(navigator, 'userAgent');
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Macintosh' });
  const view = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  try {
    equal(view.host.querySelectorAll('input[role="switch"]').length, 3);
  } finally {
    view.stop();
    if (original) Object.defineProperty(navigator, 'userAgent', original);
    else Reflect.deleteProperty(navigator, 'userAgent');
  }
});
