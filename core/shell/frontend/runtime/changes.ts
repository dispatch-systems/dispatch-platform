import { useRef, useState } from 'react';

/** One change made here: how it changes the value as read, and the reading it saved over. */
type Change<T> = { id: number; scope: string; apply: (value: T) => T; saved?: { over: unknown } };

/**
 * Changes that show the moment they're made, over `read`, the value as last read, while they save
 * one after another in the order made. Each shows over the reading and every change made before
 * it, so an earlier one's answer never undoes a later one. One that fails is dropped, leaving the
 * reading and the others; one that saved shows until `read` is read again after it, or, made
 * with `until: 'saved'`, goes once saved, its save having put its answer into `read` itself.
 * They belong to `scope`, as the DSP they were made at: in another, none of them shows. Once the
 * last of them is saved, `settled` is called, as to read the value again.
 */
export function useChanges<T>(read: T, scope = '', settled?: () => void) {
  const [changes, setChanges] = useState<Change<T>[]>([]);
  const queue = useRef(Promise.resolve());
  const next = useRef(0);
  const latest = useRef<unknown>(read);
  latest.current = read;
  const unsaved = useRef(0);
  const live = changes.filter(
    (each) => each.scope === scope && (!each.saved || each.saved.over === read),
  );
  const shown = live.reduce((value, each) => each.apply(value), read);
  /** Shows `apply` at once and saves it after those before it: `save` answers whether it did. */
  const change = (
    apply: (value: T) => T,
    save: () => Promise<boolean>,
    { until = 'read' }: { until?: 'read' | 'saved' } = {},
  ) => {
    next.current += 1;
    unsaved.current += 1;
    const id = next.current;
    setChanges((before) => [
      ...before.filter((each) => !each.saved || each.saved.over === latest.current),
      { id, scope, apply },
    ]);
    queue.current = queue.current.then(async () => {
      const saved = await save();
      const over = latest.current;
      unsaved.current -= 1;
      setChanges((before) =>
        saved && until === 'read'
          ? before.map((each) => (each.id === id ? { ...each, saved: { over } } : each))
          : before.filter((each) => each.id !== id),
      );
      if (!unsaved.current) settled?.();
    });
  };
  return { shown, change, saving: live.some((each) => !each.saved) };
}
