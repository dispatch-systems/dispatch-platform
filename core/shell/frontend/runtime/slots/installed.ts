import type { DspView } from '../../../../accounts/api/index.js';
import type { Feature } from '../../../../tenancy/api/index.js';
import { featureCatalog, hasFeature } from '../features.js';

// The manifests installed, and what every slot reads them with. Each slot is a module beside
// this one: it declares what an owner puts in it, adds that to `FrontendFeature` by declaration
// merging, and reads it from here. Only app/frontend lists the manifests; it installs them
// before the first render, so no host imports a feature.

/** An owner's frontend: what it puts in each slot. Each slot's module adds its own field. */
export interface FrontendFeature {
  /** The owner's directory name. */
  name: string;
}

let installed: readonly FrontendFeature[] = [];
let publicFeatures: readonly FrontendFeature[] = [];
let customNames = new Set<string>();
/** Called once by the app, before the first render, with every owner's manifest. */
export function installFeatures(features: readonly FrontendFeature[]) {
  publicFeatures = features;
  customNames = new Set();
  installed = features;
}
/** Installs only manifests loaded for the DSP whose view the app just admitted. */
export function installCustomFeatures(
  features: readonly FrontendFeature[],
  order?: (features: readonly FrontendFeature[]) => readonly FrontendFeature[],
) {
  customNames = new Set(features.map((feature) => feature.name));
  installed = order ? order([...publicFeatures, ...features]) : publicFeatures;
}
/** Every owner's manifest, in the order the owners are listed. */
export const manifests = () => installed;

/**
 * What the owners put in a slot that the view has, in the order the owners are listed: all of
 * core's, a feature's or a connection's while the DSP has it, and a part's while it has that.
 */
export function present<T extends { part?: Feature }>(
  view: DspView | undefined,
  slot: (feature: FrontendFeature) => readonly T[] | undefined,
) {
  return presentBy(view, slot).map(({ each }) => each);
}
/** What `present` answers, each with the owner that put it there. */
export function presentBy<T extends { part?: Feature }>(
  view: DspView | undefined,
  slot: (feature: FrontendFeature) => readonly T[] | undefined,
) {
  return installed.flatMap((owner) =>
    (customNames.has(owner.name) &&
      !view?.custom?.frontends.some((feature) => feature.name === owner.name)) ||
    (featureCatalog.some((f) => f.id === owner.name) && !hasFeature(view, owner.name as Feature))
      ? []
      : (slot(owner) ?? [])
          .filter((each) => !each.part || hasFeature(view, each.part))
          .map((each) => ({ owner, each })),
  );
}
/** The first answer an owner gives, in the order the owners are listed. */
export function first<T>(read: (feature: FrontendFeature) => T | undefined) {
  for (const feature of installed) {
    const answer = read(feature);
    if (answer !== undefined) return answer;
  }
  return undefined;
}
