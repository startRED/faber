export const duration = 14000;
export const initial = { draft: '', sent: false, phase: 0, result: false, preview: false };
export const poster = { draft: '', sent: true, phase: 4, result: true, preview: true };

// A finite story holds its result. Replay is the only reset a viewer sees.
export async function perform(a, request) {
  a.mark('request');
  await a.wait(600);
  await a.move('request', { duration: 500 });
  await a.click('request');
  await a.type('draft', request, { interval: Math.min(28, 900 / request.length) });
  await a.wait(300);
  await a.click('send');
  await a.set({ sent: true, draft: '', phase: 1 });
  a.mark('work');
  await a.wait(1400);
  await a.set({ phase: 2 });
  await a.wait(1500);
  await a.set({ phase: 3 });
  await a.wait(1700);
  a.mark('result');
  await a.set({ phase: 4, result: true });
  await a.wait(900);
  await a.click({ at: 'preview', x: 0.86, y: 0.78 });
  await a.set({ preview: true });
  a.mark('review');
  await a.wait(4700);
}
