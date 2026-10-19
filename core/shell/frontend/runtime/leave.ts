import { createContext, use, useEffect, type RefObject } from 'react';

// Unsaved edits on a page, as a schedule's in its editor, are saved or discarded before the page
// moves on, never lost without asking: the page asks each editor open on it in turn, whether it
// moves by its own controls or the address moves, as Back, the browser's Back or Forward, a link
// followed or a hash typed move it, which it hears through `holdAddress` (navigation.ts).

/** Runs `then` at once, or after the member chooses Save or Discard for unsaved edits. */
export type Leave = (then: () => void) => void;
/** The editors open on a page, each asked through its ref. */
export type Guards = Set<RefObject<Leave | null>>;
/** The editors open on the page drawn inside it, for a page that asks them before it moves on. */
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
