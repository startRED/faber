import { createSignal, type Accessor } from 'solid-js';
import { api } from './api.ts';
import { everyWhileShown } from './idle.ts';

/** The time the screen shows relative to: it moves once a minute and never while the window is hidden. */
export function useClock(): Accessor<number> {
  const [now, setNow] = createSignal(Date.now());
  // The engine also learns the time zone again, as a trip or a change of the system clock can move it.
  everyWhileShown(60_000, () => {
    setNow(Date.now());
    void api.setClock(-new Date().getTimezoneOffset() * 60);
  });
  return now;
}
