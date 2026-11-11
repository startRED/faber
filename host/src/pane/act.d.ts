/** The agent's action inside the page (`act.js`), as the screen's checks call it. */
declare const act: (request: Record<string, unknown>) => {
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
