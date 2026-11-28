import type { Act } from '../../kit';

export interface Scene {
  draft: string;
  sent: boolean;
  phase: number;
  result: boolean;
  preview: boolean;
}
export const duration: number;
export const initial: Scene;
export const poster: Scene;
export function perform(act: Act<Scene>, request: string): Promise<void>;
