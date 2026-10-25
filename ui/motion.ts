import { createEffect, createSignal, onCleanup, onMount, type Accessor } from 'solid-js';
import { motion } from './appearance.ts';

const system = matchMedia('(prefers-reduced-motion: reduce)');
const active = new Set<Animation>();
let exitLayer: HTMLDivElement | undefined;
let resizeUntil = 0;

/** A timing token of the page, in milliseconds (or its text, for a curve). The theme owns the values; the numbers here are only the fallback. */
const curve = (el: Element, name: string, fallback: string) => getComputedStyle(el).getPropertyValue(name).trim() || fallback;
export const millis = (el: Element, name: string, fallback: number) => {
  // The browser may hand a custom property back in seconds (180ms reads as .18s).
  const value = getComputedStyle(el).getPropertyValue(name).trim();
  const number = parseFloat(value);
  return number > 0 ? (value.endsWith('ms') ? number : value.endsWith('s') ? number * 1000 : number) : fallback;
};

function exitSurface(): ShadowRoot {
  if (!exitLayer) {
    exitLayer = document.createElement('div');
    exitLayer.className = 'motion-exit';
    exitLayer.inert = true;
    exitLayer.setAttribute('aria-hidden', 'true');
    exitLayer.style.cssText = 'position:fixed;inset:0;overflow:hidden;pointer-events:none;z-index:20';
    const shadow = exitLayer.attachShadow({ mode: 'open' });
    const style = document.createElement('style');
    style.textContent = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]
      .filter((rule) => !(rule instanceof CSSFontFaceRule)).map((rule) => rule.cssText)).join('\n');
    shadow.append(style);
  }
  // Reuse one small stylesheet; the shadow keeps exit copies out of live selectors and accessibility queries.
  if (!exitLayer.isConnected) document.body.append(exitLayer);
  return exitLayer.shadowRoot!;
}

function removeExit(ghost: HTMLElement): void {
  ghost.remove();
  if (exitLayer?.shadowRoot?.childElementCount === 1) exitLayer.remove();
}

/** Keep a collapsing section only until its short exit finishes. */
export function keepForExit(open: Accessor<boolean>): Accessor<boolean> {
  const [shown, setShown] = createSignal(open());
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    const visible = open();
    const full = motion() === 'full';
    clearTimeout(timer);
    if (visible) setShown(true);
    else if (full && shown()) timer = setTimeout(() => setShown(false), millis(document.documentElement, '--dur-exit', 180));
    else setShown(false);
  });
  onCleanup(() => clearTimeout(timer));
  return shown;
}

export function level(el: HTMLElement): string {
  if (performance.now() < resizeUntil) return 'none';
  const requested = el.closest<HTMLElement>('[data-motion]')?.dataset.motion ?? 'full';
  return requested === 'full' && system.matches ? 'reduced' : requested;
}

const offsetOf = (el: HTMLElement) => el.matches('.side') ? 'translateX(calc(-1 * var(--motion-slide)))' : el.matches('.runcol') ? 'translateX(var(--motion-slide))' : el.matches('.shell, .settings, .scrim, .tour') ? 'none' : el.matches('.ask, .coach, .setup-card') ? 'translateY(var(--arrive-y)) scale(var(--motion-scale))' : 'translateY(var(--arrive-y))';

function play(el: HTMLElement, frames: Keyframe[], exit = false, delay = 0): Animation | undefined {
  const motion = level(el);
  if (motion !== 'full') return;
  const duration = Math.min(250, exit ? millis(el, '--dur-exit', 180) : Math.max(150, millis(el, '--dur-ui', 220)));
  if (el.matches('.ask, .coach, .setup-card')) frames = frames.map((frame, i) => ({ ...frame, filter: (exit ? i === 0 : i !== 0) ? 'blur(0px)' : 'blur(var(--motion-blur))' }));
  // Large moving stages raster slowly with blurred shadows and rounded clipping; restore them at rest.
  if (el.matches('.runcol')) frames = frames.map((frame) => ({ ...frame, boxShadow: 'none', borderRadius: '0px' }));
  const animation = el.animate(frames, {
    duration, delay: Math.min(48, delay), fill: 'backwards', easing: curve(el, exit ? '--ease-in' : '--ease-spring', exit ? 'ease-in' : 'ease-out'),
  });
  active.add(animation);
  el.dispatchEvent(new Event('faber-motion', { bubbles: true }));
  void animation.finished.then(() => active.delete(animation), () => active.delete(animation));
  return animation;
}

