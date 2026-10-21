import { createComponent, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import themes from '../../src-tauri/src/themes.json';
import { api, type AppState, type Appearance, type CheckResult, type Look, type QuotaReading, type Settings as Saved, type Vendor, type VendorUsage } from '../../ui/api.ts';
import { App } from '../../ui/App.tsx';
import { AppearanceSection } from '../../ui/Appearance.tsx';
import { previewer } from '../../ui/appearance.ts';
import { Mini } from '../../ui/FirstRun.tsx';
import { factory, failingPairs, groundColor, groundKey, normalizeHex, readings, sameChoices, shown } from '../../ui/draft.ts';
import { de } from '../../ui/locales/de.ts';
import { en } from '../../ui/locales/en.ts';
import { es } from '../../ui/locales/es.ts';
import { fr } from '../../ui/locales/fr.ts';
import { ptBR } from '../../ui/locales/pt-BR.ts';
import { useLanguage } from '../../ui/i18n.ts';
import { accountLine, seedQuotas, usageRows } from '../../ui/sidebar.ts';
import { Settings } from '../../ui/Settings.tsx';
import { check, deepEqual, equal, frames, ok } from './check.ts';

check('R107: Settings names Chat and Projects and discloses the shared default', () => {
  ok(en['set.permissions'].includes('Chat'));
  ok(en['set.permissions'].includes('Projects'));
  ok(en['set.chatFolderMore'].includes('first message'));
  ok(en['set.chatFolderMore'].includes('private folder'));
  ok(en['set.chatFolderMore'].includes('delete the conversation'));
  ok(en['set.chatFolderHint'].length < 50, 'the row keeps one short line');
});

check('R121: the risk and the credential limits of Do not ask are spelled out in Settings, not over every thread', () => {
  ok(!Object.keys(en).some((key) => key.startsWith('project.permissions')), 'no permission sentence is kept for the top of a conversation');
  const body = en['set.permissionsDontAskBody'];
  for (const word of ['internet', 'any folder', 'steer', 'pick Ask for', '.ssh', 'Codex can read', 'directly', 'Claude Code can through commands', 'best effort', 'Stop asks the tools to stop']) ok(body.includes(word), word);
  ok(!/is kept out|cannot be kept out|Codex cannot/.test(body), 'no claim that one tool is kept away from the sign-in folders');
  ok(body.split('. ').length <= 5, 'one short paragraph');
  ok(!/sandbox|too broad|disabled/i.test(body), 'full access no longer describes a sandbox or disabled integrations');
  const ask = en['set.permissionsAskBody'];
  ok(ask.includes('ask the tools to stop and end the programs they started') && ask.includes('before changing a file') && ask.includes('plain reading commands'), ask);
  ok(ask.includes('except the ones you already allowed in the tool\x27s own settings'), 'Ask names what the tools\x27 own settings can pre-approve');
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
    'set.group.you',
    'set.group.ai',
    'set.group.system',
    'set.group.type',
    'set.advanced',
    'set.contrast',
    'set.file',
    'set.hooksHint',
    'set.hooksMore',
    'set.soundLine',
    'set.previewsMore',
    'set.chatFolderMore',
    'usage.details',
    'usage.pill.ok',
    'usage.pill.spare',
    'usage.pill.fast',
    'usage.pill.low',
    'usage.pill.out',
    'usage.pill.unknown',
    'notes.24.better',
    'set.notifyMe',
    'set.notifyHint',
    'set.playSound',
    'set.soundHint',
    'set.keepAwake',
    'set.keepAwakeHint',
    'set.autostart',
    'set.autostartHint',
    'set.updates',
    'set.previews',
    'set.previewsHint',
    'set.privacy',
    'set.accountsHint',
    'set.connected',
    'set.notConnected',
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
    'set.saveFile',
    'set.openFile',
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
/** Room for the short exit of what closes: a menu, a folded group or a row's long text stays in the page for it. */
const leave = () => new Promise((resolve) => setTimeout(resolve, 400));

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
// What the shell sends for a settings file nobody changed: each default lives in the engine (`Settings::shown`), never here.
const SHOWN: Saved = { userHooks: false, permissions: 'dontAsk', notifications: true, notificationSound: true, keepAwake: true, autostart: false };

check('settings: notification switches show what the engine sends, save separately and restore a refused change without playing anything', async () => {
  const [saved, setSaved] = createSignal<Saved>(SHOWN);
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
    const notify = view.host.querySelector<HTMLInputElement>('#set-notify-switch')!;
    const sound = view.host.querySelector<HTMLInputElement>('#set-sound-switch')!;
    ok(notify && sound, 'two switches in Notifications and system');
    equal(notify.checked, true);
    equal(sound.checked, true);
    notify.click();
    await tick();
    equal(notify.checked, true, 'a refused save restores the switch');
    deepEqual(saved(), SHOWN);
    accept = true;
    notify.click();
    await tick();
    equal(notify.checked, false);
    equal(sound.checked, true, 'disabling notifications keeps the sound preference');
    sound.click();
    await tick();
    notify.click();
    await tick();
    deepEqual(saved(), { ...SHOWN, notifications: true, notificationSound: false });
    deepEqual(patches, [{ notifications: false }, { notifications: false }, { notificationSound: false }, { notifications: true }]);
  } finally { view.stop(); }
});

check('settings: keep-awake and autostart show what the engine sends, saving separately and restoring on refusal', async () => {
  const original = Object.getOwnPropertyDescriptor(navigator, 'userAgent');
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'Windows' });
  const [saved, setSaved] = createSignal<Saved>(SHOWN);
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
    const awake = view.host.querySelector<HTMLInputElement>('#set-awake-switch')!;
    const autostart = view.host.querySelector<HTMLInputElement>('#set-autostart-switch')!;
    ok(awake && autostart, 'the two system switches are there on Windows');
    equal(awake.checked, true);
    equal(autostart.checked, false);
    awake.click();
    await tick();
    equal(awake.checked, true, 'a refused save restores the keep-awake switch');
    deepEqual(saved(), SHOWN);
    accept = true;
    awake.click();
    await tick();
    equal(awake.checked, false);
    autostart.click();
    await tick();
    equal(autostart.checked, true);
    deepEqual(saved(), { ...SHOWN, keepAwake: false, autostart: true });
    deepEqual(patches, [{ keepAwake: false }, { keepAwake: false }, { autostart: true }]);
  } finally {
    view.stop();
    if (original) Object.defineProperty(navigator, 'userAgent', original);
    else Reflect.deleteProperty(navigator, 'userAgent');
  }
});

