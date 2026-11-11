import {
  permissionLabels as generatedPermissionLabels,
  permissionGroups as generatedPermissionGroups,
  impliedPermissions as generatedImpliedPermissions,
  permissionParents as generatedPermissionParents,
  permissions as publicPermissions,
} from '../../../tenancy/api/generated/access-catalog.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';
import { featureCatalog, featureLabel, grants, type PageEntry } from './features.js';

export let permissionLabels: Partial<Record<Permission, string>> = generatedPermissionLabels;
/** The backend catalog owns the sections and each permission belongs to exactly one. */
let permissionGroups: readonly (readonly [string, readonly Permission[]])[] =
  generatedPermissionGroups;
/** The sections the role sheet shows a DSP: those whose permissions exist in it. */
export const visiblePermissionGroups = (view: DspView) =>
  permissionGroups.filter(([, items]) => items.some((p) => grants(view.features, p)));
/** Granting the key includes each of its values, however many steps away, from the backend's rules. */
export let impliedPermissions: Partial<Record<Permission, readonly Permission[]>> =
  generatedImpliedPermissions;
/** The permission each finer one sits under on the role sheet, which it includes too. */
export let permissionParents: Partial<Record<Permission, Permission>> = generatedPermissionParents;

/** The current DSP's role and scheduling choices, with no other owner's declarations. */
export let allPermissions: readonly Permission[] = publicPermissions;
export function installCustomPermissions(custom?: DspView['custom']) {
  const permissions = custom?.permissions ?? [];
  permissionLabels = {
    ...generatedPermissionLabels,
    ...Object.fromEntries(permissions.map((p) => [p.id, p.label])),
  };
  impliedPermissions = {
    ...generatedImpliedPermissions,
    ...Object.fromEntries(permissions.map((p) => [p.id, p.implied])),
  };
  permissionParents = {
    ...generatedPermissionParents,
    ...Object.fromEntries(permissions.flatMap((p) => (p.under ? [[p.id, p.under]] : []))),
  };
  const owned = new Set(featureCatalog.flatMap((entry) => entry.permissions));
  const pageGroups = featureCatalog
    .filter((entry): entry is PageEntry => entry.kind === 'page')
    .sort((a, b) => (a.place ?? 65535) - (b.place ?? 65535) || a.id.localeCompare(b.id))
    .flatMap((page) => [
      page,
      ...featureCatalog.filter((entry) => entry.kind === 'sub' && entry.page === page.id),
    ])
    .filter((entry) => entry.permissions.length)
    .map((entry): [string, readonly Permission[]] => [featureLabel(entry.id), entry.permissions]);
  permissionGroups = [
    ...pageGroups,
    ...generatedPermissionGroups
      .map(([label, items]): [string, Permission[]] => [
        label,
        items.filter((id) => !owned.has(id)),
      ])
      .filter(([, items]) => items.length),
  ];
  allPermissions = [...publicPermissions, ...permissions.map((p) => p.id as Permission)].sort();
}
