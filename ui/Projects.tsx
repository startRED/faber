import { createMemo, For, Show } from 'solid-js';
import { api, type Project, type RunningItem, type Summary, type Vendor } from './api.ts';
import { Ambient } from './Ambient.tsx';
import { Empty } from './Empty.tsx';
import { Recents } from './Recents.tsx';
import { t } from './i18n.ts';
import { animateList } from './motion.ts';
import { pickUp } from './running.ts';
import { ResumeRow } from './Welcome.tsx';

interface Props {
  vendors: Vendor[];
  /** What the screen offers to pick up. */
  projects: Project[];
  recent: Summary[];
  running: RunningItem[];
  onOpen: (conversation: Summary) => void;
  onAdd: () => void;
  /** Opens recent folders or a saved environment as projects. */
  onOpened: (projects: Project[]) => void;
  onError: (error: unknown) => void;
}

/** The Projects area with no project open: one small "Open a project" screen. A project that is open is a conversation
 * with its agent (the new-conversation screen of Chat, ui/Welcome.tsx), never a screen of its own. */
export function Projects(props: Props) {
  let root: HTMLDivElement | undefined;
  const conversations = createMemo(() => pickUp(props.recent.filter((c) => c.area === 'project'), props.running, 3));
  const projectName = (c: Summary) => props.projects.find((p) => p.id === c.projectId)?.name ?? t('area.projects');
  // What the dust keeps clear of: the text blocks, so it never lies behind words.
  const avoid = () => (root ? [...root.querySelectorAll('.p-resume')] : []);

  return (
    <div class="projects-root" ref={(el) => (root = el)}>
      <Ambient variant="project" vendors={props.vendors} avoid={avoid} />
      <div class="projects-stage home" ref={(el) => animateList(el)}>
        <div class="welcome-grid" ref={animateList}>
          <Empty
            class="w-main"
            scene="openProject"
            title={t('empty.openProjectTitle')}
            line={t('empty.openProjectBody')}
            button={{ label: t('side.addProject'), onClick: () => props.onAdd() }}
          >
            <Recents
              mode="project"
              onPick={(path) => void api.openFolders([path]).then(props.onOpened, (error) => props.onError(error))}
              onOpened={props.onOpened}
              onError={(error) => props.onError(error)}
            />
          </Empty>
          <Show when={conversations().length > 0}>
            <div class="p-side">
              {/* At most three: the sidebar already lists every project, so this screen does not list them again. */}
              <section class="p-resume" aria-labelledby="pickup-title" ref={animateList}>
                <h3 id="pickup-title">{t('s1.pickUp')}</h3>
                <For each={conversations()}>
                  {(c) => <ResumeRow conversation={c} item={props.running.find((i) => i.conversationId === c.id)} where={projectName(c)} onOpen={props.onOpen} />}
                </For>
              </section>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
