import {
  featureCatalog as generatedFeatureCatalog,
  schedulesFeature as generatedSchedulesFeature,
} from '../../../../shared/contracts/generated/access-catalog.js';
import type {
  ConnectionFeature,
  DspView,
  Feature,
  PageFeature,
  Permission,
  TabFeature,
} from '../../../../shared/contracts/index.js';

type Entry<Kind, Id> = {
  id: Id;
  label: string;
  kind: Kind;
  /** The permissions the feature owns; without it, nobody in the DSP holds them. */
  permissions: readonly Permission[];
  /** What a page needs one enabled connection of. */
  requires: readonly string[];
};
export type PageEntry = Entry<'page', PageFeature> & { provides?: undefined };
/** A tab of `page`, switched on its own; it exists only while its page is on too. */
export type TabEntry = Entry<'tab', TabFeature> & { page: PageFeature; provides?: undefined };
/** `provides` is what the connection supplies, one capability or several. */
export type ConnectionEntry = Entry<'connection', ConnectionFeature> & {
  provides: readonly string[];
};
export type FeatureEntry = PageEntry | TabEntry | ConnectionEntry;
/** The backend-owned catalog, generated alongside the wire contracts. */
export const featureCatalog: readonly FeatureEntry[] = generatedFeatureCatalog;
/** The page whose schedules, collections and jobs run. */
export const schedulesFeature: PageFeature = generatedSchedulesFeature;
/** Every capability a page requires has a label; the catalog test checks. */
const capabilities: Record<string, string> = {
  timecards: 'a timecard source',
  meal_breaks: 'a meal-break source',
  routes: 'a route source',
  dvic: 'a DVIC source',
  scorecard: 'a scorecard source',
};
export const capabilityLabel = (capability: string) => capabilities[capability] ?? capability;
/** The tabs of `page`, in catalog order. */
export const tabsOf = (page: string) =>
  featureCatalog.filter((f): f is TabEntry => f.kind === 'tab' && f.page === page);
/** The connections among `features`, in catalog order. */
export const connectionFeatures = (features: readonly string[]) =>
  featureCatalog.filter(
    (f): f is ConnectionEntry => f.kind === 'connection' && features.includes(f.id),
  );
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
/** A feature's name on its own, as the audit log shows it: a tab with its page. */
export function featureLabel(id: string): string {
  const feature = featureCatalog.find((f) => f.id === id);
  if (feature?.kind !== 'tab') return feature?.label ?? id;
  return `${featureLabel(feature.page)} · ${feature.label}`;
}
export const hasFeature = (view: DspView | undefined, id: Feature) =>
  Boolean(view?.features.includes(id));
/** Whether a permission exists with these features, mirroring `grants` in the backend. */
export function grants(features: readonly string[], permission: Permission) {
  if (permission === 'connections.manage')
    return featureCatalog.some((f) => f.kind === 'connection' && features.includes(f.id));
  const owner = featureCatalog.find((f) => f.permissions.includes(permission));
  return !owner || features.includes(owner.id);
}
