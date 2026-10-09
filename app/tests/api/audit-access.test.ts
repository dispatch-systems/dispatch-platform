import test from 'node:test';
import assert from 'node:assert/strict';
import { fixture } from '../../../core/shell/tests/support/support.js';

test('only platform owners can read and export audit events, even with old DSP audit grants', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const member = await f.client('member@dispatch.test');
  const north = member.session.dsps[0];
  await owner.select(north.id);

  // An existing custom role may still have the retired permission in storage.
  f.people(north.id, (db) =>
    db
      .prepare("UPDATE roles SET permissions=? WHERE dsp_id=? AND name='Member'")
      .run(JSON.stringify(['timecard.view', 'audit.view']), north.id),
  );
  const view = await member.select(north.id);
  assert.deepEqual(view.permissions, ['timecard.view']);
  // A DSP has no log of its own: its old paths are denied to a member, and an owner, who
  // holds everything, is told they aren't there.
  const denied = async (dsp: number) => {
    assert.equal((await member.get('/api/dsp/audit')).status, dsp);
    assert.equal((await member.post('/api/dsp/audit/export', {})).status, dsp);
    assert.equal((await member.get('/api/platform/audit')).status, 403);
    assert.equal((await member.post('/api/platform/audit/export', {})).status, 403);
  };
  await denied(403);

  // Owning a DSP still does not grant platform access.
  const roles = (await owner.get('/api/dsp/roles')).value;
  const dspOwner = roles.find((role: { owner: boolean }) => role.owner);
  const membership = (await owner.get('/api/dsp/members')).value.find(
    (row: { email: string }) => row.email === 'member@dispatch.test',
  );
  assert.equal(
    (await owner.post(`/api/dsp/members/${membership.id}`, { role: dspOwner.id })).status,
    200,
  );
  const ownerView = await member.select(north.id);
  assert.equal(ownerView.role.owner, true);
  assert(!ownerView.permissions.includes('audit.view'));
  await denied(404);

  // Even platform owners use the platform endpoints instead of a DSP log.
  await owner.select(north.id);
  assert.equal((await owner.get('/api/dsp/audit')).status, 404);
  assert.equal((await owner.post('/api/dsp/audit/export', {})).status, 404);
  const log = await owner.get(`/api/platform/audit?dsp=${north.id}`);
  assert.equal(log.status, 200);
  assert(
    log.value.events.some((event: { action: string }) => event.action === 'member.role_changed'),
  );
  assert(
    log.value.events.some((event: { action: string }) => event.action === 'dsp.owner_view_opened'),
  );
  assert(log.value.events.every((event: { dspId: string }) => event.dspId === north.id));
  assert.equal((await owner.get('/api/platform/audit?area=nowhere')).status, 400);
  const exported = await owner.post('/api/platform/audit/export', { dsp: north.id });
  assert.equal(exported.status, 200);
  assert(exported.value.events.length > 0);
  assert(exported.value.events.every((event: { dspId: string }) => event.dspId === north.id));
  const after = await owner.get('/api/platform/audit');
  assert(after.value.events.some((event: { action: string }) => event.action === 'audit.exported'));
});