check('settings: receive previews defaults off, saves on its own and is restored when the save is refused', async () => {
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
    const group = [...view.host.querySelectorAll<HTMLElement>('.st-group')].find((g) => g.querySelector('.st-title')?.textContent === en['set.updates']);
    ok(group, 'General has an Updates group');
    const previews = group!.querySelectorAll<HTMLInputElement>('input[role="switch"]');
    equal(previews.length, 1, 'one switch, no second way');
    equal(previews[0]!.checked, false, 'previews default off');
    previews[0]!.click();
    await tick();
    equal(previews[0]!.checked, false, 'a refused save restores the switch');
    accept = true;
    previews[0]!.click();
    await tick();
    equal(previews[0]!.checked, true);
    previews[0]!.click();
    await tick();
    equal(previews[0]!.checked, false, 'turning it off is saved too');
    deepEqual(patches, [{ previews: true }, { previews: true }, { previews: false }]);
  } finally { view.stop(); }
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
    equal(view.host.querySelectorAll('input[role="switch"]').length, 4);
  } finally {
    view.stop();
    if (original) Object.defineProperty(navigator, 'userAgent', original);
    else Reflect.deleteProperty(navigator, 'userAgent');
  }
});

check('R140: the permission menu lists five modes with one plain line each, and the long text stays out of the page until the "?" opens it', async () => {
  const [saved, setSaved] = createSignal<Saved>({});
  const [vendors, setVendors] = createSignal<Vendor[]>(['claude']);
  const patches: Saved[] = [];
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, get vendors() { return vendors(); }, quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {},
    onSave: async (patch) => { patches.push(patch); setSaved({ ...saved(), ...patch }); return true; },
  }));
  try {
    const pick = () => view.host.querySelector<HTMLButtonElement>('#set-permissions-pick')!;
    const options = () => [...view.host.querySelectorAll<HTMLElement>('#set-permissions-pick-list [role="option"]')];
    equal(pick().textContent, en['set.permissions.dontAsk'], 'full access is on when nothing is saved');
    equal(view.host.querySelector('[role="listbox"]'), null, 'the menu is closed until it is asked for');
    equal(view.host.querySelector('#set-permissions-hint')!.textContent, en['set.permissions.dontAsk.line'], 'the row says the line of the mode that is on');
    pick().click();
    await tick();
    deepEqual(options().map((o) => o.querySelector('b')!.textContent), [en['set.permissions.ask'], en['set.permissions.acceptEdits'], en['set.permissions.plan'], en['set.permissions.auto'], en['set.permissions.dontAsk']]);
    deepEqual(options().map((o) => o.querySelector('small')!.textContent), [en['set.permissions.ask.line'], en['set.permissions.acceptEdits.line'], en['set.permissions.plan.line'], en['set.permissions.auto.line'], en['set.permissions.dontAsk.line']]);
    for (const row of options()) ok(row.querySelector('small')!.textContent!.length < 70, 'one short line');
    deepEqual(options().map((o) => o.getAttribute('aria-selected')), ['false', 'false', 'false', 'false', 'true']);
    const body = (key: 'set.permissionsDontAskBody' | 'set.permissionsAskBody' | 'set.permissions.auto.more') => view.host.textContent!.includes(en[key]);
    equal(body('set.permissionsDontAskBody'), false, 'the long risk text is not on the page');
    const more = view.host.querySelector<HTMLButtonElement>('#set-permissions-q')!;
    equal(more.getAttribute('aria-expanded'), 'false');
    equal(more.textContent, '?');
    equal(more.getAttribute('aria-label'), en['set.learnMore']);
    more.click();
    await tick();
    equal(body('set.permissionsDontAskBody'), true);
    equal(more.getAttribute('aria-expanded'), 'true');
    options()[3]!.click();
    await tick();
    deepEqual(patches, [{ permissions: 'auto' }]);
    await leave();
    equal(view.host.querySelector('[role="listbox"]'), null, 'picking closes the menu');
    equal(pick().textContent, en['set.permissions.auto']);
    equal(view.host.querySelector('#set-permissions-hint')!.textContent, en['set.permissions.auto.line']);
    equal(body('set.permissions.auto.more'), true, 'the open text follows the mode');
    more.click();
    await leave();
    equal(body('set.permissions.auto.more'), false, 'closed, it leaves the page');
    // The keyboard opens the menu, and Esc closes it without reaching the page behind (which would leave Settings).
    let escapes = 0;
    const seen = (e: Event) => { if ((e as KeyboardEvent).key === 'Escape') escapes++; };
    window.addEventListener('keydown', seen);
    try {
      pick().dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true }));
      await tick();
      equal(options().length, 5, 'ArrowDown opens the menu');
      pick().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      await leave();
      equal(view.host.querySelector('[role="listbox"]'), null, 'Esc closes it');
      equal(escapes, 0, 'and stays inside the menu');
      pick().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      equal(escapes, 1, 'with the menu shut, Esc goes on to the page as before');
    } finally { window.removeEventListener('keydown', seen); }
    // Web search: Cached and the Codex-only choices exist only where Codex is connected.
    const labels = (id: string) => [...view.host.querySelectorAll<HTMLButtonElement>(`#${id} button`)].map((b) => b.textContent);
    deepEqual(labels('set-web-pick'), [en['set.webSearch.off'], en['set.webSearch.live']]);
    equal(view.host.querySelector('#set-codex'), null);
    equal(view.host.querySelector('#set-web-pick button[aria-pressed="true"]')!.textContent, en['set.webSearch.live'], 'live is the default');
    setVendors(['claude', 'codex']);
    await tick();
    deepEqual(labels('set-web-pick'), [en['set.webSearch.off'], en['set.webSearch.cached'], en['set.webSearch.live']]);
    // The Codex group starts closed and is not in the page until it is opened.
    const fold = view.host.querySelector<HTMLButtonElement>('#set-codex')!;
    equal(fold.getAttribute('aria-expanded'), 'false', 'the Codex group is closed by default');
    equal(view.host.querySelector('[data-opt="set-summary"]'), null);
    equal(view.host.querySelector('[data-opt="set-verbosity"]'), null);
    fold.click();
    await tick();
    equal(fold.getAttribute('aria-expanded'), 'true');
    const choices = (id: string) => [...view.host.querySelectorAll<HTMLOptionElement>(`[data-opt="${id}"] select option`)].map((o) => o.textContent);
    deepEqual(choices('set-summary'), [en['set.default'], en['set.summary.auto'], en['set.summary.concise'], en['set.summary.detailed'], en['set.summary.none']]);
    equal(view.host.querySelector('[data-opt="set-verbosity"]'), null, 'answer length is replaced by the shared reply style');
    equal(view.host.querySelector<HTMLSelectElement>('[data-opt="set-summary"] select')!.value, 'default');
    equal(view.host.textContent!.includes(en['set.webSearch.more']), false, 'the web details are closed too');
    view.host.querySelector<HTMLButtonElement>('#set-web-q')!.click();
    await tick();
    equal(view.host.textContent!.includes(en['set.webSearch.more']), true);
    view.host.querySelector<HTMLButtonElement>('#set-web-pick button')!.click();
    const choose = (id: string, value: string) => {
      const list = view.host.querySelector<HTMLSelectElement>(`[data-opt="${id}"] select`)!;
      list.value = value;
      list.dispatchEvent(new Event('change', { bubbles: true }));
    };
    choose('set-summary', 'detailed');
    await tick();
    deepEqual(patches.slice(1), [{ webSearch: 'off' }, { reasoningSummary: 'detailed' }]);
  } finally { view.stop(); }
});

