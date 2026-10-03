import test from 'node:test';
import assert from 'node:assert/strict';
import { permissions } from '../../../shared/contracts/accounts.js';
import { assignable } from '../../../features/team/frontend/assignable.js';
import type { DspView, Permission, Role } from '../../../shared/contracts/accounts.js';

const role = (id: string, rolePermissions: Permission[], owner = false): Role => ({
  id,
  name: id,
  owner,
  permissions: rolePermissions,
  members: 0,
  invitations: 0,
});

test('role assignability compares durable grants while their feature is off', () => {
  const target = role('target', ['uniforms.view', 'uniforms.manage']);
  const actor = role('actor', ['members.invite', 'members.manage', 'roles.manage']);
  const peer = role('peer', [...actor.permissions, ...target.permissions]);
  const owner = role('owner', [...permissions], true);
  const view = {
    role: { id: actor.id, name: actor.name, owner: false },
    permissions: actor.permissions,
    features: [],
  } as unknown as DspView;

  assert.equal(assignable(view, target, [actor, target]), false);
  assert.equal(
    assignable({ ...view, role: { id: peer.id, name: peer.name, owner: false } }, target, [
      peer,
      target,
    ]),
    true,
  );
  assert.equal(
    assignable({ ...view, role: { id: owner.id, name: owner.name, owner: true } }, target, [
      owner,
      target,
    ]),
    true,
  );
  assert.equal(assignable(view, owner, [actor, owner]), false);
});