/** The real column leaves before its native page and reading state are released. */
export function leaveColumn(el: HTMLElement): Animation | undefined {
  const rect = el.getBoundingClientRect();
  const style = getComputedStyle(el);
  const box: Keyframe = { position: 'fixed', left: `${rect.left}px`, top: `${rect.top}px`, width: `${rect.width}px`, height: `${rect.height}px`, margin: '0', opacity: 1,
    padding: style.padding, borderWidth: style.borderWidth, borderStyle: style.borderStyle, borderColor: style.borderColor, color: style.color };
  for (const animation of el.getAnimations()) animation.cancel();
  if (level(el) !== 'full' || !el.showPopover) return;
  // The top layer keeps viewport geometry independent of transformed, still-moving ancestors without reparenting.
  el.setAttribute('popover', 'manual');
  el.showPopover();
  const animation = play(el, [{ ...box, transform: 'translateX(0)' }, { ...box, transform: 'translateX(calc(100% + 24px))' }], true);
  const restore = () => el.removeAttribute('popover');
  if (animation) void animation.finished.then(restore, restore);
  else restore();
  return animation;
}

/**
 * A box that grew by `by` px at its top edge is already laid out at its new size; this draws it at the old size and
 * reveals it upward. Clipping costs no layout, unlike animating the height. Only in full motion.
 */
export function reveal(el: HTMLElement, by: number): Animation | undefined {
  return play(el, [{ clipPath: `inset(${by}px -80px -80px -80px)` }, { clipPath: 'inset(0 -80px -80px -80px)' }]);
}

/** Stop in-flight travel too when the person changes their motion preference. */
export function watchMotion(): void {
  onMount(() => {
    let viewWidth = innerWidth;
    let viewHeight = innerHeight;
    let settle: ReturnType<typeof setTimeout> | undefined;
    const resize = () => {
      if (viewWidth === innerWidth && viewHeight === innerHeight) return;
      viewWidth = innerWidth;
      viewHeight = innerHeight;
      resizeUntil = performance.now() + 250;
      document.documentElement.classList.add('resizing');
      for (const animation of active) animation.cancel();
      clearTimeout(settle);
      settle = setTimeout(() => document.documentElement.classList.remove('resizing'), 250);
    };
    const stop = () => {
      if (level(document.documentElement) !== 'full') for (const animation of active) animation.cancel();
    };
    const observer = new MutationObserver(stop);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['data-motion'] });
    system.addEventListener('change', stop);
    window.addEventListener('resize', resize);
    onCleanup(() => {
      observer.disconnect();
      system.removeEventListener('change', stop);
      window.removeEventListener('resize', resize);
      clearTimeout(settle);
      document.documentElement.classList.remove('resizing');
      for (const animation of active) animation.cancel();
    });
  });
}

// Every animated list needs to hear the window resize. They share one listener, which exists while any list does.
const resizers = new Set<() => void>();
const onWindowResize = () => { for (const resize of resizers) resize(); };
/** A list measures where its rows sit again, with no animation: after the person moved an edge by hand, so the next change
 * moves from where things are now. */
export const remeasure = onWindowResize;
function watchResize(resize: () => void): () => void {
  if (!resizers.size) window.addEventListener('resize', onWindowResize);
  resizers.add(resize);
  return () => {
    resizers.delete(resize);
    if (!resizers.size) window.removeEventListener('resize', onWindowResize);
  };
}

// A transition that only changes how something is painted moves nothing, so a list does not measure its children after it.
const PAINT_ONLY = new Set(['opacity', 'color', 'background-color', 'border-color', 'outline-color', 'box-shadow', 'filter', 'fill', 'stroke']);

