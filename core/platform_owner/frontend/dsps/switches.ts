import type { Feature } from '../../../tenancy/api/index.js';
import { capabilityLabels } from '../../../tenancy/api/generated/capabilities.js';
import {
  featureCatalog,
  tabsOf,
  type FeatureEntry,
} from '../../../shell/frontend/runtime/features.js';

// How the DSPs page switches a feature on or off, kept with the page rather than in the shell's
// runtime, which every page loads.

/**
 * A capability as a page that needs it names it, as the first connection listed that provides it
 * names it. Every capability a page requires has a label; the catalog test checks.
 */
export const capabilityLabel = (capability: string): string =>
  Object.hasOwn(capabilityLabels, capability)
    ? capabilityLabels[capability as keyof typeof capabilityLabels]
    : capability;

/**
 * What switching `id` would change, mirroring `set_feature` in the backend: enabling a page
 * or a part enables the one provider of each capability it, or a page's parts on with it,
 * lacks, and a page's tabs when none is on; enabling a provider switches off another of the
 * same capability; disabling a provider disables the pages and parts left without one; a page
 * left with none of its tabs on is disabled. A page switched off keeps its parts' switches.
 * `enabled` is what is switched on, parts of a page that is off included. Undefined when a
 * page or part needs a capability with several providers and none is on: the backend refuses
 * that switch until one is chosen. A mandatory feature or part has no switch: nothing changes.
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
  // On, and with its page on for a part: what exists.
  const live = (f: FeatureEntry) => current.has(f.id) && (f.kind !== 'sub' || current.has(f.page));
  const feature = featureCatalog.find((f) => f.id === id)!;
  if (feature.mandatory) return changed;
  if (on) {
    for (const capability of feature.provides ?? [])
      for (const other of featureCatalog)
        if (provides(other, capability) && other.id !== feature.id) flip(other, false);
    flip(feature, true);
    const own = tabsOf(feature.id);
    if (!own.some((t) => current.has(t.id))) for (const t of own) flip(t, true);
    const needing = featureCatalog.filter(
      (f) => (f.id === feature.id || (f.kind === 'sub' && f.page === feature.id)) && live(f),
    );
    for (const f of needing)
      for (const capability of f.requires) {
        if (provided(capability)) continue;
        const providers = featureCatalog.filter((p) => provides(p, capability));
        if (providers.length !== 1) return undefined;
        flip(providers[0]!, true);
      }
  } else {
    flip(feature, false);
    // Then whatever is left short, until nothing more is.
    let before: number;
    do {
      before = changed.length;
      for (const page of featureCatalog) {
        if (page.kind !== 'page' || !live(page)) continue;
        const own = tabsOf(page.id);
        if (own.length && !own.some((t) => current.has(t.id))) flip(page, false);
      }
      for (const f of featureCatalog)
        if (f.kind !== 'connection' && live(f) && !f.requires.every(provided)) flip(f, false);
    } while (changed.length !== before);
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
  feature.kind === 'sub' && feature.tab ? `${feature.label} tab` : feature.label;
