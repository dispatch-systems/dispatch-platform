import { featureCatalog as generatedFeatureCatalog } from '../../../tenancy/api/generated/access-catalog.js';
import type {
  ConnectionFeature,
  Feature,
  PageFeature,
  SubFeature,
} from '../../../tenancy/api/index.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';

type Entry<Kind, Id> = {
  id: Id;
  label: string;
  kind: Kind;
  /** The permissions the feature owns; without it, nobody in the DSP holds them. */
  permissions: readonly Permission[];
  /** What a page or a part needs one enabled connection of. */
  requires: readonly string[];
};
/** `mandatory`: every DSP has it, with no switch to turn it off. */
export type PageEntry = Entry<'page', PageFeature> & { mandatory?: boolean; provides?: undefined };
/**
 * A part of `page`, switched on its own: one of its tabs, or another part. It exists only
 * while its page is on too, with the permissions it owns.
 */
export type SubEntry = Entry<'sub', SubFeature> & {
  page: PageFeature;
  tab: boolean;
  mandatory?: boolean;
  provides?: undefined;
};
/** `provides` is what the connection supplies, one capability or several. */
export type ConnectionEntry = Entry<'connection', ConnectionFeature> & {
  provides: readonly string[];
  mandatory?: undefined;
};
export type FeatureEntry = PageEntry | SubEntry | ConnectionEntry;
/** The backend-owned catalog, generated alongside the wire contracts. */
export const featureCatalog: readonly FeatureEntry[] = generatedFeatureCatalog;
/** The parts of `page` switched on their own, its tabs and others, in catalog order. */
export const subsOf = (page: string) =>
  featureCatalog.filter((f): f is SubEntry => f.kind === 'sub' && f.page === page);
/** The tabs of `page`, in catalog order. */
export const tabsOf = (page: string) => subsOf(page).filter((sub) => sub.tab);
/** The connections among `features`, in catalog order. */
export const connectionFeatures = (features: readonly string[]) =>
  featureCatalog.filter(
    (f): f is ConnectionEntry => f.kind === 'connection' && features.includes(f.id),
  );
/** A feature's name on its own, as the audit log shows it: a part of a page with its page. */
export function featureLabel(id: string): string {
  const feature = featureCatalog.find((f) => f.id === id);
  if (feature?.kind !== 'sub') return feature?.label ?? id;
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
