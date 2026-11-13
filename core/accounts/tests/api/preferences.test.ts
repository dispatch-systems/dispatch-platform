import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../shell/tests/support/support.js';

const SIDEBAR = '/api/dsp/preferences/shell.sidebar';

test("a member's preferences come with their view of a DSP, theirs alone and that DSP's alone, and a save from an older revision is told what's kept", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const named = (name: string) =>
    owner.session.dsps.find((dsp: { name: string }) => dsp.name === name);
  const [north, summit] = [named('Northline Logistics'), named('Summit Delivery')];
  assert.deepEqual((await owner.select(north.id)).preferences, {});
  const arranged = { order: ['team', 'timecard'], hidden: ['documents'] };
  const saved = await owner.post(SIDEBAR, { revision: 0, value: arranged });
  assert.deepEqual([saved.status, saved.value], [200, { outcome: 'saved', revision: 1 }]);
  // Another of their screens, still at none, is told what's kept rather than replacing it.
  const stale = await owner.post(SIDEBAR, { revision: 0, value: { order: [], hidden: [] } });
  assert.deepEqual(stale.value, {
    outcome: 'stale',
    current: { revision: 1, value: arranged },
  });
  assert.deepEqual((await owner.select(north.id)).preferences, {
    'shell.sidebar': { revision: 1, value: arranged },
  });
  assert.deepEqual((await owner.select(summit.id)).preferences, {});
  const member = await f.client('member@dispatch.test');
  assert.deepEqual((await member.select(north.id)).preferences, {});
  // Only a value of its shape, from a revision, of a preference that exists, is kept.
  for (const [body, status] of [
    [{ revision: 0, value: { order: 'team' } }, 400],
    [{ revision: 0, value: { order: [''], hidden: [] } }, 400],
    [{ revision: 0, value: { pinned: ['team'] } }, 400],
    [{ value: arranged }, 400],
    [{ revision: -1, value: arranged }, 400],
  ] as const)
    assert.equal((await member.post(SIDEBAR, body)).status, status);
  assert.equal(
    (await member.post('/api/dsp/preferences/shell.nothing', { revision: 0, value: {} })).status,
    404,
  );
  assert.deepEqual((await member.select(north.id)).preferences, {});
  // Cleared, it's back to the app's own.
  const cleared = await owner.post(SIDEBAR, { revision: 1, value: null });
  assert.deepEqual(cleared.value, { outcome: 'saved', revision: 0 });
  assert.deepEqual((await owner.select(north.id)).preferences, {});
});
