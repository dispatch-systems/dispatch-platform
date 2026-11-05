import type { ComponentType } from 'react';
import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';
import type { Access } from './pages.js';

// The watchers slot: what an owner keeps running on every DSP page for the person, as something
// that alerts them wherever they are in the app, drawing nothing or a banner across the page. The
// shell mounts each the DSP has and the person passes at the top of the page, whatever page is
// open, loading its code as it first does while the browser is online. Each runs apart
// (`shell/Watchers.tsx`): one that fails stops alone.

/** Whom a watcher runs for: the person, at the DSP open. */
export type Watching = Access & { view: DspView };

/** What an owner keeps running on every DSP page, drawing nothing or a banner. */
export type Watcher = {
  /** Its id, `<owner>.<what>`. */
  id: string;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
  /** Whether it runs for the person; omitted, for everyone at a DSP with it. */
  visible?: (access: Watching) => boolean;
  /** Its code, loaded as the shell first mounts it, once the browser is online. */
  load: () => Promise<ComponentType<Watching>>;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** What it keeps running on every DSP page, drawing nothing or a banner. */
    watchers?: readonly Watcher[];
  }
}

/** The watchers the DSP has that run for the person, in the order the owners are listed. */
export const watchers = (access: Watching) =>
  present(access.view, (feature) => feature.watchers).filter(
    (each) => !each.visible || each.visible(access),
  );