check('R140: every mode has a name, a line and, for the three new ones, details, in every language', () => {
  for (const [code, messages] of Object.entries({ en, 'pt-BR': ptBR, es, fr, de })) {
    for (const mode of ['ask', 'acceptEdits', 'plan', 'auto', 'dontAsk'] as const) {
      ok(messages[`set.permissions.${mode}`].trim() && messages[`set.permissions.${mode}.line`].trim(), `${code} ${mode}`);
    }
    ok(messages['mode.fallback.auto'].trim() && messages['set.learnMore'].trim() && messages['composer.mode'].trim(), code);
  }
  equal(en['set.permissions.dontAsk'], 'Full access');
});

// ---- the calm grammar (calm screens, delivery 2) ----

const SECTION_IDS = ['set.general', 'set.projects', 'set.appearance', 'set.subscriptions', 'set.privacy'] as const;
const navButton = (host: HTMLElement, key: (typeof SECTION_IDS)[number]) => [...host.querySelectorAll<HTMLButtonElement>('.s-nav .item')].find((b) => b.textContent!.includes(en[key]))!;

check('settings: five sections remain, accounts and their limits are one, and Faber\'s version closes the list once', () => {
  const view = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: [], quotas: {}, now: 0, version: '9.9.9',
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  try {
    const items = [...view.host.querySelectorAll<HTMLButtonElement>('.s-nav .item')].slice(1).map((b) => b.textContent!.trim());
    deepEqual(items, [en['set.general'], en['set.projects'], en['set.appearance'], 'Accounts and limits', en['set.privacy']]);
    equal(en['set.subscriptions'], 'Accounts and limits');
    ok(!('set.usage' in en), 'the usage limits section is gone');
    equal(view.host.querySelectorAll('.s-version').length, 1);
    equal(view.host.querySelector('.s-nav')!.lastElementChild!.textContent, 'Faber 9.9.9', 'at the bottom of the list');
  } finally { view.stop(); }
});

