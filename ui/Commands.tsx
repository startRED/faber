import { createMemo, createSignal, For, Show, type JSX } from 'solid-js';
import { api, errorText, type BackgroundCommand, type Summary } from './api.ts';
import { Icon, VendorMark } from './Glyphs.tsx';
import { fromEngine, t } from './i18n.ts';
import { Peek } from './Mark.tsx';
import { vendorName } from './models.ts';

/** The same Stop path in Commands and Technical details, visible without opening output. */
export function CommandStop(props: { command: BackgroundCommand }) {
  const [stopping, setStopping] = createSignal(false);
  const [error, setError] = createSignal('');
  const stop = async () => {
    if (stopping()) return;
    setStopping(true);
    setError('');
    try {
      await api.stopBackground(props.command.id);
    } catch (err) {
      setError(fromEngine(errorText(err)));
    } finally {
      setStopping(false);
    }
  };
  return <>
        <Show when={props.command.state === 'running' && props.command.stoppable}>
          <button class="btn ghost command-stop" disabled={stopping()} onClick={() => void stop()}>{t(stopping() ? 'commands.stopping' : 'commands.stop')}</button>
        </Show>
      <Show when={props.command.state === 'running' && error()}><p class="bad" role="alert">{error()}</p></Show>
  </>;
}

export function CommandRow(props: { command: BackgroundCommand; title: string; output?: (text: string) => JSX.Element }) {
  return (
    <li class="ri command-row" data-command={props.command.id}>
      <div class="rhead">
        <span class="vm" classList={{ claude: props.command.vendor === 'claude', codex: props.command.vendor === 'codex' }}>
          <VendorMark vendor={props.command.vendor} /><b>{vendorName(props.command.vendor)}</b>
          <span class="verb">{t(`commands.${props.command.state}`)}</span>
        </span>
        <CommandStop command={props.command} />
      </div>
      <p class="side-meta command-conversation">{props.title}</p>
      <code class="command-line">{props.command.command}</code>
      <Show when={props.command.output}><pre class="command-output" aria-label={t('commands.output')}>{props.output ? props.output(props.command.output) : props.command.output}</pre></Show>
    </li>
  );
}

/** The running column's command view, mounted only while open; output is already bounded by the engine. */
export function Commands(props: { items: BackgroundCommand[]; conversations: Summary[]; onBack: () => void; onClose: () => void }) {
  // Keep a row and its focused Stop button alive when only its output changes.
  const byId = createMemo(() => new Map(props.items.map((item) => [item.id, item])));
  const titles = createMemo(() => new Map(props.conversations.map((c) => [c.id, c.title])));
  const ids = createMemo(() => [...byId().keys()], [], { equals: (a, b) => a.length === b.length && a.every((id, i) => id === b[i]) });
  return (
    <>
      <div class="rc-head side-head">
        <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
        <h2 id="running-title" class="display">{t('commands.title')}</h2>
        <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
      </div>
      <div class="rc-body side-body commands-view">
        <Show when={props.items.length} fallback={<div class="rc-empty"><Peek /><p>{t('commands.none')}</p></div>}>
          <ul class="ritems">
            <For each={ids()}>{(id) => {
              const item = () => byId().get(id)!;
              return <CommandRow command={item()} title={titles().get(item().conversationId) || t('run.untitled')} />;
            }}</For>
          </ul>
        </Show>
      </div>
    </>
  );
}
