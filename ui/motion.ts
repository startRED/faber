import { createEffect, createSignal, onCleanup, onMount, type Accessor } from 'solid-js';
import { motion } from './appearance.ts';

const system = matchMedia('(prefers-reduced-motion: reduce)');
const active = new Set<Animation>();
let exitLayer: HTMLDivElement | undefined;
let resizeUntil = 0;

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
    else if (full && shown()) timer = setTimeout(() => setShown(false), 180);
    else setShown(false);
  });
  onCleanup(() => clearTimeout(timer));
  return shown;
}

function level(el: HTMLElement): string {
  if (performance.now() < resizeUntil) return 'none';
  const requested = el.closest<HTMLElement>('[data-motion]')?.dataset.motion ?? 'full';
  return requested === 'full' && system.matches ? 'reduced' : requested;
}

const offsetOf = (el: HTMLElement) => el.matches('.side') ? 'translateX(-24px)' : el.matches('.runcol') ? 'translateX(24px)' : el.matches('.shell, .settings, .scrim, .tour') ? 'none' : 'translateY(var(--arrive-y))';

function play(el: HTMLElement, frames: Keyframe[], exit = false): Animation | undefined {
  const motion = level(el);
  if (motion === 'none') return;
  const style = getComputedStyle(el);
  const duration = motion === 'reduced' ? 120 : exit ? 180 : Math.min(250, Math.max(150, parseFloat(style.getPropertyValue('--dur-ui')) || 220));
  const animation = el.animate(motion === 'reduced' ? frames.map(({ opacity }) => ({ opacity })) : frames, {
    duration, easing: exit ? 'ease-in' : style.getPropertyValue('--ease').trim() || 'ease-out',
  });
  active.add(animation);
  void animation.finished.then(() => active.delete(animation), () => active.delete(animation));
  return animation;
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
    const capture = () => { before = new Map(children().map((el) => [el, el.getBoundingClientRect()])); };
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
      for (const el of items) {
        if (!animations.has(el)) continue;
        const transform = getComputedStyle(el).transform;
        offsets.set(el, transform === 'none' ? new DOMMatrix() : new DOMMatrix(transform));
        animations.get(el)?.cancel();
      }
      const next = new Map(items.map((el) => [el, el.getBoundingClientRect()]));
      let exits = 0;
      // shortcut: cap simultaneous exit copies at 12; raise only with measured memory headroom.
      for (const [el, savedRect] of before) {
        const closingSide = sidebarClosing && el === side;
        const rect = closingSide ? el.getBoundingClientRect() : savedRect;
        if ((!closingSide && (next.has(el) || el.isConnected)) || level(root) === 'none' || (el.matches('.runcol, .scrim, .side') && level(root) !== 'full') || exits >= 12 || rect.bottom <= 0 || rect.top >= innerHeight || rect.right <= 0 || rect.left >= innerWidth || !rect.width || !rect.height) continue;
        const ghost = el.cloneNode(true) as HTMLElement;
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
      for (const el of items) {
        const previous = before.get(el);
        const rect = next.get(el)!;
        if (el === side) continue;
        if (!rect.width || !rect.height || (el.matches('.runcol, .scrim') && level(root) !== 'full')) continue;
        let animation: Animation | undefined;
        if (!previous) {
          animation = play(el, [{ opacity: 0, transform: offsetOf(el) }, { opacity: 1, transform: 'translate(0)' }]);
        } else if (level(root) === 'full' && (Math.abs(previous.x - rect.x) > 0.5 || Math.abs(previous.y - rect.y) > 0.5)) {
          const offset = offsets.get(el);
          const y = document.documentElement.dataset.base === 'oficina' ? 0 : previous.y + (offset?.m42 ?? 0) - rect.y;
          animation = play(el, [{ transform: `translate(${previous.x + (offset?.m41 ?? 0) - rect.x}px, ${y}px)` }, { transform: 'translate(0, 0)' }]);
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
        return previous && (Math.abs(previous.x - rect.x) > 48 || Math.abs(previous.y - rect.y) > 48);
      });
      if (jumped) update();
      else if (!animations.size) capture();
    }) : undefined;
    reflow?.observe(root);
    observer.observe(root, { childList: true, subtree });
    root.addEventListener('scroll', capture, true);
    root.addEventListener('transitionend', capture);
    window.addEventListener('resize', capture);
    onCleanup(() => {
      observer.disconnect();
      sideObserver?.disconnect();
      reflow?.disconnect();
      delete root.dataset.motionList;
      root.removeEventListener('scroll', capture, true);
      root.removeEventListener('transitionend', capture);
      window.removeEventListener('resize', capture);
      for (const animation of animations.values()) animation.cancel();
      for (const ghost of ghosts) removeExit(ghost);
    });
  });
}

/** Native disclosure keeps its content visible until the height has finished closing. */
export function animateDetails(root: HTMLElement): void {
  const running = new Map<HTMLDetailsElement, { animation: Animation; opening: boolean }>();
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
      duration: opening ? 220 : 180, easing: opening ? 'ease-out' : 'ease-in',
    });
    active.add(animation);
    running.set(details, { animation, opening });
    const done = () => {
      active.delete(animation);
      if (running.get(details)?.animation !== animation) return;
      details.open = opening;
      running.delete(details);
    };
    void animation.finished.then(done, done);
  };
  root.addEventListener('click', click);
  onCleanup(() => {
    root.removeEventListener('click', click);
    for (const { animation } of running.values()) animation.cancel();
  });
}