check('settings: no section has a "Learn more" link, and every long text sits behind the "?" of its own row', async () => {
  const view = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: ['claude', 'codex'], quotas: {}, now: 0,
    usage: [{ vendor: 'claude', state: 'ok', ownSpendUsd: 0, windows: [] }] as unknown as VendorUsage[],
    projects: [{ id: 'p1', name: 'Demo', path: 'C:/fixture/demo' }],
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true, onChatFolder: async () => {},
  }));
  try {
    for (const key of SECTION_IDS) {
      navButton(view.host, key).click();
      await tick();
      const links = [...view.host.querySelectorAll<HTMLElement>('button, a')].filter((e) => e.textContent!.trim() === en['set.learnMore']);
      equal(links.length, 0, `${key}: a "Learn more" link is still on the page`);
    }
    navButton(view.host, 'set.general').click();
    await tick();
    const marks = [...view.host.querySelectorAll<HTMLButtonElement>('.st-q')];
    ok(marks.length >= 5, `the long texts have their "?" (${marks.length})`);
    for (const mark of marks) {
      equal(mark.getAttribute('aria-label'), en['set.learnMore'], 'one accessible name, one key');
      equal(mark.getAttribute('aria-expanded'), 'false');
      ok(mark.getAttribute('aria-controls'), 'it says what it opens');
    }
    // A row's hint is one short line; the paragraphs are not on the page until the "?" opens one.
    for (const hint of view.host.querySelectorAll('.st-hint')) ok(hint.textContent!.length <= 70, `a hint is one short line: ${hint.textContent}`);
    equal(view.host.querySelectorAll('.st-more').length, 0, 'nothing is unfolded');
    marks[0]!.click();
    await tick();
    equal(view.host.querySelectorAll('.st-more').length, 1);
    equal(!('set.more' in en), true, 'the second "Learn more" key is gone');
  } finally { view.stop(); }
});

