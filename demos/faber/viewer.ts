import c from './content.json';
import './tokens.css';
import './viewer.css';

const scene = document.querySelector<HTMLIFrameElement>('#scene')!;
const toggle = document.querySelector<HTMLButtonElement>('#toggle')!;
const step = document.querySelector<HTMLButtonElement>('#step')!;
const replay = document.querySelector<HTMLButtonElement>('#replay')!;
const slow = document.querySelector<HTMLInputElement>('#slow')!;
const error = document.querySelector<HTMLParagraphElement>('#error')!;
const reduced = matchMedia('(prefers-reduced-motion: reduce)');
let playing = !reduced.matches;
let revision = 0;
let ready = false;
let loading: ReturnType<typeof setTimeout>;
const control = (extra = {}) => scene.contentWindow?.postMessage({ type: 'stageshow:control', playing, visible: !document.hidden, speed: slow.checked ? 0.25 : 1, ...extra }, location.origin);
const update = () => {
  toggle.textContent = playing ? c.pause : c.resume;
  toggle.disabled = !ready || reduced.matches;
  step.disabled = !ready || reduced.matches;
};
function load() {
  clearTimeout(loading);
  ready = false;
  error.hidden = true;
  update();
  loading = setTimeout(() => { ready = false; playing = false; error.hidden = false; update(); }, 10000);
  scene.src = `./?embed=faber&revision=${revision++}${reduced.matches ? '&still=1' : ''}`;
}
document.documentElement.lang = c.language;
document.title = `${c.brand} · ${c.title}`;
document.querySelector('h1')!.textContent = c.title;
document.querySelector('header p')!.textContent = c.description;
scene.title = c.description;
replay.textContent = c.replay;
step.textContent = c.step;
document.querySelector('label span')!.textContent = c.slow;
error.textContent = c.error;
toggle.addEventListener('click', () => { playing = !playing; control(); update(); });
step.addEventListener('click', () => { playing = false; control({ step: true }); update(); });
replay.addEventListener('click', () => { playing = !reduced.matches; load(); });
slow.addEventListener('change', () => control());
document.addEventListener('visibilitychange', () => control());
reduced.addEventListener('change', () => { playing = !reduced.matches; load(); });
window.addEventListener('message', (event) => {
  if (event.origin !== location.origin || event.source !== scene.contentWindow || event.data?.type !== 'stageshow:status') return;
  if (event.data.error) { clearTimeout(loading); ready = false; playing = false; error.hidden = false; }
  else if (event.data.ready === true) { clearTimeout(loading); ready = true; error.hidden = true; control(); }
  if (typeof event.data.height === 'number' && Number.isFinite(event.data.height) && event.data.height > 0 && event.data.height < 2000) scene.style.height = `${Math.ceil(event.data.height)}px`;
  if (event.data.staticScene) playing = false;
  update();
});
load();
