import { Stage } from '../../kit/stage';
import { Bar, Text } from '../../kit/window';
import { Button } from '../../kit/controls/Button';
import { Field } from '../../kit/controls/Field';
import { Collapse } from '../../kit/motion/Collapse';
import { Row } from '../../kit/parts/Row';
import type { Act } from '../../kit/act';
import { duration, initial, perform, poster } from './scene.mjs';
import content from './content.json';
import assets from './assets.json';
import './tokens.css';
import './faber.css';

type Scene = typeof initial;
const c = content;
const phases = [c.reading, c.building, c.checking, c.done];
const script = (a: Act<Scene>) => perform(a, c.request);

function Glyph({ name }: { name: keyof typeof assets }) {
  return <svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
    {assets[name].map((d) => <path key={d} d={d} />)}
  </svg>;
}

// Faber's own frame and icons replace the kit's macOS chrome.
export default function FaberDemo() {
  return <Stage width={960} height={510} compact={{ below: 600, width: 390, height: 570 }}
    initial={initial} poster={poster} script={script} play="once" theme="dark" fidelity="standard"
    cursor={{ at: 'request', idle: 1300 }} className="faber-demo"
    tokens={{ '--ss-font': 'var(--font-ui)', '--ss-accent': 'var(--accent)' }}>
    {(s, _rect, layout) => <div className="faber-frame" role="img" aria-label={c.description} data-phase={s.phase} data-preview={s.preview || undefined} data-duration={duration}>
      <div className="faber-titlebar">
        <img src={new URL('./mark.svg', import.meta.url).href} alt="" />
        <Text>{c.brand}</Text>
        <span className="faber-disclosure">{c.synthetic}</span>
      </div>
      <div className="faber-workspace">
        {layout === 'wide' && <aside className="faber-sidebar" aria-hidden="true">
          <Row className="faber-nav" lead={<Glyph name="folder" />} title={c.projects} selected />
          <Row className="faber-project" title={c.project} />
          <div className="faber-past"><Bar w="67%" /><Bar w="42%" /><Bar w="56%" /></div>
        </aside>}
        <main className="faber-room">
          <div className="faber-topbar"><span>{c.project}</span><Glyph name="panel" /></div>
          <div className="faber-conversation">
            <Collapse open={s.sent} from="below">
              <div className="faber-question"><Text wrap>{c.request}</Text></div>
            </Collapse>
            <div className="faber-progress" data-on={s.sent || undefined}>
              <img src={new URL('./mark.svg', import.meta.url).href} alt="" />
              <div className="faber-status-stack">{phases.map((text, i) => <div key={text} data-active={s.phase === i + 1 || undefined}>
                <span>{text}</span>
              </div>)}</div>
            </div>
            <div className="faber-work-lines" data-on={s.sent && !s.result || undefined} aria-hidden="true">
              <Bar w="74%" /><Bar w="90%" /><Bar w="48%" />
            </div>
            <div className="faber-result" data-on={s.result || undefined}>
              <div className="faber-result-word">{c.result}</div>
              <Button data-target="preview" size="large">{c.preview}</Button>
            </div>
          </div>
          <div className="faber-composer" data-on={!s.sent || undefined}>
            <Field data-target="request" value={s.draft} placeholder={c.request} focused={!s.sent} />
            <Button data-target="send" variant="primary" aria-label={c.request}><Glyph name="up" /></Button>
          </div>
        </main>
        <section className="faber-preview" data-on={s.preview || undefined} aria-label={c.preview}>
          <div className="faber-preview-bar"><span>{c.preview}</span></div>
          <div className="faber-planner">
            <div className="faber-planner-title">{c.result}</div>
            <p>{c.resultDetail}</p>
            <div className="faber-days" aria-hidden="true">{[0, 1, 2, 3, 4, 5, 6].map((day) => <div key={day} className={day > 4 ? 'faber-day-off' : ''}><i /><b /><b /></div>)}</div>
            <div className="faber-plans" aria-hidden="true"><span /><span /><span /></div>
          </div>
        </section>
      </div>
    </div>}
  </Stage>;
}