check('settings: the chat working folder is one row, and only where its control exists', async () => {
  const withFolder = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: [], quotas: {}, now: 0, chatFolder: 'C:/work',
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true, onChatFolder: async () => {},
  }));
  const without = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  try {
    equal(withFolder.host.querySelectorAll('[data-opt="set-folder"]').length, 1);
    ok(withFolder.host.querySelector('[data-opt="set-folder"] .btn'), 'the row carries its control');
    equal(without.host.querySelector('[data-opt="set-folder"]'), null, 'no label and no explanation without a control');
    ok(!without.host.textContent!.includes(en['set.chatFolder']));
  } finally { withFolder.stop(); without.stop(); }
});

check('settings: switching sections is a short cross-fade (the old one leaves as a copy, the new one arrives), instant when reduced, and the new section opens at its top', async () => {
  const preference = Object.getOwnPropertyDescriptor(MediaQueryList.prototype, 'matches')!;
  Object.defineProperty(MediaQueryList.prototype, 'matches', { ...preference, get(this: MediaQueryList) {
    return this.media === '(prefers-reduced-motion: reduce)' ? false : preference.get!.call(this);
  } });
  const view = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: ['claude', 'codex'], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  const was = document.documentElement.dataset.motion;
  document.documentElement.dataset.motion = 'full';
  try {
    const settings = view.host.querySelector<HTMLElement>('.settings')!;
    settings.style.cssText = 'position:fixed;left:0;top:0;width:1100px;height:240px';
    const main = view.host.querySelector<HTMLElement>('.s-main')!;
    ok(main.hasAttribute('data-motion-list'), '.s-main is an animated list: the old section keeps an inert copy while it fades');
    ok(main.scrollHeight > main.clientHeight, 'General is taller than the window here');
    main.scrollTop = 200;
    ok(main.scrollTop > 0, 'the section is scrolled down');
    await new Promise((resolve) => setTimeout(resolve, 400));
    navButton(view.host, 'set.privacy').click();
    await tick();
    equal(main.scrollTop, 0, 'a section opens at its top');
    equal(view.host.querySelectorAll('.facts').length, 1, 'only the new section is in the page');
    equal(view.host.querySelectorAll('.st-group').length, 0, 'and none of General');
    ok(document.querySelector('.motion-exit')?.shadowRoot?.querySelector('.st-group'), 'the old section leaves as an inert copy');
    ok(view.host.querySelector('.s-main > .st-card')!.getAnimations().length > 0, 'the new section arrives instead of appearing');
    await new Promise((resolve) => setTimeout(resolve, 500));
    equal(document.querySelector('.motion-exit'), null, 'the copy is removed once it has faded');
    equal(main.getAnimations({ subtree: true }).length, 0, 'and the stage rests');
    // Reduced motion: the same switch is instant.
    document.documentElement.dataset.motion = 'reduced';
    navButton(view.host, 'set.general').click();
    await tick();
    equal(document.querySelector('.motion-exit'), null, 'no copy of the old section');
    for (const frame of [0, 1, 2]) {
      await frames(1);
      equal(main.getAnimations({ subtree: true }).length, 0, `nothing on the stage animates (frame ${frame})`);
    }
  } finally {
    Object.defineProperty(MediaQueryList.prototype, 'matches', preference);
    if (was === undefined) delete document.documentElement.dataset.motion;
    else document.documentElement.dataset.motion = was;
    view.stop();
  }
});

/** Runs a check with both CLIs saying someone is signed in, as the accounts section asks them when it opens. */
async function signedIn<T>(run: () => Promise<T> | T): Promise<T> {
  const was = api.setupCheck;
  api.setupCheck = async (vendor) => ({ vendor, installed: true, account: 'signedIn', canInstall: false });
  try { return await run(); } finally { api.setupCheck = was; }
}

check('settings: the accounts card writes Faber\'s spend in the person\'s format, never four decimals', () => {
  const usage = [{ vendor: 'claude', plan: 'max', state: 'ok', learning: false, relearning: false, foreignUse: false, overageOn: false, billedPerUse: false,
    billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 3.2, unmetered: 0, counting: false, windows: [] }] as unknown as VendorUsage[];
  const text = (language: 'en' | 'pt-BR' | 'de') => {
    useLanguage(language);
    const view = mount(() => createComponent(Settings, {
      look: look(), saved: {}, vendors: ['claude'], quotas: {}, now: 0, usage, start: 'accounts',
      onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
    }));
    try { return view.host.querySelector('.acct-foot')!.textContent!; } finally { view.stop(); }
  };
  try {
    ok(text('en').includes('$3.20'), text('en'));
    ok(text('pt-BR').includes('US$ 3,20'), text('pt-BR'));
    ok(text('de').includes('3,20 US$'), text('de'));
    for (const language of ['en', 'pt-BR', 'de'] as const) ok(!/\d[.,]\d{4}/.test(text(language)), `${language}: "${text(language)}" shows four decimals`);
  } finally { useLanguage('en'); }
});

