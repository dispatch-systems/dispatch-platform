import type { DspView, Role } from '../../../../shared/contracts/index.js';

// Nobody hands out access they do not hold; the server enforces the same rule,
// including stored grants whose features are currently switched off.
export function assignable(view: DspView, role: Role, roles: readonly Role[]) {
  const held = roles.find((candidate) => candidate.id === view.role.id)?.permissions ?? [];
  return role.owner
    ? view.role.owner
    : view.role.owner || role.permissions.every((permission) => held.includes(permission));
}