/** Small lists and view boundaries: arrivals, inert exit copies and FLIP for the surviving rows. No streaming observer. */
export function animateList(root: HTMLElement, subtree = false): void {
  onMount(() => {
    root.dataset.motionList = '';
    const animations = new Map<HTMLElement, Animation>();
    const ghosts = new Set<HTMLElement>();
    const side = root.matches('.shell') ? root.querySelector<HTMLElement>('.side') : null;
    let sideWasOpen = side?.classList.contains('shown') ?? false;
    let sidebarClosing = false;
    let viewWidth = innerWidth;
    let viewHeight = innerHeight;
    const children = (): HTMLElement[] => [...root.children].flatMap((el) => {
      if (!(el instanceof HTMLElement) || el.matches('.tour-ring')) return [];
      return getComputedStyle(el).display === 'contents' ? [...el.children].filter((child): child is HTMLElement => child instanceof HTMLElement) : [el];
    });
    let before = new Map(children().map((el) => [el, el.getBoundingClientRect()]));
    const capture = () => { if (!animations.size) before = new Map(children().map((el) => [el, el.getBoundingClientRect()])); };
    const resize = () => { viewWidth = innerWidth; viewHeight = innerHeight; capture(); };
    const transitioned = (event: TransitionEvent) => { if (!PAINT_ONLY.has(event.propertyName)) capture(); };
    const update = () => {
      const items = children();
      // Viewport reflow must never animate content from a position outside the new window.
      if (viewWidth !== innerWidth || viewHeight !== innerHeight) {
        viewWidth = innerWidth;
        viewHeight = innerHeight;
        for (const animation of animations.values()) animation.cancel();
        for (const ghost of ghosts) removeExit(ghost);
        before = new Map(items.map((el) => [el, el.getBoundingClientRect()]));
        sidebarClosing = false;
        return;
      }
      const offsets = new Map<HTMLElement, DOMMatrix>();
      const widths = new Map<HTMLElement, number>();
      for (const el of items) {
        if (el.dataset.exiting === 'true') continue;
        if (!animations.has(el)) continue;
        if (el.matches('.main')) widths.set(el, el.getBoundingClientRect().width);
        const transform = getComputedStyle(el).transform;
        offsets.set(el, transform === 'none' ? new DOMMatrix() : new DOMMatrix(transform));
        animations.get(el)?.cancel();
      }
      const next = new Map(items.map((el) => [el, el.getBoundingClientRect()]));
      let exits = 0;
      const closingColumn = root.matches('.shell') && [...before.keys()].some((el) => el.matches('.runcol')) && !items.some((el) => el.matches('.runcol'));
      // shortcut: cap simultaneous exit copies at 12; raise only with measured memory headroom.
      for (const [el, savedRect] of before) {
        const closingSide = sidebarClosing && el === side;
        const rect = closingSide ? el.getBoundingClientRect() : savedRect;
        if (el.dataset.exited === 'true' || el.matches('.main, .shell, .leaving') || (!closingSide && (next.has(el) || el.isConnected)) || level(root) !== 'full' || exits >= 12 || rect.bottom <= 0 || rect.top >= innerHeight || rect.right <= 0 || rect.left >= innerWidth || !rect.width || !rect.height) continue;
        const ghost = el.cloneNode(true) as HTMLElement;
        // Native cloning keeps option defaults, not the select's current choice.
        const selects = el.querySelectorAll('select');
        ghost.querySelectorAll('select').forEach((select, index) => { select.selectedIndex = selects[index].selectedIndex; });
        ghost.removeAttribute('id');
        for (const child of ghost.querySelectorAll('[id]')) child.removeAttribute('id');
        for (const child of ghost.querySelectorAll('[autofocus]')) child.removeAttribute('autofocus');
        ghost.inert = true;
        ghost.setAttribute('aria-hidden', 'true');
        ghost.dataset.motion = level(root);
        ghost.dataset.base = document.documentElement.dataset.base;
        ghost.classList.add('motion-exit');
        if (closingSide) ghost.classList.add('shown');
        ghost.style.cssText += `;position:fixed;left:${rect.left}px;top:${rect.top}px;width:${rect.width}px;height:${rect.height}px;margin:0;pointer-events:none;z-index:20;animation:none`;
        exitSurface().append(ghost);
        ghosts.add(ghost);
        const animation = play(ghost, [{ opacity: closingSide ? getComputedStyle(el).opacity : 1, transform: 'translate(0)' }, { opacity: 0, transform: offsetOf(el) }], true);
        if (animation) {
          animations.set(ghost, animation);
          void animation.finished.then(() => removeExit(ghost), () => removeExit(ghost)).finally(() => {
            ghosts.delete(ghost);
            animations.delete(ghost);
            if (!animations.size) capture();
          });
        }
        exits++;
      }
      let arrivals = 0;
      for (const el of items) {
        if (el.dataset.exiting === 'true') continue;
        const previous = before.get(el);
        const rect = next.get(el)!;
        if (el === side) continue;
        if (!rect.width || !rect.height || (el.matches('.runcol, .scrim') && level(root) !== 'full')) continue;
        let animation: Animation | undefined;
        if (!previous) {
          const delay = root.matches('.rc-body, .s-main') ? arrivals++ * (parseFloat(getComputedStyle(root).getPropertyValue('--motion-stagger')) || 18) : 0;
          animation = play(el, [{ opacity: 0, transform: offsetOf(el) }, { opacity: 1, transform: 'translate(0)' }], false, delay);
        } else if (level(root) === 'full' && (Math.abs(previous.x - rect.x) > 0.5 || Math.abs(previous.y - rect.y) > 0.5 || (el.matches('.main') && Math.abs(previous.width - rect.width) > 0.5))) {
          const offset = offsets.get(el);
          const y = document.documentElement.dataset.base === 'oficina' ? 0 : previous.y + (offset?.m42 ?? 0) - rect.y;
          const from: Keyframe = { transform: `translate(${previous.x + (offset?.m41 ?? 0) - rect.x}px, ${y}px)` };
          const to: Keyframe = { transform: 'translate(0, 0)' };
          if (el.matches('.main') && Math.abs(previous.width - rect.width) > 0.5) {
            from.width = `${widths.get(el) ?? previous.width}px`;
            to.width = `${rect.width}px`;
            from.boxShadow = to.boxShadow = 'none';
            from.borderRadius = to.borderRadius = '0px';
          }
          animation = play(el, [from, to], closingColumn);
        }
        if (animation) {
          animations.set(el, animation);
          const done = () => {
            if (animations.get(el) === animation) animations.delete(el);
            if (!animations.size) capture();
          };
          void animation.finished.then(done, done);
        }
      }
      before = next;
      sidebarClosing = false;
    };
    const observer = new MutationObserver(update);
    const sideObserver = side ? new MutationObserver(() => {
      const open = side.classList.contains('shown');
      sidebarClosing = sideWasOpen && !open && matchMedia('(max-width: 899px)').matches;
      sideWasOpen = open;
      if (sidebarClosing) update();
      else capture();
    }) : undefined;
    if (side) sideObserver?.observe(side, { attributes: true, attributeFilter: ['class'] });
    // A container breakpoint can move the resume list without adding or removing a node.
    const reflow = root.matches('.welcome-grid') ? new ResizeObserver(() => {
      const jumped = children().some((el) => {
        const previous = before.get(el);
        const rect = el.getBoundingClientRect();
        // Compare layout coordinates, excluding the translation of an active FLIP.
        const transform = animations.has(el) ? getComputedStyle(el).transform : 'none';
        const offset = transform === 'none' ? undefined : new DOMMatrix(transform);
        return previous && (Math.abs(previous.x - (rect.x - (offset?.m41 ?? 0))) > 48
          || Math.abs(previous.y - (rect.y - (offset?.m42 ?? 0))) > 48);
      });
      if (jumped) update();
      else if (!animations.size) capture();
    }) : undefined;
    reflow?.observe(root);
    observer.observe(root, { childList: true, subtree, ...(root.matches('.shell') ? { attributes: true, attributeFilter: ['class'] } : {}) });
    if (root.matches('.rc-body, .s-main, .tour')) { before.clear(); update(); }
    root.addEventListener('scroll', capture, true);
    root.addEventListener('transitionend', transitioned);
    const unwatchResize = watchResize(resize);
    onCleanup(() => {
      observer.disconnect();
      sideObserver?.disconnect();
      reflow?.disconnect();
      delete root.dataset.motionList;
      root.removeEventListener('scroll', capture, true);
      root.removeEventListener('transitionend', transitioned);
      unwatchResize();
      for (const animation of animations.values()) animation.cancel();
      for (const ghost of ghosts) removeExit(ghost);
    });
  });
}

