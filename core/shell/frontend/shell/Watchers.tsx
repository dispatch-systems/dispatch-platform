import { useEffect, useState, type ComponentType } from 'react';
import { watchers, type Watcher, type Watching } from '../runtime/slots.js';

const loaded = new WeakMap<Watcher, ComponentType<Watching>>();

/** A watcher, once its code has loaded; one that fails to load is left out. */
function Running({ watcher, access }: { watcher: Watcher; access: Watching }) {
  const [Loaded, setLoaded] = useState(() => loaded.get(watcher));
  useEffect(() => {
    if (Loaded) return;
    let live = true;
    watcher.load().then(
      (component) => {
        loaded.set(watcher, component);
        if (live) setLoaded(() => component);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [watcher, Loaded]);
  return Loaded ? <Loaded {...access} /> : null;
}

/** What the DSP's features keep running for the person on every page, each drawing nothing. */
export function Watchers(access: Watching) {
  return watchers(access).map((watcher) => (
    <Running key={watcher.id} watcher={watcher} access={access} />
  ));
}
