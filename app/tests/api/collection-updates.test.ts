import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';

test('a member follows collection updates only for the features its role may follow', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find((d: { name: string }) => d.name === 'Northline Logistics');
  await owner.select(dsp.id);
  const role = (await owner.get('/api/dsp/roles')).value.find(
    (r: { name: string }) => r.name === 'Member',
  );
  const saved = await owner.post('/api/dsp/roles/' + role.id, {
    name: 'Member',
    permissions: ['dvic.view'],
  });
  assert.equal(saved.status, 200, saved.body);
  // Changing a role ends the views signed before it.
  await owner.select(dsp.id);
  const viewer = await f.client('member@dispatch.test');
  await viewer.select(dsp.id);
  const initial = await viewer.get('/api/dsp/collection-updates?after=');
  assert.equal(initial.status, 200, initial.body);
  const waiting = viewer.get(`/api/dsp/collection-updates?after=${initial.value.revision}`);
  // A Paycom timecard collection is Timecard's, which this role can't see.
  const queued = await owner.post('/api/dsp/jobs', {
    requestId: 'live-other-feature',
    date: '2026-01-10',
  });
  assert.equal(queued.status, 202, queued.body);
  const event = await waiting;
  assert.notEqual(event.value.revision, initial.value.revision, 'the finished run wakes it');
  assert.deepEqual(event.value.changes, []);
});
