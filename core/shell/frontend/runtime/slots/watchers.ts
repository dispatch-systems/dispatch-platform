import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { present } from './installed.js';

// The watchers slot: What it keeps running on every DSP page for the member, drawing nothing.
//
// What's here is a start: what an owner puts in the slot, its field on `FrontendFeature`, and
// what a host reads it with.
// TODO(scaffold): give it what the host needs, and have the host draw it.

/** One of an owner's watchers. */
export type Watcher = {
  /** Its own id among the owner's. */
  id: string;
  label: string;
  /** Where it sits among every owner's, lowest first. */
  order: number;
  /** Who sees it; omitted means everyone. */
  visible?: (view?: DspView) => boolean;
  /** The part of its owner's feature it belongs to: it goes with the part. */
  part?: Feature;
};

declare module './installed.js' {
  interface FrontendFeature {
    /** Its watchers. */
    watchers?: readonly Watcher[];
  }
}

/** The watchers the view has and sees, in their order. */
export const watchers = (view: DspView) =>
  present(view, (feature) => feature.watchers)
    .filter((each) => !each.visible || each.visible(view))
    .sort((a, b) => a.order - b.order);
