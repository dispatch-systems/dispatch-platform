import test from 'node:test';
import assert from 'node:assert/strict';
import { permissions } from '../../../../shared/contracts/accounts.js';
import {
  impliedPermissions,
  permissionGroups,
  permissionLabels,
} from '../../../shell/frontend/runtime/permissions.js';

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
