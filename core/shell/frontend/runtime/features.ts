import {
  featureCatalog as generatedFeatureCatalog,
  schedulesFeature as generatedSchedulesFeature,
} from '../../../tenancy/api/generated/access-catalog.js';
import type {
  ConnectionFeature,
  Feature,
  PageFeature,
  TabFeature,
} from '../../../tenancy/api/index.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';

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
/** The tabs of `page`, in catalog order. */
export const tabsOf = (page: string) =>
  featureCatalog.filter((f): f is TabEntry => f.kind === 'tab' && f.page === page);
/** The connections among `features`, in catalog order. */
export const connectionFeatures = (features: readonly string[]) =>
  featureCatalog.filter(
    (f): f is ConnectionEntry => f.kind === 'connection' && features.includes(f.id),
  );
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
