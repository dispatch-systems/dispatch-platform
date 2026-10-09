import test from 'node:test';
import assert from 'node:assert/strict';
import type { DatabaseSync } from 'node:sqlite';
import { demo, fixture } from '../../../shell/tests/support/support.js';

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
