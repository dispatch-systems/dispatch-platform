import test from 'node:test';
import assert from 'node:assert/strict';
import type { DatabaseSync } from 'node:sqlite';
import { demo, fixture } from '../../../shell/tests/support/support.js';

test('memberships written without a role id resolve through the legacy role after restart', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const north = (await f.client('member@dispatch.test')).session.dsps[0];
  await f.stop();
  // A DSP's people and roles are its own directory's, which startup repairs as it does the
  // platform's.
  f.people(north.id, (db) => {
    db.exec('UPDATE memberships SET role_id=NULL');
    db.exec("DELETE FROM roles WHERE name='Member'");
  });
  await f.start();
  const member = await f.client('member@dispatch.test');
  // The Member role comes back as every role starts: with no permission.
  assert.deepEqual((await member.select(north.id)).permissions, []);
  const rows = f.people(north.id, (db) =>
    db
      .prepare(
        'SELECT m.role,r.name FROM memberships m JOIN roles r ON r.id=m.role_id AND r.dsp_id=m.dsp_id',
      )
      .all(),
  );
  assert.deepEqual(
    rows.map((row) => ({ ...row })),
    [{ role: 'member', name: 'Member' }],
  );
});

test("a DSP's roles and people are its own, and the platform owner opens any DSP without being one of them", async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const named = (name: string) =>
    owner.session.dsps.find((dsp: { name: string }) => dsp.name === name);
  const [north, summit] = [named('Northline Logistics'), named('Summit Delivery')];
  await owner.select(north.id);
  const made = await owner.post('/api/dsp/roles', { name: 'Dispatcher', permissions: [] });
  assert.equal(made.status, 201, made.body);
  const people = (await owner.get('/api/dsp/members')).value.map(
    (member: { email: string }) => member.email,
  );
  assert.deepEqual(people, [demo.member]);
  // A role made at one DSP is none of another's, and its id names nothing there.
  await owner.select(summit.id);
  const roles = (await owner.get('/api/dsp/roles')).value.map(
    (role: { name: string }) => role.name,
  );
  assert.ok(!roles.includes('Dispatcher'), JSON.stringify(roles));
  const elsewhere = await owner.post(`/api/dsp/roles/${made.value.id}`, {
    name: 'Dispatcher',
    permissions: [],
  });
  assert.deepEqual([elsewhere.status, elsewhere.value.error], [404, 'role_not_found']);
  assert.deepEqual((await owner.get('/api/dsp/members')).value, []);
  // Each DSP's directory keeps them: Northline's database, not Summit's or the platform's.
  const dispatchers = (db: DatabaseSync) =>
    (db.prepare("SELECT count(*) n FROM roles WHERE name='Dispatcher'").get() as { n: number }).n;
  assert.deepEqual([f.people(north.id, dispatchers), f.people(summit.id, dispatchers)], [1, 0]);
  assert.equal(f.database('data/platform/accounts.sqlite', dispatchers), 0);
});