check('settings: one card per AI with a capitalised plan, one status word, the reset time in full and the notes behind one Details fold', () => signedIn(async () => {
  useLanguage('en');
  const until = Math.floor(Date.now() / 1000) + 7 * 3600;
  const usage = [{ vendor: 'claude', plan: 'max', state: 'low', learning: true, relearning: false, outsideShare: 0.2, foreignUse: true, overageOn: true, billedPerUse: false,
    billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 1, unmetered: 2, counting: true,
    windows: [{ name: 'seven_day', used: 0.4, elapsed: 0.5, expected: 0.5, resetsAt: until, readAt: Date.now(), uncounted: null }] }] as unknown as VendorUsage[];
  const view = mount(() => createComponent(Settings, {
    look: look(), saved: {}, vendors: ['claude'], start: 'accounts', now: Date.now(), usage,
    quotas: { 'claude:seven_day': { vendor: 'claude', window: 'seven_day', used: 0.4, resetsAt: until } },
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  try {
    await tick();
    await tick();
    equal(view.host.querySelectorAll('.acct').length, 2, 'one card for each AI');
    const card = view.host.querySelector<HTMLElement>('.acct[data-vendor="claude"]')!;
    ok(card.textContent!.includes('Plan: Max'), 'the plan is capitalised');
    const pill = card.querySelector('.acct-pill')!;
    equal(pill.textContent, en['usage.pill.low']);
    equal(card.dataset.state, 'connected');
    ok(pill.classList.contains('warn'), 'amber for a window that is really near its limit (C002)');
    const reset = card.querySelector<HTMLElement>('.urow .reset')!;
    ok(reset.textContent!.startsWith('resets in '), reset.textContent!);
    ok(reset.scrollWidth <= reset.clientWidth + 1, 'the reset time is not cut');
    equal(card.querySelectorAll('.hint').length, 0, 'every note is behind the fold');
    const fold = card.querySelector<HTMLButtonElement>('.st-fold-head')!;
    equal(fold.getAttribute('aria-expanded'), 'false');
    fold.click();
    await tick();
    const notes = [...card.querySelectorAll('.acct-details .hint')].map((n) => n.textContent!);
    for (const key of ['quota.countingNote', 'usage.shared', 'usage.learning', 'usage.foreign', 'usage.overageOn'] as const) ok(notes.includes(en[key]), key);
    ok(notes.some((n) => n.includes('2 requests')), 'the incomplete meter');
    equal(view.host.querySelectorAll('[data-restore="manage"]').length, 1, 'Manage accounts appears once in the section');
    equal(en['set.openSetup'], 'Manage accounts', 'one label opens the account cards from Settings');
  } finally { view.stop(); }
}));

check('settings: a window still counting, or with an uncounted share, reads at least beside its number, not only inside Details (C010)', () => signedIn(async () => {
  useLanguage('en');
  const until = Math.floor(Date.now() / 1000) + 7 * 3600;
  const usage = (counting: boolean, uncounted: number | null) => [{ vendor: 'claude', plan: 'max', state: 'ok', learning: false, relearning: false, foreignUse: false, overageOn: false,
    billedPerUse: false, billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0, counting,
    windows: [{ name: 'seven_day', used: 0.44, elapsed: 0.5, expected: 0.5, resetsAt: until, readAt: Date.now(), uncounted }] }] as unknown as VendorUsage[];
  const share = async (u: VendorUsage[]) => {
    const view = mount(() => createComponent(Settings, {
      look: look(), saved: {}, vendors: ['claude'], start: 'accounts', now: Date.now(), usage: u,
      quotas: { 'claude:seven_day': { vendor: 'claude', window: 'seven_day', used: 0.44, resetsAt: until } },
      onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
    }));
    try {
      await tick();
      await tick();
      const cell = view.host.querySelector<HTMLElement>('.acct[data-vendor="claude"] .urow .share')!;
      ok(cell.scrollWidth <= cell.clientWidth + 1, `the share fits its cell (${cell.scrollWidth} > ${cell.clientWidth})`);
      return cell.textContent!.trim();
    } finally { view.stop(); }
  };
  const counting = await share(usage(true, null));
  ok(counting.startsWith('≥ 44%'), `"${counting}" does not read at least 44%`);
  ok(counting.includes(en['quota.counting']), 'screen readers still hear that it is counting');
  const extra = await share(usage(false, 0.12));
  ok(extra.startsWith('≥ 44%') && !extra.includes(en['quota.counting']), `an uncounted share alone reads at least: "${extra}"`);
  const calm = await share(usage(false, null));
  equal(calm, '44%', 'a settled reading shows the number alone');
}));

check('settings: a billed-per-use or unconfirmed account says so calmly and is never amber; a really low one stays amber (C002)', () => signedIn(async () => {
  useLanguage('en');
  const card = async (flags: Record<string, boolean>, state: string) => {
    const usage = [{ vendor: 'claude', plan: 'max', state, learning: false, relearning: false, foreignUse: false, overageOn: false, billedPerUse: false,
      billingUnknown: false, askForPlan: false, paidPossible: false, ownSpendUsd: 0, unmetered: 0, counting: false, windows: [], ...flags }] as unknown as VendorUsage[];
    const view = mount(() => createComponent(Settings, {
      look: look(), saved: {}, vendors: ['claude'], start: 'accounts', now: Date.now(), usage, quotas: {},
      onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
    }));
    try {
      await tick();
      await tick();
      const pill = view.host.querySelector<HTMLElement>('.acct[data-vendor="claude"] .acct-pill')!;
      return { text: pill.textContent!, warn: pill.classList.contains('warn') };
    } finally { view.stop(); }
  };
  deepEqual(await card({ billedPerUse: true }, 'out'), { text: en['usage.pill.billed'], warn: false });
  deepEqual(await card({ billingUnknown: true }, 'unknown'), { text: en['usage.pill.unconfirmed'], warn: false });
  deepEqual(await card({ billingUnknown: true }, 'out'), { text: en['usage.pill.unconfirmed'], warn: false });
  deepEqual(await card({}, 'out'), { text: en['usage.pill.out'], warn: true }, 'a really exhausted quota stays amber');
  const others = { 'pt-BR': ptBR, es, fr, de } as const;
  for (const key of ['usage.pill.billed', 'usage.pill.unconfirmed'] as const) {
    for (const [code, messages] of Object.entries(others)) ok(messages[key] !== undefined && messages[key] !== en[key], `${code} has its own ${key}`);
  }
}));

check('settings: in Full access the hooks switch shows that hooks always run and is locked; in a mode that asks it is the saved choice', async () => {
  const [saved, setSaved] = createSignal<Saved>({ permissions: 'dontAsk' });
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {}, onSave: async () => true,
  }));
  try {
    const sw = () => view.host.querySelector<HTMLInputElement>('#set-hooks-switch')!;
    const hint = () => view.host.querySelector('[data-opt="set-hooks"]')!.textContent!;
    ok(sw().disabled && sw().checked, 'locked and on');
    ok(hint().includes(en['set.hooksLocked']));
    setSaved({ permissions: 'ask' });
    await tick();
    ok(!sw().disabled && !sw().checked, 'its own choice, off by default');
    ok(hint().includes(en['set.hooksHint']));
  } finally { view.stop(); }
});

