import {
  permissionLabels,
  permissionGroups,
  impliedPermissions,
  permissionParents,
  permissions as publicPermissions,
  schedulingPermissions,
} from '../../../tenancy/api/generated/access-catalog.js';
import type { DspView, Permission } from '../../../accounts/api/index.js';
import { featureCatalog, featureLabel, type PageEntry } from './features.js';
import { installPermissionCatalog } from './permissions.js';

/** Build the role choices only when admitting a DSP with custom declarations. */
export function installCustomPermissions(
  custom?: DspView['custom'],
  scheduling: readonly Permission[] = [],
) {
  if (!custom) return installPermissionCatalog();
  const permissions = custom.permissions;
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
  installPermissionCatalog({
    labels: {
      ...permissionLabels,
      ...Object.fromEntries(permissions.map((p) => [p.id, p.label])),
    },
    implied: {
      ...impliedPermissions,
      ...Object.fromEntries(permissions.map((p) => [p.id, p.implied])),
    },
    parents: {
      ...permissionParents,
      ...Object.fromEntries(permissions.flatMap((p) => (p.under ? [[p.id, p.under]] : []))),
    },
    groups: [
      ...pageGroups,
      ...permissionGroups
        .map(([label, items]): [string, Permission[]] => [
          label,
          items.filter((id) => !owned.has(id)),
        ])
        .filter(([, items]) => items.length),
    ],
    permissions: [...publicPermissions, ...permissions.map((p) => p.id as Permission)].sort(),
    scheduling: [...schedulingPermissions, ...scheduling],
  });
}
