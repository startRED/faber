import { createSignal, For, Show } from 'solid-js';
import { Icon } from './Glyphs.tsx';
import { t } from './i18n.ts';

const pipelines = [
  { id: 'concept', icon: 'dice' },
  { id: 'production', icon: 'gamepad' },
  { id: 'art3d', icon: 'layers' },
  { id: 'playtest', icon: 'flask' },
  { id: 'launch', icon: 'megaphone' },
] as const;
type Pipeline = typeof pipelines[number];

export const catalogPipeline = (selection: string | undefined) => pipelines.find((pipeline) => `pipeline:${pipeline.id}` === selection);
const title = (pipeline: Pipeline) => t(`catalog.pipeline.${pipeline.id}.title`);
const summary = (pipeline: Pipeline) => t(`catalog.pipeline.${pipeline.id}.summary`);
const request = (pipeline: Pipeline) => t(`catalog.pipeline.${pipeline.id}.prompt`);
export const matchingPipelines = (search: string) => pipelines.filter((pipeline) =>
  `${title(pipeline)} ${summary(pipeline)} ${request(pipeline)}`.toLocaleLowerCase().includes(search));

export function CatalogPipelineCards(props: { visible: readonly Pipeline[]; open: (id: string, button: HTMLButtonElement) => void }) {
  return <section class="catalog-group">
    <h2>{t('catalog.pipelines')}</h2>
    <div class="catalog-grid"><For each={props.visible}>{(pipeline) => <div class="catalog-tile">
      <button class="catalog-card" data-catalog-pipeline={pipeline.id}
        onClick={(e) => props.open(`pipeline:${pipeline.id}`, e.currentTarget)}>
        <span class="catalog-icon"><Icon name={pipeline.icon} /></span>
        <span class="catalog-name">{title(pipeline)}</span>
        <span class="catalog-description">{summary(pipeline)}</span>
      </button>
    </div>}</For></div>
  </section>;
}

export function CatalogPipelineDetail(props: { pipeline: Pipeline; back: () => void; heading: (element: HTMLHeadingElement) => void }) {
  const [copyState, setCopyState] = createSignal<'copying' | 'copied' | 'failed'>();
  const prompt = () => request(props.pipeline);
  let field: HTMLTextAreaElement | undefined;
  const copy = async () => {
    if (copyState() === 'copying') return;
    setCopyState('copying');
    try {
      await navigator.clipboard.writeText(prompt());
      setCopyState('copied');
    } catch {
      setCopyState('failed');
      if (field?.isConnected) { field.focus(); field.select(); }
    }
  };
  return <article class="catalog-detail" data-pipeline-detail>
    <div class="catalog-return"><button class="btn" onClick={props.back}><Icon name="left" small />{t('catalog.back')}</button></div>
    <header class="catalog-hero">
      <span class="catalog-icon"><Icon name={props.pipeline.icon} /></span>
      <div class="catalog-hero-text"><h1 ref={props.heading} tabindex="-1">{title(props.pipeline)}</h1></div>
    </header>
    <p class="catalog-description">{summary(props.pipeline)}</p>
    <h2>{t('catalog.pipeline.promptLabel')}</h2>
    <p class="hint">{t('catalog.pipeline.use')}</p>
    <textarea class="field catalog-prompt" data-pipeline-prompt ref={(el) => (field = el)} readOnly value={prompt()} aria-label={t('catalog.pipeline.promptLabel')} />
    <div class="catalog-actions">
      <button class="btn primary" data-pipeline-copy disabled={copyState() === 'copying'} onClick={() => void copy()}>
        <Icon name="copy" small />{t('catalog.pipeline.copy')}
      </button>
      <Show when={copyState() === 'copied' || copyState() === 'failed'}>
        <span role="status" data-pipeline-copy-state>{t(copyState() === 'copied' ? 'answer.copied' : 'catalog.pipeline.copyFailed')}</span>
      </Show>
    </div>
  </article>;
}
