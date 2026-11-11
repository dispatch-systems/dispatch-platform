import { useLayoutEffect, useRef } from 'react';

/** Restore only after the list's rows exist; records and drafts never enter history state. */
export function useListScroll(ready: boolean) {
  const restored = useRef(false);
  useLayoutEffect(() => {
    if (!ready || restored.current) return;
    restored.current = true;
    const top = (history.state as { conductScroll?: number } | null)?.conductScroll;
    if (typeof top === 'number') window.scrollTo(0, top);
  }, [ready]);
}
