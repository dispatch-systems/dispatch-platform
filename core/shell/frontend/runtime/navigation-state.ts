import {
  createContext,
  useContext,
  useEffect,
  useState,
  type Dispatch,
  type SetStateAction,
} from 'react';

const storageKey = 'dispatch:browser-update:v1';
const values = new Map<string, unknown>();
const navigationValues = new Map<string, unknown>();
export function clearNavigationState() {
  navigationValues.clear();
  values.clear();
  restored = {};
}
let restored: Record<string, unknown> = {};
let position: { x: number; y: number; hash: string } | undefined;
try {
  const saved = JSON.parse(sessionStorage.getItem(storageKey) || 'null') as {
    x: number;
    y: number;
    hash: string;
    at: number;
    values: Record<string, unknown>;
  } | null;
  sessionStorage.removeItem(storageKey);
  if (saved && Date.now() - saved.at < 60_000 && saved.hash === location.hash) {
    restored = saved.values;
    position = saved;
  }
} catch {
  // Restricted storage must not break the application.
}

// A pending navigation keeps the previous page mounted. Its state still belongs to the
// committed page, even though the browser's hash already points to the destination.
export const NavigationStateContext = createContext<string | undefined>(undefined);
export function readUpdateState<T>(name: string, initial: T, hash = location.hash): T {
  const key = `${hash.split('?')[0]}:${name}`;
  const saved = restored[key] ?? navigationValues.get(key);
  return saved !== undefined ? (saved as T) : initial;
}

// Only explicitly opted-in, non-sensitive navigation state is retained.
export function useUpdateState<T>(
  name: string,
  initial: T | (() => T),
): [T, Dispatch<SetStateAction<T>>] {
  const hash = useContext(NavigationStateContext) ?? location.hash;
  const key = `${hash.split('?')[0]}:${name}`;
  const [value, setValue] = useState<T>(() => {
    const saved = restored[key] ?? navigationValues.get(key);
    return saved !== undefined
      ? (saved as T)
      : typeof initial === 'function'
        ? (initial as () => T)()
        : initial;
  });
  useEffect(() => {
    // A suspended render may retry its initializer; consume restoration only after commit.
    delete restored[key];
    values.set(key, value);
    navigationValues.delete(key);
    navigationValues.set(key, value);
    while (navigationValues.size > 200)
      navigationValues.delete(navigationValues.keys().next().value!);
    return () => {
      values.delete(key);
    };
  }, [key, value]);
  return [value, setValue];
}

/** Captures the committed page's state and scroll immediately before an update reload. */
export function saveNavigationState() {
  sessionStorage.setItem(
    storageKey,
    JSON.stringify({
      at: Date.now(),
      hash: location.hash,
      x: scrollX,
      y: scrollY,
      values: Object.fromEntries(values),
    }),
  );
}

/** Hands restored scroll to the first enabled update effect, once per page load. */
export function takeRestoredScrollPosition() {
  const saved = position;
  position = undefined;
  return saved;
}
