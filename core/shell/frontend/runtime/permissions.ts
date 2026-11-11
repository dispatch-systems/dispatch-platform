import {
  permissionLabels as generatedPermissionLabels,
  permissionGroups as generatedPermissionGroups,
  impliedPermissions as generatedImpliedPermissions,
  permissionParents as generatedPermissionParents,
  permissions as publicPermissions,
  schedulingPermissions as publicSchedulingPermissions,
} from '../../../tenancy/api/generated/access-catalog.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';
import { grants } from './features.js';

// A permission of a feature the DSP lacks is held by nobody, owners included.
export const can = (view: DspView | undefined, permission: Permission) =>
  Boolean(
    view &&
    grants(view.features, permission) &&
    (view.role.owner || view.permissions.includes(permission)),
  );
/** The permissions of `stored` that exist in the DSP; the rest are kept but never shown. */
export const visiblePermissions = (view: DspView, stored: readonly Permission[]) =>
  stored.filter((permission) => grants(view.features, permission));

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
export let schedulingPermissions: readonly Permission[] = publicSchedulingPermissions;
export function installPermissionCatalog(catalog?: {
  labels: typeof permissionLabels;
  groups: typeof permissionGroups;
  implied: typeof impliedPermissions;
  parents: typeof permissionParents;
  permissions: typeof allPermissions;
  scheduling: typeof schedulingPermissions;
}) {
  permissionLabels = catalog?.labels ?? generatedPermissionLabels;
  permissionGroups = catalog?.groups ?? generatedPermissionGroups;
  impliedPermissions = catalog?.implied ?? generatedImpliedPermissions;
  permissionParents = catalog?.parents ?? generatedPermissionParents;
  allPermissions = catalog?.permissions ?? publicPermissions;
  schedulingPermissions = catalog?.scheduling ?? publicSchedulingPermissions;
}
