import { createSignal, For, onCleanup, Show } from 'solid-js';
import { api, type ChatGroup as Group } from './api.ts';
import { inOrder } from './appearance.ts';
import { readAs } from './attachments.ts';
import { Icon } from './Glyphs.tsx';
import { failureText, t } from './i18n.ts';

/** Only the group's own context lives here; the shared settings stay in Settings. */
export function ChatGroup(props: { group: Group; inert?: boolean; onChanged: (group: Group) => void; onNew: () => Promise<void>; onReady?: (wait?: () => Promise<boolean>) => void }) {
  const id = props.group.id;
  const [instructions, setInstructions] = createSignal(props.group.instructions);
  const [saved, setSaved] = createSignal(props.group.instructions);
  const [starting, setStarting] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [pending, setPending] = createSignal(0);
  const inTurn = inOrder();
  let input: HTMLInputElement | undefined;
  const run = (job: () => Promise<unknown>) => {
    setPending((n) => n + 1);
    return inTurn(async () => {
      setError(undefined);
      try { await job(); return true; }
      catch (err) { setError(failureText(err)); return false; }
      finally { setPending((n) => n - 1); }
    });
  };
  const save = (words: string) => run(async () => {
    props.onChanged(await api.saveChatGroup(id, undefined, words));
    setSaved(words);
  });
  props.onReady?.(() => inTurn(async () => instructions() === saved() && !error()));
  onCleanup(() => props.onReady?.(undefined));
  const add = (list: Iterable<File>) => {
    const files = [...list];
    if (!files.length) return;
    if (files.length + props.group.files.length > 10 || files.reduce((n, f) => n + f.size, 0) + props.group.files.reduce((n, f) => n + f.size, 0) > 5_000_000) {
      setError(t('error.groupFiles'));
      return;
    }
    void run(async () => {
      const attachments = [];
      for (const file of files) {
        const url = await readAs(file, 'dataUrl');
        attachments.push({ name: file.name, base64: url.slice(url.indexOf(',') + 1) });
      }
      props.onChanged(await api.addChatGroupFiles(id, attachments));
    });
  };
  return (
    <section class="chat-group-page" inert={props.inert} aria-labelledby="chat-group-title" onDragOver={(e) => e.preventDefault()} onDrop={(e) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.dataTransfer) add(e.dataTransfer.files);
    }}>
      <h1 id="chat-group-title">{props.group.name}</h1>
      <p class="note">{t('group.about')}</p>
      <label class="group-instructions">
        <span>{t('group.instructions')}</span>
        <textarea class="field" data-testid="group-instructions" value={instructions()} aria-describedby="group-next-launch" onInput={(e) => {
          const words = e.currentTarget.value;
          setInstructions(words);
          if (Array.from(words).length > 8000) { setError(t('error.groupInstructions')); return; }
          void save(words);
        }} />
      </label>
      <p class="note" id="group-next-launch">{t('group.nextLaunch')}</p>
      <section aria-labelledby="group-files-title">
        <div class="group-files-heading">
          <h2 id="group-files-title">{t('group.files')}</h2>
          <button class="btn" type="button" disabled={pending() > 0} onClick={() => input?.click()}><Icon name="clip" small />{t('composer.attach')}</button>
        </div>
        <input ref={(el) => (input = el)} type="file" multiple hidden onChange={(e) => {
          if (e.currentTarget.files) add(e.currentTarget.files);
          e.currentTarget.value = '';
        }} />
        <p class="note">{t('group.fileLimit')}</p>
        <ul class="group-files">
          <For each={props.group.files}>{(file) => <li>
            <Icon name={'sheet'} small />
            <span>{file.name}</span>
            <button class="iconbtn" type="button" disabled={pending() > 0} aria-label={t('group.removeFile', { name: file.name })} onClick={() => void run(async () => props.onChanged(await api.removeChatGroupFile(id, file.id)))}><Icon name="close" small /></button>
          </li>}</For>
        </ul>
      </section>
      <Show when={error()}><p class="note err" role="alert">{error()}</p><button class="btn" type="button" disabled={pending() > 0} onClick={() => void save(instructions())}>{t('group.retrySave')}</button></Show>
      <button class="btn primary" type="button" data-action="new-group-chat" disabled={pending() > 0 || starting() || instructions() !== saved() || !!error()} onClick={() => {
        setStarting(true);
        void props.onNew().catch((err) => setError(failureText(err))).finally(() => setStarting(false));
      }}>{t('group.newChat')}</button>
    </section>
  );
}
