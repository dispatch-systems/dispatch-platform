import { useEffect, useSyncExternalStore } from 'react';

// The parts of the open page asking it to fill the window, as one of its tabs may while it is
// open: the app fills the page, as it does one that declares `fills`, while any of them asks.

const asking = new Set<object>();
const listeners = new Set<() => void>();
const notify = () => {
  for (const listener of listeners) listener();
};
function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Fills the open page to the window while this is drawn and the window matches `media`. */
export function useFillsWindow(media = 'all') {
  useEffect(() => {
    const query = matchMedia(media);
    const part = {};
    const update = () => {
      if (query.matches) asking.add(part);
      else asking.delete(part);
      notify();
    };
    update();
    query.addEventListener('change', update);
    return () => {
      query.removeEventListener('change', update);
      asking.delete(part);
      notify();
    };
  }, [media]);
}

/** Whether a part of the open page asks it to fill the window. */
export const useFillRequested = () => useSyncExternalStore(subscribe, () => asking.size > 0);
