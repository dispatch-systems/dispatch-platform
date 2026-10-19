import { createContext, use, useEffect, type RefObject } from 'react';
import type { Leave } from '../../../core/collection/frontend/schedules.js';

// An editor open on a page of Settings, as a schedule's, keeps its unsaved edits until they
// are saved or discarded: the page asks each before it moves to another of its pages.

/** The editors open on the page, each asked through its ref. */
export type Guards = Set<RefObject<Leave | null>>;
export const LeaveGuards = createContext<Guards | null>(null);

/** Asks each open editor in turn, and moves on once every one has let it. */
export function leaving(guards: Guards, then: () => void) {
  const asks = [...guards].flatMap((guard) => guard.current ?? []);
  const next = (index: number): void =>
    index < asks.length ? asks[index]!(() => next(index + 1)) : then();
  next(0);
}

/** Has the page ask `guard` before it moves on, while the component calling it is drawn. */
export function useLeaveGuard(guard: RefObject<Leave | null>) {
  const guards = use(LeaveGuards);
  useEffect(() => {
    guards?.add(guard);
    return () => void guards?.delete(guard);
  }, [guards, guard]);
}