/** Native disclosure keeps its content visible until the height has finished closing. */
export function animateDetails(root: HTMLElement): void {
  const running = new Map<HTMLDetailsElement, { animation: Animation; opening: boolean }>();
  // A controlled disclosure can close without a click (for example when another file opens).
  const observer = new MutationObserver(() => {
    for (const [details, pending] of running) {
      if (details.open && root.contains(details)) continue;
      running.delete(details);
      pending.animation.cancel();
    }
  });
  observer.observe(root, { subtree: true, attributes: true, attributeFilter: ['open'], childList: true });
  const click = (event: MouseEvent) => {
    const summary = (event.target as Element).closest('summary');
    const details = summary?.parentElement;
    if (!(details instanceof HTMLDetailsElement) || !root.contains(details) || level(details) !== 'full') return;
    event.preventDefault();
    const pending = running.get(details);
    const opening = pending ? !pending.opening : !details.open;
    const from = details.getBoundingClientRect().height;
    pending?.animation.cancel();
    details.open = true;
    const to = opening ? details.getBoundingClientRect().height : summary!.getBoundingClientRect().height;
    const animation = details.animate([{ height: `${from}px`, overflow: 'hidden' }, { height: `${to}px`, overflow: 'hidden' }], {
      duration: millis(details, opening ? '--dur-ui' : '--dur-exit', opening ? 220 : 180), easing: curve(details, opening ? '--ease' : '--ease-in', opening ? 'ease-out' : 'ease-in'),
    });
    active.add(animation);
    running.set(details, { animation, opening });
    const done = () => {
      active.delete(animation);
      if (running.get(details)?.animation !== animation) return;
      if (!details.open || !root.contains(details)) {
        running.delete(details);
        return;
      }
      if (!opening && details.contains(document.activeElement)) summary!.focus();
      details.open = opening;
      running.delete(details);
    };
    void animation.finished.then(done, done);
  };
  root.addEventListener('click', click);
  onCleanup(() => {
    observer.disconnect();
    root.removeEventListener('click', click);
    for (const { animation } of running.values()) animation.cancel();
    running.clear();
  });
}
