import { createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api, type Message, type Room } from './api.ts';
import { AnswerPictures, BeforeAfter } from './BeforeAfter.tsx';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';
import { filesOf } from './worklog.ts';

// The project as a short list of rooms (task 206): its screens and major parts, read from the project map
// (`engine/src/engine/rooms.rs`), in the right column beside the work. The room the answer on screen changes lights up;
// opening one shows its recent changes and newest before and after picture, a way back to the answer that last changed it
// and a question about it for the composer. No graph: a list a person reads at a glance.

const [opened, setOpened] = createSignal<string | undefined>(undefined, { equals: false });
/** The conversation whose map the column shows, if any. */
export const openedMap = opened;
export const showMap = (conversationId?: string) => setOpened(conversationId);

/** The steps that change files: their files are the ones the answer at work is changing. */
const CHANGING = new Set(['Edit', 'MultiEdit', 'NotebookEdit', 'Write', 'fileChange']);

/** `file` relative to the project's folder, with `/`; a file outside it has none. */
export function relativeTo(folder: string | undefined, file: string): string | undefined {
  const path = file.replaceAll('\\', '/');
  if (!/^([a-zA-Z]:)?\//.test(path)) return path.replace(/^\.\//, '');
  const root = (folder ?? '').replaceAll('\\', '/').replace(/\/+$/, '');
  if (!root) return undefined;
  const same = (a: string, b: string) => /^[a-zA-Z]:/.test(root) ? a.toLowerCase() === b.toLowerCase() : a === b;
  return same(path.slice(0, root.length + 1), `${root}/`) ? path.slice(root.length + 1) : undefined;
}

/** The room a file is in: the screen it is, else the part whose folder holds it most closely (as the engine places it). */
export function roomOf(rooms: Room[], file: string): Room | undefined {
  return rooms.find((room) => room.kind === 'screen' && room.at === file)
    ?? rooms.filter((room) => room.kind === 'part' && file.startsWith(`${room.at}/`)).sort((a, b) => b.at.length - a.at.length)[0];
}

export const roomName = (room: Room) => room.name || t('map.home');

/** The column's view of the map, mounted only while open: one read when it opens and one each time an answer ends. */
export function ProjectMapPane(props: {
  conversationId: string; messages: Message[];
  /** The folders the answers work in (the project's, and its separate copy): a changed file is read relative to them. */
  folders?: (string | undefined)[];
  onGoBack: (index: number) => void; onAsk: (text: string) => void;
  onBack: () => void; onClose: () => void;
}) {
  const latest = createMemo(() => props.messages.findLast((m) => m.role === 'assistant'));
  const working = createMemo(() => latest()?.status === 'streaming' ? latest() : undefined);
  // Reading again only when an answer ends: text streaming in never asks the disk.
  const ended = createMemo(() => props.messages.findLast((m) => m.role === 'assistant' && m.status !== 'streaming')?.id ?? '');
  const [rooms] = createResource(() => `${props.conversationId}|${ended()}`, async (key) => {
    const id = key.split('|')[0]!;
    return { id, rooms: await api.projectRooms(id).catch(() => undefined) };
  });
  const list = () => rooms.latest?.id === props.conversationId ? rooms.latest.rooms : undefined;
  // While an answer works, what its steps change lights its rooms; once it ends, the engine's own reading of the answer does.
  const lit = createMemo(() => {
    const all = list() ?? [];
    const now = working();
    if (!now) return new Set(all.filter((room) => room.touched).map((room) => room.at));
    const files = (now.tools ?? []).filter((tool) => CHANGING.has(tool.name) && !tool.declined).flatMap(filesOf);
    return new Set(files.flatMap((file) => {
      const relative = [...(props.folders ?? []), undefined].map((folder) => relativeTo(folder, file)).find((path) => path !== undefined);
      const room = relative === undefined ? undefined : roomOf(all, relative);
      return room ? [room.at] : [];
    }));
  });
  const [open, setOpen] = createSignal<string>();
  return <>
    <div class="rc-head side-head">
      <button class="iconbtn back" aria-label={t('agent.back')} title={t('agent.back')} onClick={props.onBack}><Icon name="left" /></button>
      <h2 id="running-title" class="display">{t('map.title')}</h2>
      <button class="iconbtn" aria-label={t('run.close')} title={t('run.close')} onClick={props.onClose}><Icon name="close" /></button>
    </div>
    <div class="rc-body side-body project-map">
      <p class="side-meta">{t('map.intro')}</p>
      <Show when={rooms.latest || !rooms.loading} fallback={<p class="side-meta" role="status">{t('map.loading')}</p>}>
        <Show when={list()} fallback={<p class="side-meta" role="status">{t('map.failed')}</p>}>
          {(all) => <Show when={all().length} fallback={<p class="side-meta" role="status">{t('map.empty')}</p>}>
            <ul class="map-rooms" aria-labelledby="running-title">
              <For each={all()}>{(room) => {
                const isOpen = () => open() === room.at;
                const isLit = () => lit().has(room.at);
                const panel = `map-room-${room.kind}-${room.at.replace(/[^\w-]/g, '_')}`;
                return <li class="map-room" data-at={room.at} data-lit={isLit() || undefined}>
                  <button type="button" class="map-room-head" aria-expanded={isOpen()} aria-controls={panel} onClick={() => setOpen(isOpen() ? undefined : room.at)}>
                    <span class="map-room-name">{roomName(room)}</span>
                    <span class="map-room-kind">{t(room.kind === 'screen' ? 'map.screen' : 'map.part')}</span>
                    <Show when={isLit()}><span class="map-room-lit">{t(working() ? 'map.workingNow' : 'map.lastAnswer')}</span></Show>
                  </button>
                  <Show when={isOpen()}>
                    <div class="map-room-body" id={panel}>
                      <p class="map-room-at">{room.at}</p>
                      <Show when={room.picture}>{(picture) => (
                        <AnswerPictures.Provider value={() => ({ [picture().answer]: [picture().page] })}>
                          <BeforeAfter conversationId={props.conversationId} messageId={picture().answer} />
                        </AnswerPictures.Provider>
                      )}</Show>
                      <Show when={room.recent.length} fallback={<p class="side-meta">{t('map.noChanges')}</p>}>
                        <div class="map-room-recent">
                          <span class="map-room-label">{t('map.recent')}</span>
                          <ul><For each={room.recent}>{(file) => <li>{file}</li>}</For></ul>
                        </div>
                      </Show>
                      <div class="map-room-actions">
                        <Show when={room.changedBy}>{(by) => (
                          <button type="button" class="btn" data-map-back onClick={() => props.onGoBack(by().index)}>{t('map.goBack')}</button>
                        )}</Show>
                        <button type="button" class="btn" data-map-ask onClick={() => props.onAsk(t('map.askText', { name: roomName(room), at: room.at }))}>{t('map.ask')}</button>
                      </div>
                    </div>
                  </Show>
                </li>;
              }}</For>
            </ul>
          </Show>}
        </Show>
      </Show>
    </div>
  </>;
}
