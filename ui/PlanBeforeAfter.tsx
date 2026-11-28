import { createMemo, createResource, createSignal, createUniqueId, For, onCleanup, onMount, Show } from 'solid-js';
import { Portal } from 'solid-js/web';
import { api, type Message } from './api.ts';
import { Segmented } from './controls.tsx';
import { keepFocusIn } from './focus.ts';
import { Icon } from './Glyphs.tsx';
import { lang, t } from './i18n.ts';
import { pressedAny } from './keys.ts';
import { capture, type PlanComparison } from './PlanMap.tsx';
import { looksOf } from './Site.tsx';
import './before-after.css';

type Device = 'phone' | 'computer';
type Picture = { src: string; device: Device };

async function readPicture(id: string, call: string, signal: AbortSignal, device?: Device): Promise<Picture | undefined> {
  const src = await api.sitePicture(id, call).catch(() => null);
  if (!src || signal.aborted) return;
  if (device) return { src, device };
  const image = new Image();
  const cancel = () => image.removeAttribute('src');
  signal.addEventListener('abort', cancel, { once: true });
  image.src = src;
  try {
    await image.decode();
    if (!signal.aborted) return { src, device: image.naturalHeight > image.naturalWidth ? 'phone' : 'computer' };
  } catch { return undefined; }
  finally { signal.removeEventListener('abort', cancel); }
}

