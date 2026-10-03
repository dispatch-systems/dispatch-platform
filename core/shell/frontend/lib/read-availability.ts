import { useSyncExternalStore } from 'react';

const listeners = new Set<() => void>();
const notify = () => {
  for (const listener of listeners) listener();
};
function subscribe(listener: () => void) {
  if (!listeners.size) {
    window.addEventListener('online', notify);
    window.addEventListener('offline', notify);
    document.addEventListener('visibilitychange', notify);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (!listeners.size) {
      window.removeEventListener('online', notify);
      window.removeEventListener('offline', notify);
      document.removeEventListener('visibilitychange', notify);
    }
  };
}
const snapshot = () => (!navigator.onLine ? 'offline' : document.hidden ? 'hidden' : 'ready');

export const useReadAvailability = () => useSyncExternalStore(subscribe, snapshot);
