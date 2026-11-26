/** What a person picked on the page in pick mode (`kind: 'pick'`). */
export type Picked = { role: string; name: string; text: string; selector: string; secret: boolean; rect: number[]; x: number; y: number };

/** The answer to `pick` and `unpick`: what was picked, or that the mode ended, or that nothing happened in time. */
export type PickAnswer = { ok: boolean; turn?: string; picked?: Picked; ended?: boolean; waiting?: boolean };

/** The agent's action inside the page (`act.js`), as the screen's checks call it. */
declare function act(request: Record<string, unknown> & { kind: 'pick' | 'unpick' }): PickAnswer | Promise<PickAnswer>;
declare function act(request: Record<string, unknown>): {
  ok: boolean;
  why?: string;
  turn?: string;
  seen?: string[];
  rect?: number[];
  count?: number;
  changed?: boolean;
  wider?: number;
  cut?: string[];
  view?: string;
  memo?: unknown;
  diff?: boolean;
};
export default act;
