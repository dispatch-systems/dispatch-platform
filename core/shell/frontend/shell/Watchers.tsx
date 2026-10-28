import { Component, useEffect, useState, type ComponentType, type ReactNode } from 'react';
import { watchers, type Watcher, type Watching } from '../runtime/slots.js';

// Each watcher runs apart from the page and from every other, and its failures are logged by its
// ID. Its code is asked for only while the browser is online, waiting for it to come back online
// otherwise: the browser keeps a failed download failed until the page loads again, so code that
// fails to load stops the watcher until then. One that fails as it runs stops, drawing nothing,
// until the next DSP, which mounts every watcher anew.

const loaded = new WeakMap<Watcher, ComponentType<Watching>>();

/** A watcher, once its code has loaded. */
function Running({ watcher, access }: { watcher: Watcher; access: Watching }) {
  const [Loaded, setLoaded] = useState(() => loaded.get(watcher));
  useEffect(() => {
    if (Loaded) return;
    let live = true;
    const load = () => {
      watcher.load().then(
        (component) => {
          loaded.set(watcher, component);
          if (live) setLoaded(() => component);
        },
        (error: unknown) => console.error(`The watcher ${watcher.id} failed to load`, error),
      );
    };
    if (navigator.onLine) load();
    else window.addEventListener('online', load, { once: true });
    return () => {
      live = false;
      window.removeEventListener('online', load);
    };
  }, [watcher, Loaded]);
  return Loaded ? <Loaded {...access} /> : null;
}

/** Leaves nothing of a watcher that failed as it ran, logging it by `id`. */
class WatcherBoundary extends Component<{ id: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: unknown) {
    console.error(`The watcher ${this.props.id} failed`, error);
  }
  render() {
    return this.state.failed ? null : this.props.children;
  }
}

/** What the DSP's features keep running for the person on every page, each drawing nothing. */
export function Watchers(access: Watching) {
  return watchers(access).map((watcher) => (
    <WatcherBoundary key={watcher.id} id={watcher.id}>
      <Running watcher={watcher} access={access} />
    </WatcherBoundary>
  ));
}
