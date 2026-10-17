import { keep } from './unsent.ts';

/** The project left Faber: what an earlier start card kept for it (the goal being typed, the mission shown; no screen
 * writes them any more) and the unsent checks go too. */
export function forgetProject(projectId: string): void {
  try {
    localStorage.removeItem('faber.project.goal.' + projectId);
    localStorage.removeItem('faber.project.mission.' + projectId);
  } catch { /* Storage can be unavailable. */ }
  keep(`proof.${projectId}`, '');
}
