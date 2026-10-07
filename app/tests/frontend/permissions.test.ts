import test from 'node:test';
import assert from 'node:assert/strict';
import { permissions } from '../../../core/accounts/api/index.js';
import { impliedPermissions } from '../../../core/shell/frontend/runtime/permissions.js';

// The product's own permissions, as its features declare them.

test('generated permission implications cannot cycle', () => {
  for (const [grant, implied] of Object.entries(impliedPermissions)) {
    const trail = new Set<string>([grant]);
    let next: string | undefined = implied;
    while (next) {
      assert(!trail.has(next), `permission implication cycle from ${grant}`);
      trail.add(next);
      next = impliedPermissions[next as (typeof permissions)[number]];
    }
  }
});