check('settings: the first-run answers change later in General: experience saves on its own and the tutorial can play again', async () => {
  const [saved, setSaved] = createSignal<Saved>({});
  const patches: Saved[] = [];
  let tours = 0;
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, vendors: [], quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {}, onTour: () => void tours++,
    onSave: async (patch) => {
      patches.push(patch);
      setSaved({ ...saved(), ...patch });
      return true;
    },
  }));
  try {
    const pick = view.host.querySelectorAll<HTMLButtonElement>('#set-xp-pick button');
    equal(pick.length, 2);
    equal(pick[0]!.getAttribute('aria-pressed'), 'true', 'a person who never answered is a beginner');
    pick[1]!.click();
    await tick();
    equal(pick[1]!.getAttribute('aria-pressed'), 'true');
    deepEqual(patches, [{ experience: 'experienced' }]);
    view.host.querySelector<HTMLButtonElement>('[data-opt="set-tour"] button')!.click();
    equal(tours, 1);
    ok(en['first.foot'].includes('Settings'), 'the first run says where its answers change');
  } finally { view.stop(); }
});

check('reply_style: one shared Settings row saves each choice and restores the person’s words', async () => {
  useLanguage('en');
  const [saved, setSaved] = createSignal<Saved>({});
  const [vendors, setVendors] = createSignal<Vendor[]>([]);
  let refuse = false;
  const view = mount(() => createComponent(Settings, {
    look: look(), get saved() { return saved(); }, get vendors() { return vendors(); }, quotas: {}, now: 0,
    onClose: () => {}, onOpenSetup: () => {},
    onSave: async (patch) => {
      if (refuse) return false;
      setSaved({ ...saved(), ...patch });
      return true;
    },
  }));
  const pick = () => view.host.querySelector<HTMLSelectElement>('[data-opt="set-reply-style"] select')!;
  const words = () => view.host.querySelector<HTMLTextAreaElement>('textarea[aria-labelledby="set-reply-style-name"]');
  const choose = async (style: string) => {
    pick().value = style;
    pick().dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
  };
  try {
    equal(pick().value, 'default', 'no account is needed to choose a style');
    deepEqual([...pick().options].map((o) => o.textContent), [en['set.replyStyle.default'], en['set.replyStyle.short'], en['set.replyStyle.explained'], en['set.replyStyle.own']]);
    for (const style of ['short', 'explained', 'default', 'own'] as const) {
      await choose(style);
      equal(saved().replyStyle, style);
      equal(Boolean(words()), style === 'own');
    }
    equal(words()!.maxLength, 400);
    words()!.value = '  Explain with game examples.  ';
    words()!.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    equal(saved().replyStyleText, 'Explain with game examples.');
    await choose('short');
    await choose('own');
    equal(words()!.value, 'Explain with game examples.');
    refuse = true;
    await choose('explained');
    equal(pick().value, 'own', 'a failed save restores the chosen style');
    words()!.value = 'A refused edit';
    words()!.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    equal(words()!.value, 'Explain with game examples.', 'a refused edit restores the saved words');
    for (const connected of [['claude'], ['codex'], ['claude', 'codex']] as Vendor[][]) {
      setVendors(connected);
      await tick();
      equal(view.host.querySelectorAll('[data-opt="set-reply-style"]').length, 1);
      equal(pick().value, 'own');
      equal(view.host.querySelector('[data-opt="set-verbosity"]'), null);
    }
  } finally { view.stop(); }
});