export function PlanBeforeAfter(props: {
  selection: PlanComparison;
  messages: Message[];
  opener: HTMLButtonElement;
  onClose: () => void;
}) {
  const { conversationId, part, before, after } = props.selection;
  const [picked, setPicked] = createSignal(capture(after));
  const [device, setDevice] = createSignal<Device>();
  const [divider, setDivider] = createSignal(50);
  const title = createUniqueId();
  const controller = new AbortController();
  onCleanup(() => controller.abort());
  let dialog: HTMLDialogElement | undefined;
  let close: HTMLButtonElement | undefined;

  const calls = new Map<string, string[]>();
  const pictures = part.pictures ?? [];
  const ids = pictures.some((id) => capture(id) === capture(before)) ? pictures : [before, ...pictures];
  for (const id of ids) {
    const key = capture(id);
    const variants = calls.get(key) ?? [];
    if (!variants.includes(id)) variants.push(id);
    calls.set(key, variants);
  }
  const metadata = new Map<string, { at: number; looks: ReturnType<typeof looksOf> }>();
  for (let i = props.messages.length - 1; i >= 0 && metadata.size < calls.size; i--) {
    const message = props.messages[i]!;
    for (const tool of message.tools ?? []) {
      if (tool.name === 'Browser' && calls.has(tool.id) && !metadata.has(tool.id)) {
        metadata.set(tool.id, { at: tool.ended ?? tool.started ?? message.at, looks: looksOf(tool) });
      }
    }
  }
  const captures = [...calls].map(([id, variants]) => {
    const info = metadata.get(id);
    const main = id === capture(before) ? before : id === capture(after) ? after : variants[0]!;
    const known = new Map(info?.looks.map((look) => [
      look.picture, look.screen === 'site.screen.phone' ? 'phone' as const : 'computer' as const,
    ]));
    const [picture] = createResource(() => readPicture(conversationId, main, controller.signal, known.get(main)));
    const sizes = new Set([id, ...known.keys(), ...variants]);
    sizes.delete(main);
    const active = createMemo((opened) => opened || id === capture(before) || id === picked(), false);
    const [alternates] = createResource(() => active() && sizes.size ? [...sizes] : false, (keys) =>
      Promise.all(keys.map((key) => readPicture(conversationId, key, controller.signal, known.get(key)))));
    const available = createMemo(() => [picture(), ...(alternates() ?? [])].filter((item) => item !== undefined));
    return {
      id, at: info?.at,
      loading: () => picture.loading || alternates.loading,
      available,
    };
  });
  const first = captures.find((item) => item.id === capture(before))!;
  const selected = createMemo(() => captures.find((item) => item.id === picked())!);
  const both = () => selected() !== first;
  const canSwitch = () => [first, selected()].some(({ available }) =>
    available().some((item) => item.device === 'phone') && available().some((item) => item.device === 'computer'));
  const choice = () => canSwitch() ? device() : undefined;
  const screen = () => choice() ?? selected().available()[0]?.device ?? first.available()[0]?.device ?? 'computer';
  const pictureOf = (item: typeof first) => item.available().find((picture) => picture.device === screen())
    ?? (choice() ? undefined : item.available()[0]);

  onMount(() => {
    dialog?.showModal();
    close?.focus();
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      } else if (e.key === 'Tab') keepFocusIn(e, dialog);
      else if (pressedAny(e)) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    window.addEventListener('keydown', key, true);
    onCleanup(() => {
      window.removeEventListener('keydown', key, true);
      dialog?.close();
      queueMicrotask(() => {
        const opener = props.opener.isConnected ? props.opener : document.querySelector<HTMLElement>('.composer textarea');
        opener?.focus({ preventScroll: true });
      });
    });
  });

  const label = (before: boolean) => t(before ? 'beforeAfter.before' : 'beforeAfter.after');
  const photo = (item: typeof first, before: boolean) => <Show when={pictureOf(item)} fallback={
    <span class="before-after-unavailable" role="status">
      {t(item.loading() ? 'beforeAfter.loading' : 'site.step.noPicture')}
    </span>
  }>{(picture) => <img src={picture().src} alt={label(before)} />}</Show>;
  const time = createMemo(() => new Intl.DateTimeFormat(lang(), { timeStyle: 'medium' }));
  return <Portal>
    <dialog class="before-after" ref={(el) => { dialog = el; }} aria-labelledby={title} onCancel={(e) => { e.preventDefault(); props.onClose(); }}>
      <header class="before-after-head">
        <h2 class="display" id={title}>{t('planMap.compare')}</h2>
        <button class="iconbtn before-after-close" type="button" ref={(el) => { close = el; }}
          aria-label={t('viewer.close')} title={t('viewer.close')} onClick={props.onClose}><Icon name="close" /></button>
      </header>
      <Show when={canSwitch()}>
        <div class="before-after-devices">
          <Segmented<Device> bare id={`${title}-device`} label={t('beforeAfter.device')} value={screen()}
            options={[[ 'phone', t('beforeAfter.phone') ], [ 'computer', t('beforeAfter.computer') ]]} onPick={setDevice} />
        </div>
      </Show>
      <Show when={screen() === 'phone'} fallback={
        <div class="before-after-desktop">
          <div class="before-after-labels"><span>{label(true)}</span><Show when={both()}><span>{label(false)}</span></Show></div>
          <div class="before-after-wide" style={{ '--divide': `${divider()}%` }}>
            {photo(first, true)}
            <Show when={both()}>
              <div class="before-after-overlay" classList={{ 'before-after-split': !!pictureOf(first) && !!pictureOf(selected()) }}>
                {photo(selected(), false)}
              </div>
              <Show when={pictureOf(first) && pictureOf(selected())}>
                <div class="before-after-divider" aria-hidden="true"><span>‹ ›</span></div>
                <input type="range" min="0" max="100" value={divider()} aria-label={t('beforeAfter.divider')}
                  onInput={(e) => setDivider(e.currentTarget.valueAsNumber)} />
              </Show>
            </Show>
          </div>
        </div>
      }>
        <div class="before-after-phones">
          <figure><figcaption>{label(true)}</figcaption><div>{photo(first, true)}</div></figure>
          <Show when={both()}>
            <figure><figcaption>{label(false)}</figcaption><div>{photo(selected(), false)}</div></figure>
          </Show>
        </div>
      </Show>
      <div class="before-after-strip" role="group" aria-label={t('beforeAfter.captures')}>
        <For each={captures}>{(item, index) => {
          const latest = item.id === capture(after);
          return <button type="button" class="btn" data-latest={latest} aria-pressed={selected() === item}
            onClick={() => { setPicked(item.id); setDivider(50); }}>
            <Show when={item.available()[0]}>{(picture) => <img src={picture().src} alt="" />}</Show>
            <span>{t('beforeAfter.capture', { n: index() + 1 })}</span>
            <Show when={item.at !== undefined}>
              <time dateTime={new Date(item.at!).toISOString()}>{time().format(item.at!)}</time>
            </Show>
            <Show when={latest}><strong>{t('beforeAfter.latest')}</strong></Show>
          </button>;
        }}</For>
      </div>
      <div class="before-after-request"><strong>{t('beforeAfter.request')}</strong><p>{part.name}</p></div>
    </dialog>
  </Portal>;
}
