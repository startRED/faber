import { type Accessor, createSignal, onCleanup, onMount } from 'solid-js';

// A file dragged over the window, anywhere (docs/design/layout.md). While one is over a window that has a composer, `over()` is
// true and the screen draws "Drop to attach"; letting go attaches the files through the composer's own drop, the one that
// already works on the box: the same event is handed to the composer, so its add path stays the only one.
//
// Between drags the window hears one `dragenter`. The four others are listened to only from the first `dragenter` of a drag
// until it ends: a drop, the pointer leaving the window, or a quiet of a second (a drag cancelled with Esc says nothing).

const files = (e: DragEvent) => !!e.dataTransfer && [...e.dataTransfer.types].includes('Files');
/** The composer a drop would reach: on screen and not behind a modal layer. */
const composer = () => {
  const box = document.querySelector<HTMLElement>('.composer');
  return box && !box.closest('[inert]') ? box : undefined;
};

export function createFileDrop(): Accessor<boolean> {
  const [over, setOver] = createSignal(false);
  onMount(() => {
    let depth = 0;
    let quiet: ReturnType<typeof setTimeout> | undefined;
    let forwarding = false;
    let listening = false;
    const live: Array<[string, (e: DragEvent) => void, boolean]> = [
      ['dragover', onOver, true],
      ['dragleave', onLeave, true],
      ['drop', onDrop, false],
      ['dragend', () => finish(), true],
    ];
    function finish() {
      clearTimeout(quiet);
      depth = 0;
      setOver(false);
      if (!listening) return;
      listening = false;
      for (const [type, listener, capture] of live) window.removeEventListener(type, listener as EventListener, capture);
    }
    const heartbeat = () => {
      clearTimeout(quiet);
      quiet = setTimeout(finish, 1000);
    };
    function onOver(e: DragEvent) {
      if (!files(e)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
      heartbeat();
    }
    function onLeave(e: DragEvent) {
      if (!files(e)) return;
      depth -= 1;
      // Leaving the page itself has no element it goes to.
      if (depth <= 0 || (e.relatedTarget === null && (e.clientX <= 0 || e.clientY <= 0 || e.clientX >= innerWidth || e.clientY >= innerHeight))) finish();
    }
    function onDrop(e: DragEvent) {
      if (forwarding || !files(e)) return;
      e.preventDefault();
      const box = composer();
      const dropped = e.dataTransfer?.files;
      // On the composer itself, its own handler has already taken the files (this listener runs after it).
      const taken = e.target instanceof Element && !!e.target.closest('.composer');
      finish();
      if (taken || !box || !dropped?.length) return;
      const hand = new DataTransfer();
      for (const file of dropped) hand.items.add(file);
      forwarding = true;
      try {
        box.dispatchEvent(new DragEvent('drop', { dataTransfer: hand, bubbles: true, cancelable: true }));
      } finally {
        forwarding = false;
      }
    }
    const enter = (e: DragEvent) => {
      if (!files(e)) return;
      if (!listening) {
        // No composer, no overlay: a settings page or a first-run step has nothing to attach to.
        if (!composer()) return;
        listening = true;
        for (const [type, listener, capture] of live) window.addEventListener(type, listener as EventListener, capture);
      }
      depth += 1;
      setOver(true);
      heartbeat();
    };
    window.addEventListener('dragenter', enter, true);
    onCleanup(() => {
      window.removeEventListener('dragenter', enter, true);
      finish();
    });
  });
  return over;
}