check('reply_style: Escape saves the focused words before Settings closes and reopening restores them', async () => {
  const original = { saveSettings: api.saveSettings, setClock: api.setClock, recents: api.recents };
  const place = localStorage.getItem('faber.place');
  const initial: AppState = {
    vendors: [], models: [], quotas: [], projects: [], conversations: [], version: '0.0.0', running: { revision: 0, items: [] },
    settings: { setup: 'done', tutorialDone: true, language: 'en', appearance: { base: 'sala' }, replyStyleText: 'Earlier words.' },
    look: look({ motion: 'none', effects: { ambient: false, texture: false, glow: false } }),
  };
  let stored = initial.settings;
  api.setClock = async () => {};
  api.recents = async () => ({ folders: [], environments: [] });
  api.saveSettings = async (patch) => {
    stored = { ...stored, ...patch };
    return { settings: stored, look: initial.look };
  };
  localStorage.setItem('faber.place', JSON.stringify({ area: 'chat' }));
  const view = mount(() => createComponent(App, { initial, inbox: { take: () => {} } }));
  const open = async () => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key: ',', ctrlKey: true, bubbles: true, cancelable: true }));
    await tick();
    ok(view.host.querySelector('[data-screen="settings"]'));
  };
  try {
    await tick();
    await open();
    const select = view.host.querySelector<HTMLSelectElement>('[data-opt="set-reply-style"] select')!;
    select.value = 'own';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await tick();
    const field = view.host.querySelector<HTMLTextAreaElement>('textarea[aria-labelledby="set-reply-style-name"]')!;
    field.focus();
    field.value = 'Explain with game examples.';
    field.dispatchEvent(new InputEvent('input', { data: field.value, inputType: 'insertText', bubbles: true }));
    equal(document.activeElement, field);
    // No blur or change: the real App Escape handler removes Settings while this field still has focus.
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }));
    await tick();
    equal(view.host.querySelector('[data-screen="settings"]'), null);
    equal(stored.replyStyleText, 'Explain with game examples.');
    await open();
    equal(view.host.querySelector<HTMLTextAreaElement>('textarea[aria-labelledby="set-reply-style-name"]')!.value, 'Explain with game examples.');
  } finally {
    view.stop();
    Object.assign(api, original);
    if (place === null) localStorage.removeItem('faber.place');
    else localStorage.setItem('faber.place', place);
  }
});
