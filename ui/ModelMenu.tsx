import { createEffect, createMemo, createSignal, createUniqueId, For, onCleanup, Show } from 'solid-js';
import { DEFAULT_EFFORT, type Choice, type Vendor } from './api.ts';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { type Option, prettyModel, vendorName } from './models.ts';

export function ModelMenu(props: { options: Option[]; choice: Choice; disabled?: boolean; onChoice: (choice: Choice) => void }) {
  const id = createUniqueId();
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal('');
  let trigger!: HTMLButtonElement;
  let panel: HTMLDivElement | undefined;
  let position = () => {};
  const selected = (option: Option) => option.choice.vendor === props.choice.vendor && option.choice.model === props.choice.model;
  const models = createMemo(() => props.options.filter((option) => option.choice.vendor !== 'auto'));
  const groups = createMemo(() => (['claude', 'codex'] as Vendor[]).filter((vendor) => models().some((option) => option.choice.vendor === vendor)));
  const matches = (option: Option) => option.label.toLocaleLowerCase().includes(query().trim().toLocaleLowerCase());
  const label = () => props.choice.vendor === 'auto' ? t('model.autoChoose') : props.options.find(selected)?.label ?? prettyModel(props.choice.model);
  const close = () => { if (panel?.matches(':popover-open')) panel.hidePopover(); setOpen(false); };
  const pick = (option: Option) => {
    const choice = option.choice;
    props.onChoice(choice.vendor === 'auto' ? choice : { ...choice, effort: selected(option) ? (props.choice.effort ?? DEFAULT_EFFORT) : DEFAULT_EFFORT });
    close();
    trigger.focus();
  };
  createEffect(() => {
    if (props.disabled) close();
  });
  createEffect(() => {
    if (!open()) return;
    setQuery('');
    position = () => {
      if (!panel) return;
      const box = trigger.getBoundingClientRect();
      const below = innerHeight - box.bottom - 14;
      const above = box.top - 14;
      const down = below >= Math.min(panel.scrollHeight, 360) || below >= above;
      panel.style.maxHeight = `${Math.max(0, Math.min(360, down ? below : above))}px`;
      panel.style.left = `${Math.max(8, Math.min(box.left, innerWidth - panel.offsetWidth - 8))}px`;
      panel.style.top = `${down ? box.bottom + 6 : box.top - panel.offsetHeight - 6}px`;
      panel.dataset.direction = down ? 'down' : 'up';
    };
    queueMicrotask(() => {
      if (!open() || !panel?.isConnected) return;
      panel.showPopover();
      position();
      (panel.querySelector<HTMLElement>('.model-option[aria-pressed="true"]') ?? panel.querySelector<HTMLElement>('button'))?.focus();
    });
    window.addEventListener('resize', position);
    document.addEventListener('scroll', position, true);
    onCleanup(() => {
      window.removeEventListener('resize', position);
      document.removeEventListener('scroll', position, true);
    });
  });
  createEffect(() => { query(); queueMicrotask(position); });
  const row = (option: Option) => <button type="button" class="model-option" aria-pressed={selected(option)} onClick={() => pick(option)}>
    <span>{option.label}</span><Show when={selected(option)}><Icon name="check" small /></Show>
  </button>;
  return <>
    <button ref={(el) => (trigger = el)} type="button" class="chip choose model-chip" data-tour="auto" disabled={props.disabled}
      aria-label={`${t('composer.model')}: ${label()}${props.choice.vendor !== 'auto' && props.choice.model ? ` · ${t('effort.label')}: ${t(`effort.${props.choice.effort ?? DEFAULT_EFFORT}`)}` : ''}`}
      aria-haspopup="dialog" aria-expanded={open()} aria-controls={open() ? id : undefined}
      onClick={() => open() ? close() : setOpen(true)}
      onKeyDown={(e) => { if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); setOpen(true); } }}>
      <Show when={props.choice.vendor !== 'auto'} fallback={<Icon name="auto" small />}><span class={`mk ${props.choice.vendor}`} aria-hidden="true" /></Show>
      <span class="model-label">{label()}</span>
      <Show when={props.choice.vendor !== 'auto' && props.choice.model}><span class="model-effort">· {t(`effort.${props.choice.effort ?? DEFAULT_EFFORT}`)}</span></Show>
      <Icon name="down" small />
    </button>
    <Show when={open()}>
      <div ref={(el) => (panel = el)} id={id} popover="auto" class="model-menu" role="dialog" aria-label={t('composer.model')}
        onToggle={(e) => { if (e.target === e.currentTarget && (e as ToggleEvent).newState === 'closed') setOpen(false); }}
        onKeyDown={(e) => {
          if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); trigger.focus(); return; }
          if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key) || (e.target instanceof HTMLInputElement && !e.key.startsWith('Arrow'))) return;
          const items = [...e.currentTarget.querySelectorAll<HTMLElement>('button, summary, input')].filter((item) => item.getClientRects().length > 0);
          const at = items.indexOf(document.activeElement as HTMLElement);
          const next = e.key === 'Home' ? 0 : e.key === 'End' ? items.length - 1 : (at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
          e.preventDefault(); items[next]?.focus();
        }}>
        <For each={props.options.filter((option) => option.choice.vendor === 'auto')}>{row}</For>
        <Show when={models().length > 8}>
          <input class="model-search" type="search" aria-label={t('model.search')} placeholder={t('model.search')} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
        </Show>
        <For each={groups()}>{(vendor) => {
          const options = () => models().filter((option) => option.choice.vendor === vendor && matches(option));
          const current = () => options().filter((option) => option.current !== false);
          const older = () => options().filter((option) => option.current === false);
          return <Show when={options().length}>
            <section role="group" aria-label={vendorName(vendor)}>
              <div class={`model-group vm ${vendor}`}><span class={`mk ${vendor}`} aria-hidden="true" />{vendorName(vendor)}</div>
              <For each={current()}>{row}</For>
              <Show when={older().length}>
                <Show when={query().trim()} fallback={<details open={older().some(selected)} onToggle={() => position()}><summary>{t('model.older')}</summary><For each={older()}>{row}</For></details>}>
                  <For each={older()}>{row}</For>
                </Show>
              </Show>
            </section>
          </Show>;
        }}</For>
        <Show when={query().trim() && !models().some(matches)}><p class="model-empty" role="status">{t('model.noMatches')}</p></Show>
        <Show when={props.choice.vendor !== 'auto' && props.choice.model}>
          <div class="model-efforts" role="group" aria-label={t('effort.label')}>
            <span>{t('effort.label')}</span>
            <For each={['low', 'medium', 'high'] as const}>{(effort) => <button type="button" aria-pressed={(props.choice.effort ?? DEFAULT_EFFORT) === effort} onClick={() => props.onChoice({ ...props.choice, effort })}>{t(`effort.${effort}`)}</button>}</For>
          </div>
        </Show>
      </div>
    </Show>
  </>;
}
