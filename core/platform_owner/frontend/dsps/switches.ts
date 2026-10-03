import type { Feature } from '../../../../shared/contracts/tenancy.js';
import {
  featureCatalog,
  tabsOf,
  type FeatureEntry,
} from '../../../shell/frontend/runtime/features.js';

// How the DSPs page switches a feature on or off, kept with the page rather than in the shell's
// runtime, which every page loads.

/**
 * What switching `id` would change, mirroring `set_feature` in the backend: enabling a
 * page enables the one provider of each capability it lacks, and its tabs when none is on;
 * enabling a provider switches off another of the same capability; disabling a provider
 * disables the pages left without one; disabling a page's last tab disables the page.
 * `enabled` is what is switched on, tabs of a page that is off included. Undefined when a
 * page needs a capability with several providers and none is on: the backend refuses that
 * switch until one is chosen.
 */
export function previewSwitch(enabled: readonly string[], id: Feature, on: boolean) {
  const current = new Set(enabled);
  const changed: { feature: Feature; enabled: boolean }[] = [];
  const flip = (feature: FeatureEntry, to: boolean) => {
    if (current.has(feature.id) === to) return;
    if (to) current.add(feature.id);
    else current.delete(feature.id);
    changed.push({ feature: feature.id, enabled: to });
  };
  const provides = (f: FeatureEntry, capability: string) =>
    f.provides?.includes(capability) ?? false;
  const provided = (capability: string) =>
    featureCatalog.some((f) => provides(f, capability) && current.has(f.id));
  const feature = featureCatalog.find((f) => f.id === id)!;
  if (on) {
    for (const capability of feature.provides ?? [])
      for (const other of featureCatalog)
        if (provides(other, capability) && other.id !== feature.id) flip(other, false);
    for (const capability of feature.requires) {
      if (provided(capability)) continue;
      const providers = featureCatalog.filter((f) => provides(f, capability));
      if (providers.length !== 1) return undefined;
      flip(providers[0]!, true);
    }
    flip(feature, true);
    const own = tabsOf(feature.id);
    if (!own.some((t) => current.has(t.id))) for (const t of own) flip(t, true);
  } else {
    flip(feature, false);
    if (
      feature.kind === 'tab' &&
      current.has(feature.page) &&
      !tabsOf(feature.page).some((t) => current.has(t.id))
    )
      flip(
        featureCatalog.find((f) => f.id === feature.page)!,
        false,
      );
    for (const page of featureCatalog)
      if (page.kind === 'page' && !page.requires.every(provided)) flip(page, false);
  }
  return changed;
}
/**
 * The switches of `changes` worth asking about before switching `feature`: every one but
 * the feature's own, and a page's tabs coming on with it.
 */
export const sideEffects = (
  feature: FeatureEntry,
  changes: readonly { feature: Feature; enabled: boolean }[],
) =>
  changes.filter(
    (change) =>
      change.feature !== feature.id &&
      !(change.enabled && tabsOf(feature.id).some((tab) => tab.id === change.feature)),
  );
/** A switch's name in a question or a notice: a tab says it is one. */
export const switchLabel = (feature: FeatureEntry) =>
  feature.kind === 'tab' ? `${feature.label} tab` : feature.label;
