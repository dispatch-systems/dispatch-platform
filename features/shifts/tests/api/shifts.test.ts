import test from 'node:test';
import assert from 'node:assert/strict';
import { collectNow } from '../../../../core/collection/tests/support/collect.js';
import { demo, fixture } from '../../../../core/shell/tests/support/support.js';

test('the week collects, adding to what is kept, for those who may collect it', async (t) => {
  const f = await fixture();
  t.after(f.close);
  const owner = await f.client();
  const dsp = owner.session.dsps.find(
    (item: { name: string }) => item.name === 'Northline Logistics',
  );
  await owner.select(dsp.id);
  const profile = { name: dsp.name, abbreviation: 'NLL', stationCode: 'TST1' };
  assert.equal(
    (await owner.post('/api/dsp/profile', { ...profile, timezone: dsp.timezone })).status,
    200,
  );
  await owner.select(dsp.id);
  const connected = await owner.post('/api/dsp/connections/cortex', {
    username: 'fixture@example.test',
    password: 'fixture-password',
  });
  assert.equal(connected.status, 200, connected.body);
  await collectNow(owner, 'shifts', 'first');
  await collectNow(owner, 'shifts', 'second');
  // Two runs of the same week: the first adds each assignment, the second finds none changed.
  const kept = f.database(`dsps/${dsp.id}/data/shifts/shifts.sqlite`, (db) =>
    [
      'SELECT count(*) AS n FROM shift_runs',
      'SELECT count(*) AS n FROM shift_assignments WHERE listed=1',
      "SELECT count(*) AS n FROM shift_changes WHERE change='added'",
      "SELECT count(*) AS n FROM shift_changes WHERE change<>'added'",
    ].map((sql) => Number((db.prepare(sql).get() as { n: number }).n)),
  );
  assert.deepEqual(kept, [2, 20, 20, 0]);
  // No default role holds shifts.collect, so a member is refused until a role grants it.
  const member = await f.client(demo.member);
  await member.select(dsp.id);
  const now = { collects: 'shifts', requestId: 'member' };
  assert.equal((await member.post('/api/dsp/collectors/shifts/collect', now)).status, 403);
});
