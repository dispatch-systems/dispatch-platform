import { schedulingPermissions as publicSchedulingPermissions } from '../../../tenancy/api/generated/access-catalog.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';
import { grants } from './features.js';
export {
  permissionLabels,
  visiblePermissionGroups,
  impliedPermissions,
  permissionParents,
  allPermissions,
} from './permission-catalog.js';

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

export let schedulingPermissions: readonly Permission[] = publicSchedulingPermissions;
export function installSchedulingPermissions(permissions: readonly Permission[] = []) {
  schedulingPermissions = [...publicSchedulingPermissions, ...permissions];
}
