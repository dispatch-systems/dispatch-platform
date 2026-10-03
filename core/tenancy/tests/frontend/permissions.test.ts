import test from 'node:test';
import assert from 'node:assert/strict';
import { permissions } from '../../../../shared/contracts/accounts.js';
import {
  impliedPermissions,
  permissionGroups,
  permissionLabels,
} from '../../../shell/frontend/runtime/permissions.js';
import { assignable } from '../../../../features/team/frontend/assignable.js';
import type { DspView, Permission, Role } from '../../../../shared/contracts/index.js';

const role = (id: string, rolePermissions: Permission[], owner = false): Role => ({
  id,
  name: id,
  owner,
  permissions: rolePermissions,
  members: 0,
  invitations: 0,
});

test('every permission has a label and one section in the role sheet', () => {
  const grouped = permissionGroups.flatMap(([, items]) => items);
  assert.deepEqual([...grouped].sort(), [...permissions].sort());
  assert.equal(new Set(grouped).size, grouped.length);
  assert.deepEqual(Object.keys(permissionLabels).sort(), [...permissions].sort());
});

test('generated permission implications refer to known grants and cannot cycle', () => {
  for (const [grant, implied] of Object.entries(impliedPermissions)) {
    assert(permissions.includes(grant as (typeof permissions)[number]));
    assert(permissions.includes(implied));
    const trail = new Set<string>([grant]);
    let next: string | undefined = implied;
    while (next) {
      assert(!trail.has(next), `permission implication cycle from ${grant}`);
      trail.add(next);
      next = impliedPermissions[next as (typeof permissions)[number]];
    }
  }
  assert.equal(impliedPermissions['timecard.manage'], 'timecard.view');
  assert.equal(impliedPermissions['uniforms.adjust'], 'uniforms.view');
  assert.equal(impliedPermissions['routes.collect'], 'routes.view');
  assert.equal(impliedPermissions['dvic.collect'], 'dvic.view');
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
