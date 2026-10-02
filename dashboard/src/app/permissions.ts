import {
  permissionLabels as generatedPermissionLabels,
  permissionGroups as generatedPermissionGroups,
  impliedPermissions as generatedImpliedPermissions,
} from '../../../shared/contracts/generated/access-catalog.js';
import type { DspView, Permission } from '../../../shared/contracts/index.js';
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
export const permissionGroups: readonly (readonly [string, readonly Permission[]])[] =
  generatedPermissionGroups;
/** The sections the role sheet shows a DSP: those whose permissions exist in it. */
export const visiblePermissionGroups = (view: DspView) =>
  permissionGroups.filter(([, items]) => items.some((p) => grants(view.features, p)));
/** Granting the key includes its value, from the backend's implication rules. */
export const impliedPermissions: Partial<Record<Permission, Permission>> =
  generatedImpliedPermissions;
