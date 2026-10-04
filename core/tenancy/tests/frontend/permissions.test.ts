import test from 'node:test';
import assert from 'node:assert/strict';
import { permissions } from '../../../accounts/api/index.js';
import { permissionGroups, permissionLabels } from '../../../shell/frontend/runtime/permissions.js';

test('every permission has a label and one section in the role sheet', () => {
  const grouped = permissionGroups.flatMap(([, items]) => items);
  assert.deepEqual([...grouped].sort(), [...permissions].sort());
  assert.equal(new Set(grouped).size, grouped.length);
  assert.deepEqual(Object.keys(permissionLabels).sort(), [...permissions].sort());
});
