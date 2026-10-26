import { Component, useEffect, useState, type ComponentType, type ReactNode } from 'react';
import { watchers, type Watcher, type Watching } from '../runtime/slots.js';

// Each watcher runs apart from the page and from every other, and its failures are logged by its
// ID. One whose code fails to load is tried again as the browser comes back online or the page is
// shown again, once at a time, and with the next DSP, which mounts every watcher anew; one that
// fails as it runs stops, drawing nothing, until then.

const loaded = new WeakMap<Watcher, ComponentType<Watching>>();

/** A watcher, once its code has loaded. */
function Running({ watcher, access }: { watcher: Watcher; access: Watching }) {
  const [Loaded, setLoaded] = useState(() => loaded.get(watcher));
  useEffect(() => {
    if (Loaded) return;
    let live = true;
    let loading = false;
    const load = () => {
      if (loading) return;
      loading = true;
      watcher.load().then(
        (component) => {
          loaded.set(watcher, component);
          if (live) setLoaded(() => component);
        },
        (error: unknown) => {
          loading = false;
          console.error(`The watcher ${watcher.id} failed to load`, error);
        },
      );
    };
    const shown = () => {
      if (document.visibilityState === 'visible') load();
    };
    load();
    window.addEventListener('online', load);
    document.addEventListener('visibilitychange', shown);
    return () => {
      live = false;
      window.removeEventListener('online', load);
      document.removeEventListener('visibilitychange', shown);
    };
  }, [watcher, Loaded]);
  return Loaded ? <Loaded {...access} /> : null;
}

/** Leaves nothing of a watcher that failed as it ran, logging it by `id`. */
class Apart extends Component<{ id: string; children: ReactNode }, { failed: boolean }> {
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
    <Apart key={watcher.id} id={watcher.id}>
      <Running watcher={watcher} access={access} />
    </Apart>
  ));
}
