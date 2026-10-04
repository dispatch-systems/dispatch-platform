import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';

// Core's own routes: what only the platform owner reads. Switching a DSP's features is tested
// with the features it switches, in app/tests/api/feature-switches.test.ts.
test('only the platform owner reads the platform: its DSPs, health and diagnostics', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.equal((await f.request('/api/platform/health')).status, 401);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  assert.equal((await member.get('/api/platform/dsps')).status, 403);
  const diagnostics = await owner.get('/api/platform/diagnostics');
  assert(diagnostics.value.runtime.coreMemoryBytes > 0);
  assert.equal(diagnostics.value.runtime.workerMemoryBytes, 0);
});
