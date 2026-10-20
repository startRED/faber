// The Context meter in the topbar (R221): how full the conversation's own agent's context is, and what fills it (task 190).
import { createSignal } from 'solid-js';
import type { ContextFill, ContextPart } from './api.ts';

/** From this share of the window the meter turns amber: the CLI compacts by itself not far above it (C002: never alarm sooner). */
export const NEAR = 0.85;

/** The share of the window held, 0 to 1. A window the CLI did not size counts as full, never as empty. */
export const share = (fill: ContextFill): number => (fill.window > 0 ? Math.min(1, Math.max(0, fill.used / fill.window)) : 1);

/** The whole percent the meter says. */
export const percentOf = (fill: ContextFill): number => Math.round(share(fill) * 100);

/** Near the point where the CLI would compact by itself. */
export const near = (fill: ContextFill): boolean => share(fill) >= NEAR;

const [parts, setParts] = createSignal<{ id: string; parts: ContextPart[] }>();

/** What fills a conversation's context, part by part, as its CLI last counted it (only Claude Code does, on request). */
export const partsOf = (id: string): ContextPart[] | undefined => (parts()?.id === id ? parts()!.parts : undefined);

/** The CLI answered the meter's question. Only the newest answer is kept: the meter shows one conversation at a time. */
export const heardParts = (id: string, list: ContextPart[]) => setParts({ id, parts: list });
