/** The agent's action inside the page (`act.js`), as the screen's checks call it. */
declare const act: (request: Record<string, unknown>) => { ok: boolean; why?: string; turn?: string; seen?: string[]; changed?: boolean };
export default act;
