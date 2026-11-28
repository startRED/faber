/**
 * Events the engine sends before the screen can handle them. `main.tsx` listens first, so none is lost while the first
 * snapshot is asked for and the app is drawn; the app then takes them in the order they came and hears the rest live.
 */
export function createInbox<T>() {
  let sink: ((event: T) => void) | undefined;
  const held: T[] = [];
  return {
    /** An event arrived: handed over now when someone is listening, else kept. */
    push(event: T) {
      if (sink) sink(event);
      else held.push(event);
    },
    /** Starts listening: what was kept goes first, then every event as it comes. `undefined` stops and keeps again. */
    take(next: ((event: T) => void) | undefined) {
      sink = next;
      if (next) for (const event of held.splice(0)) next(event);
    },
  };
}
