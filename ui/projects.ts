import type { DecisionKind, TaskState, TaskView } from './api.ts';

export type PresenceChoice = 'continueWithout' | 'findTools';

const GOAL_KEY_PREFIX = 'faber.project.goal.';

export function recallGoal(projectId: string): string {
  try {
    return localStorage.getItem(GOAL_KEY_PREFIX + projectId) ?? '';
  } catch {
    return '';
  }
}

export function saveGoal(projectId: string, goal: string): void {
  try {
    if (goal.trim()) {
      localStorage.setItem(GOAL_KEY_PREFIX + projectId, goal);
    } else {
      localStorage.removeItem(GOAL_KEY_PREFIX + projectId);
    }
  } catch {
    // Ignore storage quota or access errors
  }
}

export function recallMission(projectId: string): string | undefined {
  try { return localStorage.getItem('faber.project.mission.' + projectId) ?? undefined; } catch { return undefined; }
}

export function saveMission(projectId: string, missionId: string): void {
  try { localStorage.setItem('faber.project.mission.' + projectId, missionId); } catch { /* The journal remains durable. */ }
}

/** Formats own cost in millionths of an API-equivalent dollar.
 * None/null/undefined is unknown, NEVER zero. */
export function formatCost(ownMicros: number | null | undefined): { isUnknown: boolean; display: string } {
  if (ownMicros === null || ownMicros === undefined) {
    return { isUnknown: true, display: 'projects.cost.unknown' };
  }
  const dollars = ownMicros / 1_000_000;
  return { isUnknown: false, display: `$${dollars.toFixed(2)}` };
}

/** Formats the live cap in millionths of an API-equivalent dollar. */
export function formatCap(capMicros: number | null | undefined): string {
  if (capMicros === null || capMicros === undefined) return 'projects.cost.unknown';
  return `$${(capMicros / 1_000_000).toFixed(2)}`;
}

/** Formats a fraction (0.0 to 1.0) as a percentage integer. */
export function formatPercent(val: number | null | undefined): string {
  if (val === null || val === undefined) return '—';
  return `${Math.round(val * 100)}%`;
}

/** Honest receipt and assumption indicator for tasks.
 * If there is no cryptographic/toolchain proof receipt, return 'projects.receipt.unverified'
 * or 'projects.receipt.notRun' — never claim verified when unverified! */
export function taskReceiptStatus(task: TaskView): { textKey: string; isHonest: boolean; isUnverified: boolean } {
  if (task.state === 'done') {
    // In selfdev MVP, unless explicit verified receipt is supplied, show unverified honestly
    return { textKey: 'projects.receipt.unverified', isHonest: true, isUnverified: true };
  }
  return { textKey: 'projects.receipt.notRun', isHonest: true, isUnverified: true };
}

/** Filter for decisions that need human input (R77, R92). Technical choices are logged as AI decisions. */
export function isHumanDecision(kind: DecisionKind): boolean {
  return kind !== 'technical';
}

export function taskStateKey(state: TaskState): string {
  return `projects.task.${state}`;
}
