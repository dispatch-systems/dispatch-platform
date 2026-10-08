import {
  permissionLabels as generatedPermissionLabels,
  permissionGroups as generatedPermissionGroups,
  impliedPermissions as generatedImpliedPermissions,
  permissionParents as generatedPermissionParents,
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

export const permissionLabels: Record<Permission, string> = generatedPermissionLabels;
/** The backend catalog owns the sections and each permission belongs to exactly one. */
const permissionGroups: readonly (readonly [string, readonly Permission[]])[] =
  generatedPermissionGroups;
/** The sections the role sheet shows a DSP: those whose permissions exist in it. */
export const visiblePermissionGroups = (view: DspView) =>
  permissionGroups.filter(([, items]) => items.some((p) => grants(view.features, p)));
/** Granting the key includes each of its values, however many steps away, from the backend's rules. */
export const impliedPermissions: Partial<Record<Permission, readonly Permission[]>> =
  generatedImpliedPermissions;
/** The permission each finer one sits under on the role sheet, which it includes too. */
export const permissionParents: Partial<Record<Permission, Permission>> =
  generatedPermissionParents;
